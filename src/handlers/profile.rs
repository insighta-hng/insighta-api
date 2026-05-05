use crate::{
    AppState,
    errors::{AppError, Result},
    middleware::role::{RequireAdmin, RequireAny},
    models::profile::{
        CreateProfileRequest, ImportResult, ImportSkipReasons, Profile, ProfileDto, ProfileFilters,
        ProfileQuery, ProfileResponse, SearchQuery,
    },
    normalizer::build_cache_key,
    parser::parse_query,
    utils::{
        build_list_response, fetch_age_data, fetch_country_data, fetch_gender_data,
        validate_csv_row, validate_name,
    },
};
use axum::{
    Json,
    body::Body,
    extract::{
        Multipart, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{StatusCode, header},
    response::{AppendHeaders, IntoResponse, Response},
};
use uuid::Uuid;

/// Creates a new profile by querying external demography APIs (Genderize, Agify, Nationalize).
///
/// If a profile with the given name already exists, it is returned instead (idempotent).
///
/// # Arguments
///
/// * `state` - The application state containing the database repository and HTTP client.
/// * `payload` - JSON payload containing the profile `name`.
///
/// # Returns
///
/// Returns `201 Created` with the new profile data on success, or `200 OK` if it already exists.
///
/// # Errors
///
/// Returns `AppError::BadRequest` for missing/empty name.
/// Returns `AppError::BadGateway` if external APIs return unusable data.
pub async fn create_profile(
    State(state): State<AppState>,
    _auth: RequireAdmin,
    payload: std::result::Result<Json<CreateProfileRequest>, JsonRejection>,
) -> Result<impl IntoResponse> {
    let Json(payload) = payload.map_err(|err| AppError::BadRequest(err.body_text()))?;
    let name = validate_name(payload.name)?;

    if let Some(existing) = state.profile_repo.find_by_name(&name).await? {
        return Ok((
            StatusCode::OK,
            Json(ProfileResponse {
                status: "success".into(),
                message: Some("Profile already exists".into()),
                data: existing.into(),
            }),
        )
            .into_response());
    }

    let (gender_res, age_res, country_res) = tokio::try_join!(
        fetch_gender_data(&state.client, &state.config.genderize_url, &name),
        fetch_age_data(&state.client, &state.config.agify_url, &name),
        fetch_country_data(&state.client, &state.config.nationalize_url, &name)
    )?;

    let new_profile = Profile {
        id: Uuid::now_v7(),
        name: name.to_string(),
        gender: gender_res.gender.unwrap(), // Safe because fetch_gender_data validates it
        gender_probability: (gender_res.gender_probability * 100.0).round() / 100.0,
        age: age_res.age.unwrap_or(0),
        age_group: format!("{:?}", age_res.age_group).to_lowercase(),
        country_id: country_res.country_id,
        country_name: country_res.country_name,
        country_probability: (country_res.country_probability * 100.0).round() / 100.0,
        created_at: chrono::Utc::now(),
    };

    state
        .profile_repo
        .insert_profile(new_profile.clone())
        .await?;

    // A new profile means cached list/search results are stale.
    state.cache.clear();

    Ok((
        StatusCode::CREATED,
        Json(ProfileResponse {
            status: "success".into(),
            message: None,
            data: new_profile.into(),
        }),
    )
        .into_response())
}

/// Retrieves a single profile by its UUID.
///
/// # Arguments
///
/// * `state` - The application state containing the database repository.
/// * `id` - The UUID of the requested profile.
///
/// # Returns
///
/// Returns `200 OK` with the full profile object if found.
///
/// # Errors
///
/// Returns `AppError::NotFound` if no profile exists with the given ID.
pub async fn get_profile(
    State(state): State<AppState>,
    _auth: RequireAny,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| AppError::UnprocessableEntity("Invalid parameter type".into()))?;
    let profile = state
        .profile_repo
        .find_by_id(uuid)
        .await?
        .ok_or_else(|| AppError::NotFound("Profile not found".into()))?;

    Ok(Json(ProfileResponse {
        status: "success".into(),
        message: None,
        data: profile.into(),
    }))
}

/// Lists profiles with optional filtering, sorting, and pagination.
///
/// # Arguments
///
/// * `state` - The application state containing the database repository.
/// * `query` - Optional query parameters for filtering (`gender`, `age_group`, `country_id`, etc.),
///   sorting (`sort_by`, `order`), and pagination (`page`, `limit`).
///
/// # Returns
///
/// Returns a `ProfileListResponse` containing the paginated data and metadata.
///
/// # Errors
///
/// Returns `AppError::UnprocessableEntity` if query parameters are structurally invalid.
pub async fn list_profiles(
    State(state): State<AppState>,
    _auth: RequireAny,
    query: std::result::Result<Query<ProfileQuery>, QueryRejection>,
) -> Result<impl IntoResponse> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".into()))?;

    let filters = ProfileFilters {
        gender: query.gender,
        country_id: query.country_id,
        age_group: query.age_group,
        min_age: query.min_age,
        max_age: query.max_age,
        min_gender_probability: query.min_gender_probability,
        min_country_probability: query.min_country_probability,
    };

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 50);
    let sort_by = query.sort_by.unwrap_or_default();
    let order = query.order.unwrap_or_default();

    let cache_key = build_cache_key("list", &filters, &sort_by, &order, page, limit);

    if let Some(cached) = state.cache.get(&cache_key) {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(cached))
            .map_err(|err| AppError::InternalServerError(err.to_string()));
    }

    let extra_params = {
        let mut params: Vec<(String, String)> = Vec::new();
        if let Some(ref gender) = filters.gender {
            params.push(("gender".into(), gender.to_string()));
        }
        if let Some(ref country_id) = filters.country_id {
            params.push(("country_id".into(), country_id.clone()));
        }
        if let Some(ref age_group) = filters.age_group {
            params.push(("age_group".into(), age_group.clone()));
        }
        if let Some(min_age) = filters.min_age {
            params.push(("min_age".into(), min_age.to_string()));
        }
        if let Some(max_age) = filters.max_age {
            params.push(("max_age".into(), max_age.to_string()));
        }
        if let Some(probability) = filters.min_gender_probability {
            params.push(("min_gender_probability".into(), probability.to_string()));
        }
        if let Some(probability) = filters.min_country_probability {
            params.push(("min_country_probability".into(), probability.to_string()));
        }
        params.push(("sort_by".into(), sort_by.as_str().into()));
        params.push(("order".into(), order.as_str().into()));
        params
    };

    let (profiles, total) = state
        .profile_repo
        .find_paginated(filters, sort_by, order, page, limit)
        .await?;

    let data: Vec<ProfileDto> = profiles.into_iter().map(Into::into).collect();

    let response_body =
        build_list_response("/api/profiles", page, limit, total, &extra_params, data);

    let serialized = serde_json::to_vec(&response_body)
        .map_err(|err| AppError::InternalServerError(err.to_string()))?;

    state.cache.set(cache_key, serialized.clone());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serialized))
        .map_err(|err| AppError::InternalServerError(err.to_string()))
}

/// Deletes an existing profile by its UUID.
///
/// # Arguments
///
/// * `state` - The application state containing the database repository.
/// * `id` - The UUID of the profile to delete.
///
/// # Returns
///
/// Returns `204 No Content` on successful deletion.
///
/// # Errors
///
/// Returns `AppError::NotFound` if no profile matches the given ID.
pub async fn delete_profile(
    State(state): State<AppState>,
    _auth: RequireAdmin,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| AppError::UnprocessableEntity("Invalid parameter type".into()))?;
    let deleted = state.profile_repo.delete_by_id(uuid).await?;

    if !deleted {
        return Err(AppError::NotFound("Profile not found".into()));
    }

    state.cache.clear();

    Ok(StatusCode::NO_CONTENT)
}

/// Searches for profiles using a natural language query string.
///
/// The query is parsed into demographic filters (gender, age group, country) and combined
/// with any explicit query parameters (which take precedence).
///
/// # Arguments
///
/// * `state` - The application state containing the database repository.
/// * `query` - Query parameters including the mandatory `q` search string.
///
/// # Returns
///
/// Returns a `ProfileListResponse` containing the paginated search results.
///
/// # Errors
///
/// Returns `AppError::BadRequest` if the query is missing or cannot be interpreted.
pub async fn search_profiles(
    State(state): State<AppState>,
    _auth: RequireAny,
    query: std::result::Result<Query<SearchQuery>, QueryRejection>,
) -> Result<impl IntoResponse> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".into()))?;

    let search_string = query
        .q
        .ok_or_else(|| AppError::BadRequest("Missing or empty parameter".into()))?;
    if search_string.trim().is_empty() {
        return Err(AppError::BadRequest("Missing or empty parameter".into()));
    }

    let (filters, parsed_search_query) = parse_query(&search_string)?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(parsed_search_query.limit.unwrap_or(10))
        .clamp(1, 50);
    let sort_by = query
        .sort_by
        .unwrap_or(parsed_search_query.sort_by.unwrap_or_default());
    let order = query
        .order
        .unwrap_or(parsed_search_query.order.unwrap_or_default());

    let cache_key = build_cache_key("search", &filters, &sort_by, &order, page, limit);

    if let Some(cached) = state.cache.get(&cache_key) {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(cached))
            .map_err(|err| AppError::InternalServerError(err.to_string()));
    }

    let extra_params = vec![
        ("q".into(), search_string.clone()),
        ("sort_by".into(), sort_by.as_str().into()),
        ("order".into(), order.as_str().into()),
    ];

    let (profiles, total) = state
        .profile_repo
        .find_paginated(filters, sort_by, order, page, limit)
        .await?;

    let data: Vec<ProfileDto> = profiles.into_iter().map(Into::into).collect();

    let response_body = build_list_response(
        "/api/profiles/search",
        page,
        limit,
        total,
        &extra_params,
        data,
    );

    let serialized = serde_json::to_vec(&response_body)
        .map_err(|err| AppError::InternalServerError(err.to_string()))?;

    state.cache.set(cache_key, serialized.clone());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serialized))
        .map_err(|err| AppError::InternalServerError(err.to_string()))
}

/// Exports profiles as a downloadable CSV file, with optional filtering and sorting.
///
/// Accepts the same filter and sort query parameters as `list_profiles` but returns
/// all matching profiles (unpaginated) as a CSV attachment. The `format=csv` query
/// parameter is required to prevent accidental unformatted responses.
///
/// # Arguments
///
/// * `state` - The application state containing the database repository.
/// * `_auth` - Extractor that enforces authentication (user is not directly used).
/// * `query` - Query parameters for filtering (`gender`, `age_group`, `country_id`, etc.),
///   sorting (`sort_by`, `order`), and the mandatory `format=csv` flag.
///
/// # Returns
///
/// Returns `200 OK` with a `text/csv` body and a `Content-Disposition: attachment`
/// header containing a timestamped filename (e.g. `profiles_20240101T120000Z.csv`).
///
/// # Errors
///
/// Returns `AppError::UnprocessableEntity` if query parameters are structurally invalid.
/// Returns `AppError::BadRequest` if the `format` parameter is missing or not `"csv"`.
/// Returns `AppError::InternalServerError` if CSV serialization or header construction fails.
pub async fn export_profiles_to_csv(
    State(state): State<AppState>,
    _auth: RequireAny,
    query: std::result::Result<Query<ProfileQuery>, QueryRejection>,
) -> Result<impl IntoResponse> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".into()))?;

    match query.format.as_deref() {
        Some("csv") => {}
        _ => {
            return Err(AppError::BadRequest(
                "format parameter must be 'csv'".into(),
            ));
        }
    }

    let filters = ProfileFilters {
        gender: query.gender,
        country_id: query.country_id,
        age_group: query.age_group,
        min_age: query.min_age,
        max_age: query.max_age,
        min_gender_probability: query.min_gender_probability,
        min_country_probability: query.min_country_probability,
    };

    let sort_by = query.sort_by.unwrap_or_default();
    let order = query.order.unwrap_or_default();

    let profiles = state.profile_repo.find_all(filters, sort_by, order).await?;

    let mut writer = csv::Writer::from_writer(vec![]);

    writer
        .write_record([
            "id",
            "name",
            "gender",
            "gender_probability",
            "age",
            "age_group",
            "country_id",
            "country_name",
            "country_probability",
            "created_at",
        ])
        .map_err(|err| AppError::InternalServerError(format!("CSV write error: {err}")))?;

    for profile in profiles {
        writer
            .write_record([
                profile.id.to_string(),
                profile.name,
                profile.gender.to_string(),
                profile.gender_probability.to_string(),
                profile.age.to_string(),
                profile.age_group,
                profile.country_id,
                profile.country_name,
                profile.country_probability.to_string(),
                profile
                    .created_at
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ])
            .map_err(|err| AppError::InternalServerError(format!("CSV write error: {err}")))?;
    }

    let csv_bytes = writer
        .into_inner()
        .map_err(|err| AppError::InternalServerError(format!("CSV flush error: {err}")))?;

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let filename = format!("profiles_{timestamp}.csv");

    let content_disposition =
        header::HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|err| AppError::InternalServerError(format!("Invalid header value: {err}")))?;

    let headers = AppendHeaders([
        (
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/csv"),
        ),
        (header::CONTENT_DISPOSITION, content_disposition),
    ]);

    Ok((headers, csv_bytes))
}

const IMPORT_BATCH_SIZE: usize = 1_000;

pub async fn import_profiles(
    State(state): State<AppState>,
    _auth: RequireAdmin,
    mut multipart: Multipart,
) -> Result<impl IntoResponse> {
    // Pull the file field from the multipart stream.
    let file_bytes = loop {
        let field = multipart
            .next_field()
            .await
            .map_err(|err| AppError::BadRequest(format!("Multipart read error: {err}")))?
            .ok_or_else(|| AppError::BadRequest("No file field found in request".into()))?;

        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let bytes = field
                .bytes()
                .await
                .map_err(|err| AppError::BadRequest(format!("Failed to read file field: {err}")))?;
            break bytes;
        }
        // Skip non-file fields and keep looking.
    };

    // Parse and validate all rows in a blocking thread to avoid stalling the executor.
    let (total_rows, inserted, reasons) = tokio::task::spawn_blocking(move || {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(false)
            .trim(csv::Trim::All)
            .from_reader(file_bytes.as_ref());

        let headers = reader
            .headers()
            .map_err(|err| AppError::BadRequest(format!("Could not read CSV headers: {err}")))?
            .clone();

        let mut total_rows: u64 = 0;
        let mut reasons = ImportSkipReasons::default();

        // Collect all records first so we can do batched inserts below.
        // Memory is bounded: 500k rows × ~200 bytes ≈ 100 MB, which is the documented maximum.
        let mut validated: Vec<Profile> = Vec::new();

        for result in reader.records() {
            total_rows += 1;
            match result {
                Err(_) => {
                    reasons.malformed += 1;
                }
                Ok(record) => {
                    if record.len() != headers.len() {
                        reasons.malformed += 1;
                        continue;
                    }
                    if let Some(profile) = validate_csv_row(&record, &headers, &mut reasons) {
                        validated.push(profile);
                    }
                }
            }
        }

        Ok::<_, AppError>((total_rows, validated, reasons))
    })
    .await
    .map_err(|err| AppError::InternalServerError(format!("Import task failed: {err}")))??;

    // Run batched inserts outside the blocking task so we can use async repo methods.
    let mut final_reasons = reasons;
    let mut inserted_count: u64 = 0;

    for chunk in inserted.chunks(IMPORT_BATCH_SIZE) {
        let result = state.profile_repo.bulk_insert(chunk.to_vec()).await?;
        inserted_count += result.inserted;
        final_reasons.duplicate_name += result.duplicate_name;
    }

    // Imported data changes what list/search endpoints would return.
    if inserted_count > 0 {
        state.cache.clear();
    }

    let skipped = total_rows.saturating_sub(inserted_count);

    Ok(Json(ImportResult {
        status: "success".into(),
        total_rows,
        inserted: inserted_count,
        skipped,
        reasons: final_reasons,
    }))
}

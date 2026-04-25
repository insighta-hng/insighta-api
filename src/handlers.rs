use crate::{
    AppState,
    errors::{AppError, Result},
    models::profile::{
        CreateProfileRequest, ProfileDto, ProfileListResponse, ProfileQuery, ProfileResponse,
        SearchQuery,
    },
    parser::parse_query,
    repo::profile::{Profile, ProfileFilters},
    utils::{fetch_age_data, fetch_country_data, fetch_gender_data, validate_name},
};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    response::IntoResponse,
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
    payload: std::result::Result<Json<CreateProfileRequest>, JsonRejection>,
) -> Result<impl IntoResponse> {
    let Json(payload) = payload.map_err(|e| AppError::BadRequest(e.body_text()))?;
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
        fetch_gender_data(&state.client, &name),
        fetch_age_data(&state.client, &name),
        fetch_country_data(&state.client, &name)
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

    let (profiles, total) = state
        .profile_repo
        .find_paginated(filters, sort_by, order, page, limit)
        .await?;

    let data: Vec<ProfileDto> = profiles.into_iter().map(Into::into).collect();

    Ok(Json(ProfileListResponse {
        status: "success".into(),
        page,
        limit,
        total,
        data,
    }))
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
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| AppError::UnprocessableEntity("Invalid parameter type".into()))?;
    let deleted = state.profile_repo.delete_by_id(uuid).await?;

    if !deleted {
        return Err(AppError::NotFound("Profile not found".into()));
    }

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
    query: std::result::Result<Query<SearchQuery>, QueryRejection>,
) -> Result<impl IntoResponse> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".into()))?;
    let q = query
        .q
        .ok_or_else(|| AppError::BadRequest("Missing or empty parameter".into()))?;
    if q.trim().is_empty() {
        return Err(AppError::BadRequest("Missing or empty parameter".into()));
    }

    let (filters, parsed_search_query) = parse_query(&q)?;

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

    let (profiles, total) = state
        .profile_repo
        .find_paginated(filters, sort_by, order, page, limit)
        .await?;

    let data: Vec<ProfileDto> = profiles.into_iter().map(Into::into).collect();

    Ok(Json(ProfileListResponse {
        status: "success".into(),
        page,
        limit,
        total,
        data,
    }))
}

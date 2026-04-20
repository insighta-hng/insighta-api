use crate::{
    AppState,
    errors::{AppError, Result},
    models::{
        db::{Profile, ProfileFilters},
        profile::{
            CreateProfileRequest, ProfileListEntry, ProfileListResponse, ProfileQuery,
            ProfileResponse, SearchQuery,
        },
    },
    parser::parse_query,
    utils::{fetch_age_data, fetch_country_data, fetch_gender_data, validate_name},
};
use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;

pub async fn create_profile(
    State(state): State<AppState>,
    payload: std::result::Result<Json<CreateProfileRequest>, JsonRejection>,
) -> Result<impl IntoResponse> {
    let Json(payload) = payload.map_err(|e| AppError::BadRequest(e.body_text()))?;
    let name = validate_name(payload.name)?;

    if let Some(existing) = state.db.find_by_name(&name).await? {
        return Ok((
            StatusCode::OK,
            Json(ProfileResponse {
                status: "success".into(),
                message: Some("Profile already exists".into()),
                data: existing,
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
        id: Uuid::now_v7().to_string(),
        name: name.to_string(),
        gender: gender_res.gender.unwrap_or_else(|| "unknown".to_string()),
        gender_probability: (gender_res.gender_probability * 100.0).round() / 100.0,
        age: age_res.age.unwrap_or(0),
        age_group: format!("{:?}", age_res.age_group).to_lowercase(),
        country_id: country_res.country_id,
        country_name: country_res.country_name,
        country_probability: (country_res.country_probability * 100.0).round() / 100.0,
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };

    state.db.insert_profile(new_profile.clone()).await?;

    Ok((
        StatusCode::CREATED,
        Json(ProfileResponse {
            status: "success".into(),
            message: None,
            data: new_profile,
        }),
    )
        .into_response())
}

pub async fn get_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let profile = state
        .db
        .find_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("Profile not found".into()))?;

    Ok(Json(ProfileResponse {
        status: "success".into(),
        message: None,
        data: profile,
    }))
}

pub async fn list_profiles(
    State(state): State<AppState>,
    Query(query): Query<ProfileQuery>,
) -> Result<impl IntoResponse> {
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
    let limit = query.limit.unwrap_or(10).min(50);
    let sort_by = query.sort_by.unwrap_or_default();
    let order = query.order.unwrap_or_default();

    let (profiles, total) = state
        .db
        .find_paginated(filters, sort_by, order, page, limit)
        .await?;

    let data: Vec<ProfileListEntry> = profiles
        .into_iter()
        .map(|profile| ProfileListEntry {
            id: profile.id,
            name: profile.name,
            gender: profile.gender,
            gender_probability: profile.gender_probability,
            age: profile.age,
            age_group: profile.age_group,
            country_id: profile.country_id,
            country_name: profile.country_name,
            country_probability: profile.country_probability,
            created_at: profile.created_at,
        })
        .collect();

    Ok(Json(ProfileListResponse {
        status: "success".into(),
        page,
        limit,
        total,
        data,
    }))
}

pub async fn delete_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let deleted = state.db.delete_by_id(&id).await?;

    if !deleted {
        return Err(AppError::NotFound("Profile not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn search_profiles(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse> {
    let q = query
        .q
        .ok_or_else(|| AppError::BadRequest("Missing or empty parameter".to_string()))?;
    if q.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Missing or empty parameter".to_string(),
        ));
    }

    let (filters, parsed_search_query) = parse_query(&q)?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(parsed_search_query.limit.unwrap_or(10))
        .min(50);
    let sort_by = query
        .sort_by
        .unwrap_or(parsed_search_query.sort_by.unwrap_or_default());
    let order = query
        .order
        .unwrap_or(parsed_search_query.order.unwrap_or_default());

    let (profiles, total) = state
        .db
        .find_paginated(filters, sort_by, order, page, limit)
        .await?;

    let data: Vec<ProfileListEntry> = profiles
        .into_iter()
        .map(|profile| ProfileListEntry {
            id: profile.id,
            name: profile.name,
            gender: profile.gender,
            gender_probability: profile.gender_probability,
            age: profile.age,
            age_group: profile.age_group,
            country_id: profile.country_id,
            country_name: profile.country_name,
            country_probability: profile.country_probability,
            created_at: profile.created_at,
        })
        .collect();

    Ok(Json(ProfileListResponse {
        status: "success".into(),
        page,
        limit,
        total,
        data,
    }))
}

use axum::extract::Request;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    AppState,
    client::ReqwestClient,
    countries::COUNTRIES,
    errors::{AppError, Result},
    models::{
        age::{AgeGroup, AgifyResponse},
        auth::EmailEntry,
        country::{NationalizeRawResponse, NationalizeResponse},
        gender::GenderizeResponse,
        profile::{PaginationLinks, ProfileDto, ProfileListResponse},
        user::Role,
    },
};

pub fn iso_to_country_name(code: &str) -> &'static str {
    let uppercase_code = code.to_uppercase();

    COUNTRIES
        .iter()
        .find(|&(_, &val)| val == uppercase_code)
        .map(|(&key, _)| key)
        .unwrap_or("Unknown")
}

pub fn validate_name(name_value: Option<Value>) -> Result<String> {
    match name_value {
        None => Err(AppError::BadRequest(
            "Missing or empty parameter".to_string(),
        )),
        Some(Value::String(name)) => {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                Err(AppError::BadRequest(
                    "Missing or empty parameter".to_string(),
                ))
            } else {
                Ok(trimmed)
            }
        }
        Some(_) => Err(AppError::UnprocessableEntity(
            "Invalid parameter type".to_string(),
        )),
    }
}

pub async fn fetch_gender_data(
    reqwest_client: &ReqwestClient,
    name: &str,
) -> Result<GenderizeResponse> {
    let client = reqwest_client.get();
    let response: GenderizeResponse = client
        .get("https://api.genderize.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    if response.gender.is_none() || response.sample_size == 0 {
        return Err(AppError::UpstreamInvalidResponse("Genderize".to_string()));
    }

    Ok(response)
}

pub async fn fetch_age_data(reqwest_client: &ReqwestClient, name: &str) -> Result<AgifyResponse> {
    let client = reqwest_client.get();
    let mut response: AgifyResponse = client
        .get("https://api.agify.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    if response.age.is_none() {
        return Err(AppError::UpstreamInvalidResponse("Agify".to_string()));
    }

    response.age_group = AgeGroup::classify(response.age.unwrap_or(0));

    Ok(response)
}

pub async fn fetch_country_data(
    reqwest_client: &ReqwestClient,
    name: &str,
) -> Result<NationalizeResponse> {
    let client = reqwest_client.get();
    let response: NationalizeRawResponse = client
        .get("https://api.nationalize.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    let best_country = response
        .country
        .into_iter()
        .max_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| AppError::UpstreamInvalidResponse("Nationalize".to_string()))?;

    Ok(NationalizeResponse {
        country_name: iso_to_country_name(&best_country.country_id).to_string(),
        country_id: best_country.country_id,
        country_probability: best_country.probability,
    })
}

pub async fn fetch_github_primary_email(state: &AppState, github_token: &str) -> Result<String> {
    let emails: Vec<EmailEntry> = state
        .client
        .get()
        .get("https://api.github.com/user/emails")
        .header("Authorization", format!("Bearer {}", github_token))
        .header("User-Agent", "insighta-api")
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(e.to_string()))?
        .json()
        .await
        .map_err(|_| {
            AppError::UpstreamInvalidResponse("Failed to fetch GitHub emails".to_string())
        })?;

    emails
        .into_iter()
        .find(|e| e.primary && e.verified)
        .map(|e| e.email)
        .ok_or_else(|| {
            AppError::UpstreamInvalidResponse(
                "No verified primary email on GitHub account".to_string(),
            )
        })
}

pub fn extract_bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|val| val.to_str().ok())
        .and_then(|val| val.strip_prefix("Bearer "))
        .map(|t| t.to_string())
}

pub fn build_list_response(
    base_path: &str,
    page: u32,
    limit: u32,
    total: u64,
    extra_params: &[(String, String)],
    data: Vec<ProfileDto>,
) -> ProfileListResponse {
    let total_pages = (total as f64 / limit as f64).ceil() as u64;

    let extra: String = extra_params
        .iter()
        .map(|(k, v)| format!("&{}={}", k, v))
        .collect();

    let self_ = format!("{}?page={}&limit={}{}", base_path, page, limit, extra);
    let next = if (page as u64) < total_pages {
        Some(format!(
            "{}?page={}&limit={}{}",
            base_path,
            page + 1,
            limit,
            extra
        ))
    } else {
        None
    };
    let prev = if page > 1 {
        Some(format!(
            "{}?page={}&limit={}{}",
            base_path,
            page - 1,
            limit,
            extra
        ))
    } else {
        None
    };

    ProfileListResponse {
        status: "success".into(),
        page,
        limit,
        total,
        total_pages,
        links: PaginationLinks { self_, next, prev },
        data,
    }
}

/// Assigns a role to a newly created user.
///
/// Checks `ADMIN_GITHUB_IDS` (comma-separated GitHub numeric IDs). If the user's
/// `github_id` is present, they receive `Role::Admin`; otherwise `Role::Analyst`.
pub fn resolve_role(github_id: &str) -> Role {
    let admin_ids = std::env::var("ADMIN_GITHUB_IDS").unwrap_or_default();
    let is_admin = admin_ids
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|id| id == github_id);

    if is_admin {
        Role::Admin
    } else {
        Role::default()
    }
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

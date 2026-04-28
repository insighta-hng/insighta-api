use axum::extract::Request;
use rand::Rng;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tower_cookies::Cookie;

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

pub const CSRF_COOKIE: &str = "csrf_token";

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
    url: &str,
    name: &str,
) -> Result<GenderizeResponse> {
    let client = reqwest_client.get();
    let response: GenderizeResponse = client
        .get(url)
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

pub async fn fetch_age_data(
    reqwest_client: &ReqwestClient,
    url: &str,
    name: &str,
) -> Result<AgifyResponse> {
    let client = reqwest_client.get();
    let mut response: AgifyResponse = client
        .get(url)
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
    url: &str,
    name: &str,
) -> Result<NationalizeResponse> {
    let client = reqwest_client.get();
    let response: NationalizeRawResponse = client
        .get(url)
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
        .get(&state.config.github_emails_url)
        .header("Authorization", format!("Bearer {github_token}"))
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
        .map(|(k, v)| format!("&{k}={v}"))
        .collect();

    let self_ = format!("{base_path}?page={page}&limit={limit}{extra}");
    let next = if (page as u64) < total_pages {
        Some(format!(
            "{base_path}?page={}&limit={limit}{extra}",
            page + 1
        ))
    } else {
        None
    };
    let prev = if page > 1 {
        Some(format!(
            "{base_path}?page={}&limit={limit}{extra}",
            page - 1
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
pub fn resolve_role(github_id: &str, admin_ids: &str) -> Role {
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

pub fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn make_http_only_cookie<'a>(
    name: &'a str,
    value: String,
    max_age_secs: i64,
    secure: bool,
    cross_site: bool,
) -> Cookie<'a> {
    let mut cookie = Cookie::new(name, value);
    cookie.set_http_only(true);
    // cross_site (SameSite=None) requires Secure; secure flag is additive.
    cookie.set_secure(secure || cross_site);
    cookie.set_same_site(if cross_site {
        tower_cookies::cookie::SameSite::None
    } else {
        tower_cookies::cookie::SameSite::Lax
    });
    cookie.set_path("/");
    cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(max_age_secs));
    cookie
}

pub fn make_csrf_cookie(value: String, secure: bool, cross_site: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(CSRF_COOKIE, value);
    cookie.set_http_only(false);
    cookie.set_secure(secure || cross_site);
    cookie.set_same_site(if cross_site {
        tower_cookies::cookie::SameSite::None
    } else {
        tower_cookies::cookie::SameSite::Lax
    });
    cookie.set_path("/");
    cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(300));
    cookie
}

pub fn clear_cookie(name: &'static str) -> Cookie<'static> {
    let mut cookie = Cookie::new(name, "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(0));
    cookie
}

pub fn get_user_first_last_name(username: &str) -> (String, String) {
    let names: Vec<&str> = username.split_whitespace().collect();
    let first_name = names.first().cloned().unwrap_or("Test");
    let last_name = if names.len() > 1 {
        names[1..].join(" ")
    } else {
        "User".to_string()
    };
    (first_name.to_string(), last_name)
}

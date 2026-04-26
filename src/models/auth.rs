use serde::{Deserialize, Serialize};

use crate::models::user::Role;

#[derive(Debug, Deserialize)]
pub struct AuthInitQuery {
    pub code_challenge: String,
    /// Random opaque string to prevent CSRF during the OAuth roundtrip.
    pub state: String,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
    pub code_verifier: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub status: String,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubTokenResponse {
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubUser {
    pub id: i64,
    pub login: String,
    pub email: Option<String>,
    pub avatar_url: String,
}

#[derive(Deserialize)]
pub struct EmailEntry {
    pub email: String,
    pub primary: bool,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub id: uuid::Uuid,
    pub role: Role,
}

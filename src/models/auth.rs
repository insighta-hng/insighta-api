use serde::{Deserialize, Serialize};

use crate::{models::user::Role, repo::user::UserRepo};

#[derive(Debug, Default, Deserialize)]
pub struct AuthInitQuery {
    /// PKCE code challenge (base64url SHA-256 of the verifier).
    /// Present for CLI flows; omitted for web flows.
    pub code_challenge: Option<String>,
    /// Random opaque string to prevent CSRF during the OAuth roundtrip.
    pub state: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_challenge_method: Option<String>,
    /// Set to "1" by CLI clients to distinguish CLI flows from web flows.
    pub cli: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
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
    pub access_token: Option<String>,
    /// Set by GitHub when the exchange fails (e.g. bad code, PKCE mismatch).
    pub error: Option<String>,
    pub error_description: Option<String>,
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

#[derive(Clone, Debug)]
pub struct AuthMiddlewareState {
    pub user_repo: UserRepo,
    pub jwt_secret: String,
}

#[derive(Debug, Serialize)]
pub struct UserInfoResponse {
    pub status: String,
    pub data: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub github_id: String,
    pub username: String,
    pub full_name: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub avatar_url: String,
    pub role: String,
}

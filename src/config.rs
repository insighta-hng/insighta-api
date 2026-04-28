use std::env::var;

use crate::errors::{AppError, Result};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub database_name: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_uri: Option<String>,
    pub jwt_secret: String,
    pub admin_github_ids: String,
    pub secure_cookies: bool,
    pub cross_site_cookies: bool,
    pub public_host: Option<String>,
    pub server_port: u16,
    pub github_token_url: String,
    pub github_user_url: String,
    pub github_emails_url: String,
    pub genderize_url: String,
    pub agify_url: String,
    pub nationalize_url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: var("DATABASE_URL")
                .unwrap_or_else(|_| "mongodb://localhost:27017".into()),
            database_name: var("DATABASE_NAME").unwrap_or_else(|_| "stage2".into()),
            github_client_id: var("GITHUB_CLIENT_ID").map_err(|_| {
                AppError::InternalServerError("GITHUB_CLIENT_ID is required".into())
            })?,
            github_client_secret: var("GITHUB_CLIENT_SECRET").map_err(|_| {
                AppError::InternalServerError("GITHUB_CLIENT_SECRET is required".into())
            })?,
            github_redirect_uri: var("GITHUB_REDIRECT_URI").ok(),
            jwt_secret: var("JWT_SECRET")
                .map_err(|_| AppError::InternalServerError("JWT_SECRET is required".into()))?,
            admin_github_ids: var("ADMIN_GITHUB_IDS").unwrap_or_default(),
            secure_cookies: var("SECURE_COOKIES")
                .map(|val| val == "true" || val == "1")
                .unwrap_or(false),
            cross_site_cookies: var("CROSS_SITE_COOKIES")
                .map(|val| val == "true" || val == "1")
                .unwrap_or(false),
            public_host: var("PUBLIC_HOST").ok(),
            server_port: var("PORT")
                .ok()
                .and_then(|val| val.parse::<u16>().ok())
                .unwrap_or(8000),
            github_token_url: var("GITHUB_TOKEN_URL")
                .unwrap_or_else(|_| "https://github.com/login/oauth/access_token".into()),
            github_user_url: var("GITHUB_USER_URL")
                .unwrap_or_else(|_| "https://api.github.com/user".into()),
            github_emails_url: var("GITHUB_EMAILS_URL")
                .unwrap_or_else(|_| "https://api.github.com/user/emails".into()),
            genderize_url: var("GENDERIZE_URL")
                .unwrap_or_else(|_| "https://api.genderize.io".into()),
            agify_url: var("AGIFY_URL").unwrap_or_else(|_| "https://api.agify.io".into()),
            nationalize_url: var("NATIONALIZE_URL")
                .unwrap_or_else(|_| "https://api.nationalize.io".into()),
        })
    }
}

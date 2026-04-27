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
    pub server_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "mongodb://localhost:27017".into()),
            database_name: std::env::var("DATABASE_NAME").unwrap_or_else(|_| "stage2".into()),
            github_client_id: std::env::var("GITHUB_CLIENT_ID").map_err(|_| {
                AppError::InternalServerError("GITHUB_CLIENT_ID is required".into())
            })?,
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").map_err(|_| {
                AppError::InternalServerError("GITHUB_CLIENT_SECRET is required".into())
            })?,
            github_redirect_uri: std::env::var("GITHUB_REDIRECT_URI").ok(),
            jwt_secret: std::env::var("JWT_SECRET")
                .map_err(|_| AppError::InternalServerError("JWT_SECRET is required".into()))?,
            admin_github_ids: std::env::var("ADMIN_GITHUB_IDS").unwrap_or_default(),
            secure_cookies: std::env::var("SECURE_COOKIES")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            cross_site_cookies: std::env::var("CROSS_SITE_COOKIES")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            server_port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(8000),
        })
    }
}

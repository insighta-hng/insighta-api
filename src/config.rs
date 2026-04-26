use crate::errors::{AppError, Result};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub database_name: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_redirect_uri: Option<String>,
    pub jwt_secret: String,
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
        })
    }
}

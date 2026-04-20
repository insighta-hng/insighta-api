use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub struct JsonError {
    pub message: String,
    pub status: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    TryInitError(#[from] tracing_subscriber::util::TryInitError),
    #[error("{0}")]
    ServiceUnavailable(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    UnprocessableEntity(String),
    #[error("{0}")]
    InternalServerError(String),
    #[error("{0}")]
    UpstreamInvalidResponse(String),
    #[error("{0}")]
    NotFound(String),
}

impl AppError {
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::ServiceUnavailable(_) | AppError::UpstreamInvalidResponse(_) => 502,
            AppError::BadRequest(_) => 400,
            AppError::UnprocessableEntity(_) => 422,
            AppError::NotFound(_) => 404,
            AppError::IoError(_) | AppError::TryInitError(_) | AppError::InternalServerError(_) => {
                500
            }
        }
    }

    pub fn to_json_error(&self) -> JsonError {
        JsonError {
            message: match self {
                AppError::ServiceUnavailable(msg) => {
                    tracing::error!("{}", msg);
                    "Server failure".to_string()
                }
                AppError::UpstreamInvalidResponse(api) => {
                    tracing::error!("{} returned an invalid response", api);
                    format!("{} returned an invalid response", api)
                }
                AppError::IoError(msg) => {
                    tracing::error!("{}", msg);
                    "Server failure".to_string()
                }
                AppError::TryInitError(msg) => {
                    tracing::error!("{}", msg);
                    "Server failure".to_string()
                }
                AppError::InternalServerError(msg) => {
                    tracing::error!("{}", msg);
                    "Server failure".to_string()
                }
                AppError::BadRequest(msg)
                | AppError::UnprocessableEntity(msg)
                | AppError::NotFound(msg) => msg.to_string(),
            },
            status: "error".to_string(),
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        Self::ServiceUnavailable(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status_code()).unwrap();
        let body = serde_json::to_string(&self.to_json_error())
            .unwrap_or_else(|_| r#"{"status": "error", "message": "Server failure"}"#.to_string());
        (status, [("content-type", "application/json")], body).into_response()
    }
}

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

/// Represents all possible errors that can occur in the application.
///
/// These errors are automatically converted into JSON responses with the correct
/// HTTP status codes.
#[derive(Debug, Error)]
pub enum AppError {
    /// IO-related failures.
    #[error("{0}")]
    IoError(#[from] std::io::Error),
    /// Errors initialization the tracing subscriber.
    #[error("{0}")]
    TryInitError(#[from] tracing_subscriber::util::TryInitError),
    /// External service (e.g., MongoDB, Agify) is unreachable or failed.
    #[error("{0}")]
    ServiceUnavailable(String),
    /// The request was malformed or could not be interpreted by the parser.
    #[error("{0}")]
    BadRequest(String),
    /// Input validation failed (e.g., invalid query parameter type).
    #[error("{0}")]
    UnprocessableEntity(String),
    /// An unexpected internal failure.
    #[error("{0}")]
    InternalServerError(String),
    /// An external API returned data that doesn't meet our minimum quality thresholds.
    #[error("{0}")]
    UpstreamInvalidResponse(String),
    /// The requested resource (e.g., a specific profile ID) was not found.
    #[error("{0}")]
    NotFound(String),
    /// Authentication failed (missing, invalid, or expired token).
    #[error("{0}")]
    Unauthorized(String),
    /// Authenticated but not permitted to perform this action.
    #[error("{0}")]
    Forbidden(String),
}

impl AppError {
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::ServiceUnavailable(_) | AppError::UpstreamInvalidResponse(_) => 502,
            AppError::BadRequest(_) => 400,
            AppError::Unauthorized(_) => 401,
            AppError::Forbidden(_) => 403,
            AppError::NotFound(_) => 404,
            AppError::UnprocessableEntity(_) => 422,
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
                    "Server failure".to_string()
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
                | AppError::NotFound(msg)
                | AppError::Unauthorized(msg)
                | AppError::Forbidden(msg) => msg.to_string(),
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
        let status =
            StatusCode::from_u16(self.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::to_string(&self.to_json_error())
            .unwrap_or_else(|_| r#"{"status": "error", "message": "Server failure"}"#.to_string());
        (status, [("content-type", "application/json")], body).into_response()
    }
}

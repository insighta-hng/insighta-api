use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::errors::AppError;

const REQUIRED_API_VERSION: &str = "1";

pub async fn require_api_version(req: Request, next: Next) -> Response {
    let version = req
        .headers()
        .get("x-api-version")
        .and_then(|val| val.to_str().ok());

    match version {
        Some(val) if val == REQUIRED_API_VERSION => next.run(req).await,
        _ => AppError::BadRequest("API version header required".to_string()).into_response(),
    }
}

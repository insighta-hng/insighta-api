use axum::{
    extract::Request,
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tower_cookies::Cookies;

use crate::{errors::AppError, utils::CSRF_COOKIE};
const CSRF_HEADER: &str = "x-csrf-token";

/// Validates the CSRF double-submit cookie pattern on mutating requests.
///
/// For POST and DELETE requests, checks that the `X-CSRF-Token` header value
/// matches the `csrf_token` cookie. GET requests pass through unconditionally.
pub async fn csrf_protection(req: Request, next: Next) -> Response {
    let method = req.method().clone();

    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return next.run(req).await;
    }

    // Bearer-authenticated requests are not CSRF-vulnerable — skip the check.
    let has_bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|val| val.to_str().ok())
        .map(|val| val.starts_with("Bearer "))
        .unwrap_or(false);

    if has_bearer {
        return next.run(req).await;
    }

    let cookies = req.extensions().get::<Cookies>().cloned();
    let csrf_cookie = cookies
        .as_ref()
        .and_then(|cookie| cookie.get(CSRF_COOKIE))
        .map(|cookie| cookie.value().to_string());

    let csrf_header = req
        .headers()
        .get(CSRF_HEADER)
        .and_then(|val| val.to_str().ok())
        .map(|s| s.to_string());

    match (csrf_cookie, csrf_header) {
        (Some(cookie_val), Some(header_val)) if cookie_val == header_val => next.run(req).await,
        _ => AppError::Forbidden("CSRF token mismatch".to_string()).into_response(),
    }
}

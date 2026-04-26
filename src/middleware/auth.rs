use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    auth::tokens::validate_access_token, errors::AppError, models::auth::AuthenticatedUser,
    utils::extract_bearer_token,
};

pub async fn require_auth(mut req: Request, next: Next) -> Response {
    let jwt_secret = match std::env::var("JWT_SECRET") {
        Ok(secret) => secret,
        Err(_) => {
            return AppError::InternalServerError("Server misconfiguration".to_string())
                .into_response();
        }
    };

    let token = match extract_bearer_token(&req) {
        Some(val) => val,
        None => {
            return AppError::Unauthorized("Authorization header missing or malformed".to_string())
                .into_response();
        }
    };

    let claims = match validate_access_token(&token, &jwt_secret) {
        Ok(val) => val,
        Err(AppError::Unauthorized(msg)) => {
            return AppError::Unauthorized(msg).into_response();
        }
        Err(_) => {
            return AppError::Unauthorized("Invalid access token".to_string()).into_response();
        }
    };

    let user_id = match uuid::Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(_) => {
            return AppError::Unauthorized("Malformed token subject".to_string()).into_response();
        }
    };

    req.extensions_mut().insert(AuthenticatedUser {
        id: user_id,
        role: claims.role,
    });

    next.run(req).await
}

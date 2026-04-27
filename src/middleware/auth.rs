use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    auth::tokens::validate_access_token,
    errors::AppError,
    models::auth::{AuthMiddlewareState, AuthenticatedUser},
    utils::extract_bearer_token,
};

pub async fn require_auth(
    State(auth_state): State<AuthMiddlewareState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = if let Some(val) = extract_bearer_token(&req) {
        val
    } else {
        // Fallback to cookie for web clients
        let cookies = req
            .extensions()
            .get::<tower_cookies::Cookies>()
            .cloned()
            .ok_or_else(|| AppError::Unauthorized("Authorization missing".to_string()));

        let cookie_token = match cookies {
            Ok(cookie) => cookie
                .get("access_token")
                .map(|cookie| cookie.value().to_string()),
            Err(_) => None,
        };

        match cookie_token {
            Some(t) => t,
            None => {
                return AppError::Unauthorized(
                    "Authorization header missing or malformed".to_string(),
                )
                .into_response();
            }
        }
    };

    let claims = match validate_access_token(&token, &auth_state.jwt_secret) {
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

    let user = match auth_state.user_repo.find_by_id(user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return AppError::Unauthorized("User no longer exists".to_string()).into_response();
        }
        Err(_) => {
            return AppError::InternalServerError("Authentication check failed".to_string())
                .into_response();
        }
    };

    if !user.is_active {
        return AppError::Forbidden("Your account has been deactivated".to_string())
            .into_response();
    }

    req.extensions_mut().insert(AuthenticatedUser {
        id: user_id,
        role: user.role,
    });

    next.run(req).await
}

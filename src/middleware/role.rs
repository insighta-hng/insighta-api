use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{
    errors::AppError,
    models::{auth::AuthenticatedUser, user::Role},
};

pub struct RequireAny(pub AuthenticatedUser);
pub struct RequireAdmin(pub AuthenticatedUser);

impl<S> FromRequestParts<S> for RequireAny
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .map(RequireAny)
            .ok_or_else(|| AppError::Forbidden("Authentication required".to_string()))
    }
}

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or_else(|| AppError::Forbidden("Authentication required".to_string()))?;

        if user.role != Role::Admin {
            return Err(AppError::Forbidden("Admin access required".to_string()));
        }

        Ok(RequireAdmin(user))
    }
}

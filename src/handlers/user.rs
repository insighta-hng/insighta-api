use axum::{Json, extract::State, response::IntoResponse};

use crate::{
    AppState,
    errors::{AppError, Result},
    middleware::role::RequireAny,
    models::auth::{UserInfo, UserInfoResponse},
    utils::get_user_first_last_name,
};

pub async fn get_current_user(
    State(state): State<AppState>,
    auth: RequireAny,
) -> Result<impl IntoResponse> {
    let user = state
        .user_repo
        .find_by_id(auth.0.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    let (first_name, last_name) = get_user_first_last_name(&user.username);

    Ok(Json(UserInfoResponse {
        status: "success".to_string(),
        data: UserInfo {
            id: user.id.to_string(),
            github_id: user.github_id.clone(),
            username: user.username.clone(),
            full_name: user.username,
<<<<<<< HEAD
            first_name,
=======
            first_name: first_name,
>>>>>>> 6729c55a91e9f87b6b6a3fdbdf783521c2393d2c
            last_name,
            email: user.email,
            avatar_url: user.avatar_url,
            role: user.role.to_string(),
        },
    }))
}

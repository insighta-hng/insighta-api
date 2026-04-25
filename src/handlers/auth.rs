use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    AppState,
    auth::{
        pkce::verify_code_challenge,
        tokens::{issue_access_token, issue_refresh_token},
    },
    errors::{AppError, Result},
    models::{
        auth::{
            AuthInitQuery, CallbackQuery, GithubTokenResponse, GithubUser, RefreshRequest,
            TokenResponse,
        },
        user::GithubUserInfo,
    },
    utils::fetch_github_primary_email,
};

pub async fn github_init(
    State(state): State<AppState>,
    Query(query): Query<AuthInitQuery>,
) -> Result<Response> {
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| AppError::InternalServerError("GITHUB_CLIENT_ID not set".to_string()))?;

    // Store state → code_challenge mapping for callback validation.
    state
        .oauth_states
        .insert(query.state.clone(), query.code_challenge);

    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&state={}&scope=user:email",
        client_id, query.state
    );

    Ok(Redirect::to(&url).into_response())
}

pub async fn github_callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Result<impl IntoResponse> {
    let code_challenge = state
        .oauth_states
        .remove(&query.state)
        .map(|(_, v)| v)
        .ok_or_else(|| AppError::BadRequest("Invalid or expired OAuth state".to_string()))?;

    if let Some(ref verifier) = query.code_verifier {
        if !verify_code_challenge(verifier, &code_challenge) {
            return Err(AppError::BadRequest("PKCE verification failed".to_string()));
        }
    }

    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| AppError::InternalServerError("GITHUB_CLIENT_ID not set".to_string()))?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| AppError::InternalServerError("GITHUB_CLIENT_SECRET not set".to_string()))?;
    let jwt_secret = std::env::var("JWT_SECRET")
        .map_err(|_| AppError::InternalServerError("JWT_SECRET not set".to_string()))?;

    let token_res: GithubTokenResponse = state
        .client
        .get()
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", query.code.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(e.to_string()))?
        .json::<GithubTokenResponse>()
        .await
        .map_err(|_| {
            AppError::UpstreamInvalidResponse("GitHub token exchange failed".to_string())
        })?;

    let github_user: GithubUser = state
        .client
        .get()
        .get("https://api.github.com/user")
        .header(
            "Authorization",
            format!("Bearer {}", token_res.access_token),
        )
        .header("User-Agent", "insighta-api")
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(e.to_string()))?
        .json()
        .await
        .map_err(|_| {
            AppError::UpstreamInvalidResponse("Failed to fetch GitHub user profile".to_string())
        })?;

    let email = match github_user.email {
        Some(e) => e,
        None => fetch_github_primary_email(&state, &token_res.access_token).await?,
    };

    let info = GithubUserInfo {
        github_id: github_user.id.to_string(),
        username: github_user.login,
        email,
        avatar_url: github_user.avatar_url,
    };

    let user = state.user_repo.upsert(&info).await?;

    if !user.is_active {
        return Err(AppError::Forbidden(
            "Your account has been deactivated".to_string(),
        ));
    }

    let access_token = issue_access_token(user.id, &user.role, &jwt_secret)?;
    let refresh_token = issue_refresh_token(user.id, &state.refresh_token_repo).await?;

    Ok((
        StatusCode::OK,
        Json(TokenResponse {
            status: "success".to_string(),
            access_token,
            refresh_token,
        }),
    ))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<impl IntoResponse> {
    let jwt_secret = std::env::var("JWT_SECRET")
        .map_err(|_| AppError::InternalServerError("JWT_SECRET not set".to_string()))?;

    let record = state
        .refresh_token_repo
        .consume(&body.refresh_token)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Refresh token not found".to_string()))?;

    let user = state
        .user_repo
        .find_by_id(record.user_id)
        .await?
        .ok_or_else(|| AppError::InternalServerError("User not found for token".to_string()))?;

    if !user.is_active {
        return Err(AppError::Forbidden(
            "Your account has been deactivated".to_string(),
        ));
    }

    let access_token = issue_access_token(user.id, &user.role, &jwt_secret)?;
    let refresh_token = issue_refresh_token(user.id, &state.refresh_token_repo).await?;

    Ok(Json(TokenResponse {
        status: "success".to_string(),
        access_token,
        refresh_token,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<impl IntoResponse> {
    let record = state
        .refresh_token_repo
        .consume(&body.refresh_token)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Refresh token not found".to_string()))?;

    state
        .refresh_token_repo
        .delete_for_user(record.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

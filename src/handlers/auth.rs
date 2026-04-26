use axum::{
    Json,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
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

/// Initiates the GitHub OAuth 2.0 authorization flow.
///
/// Stores the `state` → `code_challenge` mapping in memory for PKCE validation
/// at callback time, then redirects the user to GitHub's authorization page.
///
/// # Arguments
///
/// * `state` - The application state containing the in-memory OAuth state store.
/// * `query` - Query parameters containing `state` (CSRF token) and `code_challenge` (PKCE).
///
/// # Returns
///
/// Returns a `302 Found` redirect to the GitHub authorization URL.
///
/// # Errors
///
/// Returns `AppError::UnprocessableEntity` if query parameters are structurally invalid.
/// Returns `AppError::InternalServerError` if `GITHUB_CLIENT_ID` is not set.
pub async fn github_init(
    State(state): State<AppState>,
    query: std::result::Result<Query<AuthInitQuery>, QueryRejection>,
) -> Result<Response> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".to_string()))?;

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

/// Handles the GitHub OAuth callback and issues application tokens.
///
/// Validates the OAuth `state` parameter against the stored PKCE code challenge,
/// exchanges the GitHub authorization code for an access token, fetches the
/// authenticated user's profile and primary email, upserts the user record,
/// and issues a new access/refresh token pair.
///
/// # Arguments
///
/// * `state` - The application state containing the HTTP client, OAuth state store, and repositories.
/// * `query` - Query parameters containing `code`, `state`, and an optional `code_verifier`.
///
/// # Returns
///
/// Returns `200 OK` with a JSON body containing `access_token` and `refresh_token`.
///
/// # Errors
///
/// Returns `AppError::UnprocessableEntity` if query parameters are structurally invalid.
/// Returns `AppError::BadRequest` if the OAuth state is invalid/expired or PKCE verification fails.
/// Returns `AppError::InternalServerError` if required environment variables are not set.
/// Returns `AppError::ServiceUnavailable` if GitHub's API is unreachable.
/// Returns `AppError::UpstreamInvalidResponse` if GitHub returns unexpected data.
/// Returns `AppError::Forbidden` if the user's account has been deactivated.
pub async fn github_callback(
    State(state): State<AppState>,
    query: std::result::Result<Query<CallbackQuery>, QueryRejection>,
) -> Result<impl IntoResponse> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".to_string()))?;

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

/// Rotates a refresh token and issues a fresh access/refresh token pair.
///
/// Consumes the provided refresh token (one-time use), validates the associated
/// user, and issues new tokens. The old refresh token is invalidated regardless
/// of whether the user account check passes.
///
/// # Arguments
///
/// * `state` - The application state containing the user and refresh token repositories.
/// * `payload` - JSON body containing the `refresh_token` to consume.
///
/// # Returns
///
/// Returns `200 OK` with a JSON body containing a new `access_token` and `refresh_token`.
///
/// # Errors
///
/// Returns `AppError::BadRequest` if the request body is malformed or missing.
/// Returns `AppError::InternalServerError` if `JWT_SECRET` is not set, or the user record is missing.
/// Returns `AppError::Unauthorized` if the refresh token is not found or already consumed.
/// Returns `AppError::Forbidden` if the user's account has been deactivated.
pub async fn refresh(
    State(state): State<AppState>,
    payload: std::result::Result<Json<RefreshRequest>, JsonRejection>,
) -> Result<impl IntoResponse> {
    let Json(body) = payload.map_err(|e| AppError::BadRequest(e.body_text()))?;

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

/// Logs out the authenticated user by invalidating all their refresh tokens.
///
/// Consumes the provided refresh token to verify ownership, then deletes all
/// remaining refresh tokens associated with that user, effectively terminating
/// all active sessions.
///
/// # Arguments
///
/// * `state` - The application state containing the refresh token repository.
/// * `payload` - JSON body containing the `refresh_token` to identify the session.
///
/// # Returns
///
/// Returns `204 No Content` on successful logout.
///
/// # Errors
///
/// Returns `AppError::BadRequest` if the request body is malformed or missing.
/// Returns `AppError::Unauthorized` if the refresh token is not found or already consumed.
pub async fn logout(
    State(state): State<AppState>,
    payload: std::result::Result<Json<RefreshRequest>, JsonRejection>,
) -> Result<impl IntoResponse> {
    let Json(body) = payload.map_err(|e| AppError::BadRequest(e.body_text()))?;

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

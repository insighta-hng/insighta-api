use std::time::Instant;

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
        github::process_github_callback,
        pkce::verify_code_challenge,
        tokens::{issue_access_token, issue_refresh_token},
    },
    errors::{AppError, Result},
    models::auth::{AuthInitQuery, CallbackQuery, RefreshRequest, TokenResponse},
    utils::generate_csrf_token,
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
    let query = match query {
        Ok(Query(query_params)) => query_params,
        Err(_) => AuthInitQuery::default(),
    };

    let oauth_state = match query.state {
        Some(q_state) if !q_state.is_empty() => q_state,
        _ => generate_csrf_token(),
    };

    let redirect_uri = match query.redirect_uri {
        Some(ref uri) if !uri.is_empty() => uri.clone(),
        _ => state.config.github_redirect_uri.clone().unwrap_or_else(|| {
            let host = state
                .config
                .public_host
                .as_deref()
                .unwrap_or("localhost:8000");
            format!("https://{host}/auth/github/callback")
        }),
    };

    state.oauth_states.insert(
        oauth_state.clone(),
        (
            query.code_challenge.clone(),
            redirect_uri.clone(),
            Instant::now(),
        ),
    );

    let mut url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&state={}&scope=user:email&redirect_uri={}",
        state.config.github_client_id, oauth_state, redirect_uri
    );

    // RFC 7636: Proof Key for Code Exchange (PKCE) - https://datatracker.ietf.org/doc/html/rfc7636
    // Tell GitHub which challenge to expect so it enforces the verifier at exchange time.
    if let Some(ref challenge) = query.code_challenge {
        url.push_str(&format!(
            "&code_challenge={challenge}&code_challenge_method=S256"
        ));
    }

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
    let Query(query) = query.unwrap_or_else(|_| Query(CallbackQuery::default()));

    let state_param = query
        .state
        .as_ref()
        .filter(|state_param| !state_param.is_empty())
        .ok_or_else(|| AppError::BadRequest("Missing query parameter: state".into()))?
        .clone();

    let code_param = query
        .code
        .as_ref()
        .filter(|code_param| !code_param.is_empty())
        .ok_or_else(|| AppError::BadRequest("Missing query parameter: code".into()))?
        .clone();

    let (code_challenge, redirect_uri) = state
        .oauth_states
        .remove(&state_param)
        .map(|(_, (challenge, uri, _created))| (challenge, uri))
        .ok_or_else(|| AppError::BadRequest("Invalid or expired OAuth state".into()))?;

    // For PKCE flows: verify application-side and capture the verifier to forward to GitHub.
    let code_verifier: Option<String> = match &code_challenge {
        Some(challenge) => {
            let verifier = query.code_verifier.as_ref().ok_or_else(|| {
                AppError::BadRequest("code_verifier required for PKCE flow".into())
            })?;
            if !verify_code_challenge(verifier, challenge) {
                return Err(AppError::BadRequest("PKCE verification failed".into()));
            }
            Some(verifier.clone())
        }
        None => None,
    };

    let user = process_github_callback(&state, code_param, code_verifier, redirect_uri).await?;

    let access_token = issue_access_token(
        user.id,
        &user.role,
        &user.username,
        &state.config.jwt_secret,
    )?;
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
    let Json(body) = payload.map_err(|err| AppError::BadRequest(err.body_text()))?;

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

    let access_token = issue_access_token(
        user.id,
        &user.role,
        &user.username,
        &state.config.jwt_secret,
    )?;
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
    let Json(body) = payload.map_err(|err| AppError::BadRequest(err.body_text()))?;

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

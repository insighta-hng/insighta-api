use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use tower_cookies::Cookies;

use crate::{
    AppState,
    auth::{
        pkce::verify_code_challenge,
        tokens::{issue_access_token, issue_refresh_token, validate_access_token},
    },
    errors::{AppError, Result},
    models::auth::{CallbackQuery, UserInfo, UserInfoResponse},
    utils::{
        CSRF_COOKIE, clear_cookie, generate_csrf_token, get_user_first_last_name, make_csrf_cookie,
        make_http_only_cookie,
    },
};

const ACCESS_COOKIE: &str = "access_token";
const REFRESH_COOKIE: &str = "refresh_token";

/// POST /auth/web/exchange
///
/// Completes the web OAuth flow. Validates state, verifies PKCE if present,
/// exchanges the code with GitHub, upserts the user, and sets HTTP-only cookies.
pub async fn web_exchange(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(query): Query<CallbackQuery>,
) -> Result<impl IntoResponse> {
    let state_param = query
        .state
        .ok_or_else(|| AppError::BadRequest("Missing query parameter: state".into()))?;

    let code_param = query
        .code
        .ok_or_else(|| AppError::BadRequest("Missing query parameter: code".into()))?;

    let (code_challenge, redirect_uri) = state
        .oauth_states
        .remove(&state_param)
        .map(|(_, (challenge, uri, _created))| (challenge, uri))
        .ok_or_else(|| AppError::BadRequest("Invalid or expired OAuth state".into()))?;

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

    let user = crate::auth::github::process_github_callback(
        &state,
        code_param,
        code_verifier,
        redirect_uri,
    )
    .await?;

    let access_token = issue_access_token(
        user.id,
        &user.role,
        &user.username,
        &state.config.jwt_secret,
    )?;
    let refresh_token = issue_refresh_token(user.id, &state.refresh_token_repo).await?;
    let csrf_token = generate_csrf_token();

    cookies.add(make_http_only_cookie(
        ACCESS_COOKIE,
        access_token,
        180,
        state.config.secure_cookies,
        state.config.cross_site_cookies,
    ));
    cookies.add(make_http_only_cookie(
        REFRESH_COOKIE,
        refresh_token,
        300,
        state.config.secure_cookies,
        state.config.cross_site_cookies,
    ));
    cookies.add(make_csrf_cookie(
        csrf_token,
        state.config.secure_cookies,
        state.config.cross_site_cookies,
    ));

    let (first_name, last_name) = get_user_first_last_name(&user.username);

    Ok((
        StatusCode::OK,
        Json(UserInfoResponse {
            status: "success".to_string(),
            data: UserInfo {
                id: user.id.to_string(),
                github_id: user.github_id,
                username: user.username.clone(),
                full_name: user.username,
                first_name,
                last_name,
                email: user.email,
                avatar_url: user.avatar_url,
                role: user.role.to_string(),
            },
        }),
    ))
}

/// GET /auth/me
///
/// Returns the current user's info from the access token cookie or Bearer token.
pub async fn me(
    State(state): State<AppState>,
    cookies: Cookies,
    headers: HeaderMap,
) -> Result<impl IntoResponse> {
    let token = {
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|val| val.to_str().ok())
            .and_then(|val| val.strip_prefix("Bearer "))
            .map(|token| token.to_string());

        if let Some(token) = bearer {
            token
        } else {
            cookies
                .get(ACCESS_COOKIE)
                .map(|cookie_token| cookie_token.value().to_string())
                .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?
        }
    };

    let claims = validate_access_token(&token, &state.config.jwt_secret)?;

    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized("Malformed token".into()))?;

    let user = state
        .user_repo
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    if !user.is_active {
        return Err(AppError::Forbidden(
            "Your account has been deactivated".into(),
        ));
    }

    let (first_name, last_name) = get_user_first_last_name(&user.username);

    Ok((Json(UserInfoResponse {
        status: "success".to_string(),
        data: UserInfo {
            id: user.id.to_string(),
            github_id: user.github_id,
            username: user.username.clone(),
            full_name: user.username,
            first_name,
            last_name,
            email: user.email,
            avatar_url: user.avatar_url,
            role: user.role.to_string(),
        },
    }),))
}

/// POST /auth/web/refresh
///
/// Reads the refresh token cookie, rotates both cookies.
pub async fn web_refresh(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<impl IntoResponse> {
    let refresh_token = cookies
        .get(REFRESH_COOKIE)
        .map(|cookie_token| cookie_token.value().to_string())
        .ok_or_else(|| AppError::Unauthorized("No refresh token".into()))?;

    let record = state
        .refresh_token_repo
        .consume(&refresh_token)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Refresh token not found".into()))?;

    let user = state
        .user_repo
        .find_by_id(record.user_id)
        .await?
        .ok_or_else(|| AppError::InternalServerError("User not found for token".into()))?;

    if !user.is_active {
        return Err(AppError::Forbidden(
            "Your account has been deactivated".into(),
        ));
    }

    let new_access = issue_access_token(
        user.id,
        &user.role,
        &user.username,
        &state.config.jwt_secret,
    )?;
    let new_refresh = issue_refresh_token(user.id, &state.refresh_token_repo).await?;
    let csrf_token = generate_csrf_token();

    cookies.add(make_http_only_cookie(
        ACCESS_COOKIE,
        new_access,
        180,
        state.config.secure_cookies,
        state.config.cross_site_cookies,
    ));
    cookies.add(make_http_only_cookie(
        REFRESH_COOKIE,
        new_refresh,
        300,
        state.config.secure_cookies,
        state.config.cross_site_cookies,
    ));
    cookies.add(make_csrf_cookie(
        csrf_token,
        state.config.secure_cookies,
        state.config.cross_site_cookies,
    ));

    Ok(StatusCode::NO_CONTENT)
}

/// POST /auth/web/logout
///
/// Clears all auth cookies and invalidates the refresh token.
pub async fn web_logout(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<impl IntoResponse> {
    if let Some(cookie_token) = cookies.get(REFRESH_COOKIE) {
        let _ = state.refresh_token_repo.consume(cookie_token.value()).await;
    }

    cookies.add(clear_cookie(ACCESS_COOKIE));
    cookies.add(clear_cookie(REFRESH_COOKIE));
    cookies.add(clear_cookie(CSRF_COOKIE));

    Ok(StatusCode::NO_CONTENT)
}

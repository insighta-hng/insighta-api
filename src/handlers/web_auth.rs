use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use tower_cookies::Cookies;

use crate::{
    AppState,
    auth::{
        pkce::verify_code_challenge,
        tokens::{issue_access_token, issue_refresh_token},
    },
    errors::{AppError, Result},
    models::{
        auth::{CallbackQuery, GithubTokenResponse, GithubUser, UserInfo, UserInfoResponse},
        user::GithubUserInfo,
    },
    utils::{
        CSRF_COOKIE, clear_cookie, fetch_github_primary_email, generate_csrf_token,
        make_csrf_cookie, make_http_only_cookie,
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
    let (code_challenge, redirect_uri) = state
        .oauth_states
        .remove(&query.state)
        .map(|(_, v)| v)
        .ok_or_else(|| AppError::BadRequest("Invalid or expired OAuth state".to_string()))?;

    let code_verifier: Option<String> = match &code_challenge {
        Some(challenge) => {
            let verifier = query.code_verifier.as_ref().ok_or_else(|| {
                AppError::BadRequest("code_verifier required for PKCE flow".to_string())
            })?;
            if !verify_code_challenge(verifier, challenge) {
                return Err(AppError::BadRequest("PKCE verification failed".to_string()));
            }
            Some(verifier.clone())
        }
        None => None,
    };

    let mut form_params: Vec<(&str, &str)> = vec![
        ("client_id", state.config.github_client_id.as_str()),
        ("client_secret", state.config.github_client_secret.as_str()),
        ("code", query.code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    if let Some(ref val) = code_verifier {
        form_params.push(("code_verifier", val.as_str()));
    }

    let token_res: GithubTokenResponse = state
        .client
        .get()
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&form_params)
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(e.to_string()))?
        .json()
        .await
        .map_err(|_| {
            AppError::UpstreamInvalidResponse("GitHub token exchange failed".to_string())
        })?;

    let github_token = token_res.access_token.ok_or_else(|| {
        let msg = token_res
            .error_description
            .or(token_res.error)
            .unwrap_or_else(|| "GitHub token exchange failed".to_string());
        AppError::BadRequest(msg)
    })?;

    let github_user: GithubUser = state
        .client
        .get()
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", github_token))
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
        None => fetch_github_primary_email(&state, &github_token).await?,
    };

    let info = GithubUserInfo {
        github_id: github_user.id.to_string(),
        username: github_user.login,
        email,
        avatar_url: github_user.avatar_url,
    };

    let user = state
        .user_repo
        .upsert(&info, &state.config.admin_github_ids)
        .await?;

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

    Ok((
        StatusCode::OK,
        Json(UserInfoResponse {
            status: "success".to_string(),
            data: UserInfo {
                id: user.id.to_string(),
                username: user.username,
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
    headers: axum::http::HeaderMap,
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
                .map(|c| c.value().to_string())
                .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()))?
        }
    };

    let claims = crate::auth::tokens::validate_access_token(&token, &state.config.jwt_secret)?;

    let user_id = uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized("Malformed token".to_string()))?;

    let user = state
        .user_repo
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

    if !user.is_active {
        return Err(AppError::Forbidden(
            "Your account has been deactivated".to_string(),
        ));
    }

    Ok(Json(UserInfoResponse {
        status: "success".to_string(),
        data: UserInfo {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
            avatar_url: user.avatar_url,
            role: user.role.to_string(),
        },
    }))
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
        .map(|c| c.value().to_string())
        .ok_or_else(|| AppError::Unauthorized("No refresh token".to_string()))?;

    let record = state
        .refresh_token_repo
        .consume(&refresh_token)
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
    if let Some(c) = cookies.get(REFRESH_COOKIE) {
        let _ = state.refresh_token_repo.consume(c.value()).await;
    }

    cookies.add(clear_cookie(ACCESS_COOKIE));
    cookies.add(clear_cookie(REFRESH_COOKIE));
    cookies.add(clear_cookie(CSRF_COOKIE));

    Ok(StatusCode::NO_CONTENT)
}

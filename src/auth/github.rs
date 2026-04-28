use crate::{
    AppState,
    errors::{AppError, Result},
    models::{
        auth::{GithubTokenResponse, GithubUser},
        user::{GithubUserInfo, User},
    },
    utils::fetch_github_primary_email,
};

/// Processes the GitHub OAuth callback: exchanges code for token, fetches user info, and upserts user.
pub async fn process_github_callback(
    state: &AppState,
    code: String,
    code_verifier: Option<String>,
    redirect_uri: String,
) -> Result<User> {
    let mut form_params: Vec<(&str, &str)> = vec![
        ("client_id", state.config.github_client_id.as_str()),
        ("client_secret", state.config.github_client_secret.as_str()),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    if let Some(ref val) = code_verifier {
        form_params.push(("code_verifier", val.as_str()));
    }

    let token_res: GithubTokenResponse = state
        .client
        .get()
        .post(&state.config.github_token_url)
        .header("Accept", "application/json")
        .form(&form_params)
        .send()
        .await
        .map_err(|err| AppError::ServiceUnavailable(err.to_string()))?
        .json::<GithubTokenResponse>()
        .await
        .map_err(|_| AppError::UpstreamInvalidResponse("GitHub token exchange failed".into()))?;

    let github_token = token_res.access_token.ok_or_else(|| {
        let msg = token_res
            .error_description
            .or(token_res.error)
            .unwrap_or_else(|| "GitHub token exchange failed".into());
        AppError::BadRequest(msg)
    })?;

    let github_user: GithubUser = state
        .client
        .get()
        .get(&state.config.github_user_url)
        .header("Authorization", format!("Bearer {github_token}"))
        .header("User-Agent", "insighta-api")
        .send()
        .await
        .map_err(|err| AppError::ServiceUnavailable(err.to_string()))?
        .json()
        .await
        .map_err(|_| {
            AppError::UpstreamInvalidResponse("Failed to fetch GitHub user profile".into())
        })?;

    let email = match github_user.email {
        Some(email) => email,
        None => fetch_github_primary_email(state, &github_token).await?,
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
            "Your account has been deactivated".into(),
        ));
    }

    Ok(user)
}

use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};

use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::{AppError, Result},
    models::user::Role,
    repo::refresh_token::{RefreshToken, RefreshTokenRepo},
};

const ACCESS_TOKEN_EXPIRY_SECS: i64 = 180; // 3-minutes
const REFRESH_TOKEN_EXPIRY_SECS: i64 = 300; // 5-minutes

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: Role,
    pub username: String,
    pub exp: i64,
    pub iat: i64,
}

pub fn issue_access_token(
    user_id: Uuid,
    role: &Role,
    username: &str,
    secret: &str,
) -> Result<String> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.clone(),
        username: username.to_string(),
        exp: now + ACCESS_TOKEN_EXPIRY_SECS,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::InternalServerError(format!("Failed to sign token: {}", e)))
}

pub fn validate_access_token(token: &str, secret: &str) -> Result<Claims> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
            AppError::Unauthorized("Access token has expired".to_string())
        }
        _ => AppError::Unauthorized("Invalid access token".to_string()),
    })
}

pub async fn issue_refresh_token(user_id: Uuid, repo: &RefreshTokenRepo) -> Result<String> {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);

    let expires_at = Utc::now() + chrono::Duration::seconds(REFRESH_TOKEN_EXPIRY_SECS);

    repo.insert(RefreshToken {
        token: token.clone(),
        user_id,
        expires_at,
    })
    .await?;

    Ok(token)
}

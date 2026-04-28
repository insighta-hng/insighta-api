use chrono::Utc;
use mongodb::{Collection, Database, IndexModel, bson, options::IndexOptions};
use uuid::Uuid;

use crate::{
    errors::{AppError, Result},
    models::auth::RefreshToken,
    utils::hash_token,
};

#[derive(Clone, Debug)]
pub struct RefreshTokenRepo {
    collection: Collection<RefreshToken>,
}

impl RefreshTokenRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("refresh_tokens"),
        }
    }

    pub async fn create_indexes(&self) -> Result<()> {
        // TTL index: MongoDB automatically removes expired documents.
        let ttl_index = IndexModel::builder()
            .keys(bson::doc! { "expires_at": 1 })
            .options(
                IndexOptions::builder()
                    .expire_after(std::time::Duration::from_secs(0))
                    .name("idx_expires_at_ttl".to_string())
                    .build(),
            )
            .build();

        let token_index = IndexModel::builder()
            .keys(bson::doc! { "token": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("idx_token_unique".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![ttl_index, token_index])
            .await
            .map_err(|err| {
                AppError::ServiceUnavailable(format!(
                    "Failed to create refresh token indexes: {err}"
                ))
            })?;

        tracing::info!("Refresh token indexes verified");
        Ok(())
    }

    pub async fn insert(&self, token: RefreshToken) -> Result<()> {
        self.collection
            .insert_one(token)
            .await
            .map_err(|err| AppError::ServiceUnavailable(format!("DB Insert Error: {err}")))?;
        Ok(())
    }

    pub async fn consume(&self, token: &str) -> Result<Option<RefreshToken>> {
        let token_hash = hash_token(token);

        let doc = self
            .collection
            .find_one_and_delete(bson::doc! { "token": &token_hash })
            .await
            .map_err(|err| AppError::ServiceUnavailable(format!("DB Delete Error: {err}")))?;

        match doc {
            None => Ok(None),
            Some(record) => {
                if record.expires_at < Utc::now() {
                    // TTL index should have cleaned this up, but just to be safe..
                    Err(AppError::Unauthorized(
                        "Refresh token has expired".to_string(),
                    ))
                } else {
                    Ok(Some(record))
                }
            }
        }
    }

    pub async fn delete_for_user(&self, user_id: Uuid) -> Result<()> {
        self.collection
            .delete_many(bson::doc! { "user_id": bson::Uuid::from(user_id) })
            .await
            .map_err(|err| AppError::ServiceUnavailable(format!("DB Delete Error: {err}")))?;
        Ok(())
    }
}

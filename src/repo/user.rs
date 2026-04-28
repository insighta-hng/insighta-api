use chrono::Utc;
use mongodb::{Collection, Database, IndexModel, options::IndexOptions};
use uuid::Uuid;

use crate::{
    errors::{AppError, Result},
    models::user::{GithubUserInfo, User},
    utils::resolve_role,
};

#[derive(Clone, Debug)]
pub struct UserRepo {
    collection: Collection<User>,
}

impl UserRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("users"),
        }
    }

    pub async fn create_indexes(&self) -> Result<()> {
        let github_id_index = IndexModel::builder()
            .keys(bson::doc! { "github_id": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("idx_github_id_unique".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![github_id_index])
            .await
            .map_err(|err| {
                AppError::ServiceUnavailable(format!("Failed to create user indexes: {err}"))
            })?;

        tracing::info!("User indexes verified");
        Ok(())
    }

    pub async fn find_by_github_id(&self, github_id: &str) -> Result<Option<User>> {
        self.collection
            .find_one(bson::doc! { "github_id": github_id })
            .await
            .map_err(|err| AppError::ServiceUnavailable(format!("DB Search Error: {err}")))
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>> {
        self.collection
            .find_one(bson::doc! { "_id": bson::Uuid::from(id) })
            .await
            .map_err(|err| AppError::ServiceUnavailable(format!("DB Search Error: {err}")))
    }

    pub async fn upsert(&self, info: &GithubUserInfo, admin_ids: &str) -> Result<User> {
        let now = Utc::now();

        match self.find_by_github_id(&info.github_id).await? {
            Some(_existing) => {
                let role = resolve_role(&info.github_id, admin_ids);
                let update = bson::doc! {
                    "$set": {
                        "username": &info.username,
                        "email": &info.email,
                        "avatar_url": &info.avatar_url,
                        "last_login_at": bson::DateTime::from_millis(now.timestamp_millis()),
                        "role": role.to_string(),
                    }
                };

                self.collection
                    .update_one(bson::doc! { "github_id": &info.github_id }, update)
                    .await
                    .map_err(|err| {
                        AppError::ServiceUnavailable(format!("DB Update Error: {err}"))
                    })?;

                // Re-fetch to return the current persisted state.
                self.find_by_github_id(&info.github_id)
                    .await?
                    .ok_or_else(|| {
                        AppError::InternalServerError("User disappeared after update".to_string())
                    })
            }
            None => {
                let user = User {
                    id: Uuid::now_v7(),
                    github_id: info.github_id.clone(),
                    username: info.username.clone(),
                    email: info.email.clone(),
                    avatar_url: info.avatar_url.clone(),
                    role: resolve_role(&info.github_id, admin_ids),
                    is_active: true,
                    last_login_at: now,
                    created_at: now,
                };

                self.collection.insert_one(&user).await.map_err(|err| {
                    AppError::ServiceUnavailable(format!("DB Insert Error: {err}"))
                })?;

                Ok(user)
            }
        }
    }
}

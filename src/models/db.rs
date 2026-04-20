use futures::stream::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel, bson,
    error::{ErrorKind, WriteFailure},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub gender: String,
    pub gender_probability: f64,
    pub age: u8,
    pub age_group: String,
    pub country_id: String,
    pub country_name: String,
    pub country_probability: f64,
    pub created_at: String,
}

#[derive(Debug, Default)]
pub struct ProfileFilters {
    pub gender: Option<String>,
    pub country_id: Option<String>,
    pub age_group: Option<String>,
}

#[derive(Clone)]
pub struct ProfileRepo {
    collection: Collection<Profile>,
}

impl std::fmt::Debug for ProfileRepo {
    fn fmt(&self, func: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        func.debug_struct("ProfileRepo").finish()
    }
}

impl ProfileRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("profiles"),
        }
    }

    pub async fn create_indexes(&self) -> Result<()> {
        let name_index = IndexModel::builder()
            .keys(bson::doc! { "name": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("idx_name_unique".to_string())
                    .build(),
            )
            .build();

        let id_index = IndexModel::builder()
            .keys(bson::doc! { "id": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("idx_id_unique".to_string())
                    .build(),
            )
            .build();

        let filter_index = IndexModel::builder()
            .keys(bson::doc! { "country_id": 1, "gender": 1, "age_group": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_filters".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![name_index, id_index, filter_index])
            .await
            .map_err(|e| {
                AppError::ServiceUnavailable(format!("Failed to create indexes: {}", e))
            })?;

        tracing::info!("Database indexes verified");
        Ok(())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Profile>> {
        self.collection
            .find_one(bson::doc! { "name": name })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Search Error: {}", e)))
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Profile>> {
        self.collection
            .find_one(bson::doc! { "id": id })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Search Error: {}", e)))
    }

    pub async fn delete_by_id(&self, id: &str) -> Result<bool> {
        let result = self
            .collection
            .delete_one(bson::doc! { "id": id })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Delete Error: {}", e)))?;
        Ok(result.deleted_count > 0)
    }

    pub async fn find_all(&self, filters: ProfileFilters) -> Result<Vec<Profile>> {
        let mut filter_doc = bson::doc! {};

        if let Some(gender) = filters.gender {
            filter_doc.insert(
                "gender",
                bson::doc! { "$regex": format!("^{}$", gender), "$options": "i" },
            );
        }

        if let Some(country) = filters.country_id {
            filter_doc.insert(
                "country_id",
                bson::doc! { "$regex": format!("^{}$", country), "$options": "i" },
            );
        }

        if let Some(age) = filters.age_group {
            filter_doc.insert(
                "age_group",
                bson::doc! { "$regex": format!("^{}$", age), "$options": "i" },
            );
        }

        let cursor = self
            .collection
            .find(filter_doc)
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB List Error: {}", e)))?;

        cursor
            .try_collect()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Cursor Error: {}", e)))
    }

    pub async fn insert_profile(&self, profile: Profile) -> Result<()> {
        match self.collection.insert_one(profile).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if let ErrorKind::Write(WriteFailure::WriteError(ref write_error)) = *e.kind {
                    if write_error.code == 11000 {
                        return Err(AppError::BadRequest(
                            "A profile with this name already exists".to_string(),
                        ));
                    }
                }
                Err(AppError::ServiceUnavailable(format!(
                    "DB Insert Error: {}",
                    e
                )))
            }
        }
    }
}

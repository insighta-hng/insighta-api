use chrono::{DateTime, Utc};
use futures::stream::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel, bson,
    error::{ErrorKind, WriteFailure},
    options::IndexOptions,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::{AppError, Result},
    models::{
        gender::Gender,
        profile::{SortBy, SortOrder},
    },
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Profile {
    #[serde(rename = "_id", with = "bson::serde_helpers::uuid_1_as_binary")]
    pub id: Uuid,
    pub name: String,
    pub gender: Gender,
    pub gender_probability: f64,
    pub age: u8,
    pub age_group: String,
    pub country_id: String,
    pub country_name: String,
    pub country_probability: f64,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct ProfileFilters {
    pub gender: Option<Gender>,
    pub country_id: Option<String>,
    pub age_group: Option<String>,
    pub min_age: Option<u8>,
    pub max_age: Option<u8>,
    pub min_gender_probability: Option<f64>,
    pub min_country_probability: Option<f64>,
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

        let filter_index = IndexModel::builder()
            .keys(bson::doc! { "country_id": 1, "gender": 1, "age_group": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_filters".to_string())
                    .build(),
            )
            .build();

        let age_index = IndexModel::builder()
            .keys(bson::doc! { "age": 1 })
            .options(IndexOptions::builder().name("idx_age".to_string()).build())
            .build();

        let created_at_index = IndexModel::builder()
            .keys(bson::doc! { "created_at": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_created_at".to_string())
                    .build(),
            )
            .build();

        let prob_index = IndexModel::builder()
            .keys(bson::doc! { "gender_probability": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_gender_prob".to_string())
                    .build(),
            )
            .build();

        let country_prob_index = IndexModel::builder()
            .keys(bson::doc! { "country_probability": 1 })
            .options(
                IndexOptions::builder()
                    .name("idx_country_prob".to_string())
                    .build(),
            )
            .build();

        self.collection
            .create_indexes(vec![
                name_index,
                filter_index,
                age_index,
                created_at_index,
                prob_index,
                country_prob_index,
            ])
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

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Profile>> {
        self.collection
            .find_one(bson::doc! { "_id": bson::Uuid::from(id) })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Search Error: {}", e)))
    }

    pub async fn delete_by_id(&self, id: Uuid) -> Result<bool> {
        let result = self
            .collection
            .delete_one(bson::doc! { "_id": bson::Uuid::from(id) })
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Delete Error: {}", e)))?;
        Ok(result.deleted_count > 0)
    }

    fn build_filter_doc(&self, filters: ProfileFilters) -> bson::Document {
        let mut filter_doc = bson::doc! {};

        if let Some(gender) = filters.gender {
            filter_doc.insert("gender", gender.to_string());
        }
        if let Some(country) = filters.country_id {
            filter_doc.insert("country_id", country.to_uppercase());
        }
        if let Some(age) = filters.age_group {
            filter_doc.insert("age_group", age.to_lowercase());
        }

        let mut age_doc = bson::doc! {};
        if let Some(min_age) = filters.min_age {
            age_doc.insert("$gte", min_age as i32);
        }
        if let Some(max_age) = filters.max_age {
            age_doc.insert("$lte", max_age as i32);
        }
        if !age_doc.is_empty() {
            filter_doc.insert("age", age_doc);
        }

        if let Some(min_gender_prob) = filters.min_gender_probability {
            filter_doc.insert("gender_probability", bson::doc! { "$gte": min_gender_prob });
        }
        if let Some(min_country_prob) = filters.min_country_probability {
            filter_doc.insert(
                "country_probability",
                bson::doc! { "$gte": min_country_prob },
            );
        }

        filter_doc
    }

    fn build_sort_doc(&self, sort_by: SortBy, order: SortOrder) -> bson::Document {
        let sort_field = match sort_by {
            SortBy::Age => "age",
            SortBy::CreatedAt => "created_at",
            SortBy::GenderProbability => "gender_probability",
        };
        let sort_direction = match order {
            SortOrder::Asc => 1,
            SortOrder::Desc => -1,
        };
        bson::doc! { sort_field: sort_direction }
    }

    pub async fn find_paginated(
        &self,
        filters: ProfileFilters,
        sort_by: SortBy,
        order: SortOrder,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<Profile>, u64)> {
        let filter_doc = self.build_filter_doc(filters);
        let sort_doc = self.build_sort_doc(sort_by, order);
        let skip = (page.saturating_sub(1)) * limit;

        let find_options = mongodb::options::FindOptions::builder()
            .sort(sort_doc)
            .skip(skip as u64)
            .limit(limit as i64)
            .build();

        let cursor_future = self
            .collection
            .find(filter_doc.clone())
            .with_options(find_options);

        let count_future = self.collection.count_documents(filter_doc);

        let (cursor_res, count_res) = tokio::join!(cursor_future, count_future);

        let cursor = cursor_res
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Find Error: {}", e)))?;
        let count = count_res
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Count Error: {}", e)))?;

        let profiles: Vec<Profile> = cursor
            .try_collect()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Cursor Error: {}", e)))?;

        Ok((profiles, count))
    }

    pub async fn insert_profile(&self, profile: Profile) -> Result<()> {
        match self.collection.insert_one(profile).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if let ErrorKind::Write(WriteFailure::WriteError(ref write_error)) = *e.kind
                    && write_error.code == 11000
                {
                    return Err(AppError::BadRequest(
                        "A profile with this name already exists".to_string(),
                    ));
                }
                Err(AppError::ServiceUnavailable(format!(
                    "DB Insert Error: {}",
                    e
                )))
            }
        }
    }

    pub async fn insert_many_profiles(&self, profiles: Vec<Profile>) -> Result<u64> {
        if profiles.is_empty() {
            return Ok(0);
        }

        let total = profiles.len() as u64;
        let options = mongodb::options::InsertManyOptions::builder()
            .ordered(false) // With ordered=false, duplicate key errors are partial failures.
            .build();

        match self
            .collection
            .insert_many(&profiles)
            .with_options(options)
            .await
        {
            Ok(result) => Ok(result.inserted_ids.len() as u64),
            Err(e) => {
                if let ErrorKind::InsertMany(ref insert_many_err) = *e.kind
                    && let Some(ref write_errors) = insert_many_err.write_errors
                {
                    let all_dup_key = write_errors.iter().all(|err| err.code == 11000);
                    if all_dup_key {
                        let inserted = total - write_errors.len() as u64;
                        return Ok(inserted);
                    } else {
                        let non_dup_count =
                            write_errors.iter().filter(|err| err.code != 11000).count();
                        return Err(AppError::ServiceUnavailable(format!(
                            "DB Bulk Insert partially failed: {} non-duplicate errors occurred",
                            non_dup_count
                        )));
                    }
                }
                Err(AppError::ServiceUnavailable(format!(
                    "DB Bulk Insert Error: {}",
                    e
                )))
            }
        }
    }

    pub async fn find_all(
        &self,
        filters: ProfileFilters,
        sort_by: SortBy,
        order: SortOrder,
    ) -> Result<Vec<Profile>> {
        let filter_doc = self.build_filter_doc(filters);
        let sort_doc = self.build_sort_doc(sort_by, order);

        let find_options = mongodb::options::FindOptions::builder()
            .sort(sort_doc)
            .build();

        let cursor = self
            .collection
            .find(filter_doc)
            .with_options(find_options)
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Find Error: {}", e)))?;

        cursor
            .try_collect()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("DB Cursor Error: {}", e)))
    }
}

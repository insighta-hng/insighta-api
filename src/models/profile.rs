use crate::models::db::Profile;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub data: Profile,
}

#[derive(Debug, Serialize)]
pub struct ProfileListEntry {
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

#[derive(Debug, Serialize)]
pub struct ProfileListResponse {
    pub status: String,
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub data: Vec<ProfileListEntry>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    Age,
    CreatedAt,
    GenderProbability,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    pub gender: Option<String>,
    pub age_group: Option<String>,
    pub country_id: Option<String>,
    pub min_age: Option<u8>,
    pub max_age: Option<u8>,
    pub min_gender_probability: Option<f64>,
    pub min_country_probability: Option<f64>,
    pub sort_by: Option<SortBy>,
    pub order: Option<SortOrder>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub sort_by: Option<SortBy>,
    pub order: Option<SortOrder>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

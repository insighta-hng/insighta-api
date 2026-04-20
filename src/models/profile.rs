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
    pub count: usize,
    pub data: Vec<ProfileListEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    pub gender: Option<String>,
    pub country_id: Option<String>,
    pub age_group: Option<String>,
}

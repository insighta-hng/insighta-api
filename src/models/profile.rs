use crate::{
    models::{db::Profile, gender::Gender},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ProfileDto {
    pub id: Uuid,
    pub name: String,
    pub gender: Gender,
    pub gender_probability: f64,
    pub age: u8,
    pub age_group: String,
    pub country_id: String,
    pub country_name: String,
    pub country_probability: f64,
    #[serde(serialize_with = "serialize_date_time")]
    pub created_at: DateTime<Utc>,
}

fn serialize_date_time<S>(
    date_time: &DateTime<Utc>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let formatted_date_time = date_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    serializer.serialize_str(&formatted_date_time)
}

impl From<Profile> for ProfileDto {
    fn from(profile: Profile) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            gender: profile.gender,
            gender_probability: profile.gender_probability,
            age: profile.age,
            age_group: profile.age_group,
            country_id: profile.country_id,
            country_name: profile.country_name,
            country_probability: profile.country_probability,
            created_at: profile.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub data: ProfileDto,
}

#[derive(Debug, Serialize)]
pub struct ProfileListResponse {
    pub status: String,
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub data: Vec<ProfileDto>,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    Age,
    CreatedAt,
    GenderProbability,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    pub gender: Option<Gender>,
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

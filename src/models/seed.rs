use crate::models::gender::Gender;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SeedProfile {
    pub name: String,
    pub gender: Gender,
    pub gender_probability: f64,
    pub age: u8,
    pub age_group: String,
    pub country_id: String,
    pub country_name: String,
    pub country_probability: f64,
}

#[derive(Debug, Deserialize)]
pub struct SeedFile {
    pub profiles: Vec<SeedProfile>,
}

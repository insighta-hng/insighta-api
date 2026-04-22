use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    Male,
    Female,
}

impl std::fmt::Display for Gender {
    fn fmt(&self, func: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Gender::Male => write!(func, "male"),
            Gender::Female => write!(func, "female"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GenderizeResponse {
    pub gender: Option<Gender>,
    #[serde(rename = "probability")]
    pub gender_probability: f64,
    #[serde(rename = "count")]
    pub sample_size: u64,
}

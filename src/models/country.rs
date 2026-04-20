use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CountryData {
    pub country_id: String,
    pub probability: f64,
}

#[derive(Debug, Deserialize)]
pub struct NationalizeRawResponse {
    pub country: Vec<CountryData>,
}

#[derive(Debug, Deserialize)]
pub struct NationalizeResponse {
    pub country_id: String,
    pub country_name: String,
    pub country_probability: f64,
}

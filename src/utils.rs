use serde_json::Value;

use crate::{
    client::ReqwestClient,
    errors::{AppError, Result},
    models::{
        age::{AgeGroup, AgifyResponse},
        country::{NationalizeRawResponse, NationalizeResponse},
        gender::GenderizeResponse,
    },
};

pub fn validate_name(name_value: Option<Value>) -> Result<String> {
    match name_value {
        None => Err(AppError::BadRequest("Missing or empty name".to_string())),
        Some(Value::String(name)) => {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                Err(AppError::BadRequest("Missing or empty name".to_string()))
            } else {
                Ok(trimmed)
            }
        }
        Some(_) => Err(AppError::UnprocessableEntity("Invalid type".to_string())),
    }
}

pub async fn fetch_gender_data(
    reqwest_client: &ReqwestClient,
    name: &str,
) -> Result<GenderizeResponse> {
    let client = reqwest_client.get();
    let response: GenderizeResponse = client
        .get("https://api.genderize.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    if response.gender.is_none() || response.sample_size == 0 {
        return Err(AppError::UpstreamInvalidResponse("Genderize".to_string()));
    }

    Ok(response)
}

pub async fn fetch_age_data(reqwest_client: &ReqwestClient, name: &str) -> Result<AgifyResponse> {
    let client = reqwest_client.get();
    let mut response: AgifyResponse = client
        .get("https://api.agify.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    if response.age.is_none() {
        return Err(AppError::UpstreamInvalidResponse("Agify".to_string()));
    }

    response.age_group = AgeGroup::classify(response.age.unwrap_or(0));

    Ok(response)
}

pub async fn fetch_country_data(
    reqwest_client: &ReqwestClient,
    name: &str,
) -> Result<NationalizeResponse> {
    let client = reqwest_client.get();
    let response: NationalizeRawResponse = client
        .get("https://api.nationalize.io")
        .query(&[("name", name)])
        .send()
        .await?
        .json()
        .await?;

    let best_country = response
        .country
        .into_iter()
        .max_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| AppError::UpstreamInvalidResponse("Nationalize".to_string()))?;

    Ok(NationalizeResponse {
        country_id: best_country.country_id,
        country_probability: best_country.probability,
    })
}

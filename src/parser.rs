use capitalize::Capitalize;

use crate::COUNTRIES;
use crate::errors::{AppError, Result};
use crate::models::db::ProfileFilters;
use crate::models::profile::{SearchQuery, SortBy, SortOrder};

pub fn parse_query(search_query: &str) -> Result<(ProfileFilters, SearchQuery)> {
    let trimmed_query = search_query.trim();
    let mut filters = ProfileFilters::default();
    let mut search_limit = 10;
    let mut sort_order = SortOrder::default();
    let mut sort_by = SortBy::default();
    let mut is_parsed_country = false;
    let mut is_value_parsed = false;
    let mut males = false;
    let mut females = false;

    if let Some(&code) = COUNTRIES.get(&trimmed_query.to_string().capitalize().as_str()) {
        filters.country_id = Some(code.to_string());
        is_parsed_country = true;
        is_value_parsed = true;
    }

    let tokens: Vec<&str> = trimmed_query.split_whitespace().collect();

    for idx in 0..tokens.len() {
        let token = tokens[idx];

        match token {
            "in" | "from" => {
                if idx + 1 < tokens.len() {
                    let next_token = tokens[idx + 1].capitalize();
                    if !is_parsed_country && COUNTRIES.contains_key(next_token.as_str()) {
                        filters.country_id =
                            Some(COUNTRIES.get(next_token.as_str()).unwrap().to_string());
                        is_parsed_country = true;
                        is_value_parsed = true;
                    }
                }
            }
            "young" => {
                filters.min_age = Some(16);
                filters.max_age = Some(24);
                is_value_parsed = true;
            }
            "above" | "over" | "least" => {
                if idx + 1 < tokens.len()
                    && let Ok(age) = tokens[idx + 1].parse::<u8>()
                {
                    filters.min_age = Some(age);
                    is_value_parsed = true;
                }
            }
            "under" | "below" | "most" => {
                if idx + 1 < tokens.len()
                    && let Ok(age) = tokens[idx + 1].parse::<u8>()
                {
                    filters.max_age = Some(age);
                    is_value_parsed = true;
                }
            }
            "top" | "first" | "latest" => {
                if idx + 1 < tokens.len()
                    && let Ok(num) = tokens[idx + 1].parse::<u8>()
                {
                    sort_order = SortOrder::Desc;
                    sort_by = SortBy::CreatedAt;
                    search_limit = num;
                    is_value_parsed = true;
                }
            }
            "last" | "oldest" | "bottom" => {
                if idx + 1 < tokens.len()
                    && let Ok(num) = tokens[idx + 1].parse::<u8>()
                {
                    sort_order = SortOrder::Asc;
                    sort_by = SortBy::CreatedAt;
                    search_limit = num;
                    is_value_parsed = true;
                }
            }
            "male" | "males" | "man" | "men" | "boy" | "boys" => {
                males = true;
                is_value_parsed = true;
            }
            "female" | "females" | "woman" | "women" | "girl" | "girls" | "lady" | "ladies" => {
                females = true;
                is_value_parsed = true;
            }
            "child" | "children" | "kid" | "kids" => {
                filters.age_group = Some("child".to_string());
                is_value_parsed = true;
            }
            "teenager" | "teenagers" | "teen" | "teens" => {
                filters.age_group = Some("teenager".to_string());
                is_value_parsed = true;
            }
            "adult" | "adults" | "grownup" | "grownups" | "middle-aged" => {
                filters.age_group = Some("adult".to_string());
                is_value_parsed = true;
            }
            "senior" | "seniors" | "old" | "elderly" => {
                filters.age_group = Some("senior".to_string());
                is_value_parsed = true;
            }
            _ => {}
        }
    }

    if males && !females {
        filters.gender = Some("male".to_string());
    } else if females && !males {
        filters.gender = Some("female".to_string());
    } else {
        filters.gender = None;
    }

    if !is_value_parsed {
        return Err(AppError::BadRequest(
            "Unable to interpret query".to_string(),
        ));
    }

    Ok((
        filters,
        SearchQuery {
            q: None,
            page: None,
            limit: Some(search_limit as u32),
            sort_by: Some(sort_by),
            order: Some(sort_order),
        },
    ))
}

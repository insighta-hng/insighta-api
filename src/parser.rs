use crate::countries::COUNTRIES_LOWER;
use crate::errors::{AppError, Result};
use crate::models::{
    gender::Gender,
    profile::{SearchQuery, SortBy, SortOrder},
};
use crate::repo::profile::ProfileFilters;

/// Parses a natural language search query into structured database filters.
///
/// Converts a plain English query (e.g., "young males from nigeria") into a `ProfileFilters`
/// struct and a `SearchQuery` struct containing limit and sort instructions.
///
/// # Arguments
///
/// * `search_query` - A string slice that holds the raw query to be parsed.
///
/// # Returns
///
/// Returns a `Result` containing a tuple of `(ProfileFilters, SearchQuery)` if the query
/// contains recognized demographic filters.
///
/// # Errors
///
/// Returns `AppError::BadRequest("Unable to interpret query")` if no valid demographic
/// tokens are recognized or if the query consists entirely of stop words.
pub fn parse_query(search_query: &str) -> Result<(ProfileFilters, SearchQuery)> {
    let trimmed_query = search_query.trim().to_lowercase();
    let mut filters = ProfileFilters::default();
    let mut search_limit = 10u8;
    let mut sort_order = SortOrder::default();
    let mut sort_by = SortBy::default();
    let mut is_parsed_country = false;
    let mut is_value_parsed = false;
    let mut males = false;
    let mut females = false;

    if let Some(&code) = COUNTRIES_LOWER.get(trimmed_query.as_str()) {
        filters.country_id = Some(code.to_string());
        is_parsed_country = true;
        is_value_parsed = true;
    }

    let tokens: Vec<&str> = trimmed_query.split_whitespace().collect();

    for idx in 0..tokens.len() {
        let token = tokens[idx];

        match token {
            "in" | "from" if idx + 1 < tokens.len() && !is_parsed_country => {
                let remaining = tokens.len() - idx - 1;
                let max_window = remaining.min(7);

                for window in (1..=max_window).rev() {
                    let candidate = tokens[idx + 1..=idx + window].join(" ");
                    if let Some(&code) = COUNTRIES_LOWER.get(candidate.as_str()) {
                        filters.country_id = Some(code.to_string());
                        is_parsed_country = true;
                        is_value_parsed = true;
                        break;
                    }
                }
            }
            "young" => {
                filters.min_age = Some(16);
                filters.max_age = Some(24);
                is_value_parsed = true;
            }
            "above" | "over" | "least" => {
                if let Some(age) = tokens.get(idx + 1).and_then(|token| parse_number(token)) {
                    filters.min_age = Some(age);
                    is_value_parsed = true;
                }
            }
            "under" | "below" | "most" => {
                if let Some(age) = tokens.get(idx + 1).and_then(|token| parse_number(token)) {
                    filters.max_age = Some(age);
                    is_value_parsed = true;
                }
            }
            "top" | "first" | "latest" => {
                if let Some(num) = tokens.get(idx + 1).and_then(|token| parse_number(token)) {
                    sort_order = SortOrder::Desc;
                    sort_by = SortBy::CreatedAt;
                    search_limit = num;
                    is_value_parsed = true;
                }
            }
            "last" | "oldest" | "bottom" => {
                if let Some(num) = tokens.get(idx + 1).and_then(|t| parse_number(t)) {
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
        filters.gender = Some(Gender::Male);
    } else if females && !males {
        filters.gender = Some(Gender::Female);
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

fn parse_number(num_str: &str) -> Option<u8> {
    if let Ok(num) = num_str.parse::<u8>() {
        return Some(num);
    }
    match num_str {
        "zero" => Some(0),
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        "eleven" => Some(11),
        "twelve" => Some(12),
        "thirteen" => Some(13),
        "fourteen" => Some(14),
        "fifteen" => Some(15),
        "sixteen" => Some(16),
        "seventeen" => Some(17),
        "eighteen" => Some(18),
        "nineteen" => Some(19),
        "twenty" => Some(20),
        "thirty" => Some(30),
        "forty" => Some(40),
        "fifty" => Some(50),
        "sixty" => Some(60),
        "seventy" => Some(70),
        "eighty" => Some(80),
        "ninety" => Some(90),
        "hundred" => Some(100),
        _ => None,
    }
}

use sha2::{Digest, Sha256};

use crate::models::profile::{ProfileFilters, SortBy, SortOrder};

/// Builds a deterministic cache key from a set of query parameters.
///
/// Two queries that represent the same intent produce the same key,
/// regardless of how the values were expressed before reaching this function.
/// Normalization rules:
///   - `country_id` is uppercased (e.g. "ng" and "NG" are the same filter)
///   - `gender` is already an enum, so it is inherently canonical
///   - `age_group` is lowercased
///   - `f64` probabilities are rounded to two decimal places before formatting
///   - Absent optional fields map to stable sentinel values (empty string or 0)
///
/// The result is a hex-encoded SHA-256 hash of the canonical parameter string,
/// which is safe to use as a `DashMap` key.
pub fn build_cache_key(
    prefix: &str,
    filters: &ProfileFilters,
    sort_by: &SortBy,
    order: &SortOrder,
    page: u32,
    limit: u32,
) -> String {
    let country = filters
        .country_id
        .as_deref()
        .map(|country_val| country_val.to_uppercase())
        .unwrap_or_default();

    let gender = filters
        .gender
        .map(|gender_val| gender_val.to_string())
        .unwrap_or_default();

    let age_group = filters
        .age_group
        .as_deref()
        .map(|age_val| age_val.to_lowercase())
        .unwrap_or_default();

    let min_age = filters.min_age.unwrap_or(0);
    let max_age = filters.max_age.unwrap_or(u8::MAX);

    let min_gender_prob = filters
        .min_gender_probability
        .map(|prob_val| (prob_val * 100.0).round() / 100.0)
        .unwrap_or(0.0);

    let min_country_prob = filters
        .min_country_probability
        .map(|prob_val| (prob_val * 100.0).round() / 100.0)
        .unwrap_or(0.0);

    let canonical = format!(
        "{prefix}|c={country}|g={gender}|ag={age_group}|mina={min_age}|maxa={max_age}|mgp={min_gender_prob:.2}|mcp={min_country_prob:.2}|s={}|o={}|p={page}|l={limit}",
        sort_by.as_str(),
        order.as_str(),
    );

    hex::encode(Sha256::digest(canonical.as_bytes()))
}

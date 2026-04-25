use insighta_api::{
    errors::AppError,
    models::{
        gender::Gender,
        profile::{SortBy, SortOrder},
    },
    parser::parse_query,
};

#[test]
fn test_stop_words_only() {
    let err = parse_query("show me all people").unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));

    let err = parse_query("who is the person that").unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[test]
fn test_gender_keywords() {
    let (filters, _) = parse_query("young males").unwrap();
    assert_eq!(filters.gender, Some(Gender::Male));

    let (filters, _) = parse_query("adult females").unwrap();
    assert_eq!(filters.gender, Some(Gender::Female));

    // Both genders cancel out
    let (filters, _) = parse_query("male and female teenagers").unwrap();
    assert_eq!(filters.gender, None);

    // Multiple variations
    let (filters, _) = parse_query("boy and girls").unwrap();
    assert_eq!(filters.gender, None);

    let (filters, _) = parse_query("ladies").unwrap();
    assert_eq!(filters.gender, Some(Gender::Female));
}

#[test]
fn test_age_groups() {
    let (filters, _) = parse_query("kids").unwrap();
    assert_eq!(filters.age_group.as_deref(), Some("child"));

    let (filters, _) = parse_query("teenagers").unwrap();
    assert_eq!(filters.age_group.as_deref(), Some("teenager"));

    let (filters, _) = parse_query("adults").unwrap();
    assert_eq!(filters.age_group.as_deref(), Some("adult"));

    let (filters, _) = parse_query("seniors").unwrap();
    assert_eq!(filters.age_group.as_deref(), Some("senior"));

    // Conflicting age groups (the last one mentioned currently wins because they override)
    let (filters, _) = parse_query("adults and seniors").unwrap();
    // Since "seniors" is checked last in the match statement or if it appears later in tokens
    assert_eq!(filters.age_group.as_deref(), Some("senior"));
}

#[test]
fn test_young_keyword() {
    let (filters, _) = parse_query("young males").unwrap();
    assert_eq!(filters.min_age, Some(16));
    assert_eq!(filters.max_age, Some(24));
}

#[test]
fn test_age_ranges() {
    let (filters, _) = parse_query("males above 30").unwrap();
    assert_eq!(filters.min_age, Some(30));

    let (filters, _) = parse_query("females below 25").unwrap();
    assert_eq!(filters.max_age, Some(25));

    let (filters, _) = parse_query("people over twenty").unwrap();
    assert_eq!(filters.min_age, Some(20));

    let (filters, _) = parse_query("at least 18").unwrap();
    assert_eq!(filters.min_age, Some(18));

    let (filters, _) = parse_query("at most 65").unwrap();
    assert_eq!(filters.max_age, Some(65));
}

#[test]
fn test_age_range_overflow() {
    // 300 exceeds u8 capacity, should be ignored
    let err = parse_query("above 300").unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_))); // Ignored, no valid tokens
}

#[test]
fn test_age_range_disconnect() {
    // The number is not immediately after the modifier
    let err = parse_query("above the age of 30").unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[test]
fn test_country_matching() {
    let (filters, _) = parse_query("people from nigeria").unwrap();
    assert_eq!(filters.country_id.as_deref(), Some("NG"));

    let (filters, _) = parse_query("adults in japan").unwrap();
    assert_eq!(filters.country_id.as_deref(), Some("JP"));

    let (filters, _) = parse_query("united states").unwrap();
    assert_eq!(filters.country_id.as_deref(), Some("US"));

    // Exact match of complex name
    let (filters, _) = parse_query("bosnia and herzegovina").unwrap();
    assert_eq!(filters.country_id.as_deref(), Some("BA"));

    // Missing preposition fails to match country embedded in query
    let (filters, _) = parse_query("males nigeria").unwrap();
    assert_eq!(filters.country_id, None);
    assert_eq!(filters.gender, Some(Gender::Male));
}

#[test]
fn test_sorting_and_limits() {
    let (_, search) = parse_query("top 5 women").unwrap();
    assert_eq!(search.limit, Some(5));
    assert_eq!(search.sort_by, Some(SortBy::CreatedAt));
    assert_eq!(search.order, Some(SortOrder::Desc));

    let (_, search) = parse_query("oldest 20 people").unwrap();
    assert_eq!(search.limit, Some(20));
    assert_eq!(search.sort_by, Some(SortBy::CreatedAt));
    assert_eq!(search.order, Some(SortOrder::Asc));
}

#[test]
fn test_sorting_and_limit_overflow() {
    // u8 limit maximum is 255. 300 should be ignored.
    let (filters, search) = parse_query("top 300 males").unwrap();
    assert_eq!(filters.gender, Some(Gender::Male));
    // Since 300 overflows u8, the 'top 300' bigram is not fully recognized.
    // It falls back to default limit.
    assert_eq!(search.limit, Some(10));
}

#[test]
fn test_complex_query() {
    let (filters, search) = parse_query("top 5 elderly men in japan above 70").unwrap();

    assert_eq!(filters.gender, Some(Gender::Male));
    assert_eq!(filters.age_group.as_deref(), Some("senior"));
    assert_eq!(filters.country_id.as_deref(), Some("JP"));
    assert_eq!(filters.min_age, Some(70));

    assert_eq!(search.limit, Some(5));
    assert_eq!(search.sort_by, Some(SortBy::CreatedAt));
    assert_eq!(search.order, Some(SortOrder::Desc));
}

#[test]
fn test_complex_query_with_noise() {
    // Random unrecognized words interspersed
    let (filters, search) = parse_query(
        "I want to find the top 10 young and energetic males from the beautiful country of nigeria",
    )
    .unwrap();

    assert_eq!(filters.gender, Some(Gender::Male));
    assert_eq!(filters.country_id.as_deref(), None);
    assert_eq!(filters.min_age, Some(16));
    assert_eq!(filters.max_age, Some(24));

    assert_eq!(search.limit, Some(10));
    assert_eq!(search.sort_by, Some(SortBy::CreatedAt));
    assert_eq!(search.order, Some(SortOrder::Desc));
}

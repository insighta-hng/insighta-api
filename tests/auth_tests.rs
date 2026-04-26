use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use insighta_api::{
    auth::{
        pkce::verify_code_challenge,
        tokens::{issue_access_token, validate_access_token},
    },
    models::user::Role,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[test]
fn test_pkce_valid_challenge() {
    let verifier = "test_code_verifier_string_long_enough";
    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hash);

    assert!(verify_code_challenge(verifier, &challenge));
}

#[test]
fn test_pkce_wrong_verifier() {
    let verifier = "correct_verifier";
    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hash);

    assert!(!verify_code_challenge("wrong_verifier", &challenge));
}

#[test]
fn test_pkce_tampered_challenge() {
    let verifier = "some_verifier";
    assert!(!verify_code_challenge(verifier, "not_a_real_challenge"));
}

#[test]
fn test_access_token_roundtrip() {
    let secret = "test-secret-that-is-long-enough-32ch";
    let user_id = Uuid::now_v7();
    let role = Role::Analyst;

    let token = issue_access_token(user_id, &role, secret).unwrap();
    let claims = validate_access_token(&token, secret).unwrap();

    assert_eq!(claims.sub, user_id.to_string());
    assert_eq!(claims.role, Role::Analyst);
}

#[test]
fn test_access_token_wrong_secret() {
    let user_id = Uuid::now_v7();
    let token =
        issue_access_token(user_id, &Role::Admin, "secret-a-long-enough-string-here").unwrap();
    let result = validate_access_token(&token, "wrong-secret-long-enough-string-here");

    assert!(result.is_err());
}

#[test]
fn test_admin_role_preserved_in_token() {
    let secret = "test-secret-that-is-long-enough-32ch";
    let user_id = Uuid::now_v7();

    let token = issue_access_token(user_id, &Role::Admin, secret).unwrap();
    let claims = validate_access_token(&token, secret).unwrap();

    assert_eq!(claims.role, Role::Admin);
}

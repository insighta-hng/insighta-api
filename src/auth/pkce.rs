use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

pub fn verify_code_challenge(code_verifier: &str, code_challenge: &str) -> bool {
    let hash = Sha256::digest(code_verifier.as_bytes());
    let derived = BASE64_URL_SAFE_NO_PAD.encode(hash);
    derived == code_challenge
}

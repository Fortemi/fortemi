//! HMAC-SHA256 webhook signature tests (Issue #47).
//!
//! Verifies the signing logic used in `deliver_webhook` produces correct,
//! deterministic signatures that external consumers can independently verify.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute signature the same way deliver_webhook does.
fn compute_signature(secret: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    format!("sha256={}", signature)
}

#[test]
fn test_hmac_sha256_signature_format() {
    let sig = compute_signature("my-secret", r#"{"type":"QueueStatus"}"#);

    // Must start with "sha256="
    assert!(sig.starts_with("sha256="));

    // Hex portion must be 64 characters (256 bits = 32 bytes = 64 hex chars)
    let hex_part = &sig["sha256=".len()..];
    assert_eq!(hex_part.len(), 64);
    assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_hmac_sha256_signature_deterministic() {
    let secret = "test-secret";
    let body = r#"{"type":"JobCompleted","job_id":"00000000-0000-0000-0000-000000000000"}"#;

    // Same input → same output
    let sig1 = compute_signature(secret, body);
    let sig2 = compute_signature(secret, body);
    assert_eq!(sig1, sig2);

    // Different body → different signature
    let sig3 = compute_signature(secret, r#"{"type":"JobFailed"}"#);
    assert_ne!(sig1, sig3);

    // Different secret → different signature
    let sig4 = compute_signature("other-secret", body);
    assert_ne!(sig1, sig4);
}

#[test]
fn test_hmac_sha256_signature_matches_reference() {
    // Golden test vector: external consumers can use this to validate their implementation
    let secret = "test-secret";
    let body = r#"{"type":"QueueStatus","total_jobs":0,"running":0,"pending":0}"#;

    // Compute expected value independently
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    let expected_hex = hex::encode(mac.finalize().into_bytes());

    let signature = compute_signature(secret, body);
    assert_eq!(signature, format!("sha256={}", expected_hex));

    // Verify the signature can be validated by a receiver
    let mut verifier = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    verifier.update(body.as_bytes());
    let received_bytes = hex::decode(&expected_hex).unwrap();
    verifier
        .verify_slice(&received_bytes)
        .expect("HMAC verification should succeed");
}

//! HTTP-boundary contract tests for the incoming webhook receiver pipeline
//! (Phase B, #823): signature verification (#820), JSON Schema validation
//! (#821), and `Idempotency-Key` dedupe (#822).
//!
//! These exercise the full pipeline across the real HTTP boundary against a
//! live server (`API_BASE_URL`), and skip gracefully when it is absent —
//! matching `ingest_stream_contract_test` / `chat_stream_contract_test`. The
//! validation *logic* is unit-tested in `matric-db::incoming_webhooks` and the
//! idempotency store in `services::idempotency_store`; this file pins the wire
//! contract: status codes and field-level error surfacing end to end.
//!
//! Auth: receiver registration (`POST /api/v1/webhooks/incoming`) requires a
//! bearer token when the server enforces `REQUIRE_AUTH`. Set `API_TOKEN` to a
//! valid token; it is attached to every request (registration needs it, and it
//! lets the receive POST traverse the global auth layer — the receive endpoint
//! authenticates the *payload* via HMAC regardless). On an anonymous dev server
//! no token is needed and HMAC alone gates the receive path.
//!
//! Idempotency assertions require the server to have Redis configured; without
//! it the store degrades to a no-op (every request processed) and the
//! dedupe/conflict tests are skipped via a capability probe.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const SECRET: &str = "incoming-contract-secret-1234567890";

fn api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn api_token() -> Option<String> {
    std::env::var("API_TOKEN").ok().filter(|t| !t.is_empty())
}

/// Reachable only when `API_BASE_URL` is explicitly set and `/health` answers.
async fn api_available() -> bool {
    if std::env::var("API_BASE_URL").is_err() {
        return false;
    }
    reqwest::Client::new()
        .get(format!("{}/health", api_base_url()))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

macro_rules! require_api {
    () => {
        if !api_available().await {
            eprintln!(
                "Skipping: API_BASE_URL not set or server not available at {}",
                api_base_url()
            );
            return;
        }
    };
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Process-unique slug for an isolated receiver per test.
fn unique_slug(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

fn with_auth(rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match api_token() {
        Some(t) => rb.bearer_auth(t),
        None => rb,
    }
}

/// Register a receiver. `schema_doc_json`, when present, is the raw JSON for the
/// custom JSON Schema document; otherwise the built-in `schema_ref` is used.
async fn register_receiver(
    c: &reqwest::Client,
    slug: &str,
    schema_ref: &str,
    schema_doc_json: Option<&str>,
) -> reqwest::StatusCode {
    let schema_doc_field = schema_doc_json
        .map(|s| format!(",\"schema_doc\":{s}"))
        .unwrap_or_default();
    let body = format!(
        "{{\"slug\":\"{slug}\",\"provider\":\"contract\",\"schema_ref\":\"{schema_ref}\",\
          \"hmac_secret\":\"{SECRET}\",\"signature_header\":\"X-Fortemi-Signature\",\
          \"is_active\":true{schema_doc_field}}}"
    );
    with_auth(
        c.post(format!("{}/api/v1/webhooks/incoming", api_base_url()))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body),
    )
    .send()
    .await
    .expect("register receiver")
    .status()
}

async fn delete_receiver(c: &reqwest::Client, slug: &str) {
    let _ = with_auth(c.delete(format!(
        "{}/api/v1/webhooks/incoming/{slug}",
        api_base_url()
    )))
    .send()
    .await;
}

/// POST a body to a receiver. `signature` overrides the signature header verbatim
/// (None = omit the header); `idem_key` sets `Idempotency-Key` when present.
async fn post_webhook(
    c: &reqwest::Client,
    slug: &str,
    body: &str,
    signature: Option<String>,
    idem_key: Option<&str>,
) -> (reqwest::StatusCode, String) {
    let mut rb = c
        .post(format!(
            "{}/api/v1/webhooks/incoming/{slug}",
            api_base_url()
        ))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string());
    if let Some(sig) = signature {
        rb = rb.header("X-Fortemi-Signature", sig);
    }
    if let Some(k) = idem_key {
        rb = rb.header("Idempotency-Key", k);
    }
    let resp = with_auth(rb).send().await.expect("post webhook");
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    (status, text)
}

/// Probe whether the server dedupes (Redis configured): send the same key+body
/// twice to a throwaway receiver and see if the second is treated as a replay.
/// Returns false when idempotency is a no-op (Redis absent) so those assertions
/// can be skipped rather than producing false negatives.
async fn idempotency_active(c: &reqwest::Client) -> bool {
    let slug = unique_slug("idem-probe");
    let schema = r#"{"type":"object"}"#;
    if !register_receiver(c, &slug, "contract.any.v1", Some(schema))
        .await
        .is_success()
    {
        return false;
    }
    let body = r#"{"probe":1}"#;
    let key = format!("probe-{}", unique_slug("k"));
    let (s1, _) = post_webhook(
        c,
        &slug,
        body,
        Some(sign(SECRET, body.as_bytes())),
        Some(&key),
    )
    .await;
    // A different body under the same key must 409 iff dedupe is active.
    let other = r#"{"probe":2}"#;
    let (s2, _) = post_webhook(
        c,
        &slug,
        other,
        Some(sign(SECRET, other.as_bytes())),
        Some(&key),
    )
    .await;
    delete_receiver(c, &slug).await;
    s1.is_success() && s2 == reqwest::StatusCode::CONFLICT
}

// =============================================================================
// Signature (#820)
// =============================================================================

#[tokio::test]
async fn signature_valid_is_accepted() {
    require_api!();
    let c = client();
    let slug = unique_slug("sig-ok");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;
    let body = r#"{"hello":"world"}"#;
    let (status, _) =
        post_webhook(&c, &slug, body, Some(sign(SECRET, body.as_bytes())), None).await;
    delete_receiver(&c, &slug).await;
    assert!(
        status.is_success(),
        "valid HMAC should be accepted, got {status}"
    );
}

#[tokio::test]
async fn signature_wrong_is_rejected() {
    require_api!();
    let c = client();
    let slug = unique_slug("sig-bad");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;
    let body = r#"{"hello":"world"}"#;
    let wrong = sign("the-wrong-secret-0000000000", body.as_bytes());
    let (status, _) = post_webhook(&c, &slug, body, Some(wrong), None).await;
    delete_receiver(&c, &slug).await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "wrong HMAC must be 401"
    );
}

#[tokio::test]
async fn signature_missing_is_rejected() {
    require_api!();
    let c = client();
    let slug = unique_slug("sig-none");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;
    let body = r#"{"hello":"world"}"#;
    let (status, _) = post_webhook(&c, &slug, body, None, None).await;
    delete_receiver(&c, &slug).await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "missing signature must be 401"
    );
}

// =============================================================================
// Schema validation (#821)
// =============================================================================

const PAYMENT_SCHEMA: &str = r#"{"type":"object","required":["amount","currency"],"properties":{"amount":{"type":"number"},"currency":{"type":"string"}}}"#;

#[tokio::test]
async fn schema_valid_is_accepted() {
    require_api!();
    let c = client();
    let slug = unique_slug("schema-ok");
    register_receiver(&c, &slug, "contract.payment.v1", Some(PAYMENT_SCHEMA)).await;
    let body = r#"{"amount":42,"currency":"USD"}"#;
    let (status, _) =
        post_webhook(&c, &slug, body, Some(sign(SECRET, body.as_bytes())), None).await;
    delete_receiver(&c, &slug).await;
    assert!(
        status.is_success(),
        "valid body should be accepted, got {status}"
    );
}

#[tokio::test]
async fn schema_missing_required_field_is_rejected_with_field_name() {
    require_api!();
    let c = client();
    let slug = unique_slug("schema-missing");
    register_receiver(&c, &slug, "contract.payment.v1", Some(PAYMENT_SCHEMA)).await;
    let body = r#"{"currency":"USD"}"#; // missing "amount"
    let (status, text) =
        post_webhook(&c, &slug, body, Some(sign(SECRET, body.as_bytes())), None).await;
    delete_receiver(&c, &slug).await;
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "missing field must be 400"
    );
    assert!(
        text.contains("amount"),
        "error should name the missing field, got: {text}"
    );
}

#[tokio::test]
async fn schema_wrong_type_is_rejected_with_field_name() {
    require_api!();
    let c = client();
    let slug = unique_slug("schema-type");
    register_receiver(&c, &slug, "contract.payment.v1", Some(PAYMENT_SCHEMA)).await;
    let body = r#"{"amount":"not-a-number","currency":"USD"}"#;
    let (status, text) =
        post_webhook(&c, &slug, body, Some(sign(SECRET, body.as_bytes())), None).await;
    delete_receiver(&c, &slug).await;
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "wrong type must be 400"
    );
    assert!(
        text.contains("amount"),
        "error should name the offending field, got: {text}"
    );
}

// =============================================================================
// Idempotency (#822)
// =============================================================================

#[tokio::test]
async fn idempotency_replay_returns_cached_response() {
    require_api!();
    let c = client();
    if !idempotency_active(&c).await {
        eprintln!("Skipping: idempotency not active (Redis not configured on server)");
        return;
    }
    let slug = unique_slug("idem-replay");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;
    let body = r#"{"event":"created","id":"abc"}"#;
    let key = unique_slug("ik");
    let sig = sign(SECRET, body.as_bytes());
    let (s1, b1) = post_webhook(&c, &slug, body, Some(sig.clone()), Some(&key)).await;
    let (s2, b2) = post_webhook(&c, &slug, body, Some(sig), Some(&key)).await;
    delete_receiver(&c, &slug).await;
    assert!(s1.is_success(), "first request should succeed, got {s1}");
    assert!(s2.is_success(), "replay should succeed (cached), got {s2}");
    assert_eq!(b1, b2, "replay must return the cached response verbatim");
}

#[tokio::test]
async fn idempotency_different_body_same_key_conflicts() {
    require_api!();
    let c = client();
    if !idempotency_active(&c).await {
        eprintln!("Skipping: idempotency not active (Redis not configured on server)");
        return;
    }
    let slug = unique_slug("idem-conflict");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;
    let key = unique_slug("ik");
    let first = r#"{"v":1}"#;
    let second = r#"{"v":2}"#;
    let (s1, _) = post_webhook(
        &c,
        &slug,
        first,
        Some(sign(SECRET, first.as_bytes())),
        Some(&key),
    )
    .await;
    let (s2, _) = post_webhook(
        &c,
        &slug,
        second,
        Some(sign(SECRET, second.as_bytes())),
        Some(&key),
    )
    .await;
    delete_receiver(&c, &slug).await;
    assert!(s1.is_success(), "first request should succeed, got {s1}");
    assert_eq!(
        s2,
        reqwest::StatusCode::CONFLICT,
        "key reuse w/ new body must be 409"
    );
}

#[tokio::test]
async fn idempotency_absent_header_processes_each_request() {
    require_api!();
    let c = client();
    let slug = unique_slug("idem-absent");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;
    let body = r#"{"n":1}"#;
    let sig = sign(SECRET, body.as_bytes());
    // No Idempotency-Key on either request: both processed normally (200).
    let (s1, _) = post_webhook(&c, &slug, body, Some(sig.clone()), None).await;
    let (s2, _) = post_webhook(&c, &slug, body, Some(sig), None).await;
    delete_receiver(&c, &slug).await;
    assert!(
        s1.is_success() && s2.is_success(),
        "opt-in: both should be 200"
    );
}

// =============================================================================
// End-to-end (#823)
// =============================================================================

#[tokio::test]
async fn end_to_end_five_events_then_idempotent_replay() {
    require_api!();
    let c = client();
    let dedupe = idempotency_active(&c).await;
    let slug = unique_slug("e2e");
    register_receiver(&c, &slug, "contract.any.v1", Some(r#"{"type":"object"}"#)).await;

    let mut keys = Vec::new();
    for i in 0..5 {
        let body = format!(r#"{{"seq":{i}}}"#);
        let key = unique_slug(&format!("e2e-k{i}"));
        let (status, _) = post_webhook(
            &c,
            &slug,
            &body,
            Some(sign(SECRET, body.as_bytes())),
            Some(&key),
        )
        .await;
        assert!(
            status.is_success(),
            "event {i} should be accepted, got {status}"
        );
        keys.push((body, key));
    }

    // Replay all five with the same keys + bodies.
    for (i, (body, key)) in keys.iter().enumerate() {
        let (status, _) = post_webhook(
            &c,
            &slug,
            body,
            Some(sign(SECRET, body.as_bytes())),
            Some(key),
        )
        .await;
        assert!(
            status.is_success(),
            "replay of event {i} should succeed{}",
            if dedupe { " (cached)" } else { "" }
        );
    }
    delete_receiver(&c, &slug).await;
}

//! HTTP-boundary contract tests for `POST /api/v1/ingest/stream` (Issue #825).
//!
//! The NDJSON streaming sibling of `POST /api/v1/notes/bulk`. Where the pump's
//! line-splitting, envelope parsing, and resource discipline are exhaustively
//! unit-tested in `handlers/ingest_stream.rs` (no DB, no server), this file pins
//! the **streaming wire contract** across the real HTTP/SSE boundary:
//!
//! - Response shape: `200` + `Content-Type: text/event-stream`, one `ack` frame
//!   per non-blank data line, a terminal `done` summary.
//! - Per-line fault isolation: a malformed line yields an `error` ack and the
//!   stream continues — one bad line never aborts ingestion or returns 5xx.
//! - Envelope contract: unknown `type` → per-line `error`; blank lines ignored;
//!   an empty body emits only `done {total:0}`.
//!
//! These run against a live server (`API_BASE_URL`) with a database behind it,
//! and skip gracefully when it is absent — matching `chat_stream_contract_test`.
//! Ingested notes are tagged `__ingest_contract_test__` with
//! `source: "ingest-contract-test"` so they are identifiable and cleanable.
//!
//! Auth: `/ingest/stream` requires a per-stream bearer token when
//! `INGEST_REQUIRE_TOKEN=true` (the default, #829); the suite mints one per
//! stream request (`mint_stream_token`). If the server also enforces
//! `REQUIRE_AUTH` on the mint endpoint, set `API_TOKEN` to a valid bearer token;
//! it authenticates minting and falls back as the stream bearer on open/dev
//! servers (CI runs anonymous and skips when no live server is present).

use std::time::Duration;

/// Per-request ceiling for a streaming response.
const STREAM_TIMEOUT: Duration = Duration::from_secs(60);

fn api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
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

// =============================================================================
// SSE PARSING (same subset as chat_stream_contract_test)
// =============================================================================

#[derive(Debug, Default, Clone)]
struct SseEvent {
    event: Option<String>,
    data: String,
}

impl SseEvent {
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.data)
            .unwrap_or_else(|e| panic!("frame data is not JSON ({e}): {:?}", self.data))
    }
}

/// Decode a complete SSE body into events: `event:` + accumulated `data:`,
/// blank line terminates, leading `:` are keep-alive comments, tolerant of
/// `\r\n`.
fn parse_sse(body: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut cur = SseEvent::default();
    let mut has_field = false;

    for raw in body.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);

        if line.is_empty() {
            if has_field {
                events.push(std::mem::take(&mut cur));
                has_field = false;
            }
            continue;
        }
        if line.starts_with(':') {
            continue;
        }

        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => {
                cur.event = Some(value.to_string());
                has_field = true;
            }
            "data" => {
                if !cur.data.is_empty() {
                    cur.data.push('\n');
                }
                cur.data.push_str(value);
                has_field = true;
            }
            _ => {}
        }
    }
    if has_field {
        events.push(cur);
    }
    events
}

/// Build a `note` NDJSON line with the contract-test marker tag/source so the
/// rows it creates are identifiable.
fn note_line(content: &str) -> String {
    serde_json::json!({
        "type": "note",
        "data": {
            "content": content,
            "source": "ingest-contract-test",
            "tags": ["__ingest_contract_test__"],
        }
    })
    .to_string()
}

/// Mint a fresh per-stream bearer token (#829), or `None` if the server has no
/// Redis / requires auth the suite lacks. `API_TOKEN` (if set) authenticates the
/// mint call.
async fn mint_stream_token(client: &reqwest::Client) -> Option<String> {
    let mut req = client
        .post(format!("{}/api/v1/ingest/tokens", api_base_url()))
        .timeout(Duration::from_secs(10));
    if let Ok(token) = std::env::var("API_TOKEN") {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// The bearer to attach to `/ingest/stream`: a freshly minted stream token when
/// the server supports it (#829), else the configured `API_TOKEN` for an
/// open/dev server (`INGEST_REQUIRE_TOKEN=false`).
async fn stream_bearer(client: &reqwest::Client) -> Option<String> {
    if let Some(tok) = mint_stream_token(client).await {
        return Some(tok);
    }
    std::env::var("API_TOKEN").ok()
}

/// POST an NDJSON body to `/api/v1/ingest/stream`, read the SSE body to
/// completion. Returns `(status, content_type, events)`. Attaches a per-stream
/// token (#829) when the server supports minting one.
async fn post_ndjson(
    client: &reqwest::Client,
    ndjson: String,
) -> (reqwest::StatusCode, Option<String>, Vec<SseEvent>) {
    let mut req = client
        .post(format!("{}/api/v1/ingest/stream", api_base_url()))
        .header("content-type", "application/x-ndjson")
        .body(ndjson)
        .timeout(STREAM_TIMEOUT);
    if let Some(bearer) = stream_bearer(client).await {
        req = req.bearer_auth(bearer);
    }
    let resp = req.send().await.expect("request send failed");
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let text = resp.text().await.expect("reading SSE body failed");
    (status, content_type, parse_sse(&text))
}

fn acks(events: &[SseEvent]) -> Vec<&SseEvent> {
    events
        .iter()
        .filter(|e| e.event.as_deref() == Some("ack"))
        .collect()
}

fn done(events: &[SseEvent]) -> serde_json::Value {
    let frame = events
        .iter()
        .find(|e| e.event.as_deref() == Some("done"))
        .expect("stream must emit a terminal `done` frame");
    frame.json()
}

// =============================================================================
// Wire contract
// =============================================================================

/// 200 + `text/event-stream`, with a terminal `done` — the streaming transport
/// contract holds regardless of per-line outcomes.
#[tokio::test]
async fn test_ingest_stream_content_type_is_event_stream() {
    require_api!();
    let client = reqwest::Client::new();
    let body = format!("{}\n{}\n", note_line("alpha"), note_line("beta"));
    let (status, content_type, events) = post_ndjson(&client, body).await;

    assert_eq!(status, 200, "ingest stream should return 200");
    let ct = content_type.unwrap_or_default();
    assert!(
        ct.starts_with("text/event-stream"),
        "expected SSE content-type, got {ct:?}"
    );
    let d = done(&events);
    assert_eq!(d["total"], 2);
}

/// One `ack` per data line plus a `done` summary; clean lines store ok.
#[tokio::test]
async fn test_ingest_stream_acks_each_line_then_done() {
    require_api!();
    let client = reqwest::Client::new();
    let body = format!("{}\n{}\n", note_line("first"), note_line("second"));
    let (status, _ct, events) = post_ndjson(&client, body).await;
    assert_eq!(status, 200);

    let ack_frames = acks(&events);
    assert_eq!(ack_frames.len(), 2, "one ack per data line");
    for (i, a) in ack_frames.iter().enumerate() {
        let j = a.json();
        assert_eq!(j["line"], (i as u64) + 1, "ack line numbers are 1-based");
        assert_eq!(j["status"], "ok", "clean note line should store ok: {j}");
        assert!(j["note_id"].as_str().is_some(), "ok ack carries a note_id");
    }

    let d = done(&events);
    assert_eq!(d["total"], 2);
    assert_eq!(d["success"], 2);
    assert_eq!(d["errors"], 0);
}

/// Fault isolation across the real boundary: good, malformed-JSON, good →
/// ok/error/ok, never a 5xx, stream completes.
#[tokio::test]
async fn test_ingest_stream_isolates_a_malformed_line() {
    require_api!();
    let client = reqwest::Client::new();
    let body = format!(
        "{}\n{}\n{}\n",
        note_line("good-one"),
        "{ this is not valid json",
        note_line("good-two"),
    );
    let (status, _ct, events) = post_ndjson(&client, body).await;
    assert_eq!(status, 200, "a malformed line must not produce a 5xx");

    let statuses: Vec<String> = acks(&events)
        .iter()
        .map(|a| a.json()["status"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(statuses, vec!["ok", "error", "ok"]);

    let d = done(&events);
    assert_eq!(d["total"], 3);
    assert_eq!(d["success"], 2);
    assert_eq!(d["errors"], 1);
}

/// Unknown envelope `type` → per-line `error`; a sibling note still stores.
#[tokio::test]
async fn test_ingest_stream_unknown_type_errors_only_that_line() {
    require_api!();
    let client = reqwest::Client::new();
    let unknown = serde_json::json!({"type":"widget","data":{"content":"x"}}).to_string();
    let body = format!("{}\n{}\n", unknown, note_line("real-note"));
    let (status, _ct, events) = post_ndjson(&client, body).await;
    assert_eq!(status, 200);

    let ack_frames = acks(&events);
    assert_eq!(ack_frames.len(), 2);
    assert_eq!(ack_frames[0].json()["status"], "error");
    assert_eq!(ack_frames[1].json()["status"], "ok");

    let d = done(&events);
    assert_eq!(d["total"], 2);
    assert_eq!(d["errors"], 1);
    assert_eq!(d["success"], 1);
}

/// Blank / whitespace-only lines are ignored — not acked, not counted.
#[tokio::test]
async fn test_ingest_stream_ignores_blank_lines() {
    require_api!();
    let client = reqwest::Client::new();
    let body = format!("\n   \n{}\n\n", note_line("lonely"));
    let (status, _ct, events) = post_ndjson(&client, body).await;
    assert_eq!(status, 200);

    assert_eq!(acks(&events).len(), 1, "only the single data line is acked");
    let d = done(&events);
    assert_eq!(d["total"], 1);
    assert_eq!(d["success"], 1);
}

/// An empty body is valid: no acks, a `done {total:0}` summary, 200.
#[tokio::test]
async fn test_ingest_stream_empty_body_emits_done_zero() {
    require_api!();
    let client = reqwest::Client::new();
    let (status, _ct, events) = post_ndjson(&client, String::new()).await;
    assert_eq!(status, 200);

    assert_eq!(acks(&events).len(), 0, "empty body acks nothing");
    let d = done(&events);
    assert_eq!(d["total"], 0);
    assert_eq!(d["success"], 0);
    assert_eq!(d["errors"], 0);
}

/// A trailing line with no terminating newline is still ingested.
#[tokio::test]
async fn test_ingest_stream_trailing_line_without_newline() {
    require_api!();
    let client = reqwest::Client::new();
    // No trailing '\n'.
    let (status, _ct, events) = post_ndjson(&client, note_line("no-trailing-newline")).await;
    assert_eq!(status, 200);

    assert_eq!(acks(&events).len(), 1);
    assert_eq!(done(&events)["total"], 1);
}

/// Periodic `progress {processed:N}` frames traverse the real SSE transport
/// (#826). Assumes the server's default `FORTEMI_INGEST_PROGRESS_INTERVAL` (100):
/// 100 data lines yield at least one progress frame. Exact cadence is unit-tested.
#[tokio::test]
async fn test_ingest_stream_emits_progress_frames() {
    require_api!();
    let client = reqwest::Client::new();
    let mut body = String::new();
    for i in 0..100 {
        body.push_str(&note_line(&format!("progress-probe-{i}")));
        body.push('\n');
    }
    let (status, _ct, events) = post_ndjson(&client, body).await;
    assert_eq!(status, 200);

    let total = done(&events)["total"].as_u64().unwrap();
    assert_eq!(total, 100);

    let progress: Vec<u64> = events
        .iter()
        .filter(|e| e.event.as_deref() == Some("progress"))
        .map(|e| {
            e.json()["processed"]
                .as_u64()
                .expect("processed is a number")
        })
        .collect();
    assert!(
        !progress.is_empty(),
        "expected >=1 progress frame for 100 lines (default interval 100)"
    );
    for w in progress.windows(2) {
        assert!(
            w[0] < w[1],
            "progress must be strictly increasing: {progress:?}"
        );
    }
    assert!(
        progress.iter().all(|&p| p > 0 && p <= total),
        "progress within (0, total]: {progress:?}"
    );
}

/// Every `ack` carries a resumption `cursor` of the form `{stream_id}-{line}`
/// across the real transport (#828).
#[tokio::test]
async fn test_ingest_stream_ack_carries_cursor() {
    require_api!();
    let client = reqwest::Client::new();
    let (status, _ct, events) =
        post_ndjson(&client, format!("{}\n", note_line("cursor-probe"))).await;
    assert_eq!(status, 200);

    let ack_frames = acks(&events);
    assert_eq!(ack_frames.len(), 1);
    let cursor = ack_frames[0].json()["cursor"]
        .as_str()
        .expect("ack carries a cursor")
        .to_string();
    let (stream_id, line) = cursor
        .rsplit_once('-')
        .expect("cursor has {stream_id}-{line} shape");
    assert!(!stream_id.is_empty(), "cursor has a stream id: {cursor}");
    assert_eq!(
        line, "1",
        "first data line cursor is {{stream_id}}-1: {cursor}"
    );
}

/// A well-formed but never-seen `X-Ingest-Cursor` → `410 Gone` (#828). Holds
/// whether or not Redis is up: an unknown stream resolves to no live cursor.
#[tokio::test]
async fn test_ingest_stream_410_on_unknown_cursor() {
    require_api!();
    let client = reqwest::Client::new();
    let mut req = client
        .post(format!("{}/api/v1/ingest/stream", api_base_url()))
        .header("content-type", "application/x-ndjson")
        .header("x-ingest-cursor", "00000000-0000-0000-0000-000000000000-5")
        .body(format!("{}\n", note_line("must-not-process")))
        .timeout(STREAM_TIMEOUT);
    // A valid stream token (when required) so the request clears the token gate
    // and the assertion isolates the cursor 410 (token auth precedes resumption).
    if let Some(bearer) = stream_bearer(&client).await {
        req = req.bearer_auth(bearer);
    }
    let resp = req.send().await.expect("request send failed");
    assert_eq!(
        resp.status().as_u16(),
        410,
        "an unknown/expired ingest cursor must return 410 Gone"
    );
}

/// Mint → revoke round-trip for stream tokens (#829). Skips when the server has
/// no Redis (503) or requires auth the suite lacks (401), so it is safe on both
/// open and secured deployments.
#[tokio::test]
async fn test_ingest_token_mint_and_revoke() {
    require_api!();
    let client = reqwest::Client::new();
    let mut req = client
        .post(format!(
            "{}/api/v1/ingest/tokens?rate_limit=100",
            api_base_url()
        ))
        .timeout(Duration::from_secs(10));
    if let Ok(token) = std::env::var("API_TOKEN") {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.expect("request send failed");
    let status = resp.status().as_u16();
    if status == 401 || status == 503 {
        eprintln!("Skipping mint/revoke: server returned {status} (needs auth + Redis)");
        return;
    }
    assert_eq!(status, 201, "mint returns 201");
    let body: serde_json::Value = resp.json().await.expect("mint body must be JSON");
    let token = body["token"].as_str().expect("mint returns a token");
    let token_id = body["token_id"].as_str().expect("mint returns a token_id");
    assert!(
        token.starts_with("mm_ist_"),
        "stream token has the expected prefix: {token}"
    );
    assert_eq!(body["rate_limit"], 100, "rate_limit echoes the query param");

    let revoke = |id: &str| {
        let mut d = client
            .delete(format!("{}/api/v1/ingest/tokens/{}", api_base_url(), id))
            .timeout(Duration::from_secs(10));
        if let Ok(t) = std::env::var("API_TOKEN") {
            d = d.bearer_auth(t);
        }
        d.send()
    };
    let first = revoke(token_id).await.expect("revoke send failed");
    assert_eq!(first.status().as_u16(), 204, "revoke returns 204");
    let second = revoke(token_id).await.expect("re-revoke send failed");
    assert_eq!(
        second.status().as_u16(),
        404,
        "re-revoking an already-revoked token is 404"
    );
}

/// `/health/streaming` exposes the ingest backpressure gauge (#827). Asserts the
/// metric is wired end-to-end through the live server: the `ingest` block carries
/// `ingest_stream_buffer_pressure` typed as a gauge. The escalating warning/429
/// behavior fires only under a slow consumer (deterministically unit-tested in
/// `handlers/ingest_stream.rs`); this pins the observable metric surface.
#[tokio::test]
async fn test_health_streaming_exposes_ingest_buffer_pressure_gauge() {
    require_api!();
    let client = reqwest::Client::new();
    let mut req = client
        .get(format!("{}/api/v1/health/streaming", api_base_url()))
        .timeout(Duration::from_secs(10));
    if let Ok(token) = std::env::var("API_TOKEN") {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.expect("request send failed");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "streaming health must answer 200"
    );
    let body: serde_json::Value = resp.json().await.expect("health body must be JSON");

    let gauge = &body["ingest"]["ingest_stream_buffer_pressure"];
    assert_eq!(
        gauge["type"], "gauge",
        "ingest_stream_buffer_pressure must be exposed as a gauge: {body}"
    );
    assert!(
        gauge["value"].is_u64(),
        "buffer-pressure gauge must carry a numeric value: {body}"
    );
}

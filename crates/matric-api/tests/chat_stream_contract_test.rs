//! HTTP-boundary contract tests for `POST /api/v1/chat/stream` (Issue #811, A5).
//!
//! The SSE-streamed sibling of `/chat` (#549). Where `chat_contract_test.rs`
//! pins the JSON request/response contract HotM consumes, this file pins the
//! **streaming** wire contract across the real HTTP/SSE boundary:
//!
//! - Request validation: empty / whitespace / missing / invalid JSON → 4xx,
//!   never 5xx (input sanitization).
//! - SSE response shape: `Content-Type: text/event-stream`, one `delta` frame
//!   per content chunk, a terminal `done` frame, monotonic `{session}-{seq}`
//!   event ids (#812, #815).
//! - Resumption (#815): a `Last-Event-ID` for an unknown / expired session
//!   replays a terminal `STREAM_INTERRUPTED` error rather than hanging — and a
//!   *malformed* `Last-Event-ID` must NOT bypass input validation.
//! - Backpressure metric (#814): `chat_stream_dropped_tokens_total` (and the
//!   sibling counters) are exposed on `/api/v1/health/streaming`.
//!
//! Pump-level framing, metric accounting, and resume-frame construction are
//! exhaustively unit-tested in `handlers/chat.rs` (#816); these tests assert
//! the same contract survives the real transport. They run against a live
//! server (`API_BASE_URL`) and skip gracefully when it is absent. Generation-
//! dependent cases additionally require Ollama; the resumption and metric
//! cases need only the server, since the resume path takes no GPU permit.

use std::time::Duration;

/// Per-request ceiling for a streaming response. The server closes the SSE
/// channel after the terminal frame, so a healthy stream completes well within
/// this; the bound only guards against a regression that leaves the stream open.
const STREAM_TIMEOUT: Duration = Duration::from_secs(120);

/// Get the API base URL for testing.
fn api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

/// Check if the API server is reachable and `API_BASE_URL` is explicitly set.
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

/// Check if the chat endpoint is configured (Ollama reachable).
async fn chat_available(client: &reqwest::Client) -> bool {
    let resp = client
        .get(format!("{}/health", api_base_url()))
        .send()
        .await
        .ok();
    if let Some(r) = resp {
        if let Ok(body) = r.json::<serde_json::Value>().await {
            return body
                .pointer("/capabilities/chat/available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
    }
    false
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

macro_rules! require_chat {
    ($client:expr) => {
        if !chat_available($client).await {
            eprintln!("Skipping: chat not available (Ollama not configured)");
            return;
        }
    };
}

// =============================================================================
// SSE PARSING
// =============================================================================

/// One decoded Server-Sent Event: its `event:` name, accumulated `data:`
/// payload, and optional `id:`.
#[derive(Debug, Default, Clone)]
struct SseEvent {
    event: Option<String>,
    data: String,
    id: Option<String>,
}

impl SseEvent {
    /// Parse the `data:` payload as JSON (every frame this endpoint emits is a
    /// JSON object).
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.data)
            .unwrap_or_else(|e| panic!("frame data is not JSON ({e}): {:?}", self.data))
    }
}

/// Decode a complete SSE response body into discrete events.
///
/// Implements the subset of the SSE grammar this endpoint uses: `event:`,
/// `data:` (accumulated across multiple lines per the spec), and `id:` fields,
/// with blank lines as event terminators. Comment / keep-alive lines (leading
/// `:`) are ignored. Tolerant of `\r\n` line endings.
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
            continue; // keep-alive heartbeat / comment
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
            "id" => {
                cur.id = Some(value.to_string());
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

/// POST to `/api/v1/chat/stream` and read the full SSE body to completion.
/// Returns the HTTP status, the `Content-Type` header, and the decoded events.
async fn post_stream(
    client: &reqwest::Client,
    body: serde_json::Value,
    last_event_id: Option<&str>,
) -> (reqwest::StatusCode, Option<String>, Vec<SseEvent>) {
    let mut req = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .json(&body)
        .timeout(STREAM_TIMEOUT);
    if let Some(id) = last_event_id {
        req = req.header("last-event-id", id);
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

// =============================================================================
// Request validation (sanitization) — no model required
// =============================================================================

/// Empty input returns 400 — parallels `/chat` test case #9.
#[tokio::test]
async fn test_chat_stream_empty_input_returns_400() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .json(&serde_json::json!({"input": ""}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "empty input should return 400");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.get("error").is_some(),
        "error response should contain 'error' field"
    );
}

/// Whitespace-only input returns 400.
#[tokio::test]
async fn test_chat_stream_whitespace_only_input_returns_400() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .json(&serde_json::json!({"input": "   \t\n  "}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "whitespace-only input should return 400"
    );
}

/// Missing `input` field returns 400/422 — parallels `/chat` test case #17.
#[tokio::test]
async fn test_chat_stream_missing_input_field_returns_error() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .json(&serde_json::json!({"context": {"note_id": "abc"}}))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 422,
        "missing input should return 400 or 422, got {status}"
    );
}

/// Invalid JSON body returns 400/422 — parallels `/chat` test case #16.
#[tokio::test]
async fn test_chat_stream_invalid_json_returns_error() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .header("content-type", "application/json")
        .body("{not valid json")
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 422,
        "invalid JSON should return 400 or 422, got {status}"
    );
}

/// Sanitization: a **malformed** `Last-Event-ID` (non-numeric sequence) must
/// fall through to normal request validation rather than silently entering the
/// resumption replay path. A bogus resume header must not smuggle an invalid
/// body past input validation — empty input still yields 400 (#815).
#[tokio::test]
async fn test_chat_stream_malformed_resume_cursor_does_not_bypass_validation() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        // `rfind('-')` splits to seq="cursor", which fails to parse as u64 →
        // ResumeCursor::parse returns None → handler takes the normal path.
        .header("last-event-id", "not-a-valid-cursor")
        .json(&serde_json::json!({"input": ""}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "a malformed Last-Event-ID must not bypass input validation"
    );
}

// =============================================================================
// Resumption contract (#815) — no model required (resume path takes no permit)
// =============================================================================

/// A `Last-Event-ID` for an unknown / expired session replays a terminal
/// `STREAM_INTERRUPTED` error frame and closes the stream — it never hangs and
/// never starts a fresh generation. This holds whether or not Ollama is up,
/// since the resume path acquires no GPU permit (#815).
#[tokio::test]
async fn test_chat_stream_resume_unknown_session_emits_interrupted() {
    require_api!();
    let client = reqwest::Client::new();
    // Well-formed cursor (`{uuid}-{seq}`) for a session that does not exist.
    let cursor = "00000000-0000-0000-0000-000000000000-0";
    let (status, content_type, events) = post_stream(
        &client,
        serde_json::json!({"input": "body is ignored on the resume path"}),
        Some(cursor),
    )
    .await;

    assert_eq!(status, 200, "resume path should return 200 SSE");
    assert!(
        content_type
            .as_deref()
            .unwrap_or_default()
            .contains("text/event-stream"),
        "resume response must be an SSE stream, got {content_type:?}"
    );
    assert!(
        !events.is_empty(),
        "resume stream must emit at least one frame"
    );

    let terminal = events.last().expect("non-empty");
    assert_eq!(
        terminal.event.as_deref(),
        Some("error"),
        "unknown-session resume must terminate with an error frame"
    );
    assert_eq!(
        terminal.json()["code"],
        "STREAM_INTERRUPTED",
        "expected STREAM_INTERRUPTED terminator, got {:?}",
        terminal.data
    );
}

// =============================================================================
// Backpressure metric (#814) — no model required
// =============================================================================

/// `/api/v1/health/streaming` exposes the chat-stream counters, including the
/// headline `chat_stream_dropped_tokens_total` backpressure metric (#814). Each
/// is a non-negative counter.
#[tokio::test]
async fn test_streaming_health_exposes_chat_stream_counters() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/v1/health/streaming", api_base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();

    let chat = body
        .get("chat")
        .expect("streaming health must include a 'chat' metrics block");

    for key in [
        "chat_stream_started_total",
        "chat_stream_completed_total",
        "chat_stream_errored_total",
        "chat_stream_client_disconnect_total",
        "chat_stream_tokens_total",
        "chat_stream_dropped_tokens_total",
    ] {
        let metric = chat
            .get(key)
            .unwrap_or_else(|| panic!("missing chat-stream counter '{key}'"));
        assert_eq!(metric["type"], "counter", "'{key}' should be a counter");
        assert!(
            metric["value"].as_u64().is_some(),
            "'{key}' value should be a non-negative integer, got {:?}",
            metric["value"]
        );
    }
}

// =============================================================================
// SSE happy path — require Ollama
// =============================================================================

/// A successful stream responds 200 with `Content-Type: text/event-stream`.
#[tokio::test]
async fn test_chat_stream_content_type_is_event_stream() {
    require_api!();
    let client = reqwest::Client::new();
    require_chat!(&client);

    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .json(&serde_json::json!({"input": "Say hi in one word."}))
        .timeout(STREAM_TIMEOUT)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        ct.contains("text/event-stream"),
        "expected SSE content-type, got '{ct}'"
    );
}

/// A clean stream emits one or more `delta` frames carrying `{"content": ...}`
/// and terminates with a `done` frame `{"finish_reason":"stop","model":...}`;
/// the concatenated deltas form non-empty assistant output (#812).
#[tokio::test]
async fn test_chat_stream_emits_delta_then_done() {
    require_api!();
    let client = reqwest::Client::new();
    require_chat!(&client);

    let (status, content_type, events) = post_stream(
        &client,
        serde_json::json!({"input": "Reply with the single word: hello"}),
        None,
    )
    .await;

    assert_eq!(status, 200);
    assert!(content_type
        .as_deref()
        .unwrap_or_default()
        .contains("text/event-stream"));
    assert!(!events.is_empty(), "stream produced no frames");

    let deltas: Vec<&SseEvent> = events
        .iter()
        .filter(|e| e.event.as_deref() == Some("delta"))
        .collect();
    assert!(
        !deltas.is_empty(),
        "stream must emit at least one delta frame"
    );

    // Terminal frame must be `done` with the documented payload.
    let terminal = events.last().unwrap();
    assert_eq!(
        terminal.event.as_deref(),
        Some("done"),
        "stream must terminate with a 'done' frame, got {:?}",
        terminal.event
    );
    let done = terminal.json();
    assert_eq!(done["finish_reason"], "stop");
    assert!(
        done.get("model").and_then(|m| m.as_str()).is_some(),
        "done frame must name the model"
    );

    // Reassembled content is non-empty.
    let reassembled: String = deltas
        .iter()
        .map(|e| {
            e.json()["content"]
                .as_str()
                .expect("delta frame must carry string content")
                .to_string()
        })
        .collect();
    assert!(
        !reassembled.trim().is_empty(),
        "concatenated delta content should be non-empty"
    );
}

/// Every emitted frame carries a monotonic `{session}-{seq}` SSE id so a
/// reconnecting client's `Last-Event-ID` resolves to a cursor (#815). The
/// session component is stable across the stream; the sequence increases.
#[tokio::test]
async fn test_chat_stream_events_carry_sequential_ids() {
    require_api!();
    let client = reqwest::Client::new();
    require_chat!(&client);

    let (status, _ct, events) = post_stream(
        &client,
        serde_json::json!({"input": "Reply with the single word: ok"}),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(!events.is_empty());

    let mut session: Option<String> = None;
    let mut last_seq: Option<u64> = None;
    for ev in &events {
        let id = ev
            .id
            .as_deref()
            .unwrap_or_else(|| panic!("frame {:?} is missing an SSE id", ev.event));
        let (sess, seq) = id
            .rsplit_once('-')
            .unwrap_or_else(|| panic!("id '{id}' is not of the form {{session}}-{{seq}}"));
        let seq: u64 = seq
            .parse()
            .unwrap_or_else(|_| panic!("id '{id}' has a non-numeric sequence"));

        match &session {
            None => session = Some(sess.to_string()),
            Some(s) => assert_eq!(s, sess, "session id must be stable across the stream"),
        }
        if let Some(prev) = last_seq {
            assert!(
                seq > prev,
                "sequence must strictly increase: {prev} -> {seq}"
            );
        }
        last_seq = Some(seq);
    }
    assert!(
        last_seq.is_some(),
        "at least one frame should carry a sequence"
    );
}

/// An uninstalled model slug is rejected with 400 before any generation — the
/// streaming path enforces the same model validation as `/chat`.
#[tokio::test]
async fn test_chat_stream_invalid_model_returns_400() {
    require_api!();
    let client = reqwest::Client::new();
    // Requires chat available so model discovery succeeds and can prove the
    // slug is not installed (discovery failure fails open by design).
    require_chat!(&client);

    let resp = client
        .post(format!("{}/api/v1/chat/stream", api_base_url()))
        .json(&serde_json::json!({
            "input": "hello",
            "model": "definitely-not-installed-model:0b"
        }))
        .timeout(STREAM_TIMEOUT)
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        400,
        "an uninstalled model should be rejected with 400"
    );
}

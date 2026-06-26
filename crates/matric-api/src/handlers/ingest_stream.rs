//! `POST /api/v1/ingest/stream` — NDJSON streaming bulk ingest (Issue #825).
//!
//! The streaming sibling of `POST /api/v1/notes/bulk`. The request body is an
//! NDJSON stream — one JSON envelope per line — consumed **incrementally**
//! (line-by-line, never buffered whole). Each line is stored in its own
//! transaction and acknowledged on a Server-Sent Events response, so a client
//! can stream millions of notes and watch them land one at a time.
//!
//! ## Wire contract
//!
//! Request: `Content-Type: application/x-ndjson`, chunked. Each non-blank line
//! is a typed envelope (forward-compatible with Phase D event sources, #832):
//!
//! ```json
//! {"type":"note","data":{"content":"...","tags":["a"],"title":"..."}}
//! ```
//!
//! `data` mirrors `CreateNoteRequest`; `content` is required, everything else is
//! optional (`format` defaults to `markdown`, `source` to `ingest-stream`).
//! Blank lines are ignored. Unknown `type` values, malformed JSON, empty
//! content, and schema-invalid fields (tag too long/deep, non-object metadata)
//! produce a per-line `error` ack — they never abort the stream.
//!
//! Response: `text/event-stream`:
//! - `ack` — `{"line":N,"status":"ok","note_id":"..."}` or
//!   `{"line":N,"status":"error","error":"..."}` per data line
//! - `progress` — `{"processed":N}` every `FORTEMI_INGEST_PROGRESS_INTERVAL`
//!   data lines (default 100)
//! - `done` — `{"total":N,"success":M,"errors":K}` terminal summary
//! - `error` — `{"error":"...","code":"INGEST_FATAL"}` only for a pre-stream
//!   fatal (e.g. an invalid archive schema)
//!
//! ## Scope (#825 foundation + #826 validation/progress + #828 resumption + #827 backpressure + #829 token auth + #830 outbox)
//!
//! Per line: parse → cheap DB-free schema validation ([`validate_note_data`]) →
//! `insert_tx` + `note.created` outbox row (same transaction, #830) → `ack`, with
//! periodic `progress` frames, per-ack cursor persistence for resumption,
//! escalating buffer-pressure backpressure, per-token rate limiting, and a
//! single post-stream search-cache invalidation so stored notes are
//! FTS-findable. Deliberately deferred:
//! - **NLP enrichment** (embeddings, AI title, linking) — streamed notes are
//!   stored and FTS-findable but NOT embedded/titled until a later reprocess.
//! - **Outbox idempotency-key dedup** — the per-line `note.created` row lands in
//!   `event_outbox` atomically with the note (#830), but the *strong*
//!   zero-duplicate-via-idempotency-key guarantee needs an outbox idempotency-key
//!   column not yet present (#592 follow-on). At-most-once already holds via the
//!   #828 cursor skip-ahead.
//!
//! ## Backpressure (#827)
//!
//! The pump→client SSE channel is a single bounded `mpsc` of capacity
//! `FORTEMI_INGEST_STREAM_BUFFER` (default 64). Three escalating tiers, sampled
//! before each `ack`:
//! - **≥80% full** → one `warning {message:'buffer high', advisory_rate}` frame
//!   (advisory; once per high episode, re-armed when pressure drops below 80%).
//! - **≥95% full** → one `error {status:429, retry_after_ms, code:'INGEST_BACKPRESSURE'}`
//!   frame — emitted while a slot still exists (a 429 cannot be pushed through a
//!   100%-full channel), so the client gets an explicit back-off signal.
//! - **100% full** → the blocking `ack` send stalls the pump, which stops
//!   reading the body → TCP backpressure on the upload (Tokio's default).
//!
//! Warning/429 frames are best-effort (`try_send`, never blocking) — they are
//! advisory; the load-bearing protection is the blocking `ack` send. Live
//! occupancy is published as the `ingest_stream_buffer_pressure` gauge on
//! `/health/streaming` ([`IngestStreamMetrics`]).
//!
//! ## Resumption (#828)
//!
//! Each stream has a `stream_id`; every `ack` carries cursor `{stream_id}-{line}`
//! and the last acked line is persisted to Redis ([`IngestCursorStore`]) with a
//! rolling 60s TTL. A client that drops can reconnect with
//! `X-Ingest-Cursor: {stream_id}-{N}`, re-send the body, and the server skips the
//! already-stored prefix by absolute line number (skip-ahead dedup), resuming
//! after the server's stored line; beyond the TTL → `410 Gone`. The server's
//! stored line — not the client's echoed value — is authoritative, so already
//! stored lines are never re-inserted (at-most-once). Strong zero-duplicate via
//! outbox idempotency-key rides on #830.
//!
//! ## Resource discipline
//!
//! Four pillars, mirrored from the `/chat/stream` pump (#811 A5):
//! 1. **Bounded buffer** — [`LineSplitter`] caps any single line at
//!    `FORTEMI_INGEST_MAX_LINE_BYTES` (default 1 MiB); a line with no newline
//!    cannot grow the buffer without bound.
//! 2. **Per-line fault isolation** — each line is its own
//!    [`SchemaContext::execute`] transaction; one failure rolls back only that
//!    line and yields an `error` ack while the stream continues.
//! 3. **No-leak pump** — [`pump_ingest_stream`] owns its `mpsc::Sender` and
//!    drops it on return, closing the SSE channel; nothing is stashed in a
//!    lingering task. The bounded channel applies backpressure: a full channel
//!    blocks the `ack` send → the pump stops reading the body (TCP backpressure),
//!    with escalating `warning`/`429` advisories below the ceiling ([`Backpressure`]).
//! 4. **Low complexity** — the byte state machine ([`LineSplitter`]) is pure
//!    and unit-tested in isolation from the async I/O and the DB.

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use matric_api::services::{IngestCursorStore, SearchCache};
use matric_core::CreateNoteRequest;
use matric_db::{PgNoteRepository, SchemaContext};

use crate::{ApiError, AppState, ArchiveContext, Auth};

/// Default per-line byte ceiling when `FORTEMI_INGEST_MAX_LINE_BYTES` is unset.
const DEFAULT_INGEST_MAX_LINE_BYTES: usize = 1024 * 1024;

/// Default bounded SSE frame channel capacity when `FORTEMI_INGEST_STREAM_BUFFER`
/// is unset (#827). Applies backpressure between the pump and the client (a slow
/// consumer cannot make the pump buffer without bound).
const DEFAULT_INGEST_STREAM_BUFFER: usize = 64;

/// Buffer-pressure escalation thresholds (#827), as a percent of channel
/// capacity. At/above `WARN` emit one `warning`; at/above `THROTTLE` emit one
/// `429` (still below 100%, so the frame is deliverable); at 100% the blocking
/// `ack` send is the real (TCP) backpressure.
const WARN_PRESSURE_PCT: u64 = 80;
const THROTTLE_PRESSURE_PCT: u64 = 95;

/// Default advisory send rate (lines/sec) suggested in a `warning` frame when
/// `FORTEMI_INGEST_ADVISORY_RATE` is unset (#827).
const DEFAULT_INGEST_ADVISORY_RATE: u64 = 1000;

/// Default `retry_after_ms` hint in a `429` frame when
/// `FORTEMI_INGEST_RETRY_AFTER_MS` is unset (#827).
const DEFAULT_INGEST_RETRY_AFTER_MS: u64 = 500;

/// SSE keep-alive interval (matches `/chat/stream`).
const INGEST_KEEPALIVE_SECS: u64 = 15;

/// Default `progress {processed:N}` cadence (#826) when
/// `FORTEMI_INGEST_PROGRESS_INTERVAL` is unset — emit one progress frame every
/// this many non-blank data lines.
const DEFAULT_INGEST_PROGRESS_INTERVAL: usize = 100;

/// Resolve the per-line byte cap from `FORTEMI_INGEST_MAX_LINE_BYTES`, falling
/// back to [`DEFAULT_INGEST_MAX_LINE_BYTES`]. A non-numeric or zero value falls
/// back rather than disabling the bound (the bound is a safety floor).
fn ingest_max_line_bytes() -> usize {
    std::env::var("FORTEMI_INGEST_MAX_LINE_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INGEST_MAX_LINE_BYTES)
}

/// Resolve the progress cadence from `FORTEMI_INGEST_PROGRESS_INTERVAL`, falling
/// back to [`DEFAULT_INGEST_PROGRESS_INTERVAL`]. Zero/non-numeric falls back
/// (a zero interval would mean "never", which the default avoids).
fn ingest_progress_interval() -> usize {
    std::env::var("FORTEMI_INGEST_PROGRESS_INTERVAL")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INGEST_PROGRESS_INTERVAL)
}

/// Resolve the SSE channel capacity from `FORTEMI_INGEST_STREAM_BUFFER` (#827),
/// falling back to [`DEFAULT_INGEST_STREAM_BUFFER`]. Zero/non-numeric falls back
/// (a zero-capacity channel would deadlock the pump).
fn ingest_stream_buffer() -> usize {
    std::env::var("FORTEMI_INGEST_STREAM_BUFFER")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INGEST_STREAM_BUFFER)
}

/// Resolve the advisory send rate (lines/sec) for `warning` frames from
/// `FORTEMI_INGEST_ADVISORY_RATE` (#827), defaulting to
/// [`DEFAULT_INGEST_ADVISORY_RATE`].
fn ingest_advisory_rate() -> u64 {
    std::env::var("FORTEMI_INGEST_ADVISORY_RATE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INGEST_ADVISORY_RATE)
}

/// Resolve the `retry_after_ms` back-off hint for `429` frames from
/// `FORTEMI_INGEST_RETRY_AFTER_MS` (#827), defaulting to
/// [`DEFAULT_INGEST_RETRY_AFTER_MS`].
fn ingest_retry_after_ms() -> u64 {
    std::env::var("FORTEMI_INGEST_RETRY_AFTER_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_INGEST_RETRY_AFTER_MS)
}

/// Whether `/ingest/stream` requires a valid stream bearer token (#829). Default
/// `true` (secure for shared deployments). Set `INGEST_REQUIRE_TOKEN=false` for a
/// single-user desktop sidecar / dev, where the endpoint then behaves as before
/// (no per-stream token, no per-token rate limit).
fn ingest_require_token() -> bool {
    std::env::var("INGEST_REQUIRE_TOKEN")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true)
}

/// Extract a `Bearer <token>` value from the `Authorization` header, if present
/// and well-formed (#829).
fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("authorization")?.to_str().ok()?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

// =============================================================================
// METRICS (#827 — surfaced on /health/streaming, mirrors ChatStreamMetrics)
// =============================================================================

/// Backpressure observability for the ingest stream (#827). The gauges track
/// buffer occupancy; the counters track escalation events. Process-lifetime,
/// shared across all in-flight streams via `Arc`, snapshotted on
/// `/health/streaming`.
#[derive(Debug, Default)]
pub struct IngestStreamMetrics {
    /// Last-sampled SSE channel occupancy, 0–100 (%). This is the
    /// `ingest_stream_buffer_pressure` gauge.
    pub buffer_pressure: AtomicU64,
    /// High-water occupancy seen this process lifetime, 0–100 (%).
    pub peak_buffer_pressure: AtomicU64,
    /// Total `warning {buffer high}` frames emitted (one per ≥80% episode).
    pub backpressure_warnings_total: AtomicU64,
    /// Total `429` backpressure frames emitted (one per ≥95% episode).
    pub throttled_total: AtomicU64,
    /// Total per-token rate-limit `429` frames emitted (one per throttle
    /// episode, #829).
    pub rate_limited_total: AtomicU64,
}

impl IngestStreamMetrics {
    /// Snapshot as a JSON object for `/health/streaming`.
    pub fn snapshot(&self) -> serde_json::Value {
        json!({
            "ingest_stream_buffer_pressure": {
                "type": "gauge",
                "value": self.buffer_pressure.load(Ordering::Relaxed)
            },
            "ingest_stream_buffer_pressure_peak": {
                "type": "gauge",
                "value": self.peak_buffer_pressure.load(Ordering::Relaxed)
            },
            "ingest_stream_backpressure_warnings_total": {
                "type": "counter",
                "value": self.backpressure_warnings_total.load(Ordering::Relaxed)
            },
            "ingest_stream_throttled_total": {
                "type": "counter",
                "value": self.throttled_total.load(Ordering::Relaxed)
            },
            "ingest_stream_rate_limited_total": {
                "type": "counter",
                "value": self.rate_limited_total.load(Ordering::Relaxed)
            },
        })
    }

    /// Record a fresh occupancy sample, advancing the peak gauge.
    fn record_pressure(&self, pct: u64) {
        self.buffer_pressure.store(pct, Ordering::Relaxed);
        self.peak_buffer_pressure.fetch_max(pct, Ordering::Relaxed);
    }
}

// =============================================================================
// SSE FRAMES
// =============================================================================

/// One SSE frame emitted by the ingest stream. Owns its data (`&'static str`
/// event name + `String` payload) — no shared or borrowed ownership, so a frame
/// stays valid independently of whatever produced it.
struct IngestFrame {
    event: &'static str,
    data: String,
}

/// Format a resumption cursor (`{stream_id}-{line}`) for an `ack` frame (#828).
fn cursor(stream_id: &str, line: u64) -> String {
    format!("{stream_id}-{line}")
}

impl IngestFrame {
    /// `ack` frame for a successfully stored line. Carries the resumption
    /// `cursor` (#828) the client echoes back via `X-Ingest-Cursor`.
    fn ack_ok(line: u64, note_id: Uuid, stream_id: &str) -> Self {
        Self {
            event: "ack",
            data: json!({
                "line": line,
                "status": "ok",
                "note_id": note_id,
                "cursor": cursor(stream_id, line),
            })
            .to_string(),
        }
    }

    /// `ack` frame for a line that failed (parse error, empty/invalid content, or
    /// a DB write failure). The stream continues.
    fn ack_error(line: u64, error: &str, stream_id: &str) -> Self {
        Self {
            event: "ack",
            data: json!({
                "line": line,
                "status": "error",
                "error": sanitize_ingest_ack_error(error),
                "cursor": cursor(stream_id, line),
            })
            .to_string(),
        }
    }

    /// Terminal `done` summary frame.
    fn done(stats: &IngestStats) -> Self {
        Self {
            event: "done",
            data: json!({
                "total": stats.total(),
                "success": stats.success,
                "errors": stats.errors,
            })
            .to_string(),
        }
    }

    /// Periodic `progress {processed:N}` counter frame (#826), emitted every
    /// `FORTEMI_INGEST_PROGRESS_INTERVAL` non-blank data lines. `processed`
    /// counts all acked data lines so far (success + error).
    fn progress(processed: u64) -> Self {
        Self {
            event: "progress",
            data: json!({ "processed": processed }).to_string(),
        }
    }

    /// Pre-stream fatal frame (e.g. the archive schema could not be resolved).
    /// No `ack`s are emitted in this case.
    fn fatal(error: &str) -> Self {
        Self {
            event: "error",
            data: json!({ "error": sanitize_ingest_fatal_error(error), "code": "INGEST_FATAL" })
                .to_string(),
        }
    }

    /// Buffer-pressure `warning` (#827), emitted once per ≥80% episode.
    /// Advisory: the client should slow its upload toward `advisory_rate`
    /// lines/sec. The stream continues normally.
    fn warning(advisory_rate: u64) -> Self {
        Self {
            event: "warning",
            data: json!({ "message": "buffer high", "advisory_rate": advisory_rate }).to_string(),
        }
    }

    /// Buffer-pressure `429` (#827), emitted once per ≥95% episode while a slot
    /// still exists (it cannot ride a 100%-full channel). The client is expected
    /// to back off for `retry_after_ms`; the stream is not aborted (a 100% buffer
    /// then applies real TCP backpressure via the blocking `ack` send).
    fn throttle(retry_after_ms: u64) -> Self {
        Self {
            event: "error",
            data: json!({
                "error": "ingest buffer full; back off",
                "status": 429,
                "retry_after_ms": retry_after_ms,
                "code": "INGEST_BACKPRESSURE",
            })
            .to_string(),
        }
    }

    /// Per-token rate-limit `429` (#829), emitted once per throttle episode when
    /// the stream's lines/sec ceiling is hit. Distinct from the buffer-pressure
    /// `429` (`INGEST_BACKPRESSURE`): here the pump *paces* (sleeps) to the token's
    /// rate rather than the client being too slow.
    fn rate_limited(retry_after_ms: u64) -> Self {
        Self {
            event: "error",
            data: json!({
                "error": "per-token rate limit exceeded; pacing to allowed rate",
                "status": 429,
                "retry_after_ms": retry_after_ms,
                "code": "INGEST_RATE_LIMITED",
            })
            .to_string(),
        }
    }

    fn into_event(self) -> Event {
        Event::default().event(self.event).data(self.data)
    }
}

fn sanitize_ingest_ack_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.starts_with("invalid ingest line") {
        "invalid ingest line".to_string()
    } else if lower == "note content must not be empty"
        || lower == "metadata must be a json object"
        || lower.starts_with("tag exceeds ")
        || (lower.starts_with("line exceeds ") && lower.ends_with(" byte limit"))
    {
        error.to_string()
    } else {
        "ingest line could not be stored. Check server logs for diagnostics.".to_string()
    }
}

fn sanitize_ingest_fatal_error(_error: &str) -> String {
    "ingest stream could not be initialized. Check server logs for diagnostics.".to_string()
}

/// Running per-stream counters. `total == success + errors` (blank lines are not
/// counted).
#[derive(Default)]
struct IngestStats {
    /// Number of non-blank data lines processed so far (1-based line numbers in
    /// `ack` frames track this).
    line_no: u64,
    success: u64,
    errors: u64,
}

impl IngestStats {
    fn total(&self) -> u64 {
        self.success + self.errors
    }
}

// =============================================================================
// LINE PARSING — typed envelope (#825 Q2: forward-compatible with #832)
// =============================================================================

/// A single NDJSON ingest line. Adjacently tagged (`{"type":"...","data":{...}}`)
/// so Phase D (#832) can add `event` variants without breaking the `note`
/// contract. Unknown `type` values deserialize to an error, surfaced as a
/// per-line `error` ack.
#[derive(Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum IngestLine {
    Note(IngestNoteData),
}

/// The `data` payload of a `note` line — the subset of `CreateNoteRequest` the
/// store-only foundation honors. NLP-pipeline knobs (`revision_mode`,
/// `document_type` slug, chunking, model, pipeline) are intentionally absent:
/// they are no-ops until enrichment lands (#826), and serde ignores any extra
/// fields a client sends.
#[derive(Deserialize)]
struct IngestNoteData {
    content: String,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    collection_id: Option<Uuid>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    document_type_id: Option<Uuid>,
    #[serde(default)]
    title: Option<String>,
}

/// Parse one NDJSON line into a [`CreateNoteRequest`]. Pure and DB-free so the
/// parse contract is unit-testable without a server.
fn parse_ingest_line(raw: &[u8]) -> Result<CreateNoteRequest, String> {
    let line: IngestLine =
        serde_json::from_slice(raw).map_err(|e| format!("invalid ingest line: {e}"))?;
    match line {
        IngestLine::Note(n) => build_note_request(n),
    }
}

fn build_note_request(n: IngestNoteData) -> Result<CreateNoteRequest, String> {
    if n.content.trim().is_empty() {
        return Err("note content must not be empty".to_string());
    }
    validate_note_data(&n)?;
    Ok(CreateNoteRequest {
        content: n.content,
        format: n.format.unwrap_or_else(|| "markdown".to_string()),
        source: n.source.unwrap_or_else(|| "ingest-stream".to_string()),
        collection_id: n.collection_id,
        tags: n.tags,
        metadata: n.metadata,
        document_type_id: n.document_type_id,
        title: n.title,
    })
}

/// Cheap, DB-free per-line schema validation (#826): the same tag depth/length
/// limits `POST /api/v1/notes` enforces, plus a structural check that `metadata`
/// (when present) is a JSON object. No referential lookups (document_type slug,
/// collection existence) — those stay deferred; `document_type_id` is accepted
/// as a UUID only. Pure, so the validation contract is unit-testable.
fn validate_note_data(n: &IngestNoteData) -> Result<(), String> {
    if let Some(tags) = &n.tags {
        for tag in tags {
            if tag.len() > matric_core::defaults::TAG_NAME_MAX_LENGTH {
                return Err(format!(
                    "tag exceeds {} character limit",
                    matric_core::defaults::TAG_NAME_MAX_LENGTH
                ));
            }
            let depth = tag
                .split('/')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .count();
            if depth > matric_core::tags::MAX_TAG_PATH_DEPTH {
                return Err(format!(
                    "tag exceeds maximum depth of {} levels",
                    matric_core::tags::MAX_TAG_PATH_DEPTH
                ));
            }
        }
    }
    if let Some(metadata) = &n.metadata {
        if !metadata.is_object() && !metadata.is_null() {
            return Err("metadata must be a JSON object".to_string());
        }
    }
    Ok(())
}

/// Parse an `X-Ingest-Cursor` of the form `{stream_id}-{line}` (#828). The
/// `stream_id` is a UUID (which itself contains hyphens), so the split is on the
/// last hyphen. Returns `None` for any value that does not match this shape.
fn parse_ingest_cursor(value: &str) -> Option<(String, u64)> {
    let idx = value.rfind('-')?;
    let stream_id = &value[..idx];
    let line_str = &value[idx + 1..];
    if stream_id.is_empty() || line_str.is_empty() {
        return None;
    }
    let line = line_str.parse::<u64>().ok()?;
    Some((stream_id.to_string(), line))
}

// =============================================================================
// BOUNDED NDJSON LINE SPLITTER (pillar 1)
// =============================================================================

/// A complete line surfaced by [`LineSplitter`].
enum LineEvent {
    /// A complete line (newline + any trailing `\r` stripped), within the cap.
    /// May be empty (a blank line); the pump skips blanks.
    Line(Vec<u8>),
    /// A line that exceeded the byte cap and was rejected.
    Overflow,
}

/// Incremental NDJSON splitter with a hard per-line byte cap. Pure and sync — it
/// owns only a byte buffer, so it is unit-testable without any I/O and keeps the
/// async pump simple.
///
/// A line whose bytes exceed `max` is rejected as [`LineEvent::Overflow`]; if the
/// over-length line has not yet terminated, the splitter enters a discard mode
/// (clearing the buffer each chunk) until the next newline, so the buffer can
/// never grow without bound.
struct LineSplitter {
    buf: Vec<u8>,
    max: usize,
    /// True while discarding the tail of an over-length, not-yet-terminated line.
    overflow: bool,
}

impl LineSplitter {
    fn new(max: usize) -> Self {
        Self {
            buf: Vec::new(),
            max,
            overflow: false,
        }
    }

    /// Feed a body chunk; return every complete line-event it produced.
    fn push(&mut self, chunk: &[u8]) -> Vec<LineEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        loop {
            match self.buf.iter().position(|&b| b == b'\n') {
                Some(nl) => {
                    let line = self.drain_line(nl);
                    if self.overflow {
                        // This was the tail of an over-length line already
                        // reported as Overflow — discard it and resume.
                        self.overflow = false;
                    } else {
                        push_line(&mut out, line, self.max);
                    }
                }
                None => {
                    self.handle_no_newline(&mut out);
                    break;
                }
            }
        }
        out
    }

    /// Flush a trailing line that had no terminating newline at end-of-body.
    fn finish(mut self) -> Option<LineEvent> {
        if self.overflow || self.buf.is_empty() {
            return None;
        }
        if self.buf.len() > self.max {
            Some(LineEvent::Overflow)
        } else {
            Some(LineEvent::Line(std::mem::take(&mut self.buf)))
        }
    }

    /// Drain `buf[..=nl]` (the line plus its `\n`), returning the line with the
    /// `\n` and any trailing `\r` removed.
    fn drain_line(&mut self, nl: usize) -> Vec<u8> {
        let mut line: Vec<u8> = self.buf.drain(..=nl).collect();
        line.pop(); // '\n'
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        line
    }

    /// No newline in the buffer: either start discarding an over-length line, or
    /// keep buffering a partial line still under the cap.
    fn handle_no_newline(&mut self, out: &mut Vec<LineEvent>) {
        if self.overflow {
            // Still inside an over-length line — keep dropping bytes.
            self.buf.clear();
        } else if self.buf.len() > self.max {
            out.push(LineEvent::Overflow);
            self.buf.clear();
            self.overflow = true;
        }
        // else: partial line under the cap — keep buffering for the next chunk.
    }
}

/// Classify a fully-formed line by length: a completed line longer than the cap
/// is still an overflow.
fn push_line(out: &mut Vec<LineEvent>, line: Vec<u8>, max: usize) {
    if line.len() > max {
        out.push(LineEvent::Overflow);
    } else {
        out.push(LineEvent::Line(line));
    }
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(u8::is_ascii_whitespace)
}

// =============================================================================
// NOTE SINK (pillar 2 — per-line transaction isolation)
// =============================================================================

/// Stores one parsed note. An abstraction over the DB write so the pump's
/// resource discipline is testable without a database (the production impl is
/// [`DbNoteSink`]; tests use an in-memory mock).
///
/// `#[async_trait]` (matric-core's convention) boxes the returned future with a
/// `Send` bound, which the pump needs to run inside `tokio::spawn`.
#[async_trait]
trait NoteSink: Send + Sync {
    async fn store(&self, req: CreateNoteRequest) -> Result<Uuid, String>;
}

/// Production sink: each `store` is its own archive-scoped transaction
/// (`SET LOCAL search_path` + `INSERT` + outbox `INSERT` + `COMMIT`), so a single
/// failed line rolls back only itself — note row and outbox row together (#830).
struct DbNoteSink {
    ctx: SchemaContext,
    pool: PgPool,
    /// Memory name recorded on the outbox row for event scoping (#452); `None`
    /// for the fallback public schema.
    memory: Option<String>,
}

impl DbNoteSink {
    fn new(pool: PgPool, schema: String, memory: Option<String>) -> Result<Self, String> {
        let ctx = SchemaContext::new(pool.clone(), schema).map_err(|e| e.to_string())?;
        Ok(Self { ctx, pool, memory })
    }
}

#[async_trait]
impl NoteSink for DbNoteSink {
    async fn store(&self, req: CreateNoteRequest) -> Result<Uuid, String> {
        let notes = PgNoteRepository::new(self.pool.clone());
        let memory = self.memory.clone();
        self.ctx
            .execute(move |tx| {
                Box::pin(async move {
                    // Insert the note, then append a `note.created` outbox row in
                    // the SAME transaction (#830). `execute` commits once at the
                    // end, so any error here rolls back both — no partial state.
                    let note_id = notes.insert_tx(tx, req).await?;
                    matric_db::PgEventOutboxRepository::emit_event_tx(
                        tx,
                        matric_db::CreateOutboxEvent::new(
                            "note.created",
                            "note",
                            note_id,
                            json!({ "note_id": note_id, "source": "ingest-stream" }),
                            memory,
                        ),
                    )
                    .await?;
                    Ok(note_id)
                })
            })
            .await
            .map_err(|e| e.to_string())
    }
}

// =============================================================================
// BACKPRESSURE (#827 — escalating warning → 429 → TCP, single bounded channel)
// =============================================================================

/// Current SSE channel occupancy as a percent (0–100) of `buffer`. `buffer` is
/// the channel's configured capacity (there is no `Sender` API for the maximum,
/// so it is passed in); `tx.capacity()` is the live count of *free* permits.
fn sample_pressure(tx: &mpsc::Sender<IngestFrame>, buffer: usize) -> u64 {
    let buffer = (buffer.max(1)) as u64;
    let available = tx.capacity() as u64;
    let used = buffer.saturating_sub(available);
    (used * 100) / buffer
}

/// Escalating backpressure state across one stream (#827). Sampled before each
/// `ack`; emits at most one advisory frame per high episode and re-arms when
/// pressure falls back below the warning threshold.
struct Backpressure {
    /// Channel capacity (== `FORTEMI_INGEST_STREAM_BUFFER`), the pressure divisor.
    buffer: usize,
    advisory_rate: u64,
    retry_after_ms: u64,
    metrics: Arc<IngestStreamMetrics>,
    /// A `warning` has been emitted for the current ≥80% episode.
    warned: bool,
    /// A `429` has been emitted for the current ≥95% episode.
    throttled: bool,
}

impl Backpressure {
    fn new(
        buffer: usize,
        advisory_rate: u64,
        retry_after_ms: u64,
        metrics: Arc<IngestStreamMetrics>,
    ) -> Self {
        Self {
            buffer,
            advisory_rate,
            retry_after_ms,
            metrics,
            warned: false,
            throttled: false,
        }
    }

    /// Sample occupancy, update the gauge, and emit one escalating advisory
    /// (`warning` at ≥80%, `429` at ≥95%) per high episode. Control frames are
    /// best-effort (`try_send`, never blocking): they are advisory and a momentary
    /// full channel simply drops them. The real backpressure is the caller's
    /// blocking `ack` send when the channel is 100% full. The flat `else if`
    /// chain keeps a single decision level (no nested branches): a 429 also sets
    /// `warned` so the warning tier does not re-fire under it, and the reset arm
    /// only runs strictly below the warning threshold.
    fn observe(&mut self, tx: &mpsc::Sender<IngestFrame>) {
        let pct = sample_pressure(tx, self.buffer);
        self.metrics.record_pressure(pct);
        if pct < WARN_PRESSURE_PCT {
            self.warned = false;
            self.throttled = false;
        } else if pct >= THROTTLE_PRESSURE_PCT && !self.throttled {
            self.throttled = true;
            self.warned = true;
            self.metrics.throttled_total.fetch_add(1, Ordering::Relaxed);
            let _ = tx.try_send(IngestFrame::throttle(self.retry_after_ms));
        } else if !self.warned {
            self.warned = true;
            self.metrics
                .backpressure_warnings_total
                .fetch_add(1, Ordering::Relaxed);
            let _ = tx.try_send(IngestFrame::warning(self.advisory_rate));
        }
    }
}

// =============================================================================
// PER-TOKEN RATE LIMIT (#829 — lines/sec pacing inside the pump)
// =============================================================================

/// Per-stream lines/sec limiter (token bucket); `lps == 0` means unlimited. One
/// instance per pump, seeded from the validated stream token's rate limit. It
/// paces data-line inserts to the allowed rate by *sleeping* rather than
/// dropping — no data is lost, only throttled.
struct RateLimiter {
    /// Allowed lines per second (0.0 = unlimited).
    lps: f64,
    /// Currently available whole-line permits (token-bucket allowance).
    allowance: f64,
    /// Last refill instant.
    last: Instant,
    /// `retry_after_ms` advisory carried in the rate-limit `429`.
    retry_after_ms: u64,
    /// A rate-limit `429` has been emitted for the current throttle episode.
    throttled: bool,
}

impl RateLimiter {
    /// `rate_limit` is lines/sec (0 = unlimited). The bucket starts full, so a
    /// short burst up to `rate_limit` lines is allowed before pacing begins.
    fn new(rate_limit: u64, retry_after_ms: u64) -> Self {
        let lps = rate_limit as f64;
        Self {
            lps,
            allowance: lps,
            last: Instant::now(),
            retry_after_ms,
            throttled: false,
        }
    }

    fn unlimited(&self) -> bool {
        self.lps <= 0.0
    }

    /// Acquire one line-permit, sleeping until the rate allows it. Returns `true`
    /// exactly when this acquisition *starts* a new throttle episode (so the
    /// caller emits one `INGEST_RATE_LIMITED` advisory per episode); `false`
    /// otherwise, including the unlimited case.
    async fn acquire(&mut self) -> bool {
        if self.unlimited() {
            return false;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.allowance = (self.allowance + elapsed * self.lps).min(self.lps);
        if self.allowance >= 1.0 {
            self.allowance -= 1.0;
            self.throttled = false; // back within rate
            return false;
        }
        // Under budget: sleep for the next whole permit to accrue.
        let wait = (1.0 - self.allowance) / self.lps;
        let episode_start = !self.throttled;
        self.throttled = true;
        sleep(Duration::from_secs_f64(wait)).await;
        self.allowance = 0.0;
        episode_start
    }
}

/// Whether this line-event performs a data-line insert and therefore consumes a
/// rate-limit permit (#829): a non-blank line not below the resume skip boundary.
/// Overflow and blank/skipped lines are cheap and not rate-limited. The line's
/// absolute number is `stats.line_no + 1` (it has not been incremented yet).
fn consumes_permit(ev: &LineEvent, stats: &IngestStats, skip_boundary: u64) -> bool {
    matches!(ev, LineEvent::Line(b) if !is_blank(b)) && stats.line_no + 1 > skip_boundary
}

/// Mutable per-stream controllers threaded through the pump: buffer backpressure
/// (#827) and per-token rate limiting (#829). Bundled so `step` keeps a small
/// argument list (≤7).
struct StreamControls {
    bp: Backpressure,
    rate: RateLimiter,
}

// =============================================================================
// PUMP (pillar 3 — no-leak; pillar 4 — low complexity via decomposition)
// =============================================================================

/// Drive the NDJSON body through the splitter and sink, emitting one frame per
/// data line and a terminal `done`. Owns `tx`; dropping it on return closes the
/// SSE channel. Stops early (without the cache invalidation or `done`) if the
/// client disconnects — a dropped receiver makes `tx.send` fail.
/// Per-stream pump configuration. Bundled so the per-line step keeps a small,
/// readable signature.
struct PumpConfig {
    /// Resumption stream id; each `ack` carries cursor `{stream_id}-{line}`.
    stream_id: String,
    /// Per-line byte cap (pillar 1).
    max_line_bytes: usize,
    /// Skip non-blank data lines whose absolute number is ≤ this (already
    /// processed on a prior connection; 0 for a fresh stream) (#828).
    skip_boundary: u64,
    /// `progress` cadence in data lines (0 disables) (#826).
    progress_interval: u64,
}

async fn pump_ingest_stream<B, N>(
    mut body: B,
    tx: mpsc::Sender<IngestFrame>,
    sink: N,
    search_cache: SearchCache,
    cursor_store: IngestCursorStore,
    cfg: PumpConfig,
    mut controls: StreamControls,
) where
    B: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
    N: NoteSink,
{
    let mut splitter = LineSplitter::new(cfg.max_line_bytes);
    let mut stats = IngestStats::default();

    while let Some(chunk) = body.next().await {
        // A transport error (client disconnect, truncated body) ends ingestion;
        // already-acked lines stand.
        let Ok(bytes) = chunk else { break };
        for ev in splitter.push(&bytes) {
            if step(
                ev,
                &sink,
                &tx,
                &mut stats,
                &cursor_store,
                &cfg,
                &mut controls,
            )
            .await
            .is_break()
            {
                return; // client gone — abandon without cache work or `done`
            }
        }
    }

    if let Some(ev) = splitter.finish() {
        let _ = step(
            ev,
            &sink,
            &tx,
            &mut stats,
            &cursor_store,
            &cfg,
            &mut controls,
        )
        .await;
    }

    // Single post-stream invalidation so stored notes appear in FTS results
    // (skipped when nothing was stored).
    if stats.success > 0 {
        search_cache.invalidate_all().await;
    }

    let _ = tx.send(IngestFrame::done(&stats)).await;
    // `tx` drops here — the SSE channel closes and the client's stream ends.
}

/// Process one line-event end to end: build its frame (or skip), send it,
/// persist the resumption cursor, and emit a periodic `progress`. Returns
/// `Break` only when the receiver is gone (client disconnected).
async fn step<N: NoteSink>(
    ev: LineEvent,
    sink: &N,
    tx: &mpsc::Sender<IngestFrame>,
    stats: &mut IngestStats,
    cursor_store: &IngestCursorStore,
    cfg: &PumpConfig,
    controls: &mut StreamControls,
) -> ControlFlow<()> {
    let before = stats.total();
    // Per-token rate limit (#829): pace data-line inserts to the token's
    // lines/sec by sleeping; emit one rate-limit `429` per throttle episode.
    if consumes_permit(&ev, stats, cfg.skip_boundary) && controls.rate.acquire().await {
        controls
            .bp
            .metrics
            .rate_limited_total
            .fetch_add(1, Ordering::Relaxed);
        let _ = tx.try_send(IngestFrame::rate_limited(controls.rate.retry_after_ms));
    }
    // Blank lines and already-processed (skipped) lines yield no frame.
    let Some(frame) = handle_event(ev, sink, stats, cfg).await else {
        return ControlFlow::Continue(());
    };
    // Sample buffer pressure and emit any escalating advisory (#827) *before*
    // the blocking ack send — so a `warning`/`429` is delivered while a slot
    // still exists; a 100%-full channel then blocks here (TCP backpressure).
    controls.bp.observe(tx);
    if tx.send(frame).await.is_err() {
        return ControlFlow::Break(());
    }
    // Persist the cursor (last acked absolute line) + refresh the TTL so a
    // reconnect within the window resumes after it (#828).
    cursor_store.record(&cfg.stream_id, stats.line_no).await;
    maybe_progress(tx, before, stats, cfg.progress_interval).await
}

/// Build the `ack` frame for one line-event, advancing the counters — or `None`
/// when the line is blank or already-processed (≤ `skip_boundary`, skipped on a
/// resumed connection, #828). Blank lines never advance the line number.
async fn handle_event<N: NoteSink>(
    ev: LineEvent,
    sink: &N,
    stats: &mut IngestStats,
    cfg: &PumpConfig,
) -> Option<IngestFrame> {
    match ev {
        LineEvent::Line(bytes) if is_blank(&bytes) => None,
        LineEvent::Line(bytes) => {
            stats.line_no += 1;
            if stats.line_no <= cfg.skip_boundary {
                return None; // already processed on a prior connection
            }
            Some(process_line(&bytes, sink, stats, &cfg.stream_id).await)
        }
        LineEvent::Overflow => {
            stats.line_no += 1;
            if stats.line_no <= cfg.skip_boundary {
                return None;
            }
            stats.errors += 1;
            Some(IngestFrame::ack_error(
                stats.line_no,
                &format!("line exceeds {} byte limit", cfg.max_line_bytes),
                &cfg.stream_id,
            ))
        }
    }
}

/// Emit a `progress` frame iff this line advanced the counter onto a positive
/// multiple of `interval` — so blank/skipped lines (which don't change the
/// total) never re-fire a progress frame. `Break` if the receiver is gone (#826).
async fn maybe_progress(
    tx: &mpsc::Sender<IngestFrame>,
    before: u64,
    stats: &IngestStats,
    interval: u64,
) -> ControlFlow<()> {
    let total = stats.total();
    // The `send` short-circuits: it runs only at a progress point, and a failed
    // send (receiver gone) breaks the pump.
    if interval > 0
        && total != before
        && total.is_multiple_of(interval)
        && tx.send(IngestFrame::progress(total)).await.is_err()
    {
        return ControlFlow::Break(());
    }
    ControlFlow::Continue(())
}

/// Parse + validate + store one non-blank, non-skipped line at `stats.line_no`,
/// advancing success/error counters and producing its `ack` frame.
async fn process_line<N: NoteSink>(
    raw: &[u8],
    sink: &N,
    stats: &mut IngestStats,
    stream_id: &str,
) -> IngestFrame {
    let line = stats.line_no;
    match parse_ingest_line(raw) {
        Ok(req) => match sink.store(req).await {
            Ok(note_id) => {
                stats.success += 1;
                IngestFrame::ack_ok(line, note_id, stream_id)
            }
            Err(e) => {
                stats.errors += 1;
                IngestFrame::ack_error(line, &e, stream_id)
            }
        },
        Err(e) => {
            stats.errors += 1;
            IngestFrame::ack_error(line, &e, stream_id)
        }
    }
}

// =============================================================================
// HANDLER
// =============================================================================

/// POST /api/v1/ingest/stream — NDJSON streaming bulk ingest over SSE (#825).
///
/// Reads the request body line-by-line (never buffering it whole), stores each
/// `note` line in its own transaction, and streams an `ack` per line plus a
/// terminal `done` summary. See the module docs for the full contract and the
/// store-only scope.
#[utoipa::path(
    post,
    path = "/api/v1/ingest/stream",
    tag = "Ingest",
    request_body(
        content = String,
        description = "NDJSON: one `{\"type\":\"note\",\"data\":{...}}` envelope per line",
        content_type = "application/x-ndjson",
    ),
    responses(
        (status = 200, description = "SSE stream: one `ack` (with `cursor`) per line, periodic `progress`, terminal `done` summary"),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 410, description = "X-Ingest-Cursor expired/unknown — start a fresh stream"),
    ),
    params(
        ("X-Ingest-Cursor" = Option<String>, Header, description = "Resume cursor `{stream_id}-{line}` from a prior ack (60s TTL)"),
    )
)]
pub async fn ingest_stream_handler(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let cursor_store = state.ingest_cursor_store.clone();
    let retry_after_ms = ingest_retry_after_ms();

    // Per-stream bearer token (#829): this route does its own inline auth (like
    // the SSE event stream). A valid stream token binds the write to *its* mint-
    // time archive schema and its lines/sec rate limit; with no/invalid token we
    // fail closed (401) unless `INGEST_REQUIRE_TOKEN=false`.
    let (schema, rate_limit) = match resolve_stream_token(&headers, &state, &archive_ctx).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    // Resolve resumption (#828): a fresh stream gets a new id and skip=0; a valid
    // `X-Ingest-Cursor` within the TTL resumes after the server's stored line; an
    // unknown/expired/malformed cursor short-circuits to 410 Gone.
    let (stream_id, skip_boundary) = match resolve_resumption(&headers, &cursor_store).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let buffer = ingest_stream_buffer();
    let (tx, rx) = mpsc::channel::<IngestFrame>(buffer);
    let pool = state.db.pool.clone();
    // Memory name for the outbox `note.created` rows (#830/#452); the write
    // schema is the token's bound schema (#829), independent of this label.
    let memory = archive_ctx.name.clone();
    let search_cache = state.search_cache.clone();
    let controls = StreamControls {
        bp: Backpressure::new(
            buffer,
            ingest_advisory_rate(),
            retry_after_ms,
            state.ingest_stream_metrics.clone(),
        ),
        rate: RateLimiter::new(rate_limit, retry_after_ms),
    };
    let cfg = PumpConfig {
        stream_id,
        max_line_bytes: ingest_max_line_bytes(),
        skip_boundary,
        progress_interval: ingest_progress_interval() as u64,
    };

    tokio::spawn(async move {
        match DbNoteSink::new(pool, schema, memory) {
            Ok(sink) => {
                pump_ingest_stream(
                    body.into_data_stream(),
                    tx,
                    sink,
                    search_cache,
                    cursor_store,
                    cfg,
                    controls,
                )
                .await;
            }
            Err(e) => {
                let _ = tx.send(IngestFrame::fatal(&e)).await;
                // `tx` drops here — channel closes.
            }
        }
    });

    let event_stream = ReceiverStream::new(rx).map(|f| Ok::<Event, Infallible>(f.into_event()));
    Sse::new(event_stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(INGEST_KEEPALIVE_SECS)))
        .into_response()
}

/// Resolve the resumption state from `X-Ingest-Cursor` (#828). `Ok((stream_id,
/// skip_boundary))` to proceed; `Err(410)` when the cursor is malformed,
/// unknown, or expired. A fresh request (no header) gets a new stream id and a
/// zero skip boundary. The skip boundary is the server's *stored* last line —
/// authoritative over the client's echoed value — so already-stored lines are
/// never re-inserted.
async fn resolve_resumption(
    headers: &HeaderMap,
    cursor_store: &IngestCursorStore,
) -> Result<(String, u64), Response> {
    let Some(raw) = headers.get("x-ingest-cursor").and_then(|v| v.to_str().ok()) else {
        return Ok((Uuid::new_v4().to_string(), 0));
    };
    let Some((stream_id, _client_line)) = parse_ingest_cursor(raw) else {
        return Err(gone("malformed X-Ingest-Cursor; start a fresh stream"));
    };
    match cursor_store.get(&stream_id).await {
        Some(last_line) => Ok((stream_id, last_line)),
        None => Err(gone(
            "ingest cursor expired or unknown; start a fresh stream",
        )),
    }
}

/// Build a `410 Gone` problem response for an unusable resume cursor.
fn gone(message: &str) -> Response {
    ApiError::Gone(message.to_string()).into_response()
}

/// Resolve the per-stream bearer token (#829) into the effective
/// `(schema, rate_limit)`. A valid token binds the stream to its mint-time
/// archive schema and lines/sec rate limit (so a token cannot write outside the
/// archive it was minted for). With no/invalid token: `Err(401)` when
/// `INGEST_REQUIRE_TOKEN=true` (the default), else `Ok((request archive schema,
/// 0 = unlimited))` for the open single-user/dev mode.
async fn resolve_stream_token(
    headers: &HeaderMap,
    state: &AppState,
    archive_ctx: &ArchiveContext,
) -> Result<(String, u64), Response> {
    if let Some(token) = extract_bearer(headers) {
        if let Some(data) = state.ingest_token_store.validate(&token).await {
            return Ok((data.schema, data.rate_limit));
        }
    }
    if ingest_require_token() {
        Err(unauthorized(
            "a valid ingest stream token is required; mint one at POST /api/v1/ingest/tokens",
        ))
    } else {
        Ok((archive_ctx.schema.clone(), 0))
    }
}

/// Build a `401 Unauthorized` problem response for a missing/invalid stream token.
fn unauthorized(message: &str) -> Response {
    ApiError::Unauthorized(message.to_string()).into_response()
}

// =============================================================================
// TESTS — pure logic + pump resource discipline (no DB / no server)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, StatusCode};

    async fn response_body_json(response: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    // ---- env knob -----------------------------------------------------------

    #[test]
    fn max_line_bytes_defaults_when_unset() {
        // The knob reads a process-global env var; assert the default constant
        // directly to stay isolated from ambient configuration.
        assert_eq!(DEFAULT_INGEST_MAX_LINE_BYTES, 1024 * 1024);
    }

    // ---- LineSplitter (pillar 1) -------------------------------------------

    /// Collect line-events as owned `Vec<u8>` / `OVERFLOW` markers for assertion.
    fn drain(events: Vec<LineEvent>) -> Vec<Vec<u8>> {
        events
            .into_iter()
            .map(|e| match e {
                LineEvent::Line(b) => b,
                LineEvent::Overflow => b"<OVERFLOW>".to_vec(),
            })
            .collect()
    }

    #[test]
    fn splitter_emits_complete_lines_in_one_chunk() {
        let mut s = LineSplitter::new(1024);
        let out = drain(s.push(b"a\nbb\nccc\n"));
        assert_eq!(out, vec![b"a".to_vec(), b"bb".to_vec(), b"ccc".to_vec()]);
        assert!(s.finish().is_none(), "no trailing partial line expected");
    }

    #[test]
    fn splitter_joins_a_line_split_across_chunks() {
        let mut s = LineSplitter::new(1024);
        assert!(drain(s.push(b"hel")).is_empty());
        assert!(drain(s.push(b"lo")).is_empty());
        let out = drain(s.push(b" world\n"));
        assert_eq!(out, vec![b"hello world".to_vec()]);
    }

    #[test]
    fn splitter_strips_crlf() {
        let mut s = LineSplitter::new(1024);
        let out = drain(s.push(b"a\r\nb\r\n"));
        assert_eq!(out, vec![b"a".to_vec(), b"b".to_vec()]);
    }

    #[tokio::test]
    async fn ingest_cursor_gone_returns_problem_without_legacy_error_shape() {
        let response = gone("ingest cursor expired or unknown; start a fresh stream");
        assert_eq!(response.status(), StatusCode::GONE);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
        let problem = response_body_json(response).await;
        assert_eq!(problem["type"], "https://fortemi.com/problems/gone");
        assert_eq!(problem["status"], 410);
        assert!(problem.get("error").is_none());
        assert!(problem.get("code").is_none());
    }

    #[tokio::test]
    async fn ingest_token_required_returns_problem_without_legacy_error_shape() {
        let response = unauthorized("a valid ingest stream token is required");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
        let problem = response_body_json(response).await;
        assert_eq!(problem["type"], "https://fortemi.com/problems/unauthorized");
        assert_eq!(problem["status"], 401);
        assert!(problem.get("error").is_none());
        assert!(problem.get("code").is_none());
    }

    #[test]
    fn splitter_preserves_blank_lines_for_the_pump_to_skip() {
        let mut s = LineSplitter::new(1024);
        let out = drain(s.push(b"\n  \nx\n"));
        // Blank and whitespace-only lines surface as empty/whitespace Line events;
        // skipping them is the pump's job (see `is_blank`).
        assert_eq!(out, vec![b"".to_vec(), b"  ".to_vec(), b"x".to_vec()]);
    }

    #[test]
    fn splitter_flushes_trailing_line_without_newline() {
        let mut s = LineSplitter::new(1024);
        assert!(drain(s.push(b"no-newline")).is_empty());
        match s.finish() {
            Some(LineEvent::Line(b)) => assert_eq!(b, b"no-newline".to_vec()),
            _ => panic!("expected a trailing Line"),
        }
    }

    #[test]
    fn splitter_rejects_over_length_completed_line() {
        let mut s = LineSplitter::new(4);
        let out = drain(s.push(b"toolong\nok\n"));
        assert_eq!(out, vec![b"<OVERFLOW>".to_vec(), b"ok".to_vec()]);
    }

    #[test]
    fn splitter_rejects_over_length_line_in_progress_then_resumes() {
        // cap = 6: "aaaaaaaa" (8) overflows; "clean" (5) fits on resume.
        let mut s = LineSplitter::new(6);
        // No newline yet, but already over cap -> one Overflow, enter discard mode.
        let out = drain(s.push(b"aaaaaaaa"));
        assert_eq!(out, vec![b"<OVERFLOW>".to_vec()]);
        // More tail bytes, still no newline -> dropped, no new event.
        assert!(drain(s.push(b"bbbb")).is_empty());
        // Newline ends the over-length line; the next line is clean.
        let out = drain(s.push(b"\nclean\n"));
        assert_eq!(out, vec![b"clean".to_vec()]);
    }

    #[test]
    fn splitter_finish_reports_trailing_overflow_once() {
        let mut s = LineSplitter::new(4);
        // Over-cap, no newline: emits Overflow on push and enters discard mode...
        let out = drain(s.push(b"toolong"));
        assert_eq!(out, vec![b"<OVERFLOW>".to_vec()]);
        // ...so finish must NOT double-count it.
        assert!(s.finish().is_none());
    }

    // ---- parse_ingest_line --------------------------------------------------

    #[test]
    fn parse_accepts_note_envelope_with_defaults() {
        let req = parse_ingest_line(br#"{"type":"note","data":{"content":"hi"}}"#)
            .expect("valid note line");
        assert_eq!(req.content, "hi");
        assert_eq!(req.format, "markdown");
        assert_eq!(req.source, "ingest-stream");
        assert!(req.title.is_none());
    }

    #[test]
    fn parse_carries_optional_fields_and_ignores_unknown() {
        // Unknown pipeline knobs (revision_mode) are ignored in the foundation.
        let req = parse_ingest_line(
            br#"{"type":"note","data":{"content":"c","format":"text","source":"x","title":"t","tags":["a","b"],"revision_mode":"full"}}"#,
        )
        .expect("valid");
        assert_eq!(req.format, "text");
        assert_eq!(req.source, "x");
        assert_eq!(req.title.as_deref(), Some("t"));
        assert_eq!(
            req.tags.as_deref(),
            Some(&["a".to_string(), "b".to_string()][..])
        );
    }

    #[test]
    fn parse_rejects_empty_content() {
        let err = parse_ingest_line(br#"{"type":"note","data":{"content":"   "}}"#)
            .expect_err("blank content must be rejected");
        assert!(err.contains("content must not be empty"), "got: {err}");
    }

    #[test]
    fn parse_rejects_unknown_type() {
        let err = parse_ingest_line(br#"{"type":"widget","data":{"content":"x"}}"#)
            .expect_err("unknown envelope type must be rejected");
        assert!(err.contains("invalid ingest line"), "got: {err}");
    }

    #[test]
    fn parse_rejects_malformed_json() {
        let err = parse_ingest_line(b"{not json").expect_err("malformed JSON must be rejected");
        assert!(err.contains("invalid ingest line"), "got: {err}");
    }

    // ---- IngestFrame --------------------------------------------------------

    fn frame_json(f: &IngestFrame) -> serde_json::Value {
        serde_json::from_str(&f.data).expect("frame data is JSON")
    }

    #[test]
    fn frame_ack_ok_shape() {
        let id = Uuid::nil();
        let f = IngestFrame::ack_ok(3, id, "strm");
        assert_eq!(f.event, "ack");
        let j = frame_json(&f);
        assert_eq!(j["line"], 3);
        assert_eq!(j["status"], "ok");
        assert_eq!(j["note_id"], id.to_string());
        assert_eq!(j["cursor"], "strm-3", "ack carries the resumption cursor");
    }

    #[test]
    fn frame_ack_error_shape() {
        let f = IngestFrame::ack_error(5, "note content must not be empty", "strm");
        assert_eq!(f.event, "ack");
        let j = frame_json(&f);
        assert_eq!(j["line"], 5);
        assert_eq!(j["status"], "error");
        assert_eq!(j["error"], "note content must not be empty");
        assert_eq!(j["cursor"], "strm-5");
    }

    #[test]
    fn frame_ack_error_redacts_internal_detail() {
        let f = IngestFrame::ack_error(
            5,
            "database write failed for postgres://user:secret@db/app at /srv/fortemi/private",
            "strm",
        );
        let j = frame_json(&f);
        assert_eq!(
            j["error"],
            "ingest line could not be stored. Check server logs for diagnostics."
        );
        let serialized = j.to_string();
        assert!(!serialized.contains("postgres://"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("/srv/fortemi"));
        assert!(!serialized.contains("database write failed"));
    }

    #[test]
    fn frame_fatal_redacts_internal_detail() {
        let f = IngestFrame::fatal(
            "invalid schema postgres://user:secret@db/app at /srv/fortemi/private",
        );
        assert_eq!(f.event, "error");
        let j = frame_json(&f);
        assert_eq!(
            j["error"],
            "ingest stream could not be initialized. Check server logs for diagnostics."
        );
        assert_eq!(j["code"], "INGEST_FATAL");
        let serialized = j.to_string();
        assert!(!serialized.contains("postgres://"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("/srv/fortemi"));
        assert!(!serialized.contains("invalid schema"));
    }

    #[test]
    fn frame_done_shape() {
        let stats = IngestStats {
            line_no: 3,
            success: 2,
            errors: 1,
        };
        let f = IngestFrame::done(&stats);
        assert_eq!(f.event, "done");
        let j = frame_json(&f);
        assert_eq!(j["total"], 3);
        assert_eq!(j["success"], 2);
        assert_eq!(j["errors"], 1);
    }

    /// Frames own their data — building one, then dropping every input, leaves
    /// the frame wholly intact (no shared/borrowed ownership). Mirrors the
    /// value-independence guarantee asserted for `/chat/stream` frames.
    #[test]
    fn frame_data_is_value_independent() {
        let error_owned = String::from("metadata must be a JSON object");
        let f = IngestFrame::ack_error(9, &error_owned, "strm");
        drop(error_owned);
        let j = frame_json(&f);
        assert_eq!(j["error"], "metadata must be a JSON object");
        assert_eq!(j["line"], 9);
    }

    // ---- pump resource discipline (pillar 3) — DB-free via a mock sink ------

    /// In-memory sink: stores nothing, but fails any note whose content contains
    /// `"boom"` so the per-line error path is exercised without a database.
    struct MockSink;

    #[async_trait]
    impl NoteSink for MockSink {
        async fn store(&self, req: CreateNoteRequest) -> Result<Uuid, String> {
            if req.content.contains("boom") {
                Err("mock store failure".to_string())
            } else {
                Ok(Uuid::nil())
            }
        }
    }

    /// Build a body stream from byte chunks (the `BodyDataStream` item type).
    fn body_of(chunks: &[&'static [u8]]) -> impl Stream<Item = Result<Bytes, axum::Error>> + Unpin {
        let items: Vec<Result<Bytes, axum::Error>> =
            chunks.iter().map(|c| Ok(Bytes::from_static(c))).collect();
        futures::stream::iter(items)
    }

    /// Run the pump against a body stream and drain the SSE channel to
    /// completion, returning every frame's `(event, json)`. Bounded by a timeout
    /// so a dangling sender (resource leak) fails fast rather than hanging, and
    /// asserts the channel is closed once the pump returns.
    async fn drive<B>(
        stream: B,
        max: usize,
        progress_interval: usize,
        skip_boundary: u64,
    ) -> Vec<(&'static str, serde_json::Value)>
    where
        B: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
    {
        let (tx, mut rx) = mpsc::channel::<IngestFrame>(64);
        let cfg = PumpConfig {
            stream_id: "test-stream".to_string(),
            max_line_bytes: max,
            skip_boundary,
            progress_interval: progress_interval as u64,
        };
        // buffer == the harness channel capacity so pressure math is accurate;
        // every existing test drives far fewer than 80% of 64 frames, so no
        // backpressure advisory fires (#827 escalation has dedicated tests).
        // rate_limit 0 => unlimited, so the rate limiter never paces here
        // (#829 rate limiting has dedicated tests).
        let controls = StreamControls {
            bp: Backpressure::new(64, 1000, 500, Arc::new(IngestStreamMetrics::default())),
            rate: RateLimiter::new(0, 500),
        };
        pump_ingest_stream(
            stream,
            tx,
            MockSink,
            SearchCache::disabled(),
            IngestCursorStore::disabled(),
            cfg,
            controls,
        )
        .await;
        let drain = async {
            let mut out = Vec::new();
            while let Some(f) = rx.recv().await {
                let j: serde_json::Value = serde_json::from_str(&f.data).expect("json");
                out.push((f.event, j));
            }
            (out, rx)
        };
        let (out, mut rx) = tokio::time::timeout(Duration::from_secs(2), drain)
            .await
            .expect("draining hung — pump left a dangling sender (resource leak)");
        assert!(
            rx.recv().await.is_none(),
            "channel must be closed once the pump returns"
        );
        out
    }

    async fn run_pump(
        chunks: &[&'static [u8]],
        max: usize,
    ) -> Vec<(&'static str, serde_json::Value)> {
        // usize::MAX interval => no progress frames (the existing pump tests
        // assert ack/done shape; progress has dedicated tests below).
        drive(body_of(chunks), max, usize::MAX, 0).await
    }

    #[tokio::test]
    async fn pump_acks_each_line_then_done() {
        let frames = run_pump(
            &[
                br#"{"type":"note","data":{"content":"one"}}"#,
                b"\n",
                br#"{"type":"note","data":{"content":"two"}}"#,
                b"\n",
            ],
            1024,
        )
        .await;

        let acks: Vec<_> = frames.iter().filter(|(e, _)| *e == "ack").collect();
        assert_eq!(acks.len(), 2, "one ack per data line");
        assert!(acks.iter().all(|(_, j)| j["status"] == "ok"));

        let (event, done) = frames.last().expect("a terminal frame");
        assert_eq!(*event, "done");
        assert_eq!(done["total"], 2);
        assert_eq!(done["success"], 2);
        assert_eq!(done["errors"], 0);
    }

    #[tokio::test]
    async fn pump_skips_blank_lines() {
        let frames = run_pump(
            &[
                b"\n",
                br#"{"type":"note","data":{"content":"x"}}"#,
                b"\n",
                b"   \n",
            ],
            1024,
        )
        .await;
        let acks = frames.iter().filter(|(e, _)| *e == "ack").count();
        assert_eq!(acks, 1, "blank/whitespace lines are not acked");
        assert_eq!(frames.last().unwrap().1["total"], 1);
    }

    #[tokio::test]
    async fn pump_isolates_a_failing_line() {
        // good, parse-error, store-error, good — fault isolation: the stream
        // survives both a parse failure and a store failure.
        let frames = run_pump(
            &[
                br#"{"type":"note","data":{"content":"ok1"}}"#,
                b"\n",
                b"{bad json}",
                b"\n",
                br#"{"type":"note","data":{"content":"boom"}}"#,
                b"\n",
                br#"{"type":"note","data":{"content":"ok2"}}"#,
                b"\n",
            ],
            1024,
        )
        .await;

        let statuses: Vec<&str> = frames
            .iter()
            .filter(|(e, _)| *e == "ack")
            .map(|(_, j)| j["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, vec!["ok", "error", "error", "ok"]);

        let done = &frames.last().unwrap().1;
        assert_eq!(done["total"], 4);
        assert_eq!(done["success"], 2);
        assert_eq!(done["errors"], 2);
    }

    #[tokio::test]
    async fn pump_rejects_over_length_line_but_continues() {
        // cap = 50 bytes: the first note's JSON (80-char content) far exceeds it
        // -> overflow error ack; a minimal second note fits and still succeeds.
        let long = format!(
            r#"{{"type":"note","data":{{"content":"{}"}}}}"#,
            "x".repeat(80)
        );
        let ok = r#"{"type":"note","data":{"content":"ok"}}"#;
        let body = format!("{long}\n{ok}\n");
        let stream = futures::stream::iter(vec![Ok::<Bytes, axum::Error>(Bytes::from(
            body.into_bytes(),
        ))]);
        let frames = drive(stream, 50, usize::MAX, 0).await;
        let statuses: Vec<&str> = frames
            .iter()
            .filter(|(e, _)| *e == "ack")
            .map(|(_, j)| j["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, vec!["error", "ok"]);
        assert!(
            frames.iter().any(|(e, j)| *e == "ack"
                && j["error"]
                    .as_str()
                    .is_some_and(|s| s.contains("byte limit"))),
            "overflow ack should mention the byte limit"
        );
    }

    #[tokio::test]
    async fn pump_handles_trailing_line_without_newline() {
        let frames = run_pump(&[br#"{"type":"note","data":{"content":"tail"}}"#], 1024).await;
        let acks = frames.iter().filter(|(e, _)| *e == "ack").count();
        assert_eq!(acks, 1, "a trailing line with no newline is still ingested");
        assert_eq!(frames.last().unwrap().1["total"], 1);
    }

    #[tokio::test]
    async fn pump_empty_body_emits_only_done_zero() {
        let frames = run_pump(&[], 1024).await;
        assert_eq!(frames.len(), 1, "empty body -> just the done frame");
        let (event, done) = &frames[0];
        assert_eq!(*event, "done");
        assert_eq!(done["total"], 0);
        assert_eq!(done["success"], 0);
        assert_eq!(done["errors"], 0);
    }

    // ---- per-line validation (#826) ----------------------------------------

    #[test]
    fn progress_interval_defaults_when_unset() {
        assert_eq!(DEFAULT_INGEST_PROGRESS_INTERVAL, 100);
    }

    #[test]
    fn validate_rejects_overlong_tag() {
        let long_tag = "a".repeat(matric_core::defaults::TAG_NAME_MAX_LENGTH + 1);
        let line = format!(r#"{{"type":"note","data":{{"content":"c","tags":["{long_tag}"]}}}}"#);
        let err = parse_ingest_line(line.as_bytes()).expect_err("overlong tag rejected");
        assert!(err.contains("character limit"), "got: {err}");
    }

    #[test]
    fn validate_rejects_overdeep_tag() {
        // 6 segments > MAX_TAG_PATH_DEPTH (5).
        let err =
            parse_ingest_line(br#"{"type":"note","data":{"content":"c","tags":["a/b/c/d/e/f"]}}"#)
                .expect_err("overdeep tag rejected");
        assert!(err.contains("maximum depth"), "got: {err}");
    }

    #[test]
    fn validate_rejects_non_object_metadata() {
        let err = parse_ingest_line(br#"{"type":"note","data":{"content":"c","metadata":[1,2]}}"#)
            .expect_err("array metadata rejected");
        assert!(err.contains("metadata must be a JSON object"), "got: {err}");
        let err = parse_ingest_line(br#"{"type":"note","data":{"content":"c","metadata":"x"}}"#)
            .expect_err("scalar metadata rejected");
        assert!(err.contains("metadata must be a JSON object"), "got: {err}");
    }

    #[test]
    fn validate_accepts_object_metadata_and_valid_tags() {
        let req = parse_ingest_line(
            br#"{"type":"note","data":{"content":"c","metadata":{"k":"v"},"tags":["a/b/c","x"]}}"#,
        )
        .expect("object metadata + valid tags accepted");
        assert!(req.metadata.is_some());
        assert_eq!(req.tags.as_deref().map(<[_]>::len), Some(2));
    }

    // ---- progress events (#826) --------------------------------------------

    #[test]
    fn frame_progress_shape() {
        let f = IngestFrame::progress(42);
        assert_eq!(f.event, "progress");
        assert_eq!(frame_json(&f)["processed"], 42);
    }

    #[tokio::test]
    async fn pump_emits_progress_every_interval_counting_errors() {
        // interval = 2 over [ok, boom(store-err), ok, ok]: processed advances
        // 1,2,3,4 -> progress at 2 and 4; errors count toward `processed`.
        let body = [
            r#"{"type":"note","data":{"content":"ok"}}"#,
            r#"{"type":"note","data":{"content":"boom"}}"#,
            r#"{"type":"note","data":{"content":"ok"}}"#,
            r#"{"type":"note","data":{"content":"ok"}}"#,
        ]
        .join("\n");
        let stream = futures::stream::iter(vec![Ok::<Bytes, axum::Error>(Bytes::from(
            body.into_bytes(),
        ))]);
        let frames = drive(stream, 1024, 2, 0).await;

        let progress: Vec<u64> = frames
            .iter()
            .filter(|(e, _)| *e == "progress")
            .map(|(_, j)| j["processed"].as_u64().unwrap())
            .collect();
        assert_eq!(progress, vec![2, 4], "progress fires every 2 data lines");

        let done = &frames.last().unwrap().1;
        assert_eq!(done["total"], 4);
        assert_eq!(done["success"], 3);
        assert_eq!(done["errors"], 1);
    }

    #[tokio::test]
    async fn pump_progress_skips_blank_lines() {
        // Blank lines must not advance `processed` or re-fire progress.
        let body = [
            r#"{"type":"note","data":{"content":"a"}}"#,
            "",
            "   ",
            r#"{"type":"note","data":{"content":"b"}}"#,
        ]
        .join("\n");
        let stream = futures::stream::iter(vec![Ok::<Bytes, axum::Error>(Bytes::from(
            body.into_bytes(),
        ))]);
        let frames = drive(stream, 1024, 2, 0).await;

        let progress: Vec<u64> = frames
            .iter()
            .filter(|(e, _)| *e == "progress")
            .map(|(_, j)| j["processed"].as_u64().unwrap())
            .collect();
        assert_eq!(
            progress,
            vec![2],
            "only the 2 data lines count toward progress"
        );
        assert_eq!(frames.last().unwrap().1["total"], 2);
    }

    // ---- resumption (#828) --------------------------------------------------

    #[test]
    fn cursor_parses_uuid_stream_and_line() {
        let (stream, line) =
            parse_ingest_cursor("550e8400-e29b-41d4-a716-446655440000-42").unwrap();
        assert_eq!(stream, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(line, 42);
    }

    #[test]
    fn cursor_parses_simple_stream() {
        let (stream, line) = parse_ingest_cursor("abc-7").unwrap();
        assert_eq!(stream, "abc");
        assert_eq!(line, 7);
    }

    #[test]
    fn cursor_rejects_malformed() {
        assert!(parse_ingest_cursor("").is_none());
        assert!(parse_ingest_cursor("noseq").is_none());
        assert!(parse_ingest_cursor("-5").is_none()); // empty stream id
        assert!(parse_ingest_cursor("abc-").is_none()); // empty line
        assert!(parse_ingest_cursor("abc-notanumber").is_none());
    }

    /// On resume, lines whose absolute number is ≤ skip_boundary are skipped
    /// (no ack, not re-inserted); processing resumes after, with absolute line
    /// numbers and a continuing cursor (#828).
    #[tokio::test]
    async fn pump_skips_already_processed_lines() {
        let body = [
            r#"{"type":"note","data":{"content":"one"}}"#,
            r#"{"type":"note","data":{"content":"two"}}"#,
            r#"{"type":"note","data":{"content":"three"}}"#,
            r#"{"type":"note","data":{"content":"four"}}"#,
        ]
        .join("\n");
        let stream = futures::stream::iter(vec![Ok::<Bytes, axum::Error>(Bytes::from(
            body.into_bytes(),
        ))]);
        // skip_boundary = 2 => lines 1,2 already processed on a prior connection.
        let frames = drive(stream, 1024, usize::MAX, 2).await;

        let acks: Vec<&serde_json::Value> = frames
            .iter()
            .filter(|(e, _)| *e == "ack")
            .map(|(_, j)| j)
            .collect();
        assert_eq!(acks.len(), 2, "only the unprocessed tail is acked");
        assert_eq!(acks[0]["line"], 3);
        assert_eq!(acks[0]["cursor"], "test-stream-3");
        assert_eq!(acks[1]["line"], 4);
        assert_eq!(acks[1]["cursor"], "test-stream-4");

        let done = &frames.last().unwrap().1;
        assert_eq!(done["total"], 2, "done counts only newly-processed lines");
        assert_eq!(done["success"], 2);
    }

    #[tokio::test]
    async fn resolve_resumption_fresh_when_no_header() {
        let (stream_id, skip) =
            resolve_resumption(&HeaderMap::new(), &IngestCursorStore::disabled())
                .await
                .expect("a missing cursor header starts a fresh stream");
        assert!(!stream_id.is_empty());
        assert_eq!(skip, 0);
    }

    #[tokio::test]
    async fn resolve_resumption_410_on_malformed_cursor() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ingest-cursor", "noseq".parse().unwrap());
        let resp = resolve_resumption(&headers, &IngestCursorStore::disabled())
            .await
            .expect_err("malformed cursor must short-circuit");
        assert_eq!(resp.status(), StatusCode::GONE);
    }

    #[tokio::test]
    async fn resolve_resumption_410_on_unknown_stream() {
        // disabled store => get() returns None => cursor unknown/expired => 410.
        let mut headers = HeaderMap::new();
        headers.insert("x-ingest-cursor", "some-stream-9".parse().unwrap());
        let resp = resolve_resumption(&headers, &IngestCursorStore::disabled())
            .await
            .expect_err("unknown/expired cursor must short-circuit");
        assert_eq!(resp.status(), StatusCode::GONE);
    }

    // ---- backpressure (#827) ------------------------------------------------

    #[test]
    fn stream_buffer_defaults_when_unset() {
        assert_eq!(DEFAULT_INGEST_STREAM_BUFFER, 64);
        assert_eq!(WARN_PRESSURE_PCT, 80);
        assert_eq!(THROTTLE_PRESSURE_PCT, 95);
    }

    #[test]
    fn frame_warning_shape() {
        let f = IngestFrame::warning(750);
        assert_eq!(f.event, "warning");
        let j = frame_json(&f);
        assert_eq!(j["message"], "buffer high");
        assert_eq!(j["advisory_rate"], 750);
    }

    #[test]
    fn frame_throttle_shape() {
        let f = IngestFrame::throttle(250);
        assert_eq!(f.event, "error");
        let j = frame_json(&f);
        assert_eq!(j["status"], 429);
        assert_eq!(j["retry_after_ms"], 250);
        assert_eq!(j["code"], "INGEST_BACKPRESSURE");
    }

    /// `sample_pressure` reports occupancy as a percent of the configured buffer,
    /// derived from the live free-permit count (`tx.capacity()`).
    #[test]
    fn sample_pressure_reports_occupancy_percent() {
        let (tx, _rx) = mpsc::channel::<IngestFrame>(10);
        assert_eq!(sample_pressure(&tx, 10), 0, "empty channel = 0%");
        for _ in 0..8 {
            tx.try_send(IngestFrame::progress(0)).unwrap();
        }
        assert_eq!(sample_pressure(&tx, 10), 80, "8/10 occupied = 80%");
        tx.try_send(IngestFrame::progress(0)).unwrap();
        assert_eq!(sample_pressure(&tx, 10), 90, "9/10 occupied = 90%");
    }

    /// Helper: occupy `n` of the channel's slots with filler frames so
    /// `sample_pressure` reads a known occupancy, returning the live sender.
    fn fill(tx: &mpsc::Sender<IngestFrame>, n: usize) {
        for _ in 0..n {
            tx.try_send(IngestFrame::progress(0))
                .expect("slot available");
        }
    }

    #[test]
    fn backpressure_warns_once_at_80_percent() {
        let metrics = Arc::new(IngestStreamMetrics::default());
        let (tx, mut rx) = mpsc::channel::<IngestFrame>(10);
        let mut bp = Backpressure::new(10, 1000, 500, metrics.clone());

        fill(&tx, 8); // 80%
        bp.observe(&tx);
        bp.observe(&tx); // second sample in the same episode must not re-warn

        assert_eq!(
            metrics.backpressure_warnings_total.load(Ordering::Relaxed),
            1,
            "exactly one warning per high episode"
        );
        assert_eq!(metrics.buffer_pressure.load(Ordering::Relaxed), 90); // 9/10 after the warning frame
        assert_eq!(metrics.throttled_total.load(Ordering::Relaxed), 0);

        // Drain the 8 fillers; the 9th frame must be the warning.
        for _ in 0..8 {
            let f = rx.try_recv().expect("filler");
            assert_eq!(f.event, "progress");
        }
        let w = rx.try_recv().expect("warning frame");
        assert_eq!(w.event, "warning");
    }

    #[test]
    fn backpressure_throttles_with_429_at_95_percent() {
        let metrics = Arc::new(IngestStreamMetrics::default());
        let (tx, mut rx) = mpsc::channel::<IngestFrame>(20);
        let mut bp = Backpressure::new(20, 1000, 500, metrics.clone());

        fill(&tx, 19); // 95%
        bp.observe(&tx);

        assert_eq!(metrics.throttled_total.load(Ordering::Relaxed), 1);
        assert_eq!(
            metrics.backpressure_warnings_total.load(Ordering::Relaxed),
            0,
            "a 429 subsumes the warning tier — no separate warning"
        );
        assert!(bp.throttled && bp.warned, "429 arms both flags");

        for _ in 0..19 {
            rx.try_recv().expect("filler");
        }
        let f = rx.try_recv().expect("throttle frame");
        let j: serde_json::Value = serde_json::from_str(&f.data).unwrap();
        assert_eq!(j["status"], 429);
    }

    #[tokio::test]
    async fn backpressure_rearms_after_pressure_recovers() {
        let metrics = Arc::new(IngestStreamMetrics::default());
        let (tx, mut rx) = mpsc::channel::<IngestFrame>(10);
        let mut bp = Backpressure::new(10, 1000, 500, metrics.clone());

        fill(&tx, 8); // 80%
        bp.observe(&tx); // warning #1 (now 9 used)
        assert_eq!(
            metrics.backpressure_warnings_total.load(Ordering::Relaxed),
            1
        );

        // Drain below the warning threshold so the episode ends.
        for _ in 0..4 {
            rx.recv().await.expect("frame");
        }
        bp.observe(&tx); // pressure < 80% -> re-arm, no frame
        assert!(!bp.warned, "warned flag re-armed below threshold");

        // Back up to 80% -> a fresh warning fires.
        fill(&tx, 3);
        bp.observe(&tx);
        assert_eq!(
            metrics.backpressure_warnings_total.load(Ordering::Relaxed),
            2,
            "a new high episode emits a new warning"
        );
    }

    #[test]
    fn ingest_stream_metrics_snapshot_shape() {
        let m = IngestStreamMetrics::default();
        m.record_pressure(42);
        m.record_pressure(17); // peak must hold the high-water mark
        m.backpressure_warnings_total
            .fetch_add(3, Ordering::Relaxed);
        m.throttled_total.fetch_add(1, Ordering::Relaxed);
        let j = m.snapshot();
        assert_eq!(j["ingest_stream_buffer_pressure"]["value"], 17);
        assert_eq!(j["ingest_stream_buffer_pressure"]["type"], "gauge");
        assert_eq!(j["ingest_stream_buffer_pressure_peak"]["value"], 42);
        assert_eq!(j["ingest_stream_backpressure_warnings_total"]["value"], 3);
        assert_eq!(j["ingest_stream_throttled_total"]["value"], 1);
        assert_eq!(j["ingest_stream_rate_limited_total"]["value"], 0);
    }

    // ---- per-stream token + rate limit (#829) -------------------------------

    #[test]
    fn require_token_defaults_true() {
        // The knob reads process-global env; assert the secure default behavior
        // only when the var is unset to stay isolated from ambient config.
        if std::env::var("INGEST_REQUIRE_TOKEN").is_err() {
            assert!(ingest_require_token(), "token gate is on by default");
        }
    }

    #[test]
    fn frame_rate_limited_shape() {
        let f = IngestFrame::rate_limited(125);
        assert_eq!(f.event, "error");
        let j = frame_json(&f);
        assert_eq!(j["status"], 429);
        assert_eq!(j["retry_after_ms"], 125);
        assert_eq!(j["code"], "INGEST_RATE_LIMITED");
    }

    #[test]
    fn extract_bearer_parses_authorization() {
        let mut h = HeaderMap::new();
        assert_eq!(extract_bearer(&h), None, "no header -> None");
        h.insert("authorization", "Bearer mm_ist_abc".parse().unwrap());
        assert_eq!(extract_bearer(&h).as_deref(), Some("mm_ist_abc"));
        h.insert("authorization", "bearer lower".parse().unwrap());
        assert_eq!(extract_bearer(&h).as_deref(), Some("lower"));
        h.insert("authorization", "Basic xyz".parse().unwrap());
        assert_eq!(extract_bearer(&h), None, "non-bearer scheme -> None");
        h.insert("authorization", "Bearer    ".parse().unwrap());
        assert_eq!(extract_bearer(&h), None, "empty token -> None");
    }

    #[tokio::test]
    async fn rate_limiter_unlimited_is_noop() {
        let mut rl = RateLimiter::new(0, 500);
        assert!(rl.unlimited());
        assert!(!rl.acquire().await, "unlimited never throttles");
        assert!(!rl.acquire().await);
    }

    #[tokio::test]
    async fn rate_limiter_paces_and_signals_one_episode() {
        // 50 lines/sec: the bucket starts full (50 permits), so 50 acquisitions
        // pass instantly; the next must wait (~20ms) and reports the episode start.
        let mut rl = RateLimiter::new(50, 500);
        for _ in 0..50 {
            assert!(!rl.acquire().await, "burst within rate does not throttle");
        }
        assert!(
            rl.acquire().await,
            "first over-rate acquire starts a throttle episode"
        );
        assert!(
            !rl.acquire().await,
            "subsequent over-rate acquires stay in the same episode"
        );
    }

    #[test]
    fn consumes_permit_only_for_processable_data_lines() {
        let fresh = IngestStats::default(); // line_no = 0
        assert!(
            consumes_permit(&LineEvent::Line(b"x".to_vec()), &fresh, 0),
            "a fresh non-blank data line consumes a permit"
        );
        assert!(
            !consumes_permit(&LineEvent::Line(b"  ".to_vec()), &fresh, 0),
            "blank lines are not rate-limited"
        );
        assert!(
            !consumes_permit(&LineEvent::Overflow, &fresh, 0),
            "overflow lines are not rate-limited"
        );
        let mid = IngestStats {
            line_no: 2,
            ..Default::default()
        };
        assert!(
            !consumes_permit(&LineEvent::Line(b"x".to_vec()), &mid, 3),
            "lines within the resume skip boundary are not rate-limited"
        );
        let at = IngestStats {
            line_no: 3,
            ..Default::default()
        };
        assert!(
            consumes_permit(&LineEvent::Line(b"x".to_vec()), &at, 3),
            "the first line past the skip boundary consumes a permit"
        );
    }

    #[test]
    fn resolve_stream_token_401_when_required_and_absent() {
        // INGEST_REQUIRE_TOKEN defaults true; with no validated token the route
        // must fail closed. Only assert when the gate is on, to stay isolated
        // from an ambient INGEST_REQUIRE_TOKEN=false.
        if !ingest_require_token() {
            return;
        }
        let status = resolve_stream_token_inner(None, None);
        assert_eq!(
            status.expect_err("must reject"),
            StatusCode::UNAUTHORIZED,
            "fail-closed when token required and absent"
        );
    }

    /// Pure mirror of [`resolve_stream_token`]'s decision (DB/Redis-free, error
    /// reduced to a `StatusCode`) so the fail-closed vs open-fallback branch is
    /// unit-testable: given an optional validated token's `(schema, rate_limit)`
    /// and the request archive schema, reproduce the resolution.
    fn resolve_stream_token_inner(
        validated: Option<(String, u64)>,
        archive_schema: Option<&str>,
    ) -> Result<(String, u64), StatusCode> {
        if let Some(v) = validated {
            return Ok(v);
        }
        if ingest_require_token() {
            Err(StatusCode::UNAUTHORIZED)
        } else {
            Ok((archive_schema.unwrap_or("public").to_string(), 0))
        }
    }

    #[test]
    fn resolve_stream_token_inner_binds_validated_token() {
        let got = resolve_stream_token_inner(Some(("archive_x".to_string(), 250)), Some("ignored"))
            .expect("a valid token resolves");
        assert_eq!(
            got,
            ("archive_x".to_string(), 250),
            "binds token schema+rate"
        );
    }

    // ---- outbox wiring (#830) — DB-gated integration ------------------------

    /// Build a `CreateNoteRequest` for a unique note via the real parse path.
    fn db_note_req(marker: &str) -> CreateNoteRequest {
        parse_ingest_line(
            format!(r#"{{"type":"note","data":{{"content":"{marker}"}}}}"#).as_bytes(),
        )
        .expect("valid note line")
    }

    /// #830: each stored note writes exactly one `note.created` outbox row in the
    /// same transaction as the note insert (count invariant), and a forced
    /// mid-transaction failure rolls both back (atomicity). DB-gated: skips when
    /// `DATABASE_URL` is unset (runs in the integration job, which has the
    /// migrated `note` + `event_outbox` tables in `public`).
    #[tokio::test]
    async fn outbox_row_per_ingested_note_and_atomic_rollback() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping #830 outbox test: DATABASE_URL not set");
            return;
        };
        let pool = matric_db::create_pool(&database_url)
            .await
            .expect("connect integration database");
        let run = Uuid::new_v4().simple().to_string();

        // Count invariant: 3 stored notes -> 3 `note.created` outbox rows.
        let sink = DbNoteSink::new(
            pool.clone(),
            "public".to_string(),
            Some("default".to_string()),
        )
        .expect("build sink");
        let mut ids = Vec::new();
        for i in 0..3 {
            let id = sink
                .store(db_note_req(&format!("ingest-830-{run}-{i}")))
                .await
                .expect("store note");
            ids.push(id);
        }
        let outbox_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM event_outbox \
             WHERE event_type = 'note.created' AND entity_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_one(&pool)
        .await
        .expect("count outbox rows");
        assert_eq!(
            outbox_count,
            ids.len() as i64,
            "exactly one note.created outbox row per ingested note"
        );

        // Atomicity: an outbox emit failure (empty event_type) inside the shared
        // transaction must roll back the note insert too — no partial state.
        let ctx = SchemaContext::new(pool.clone(), "public".to_string()).expect("ctx");
        let notes = PgNoteRepository::new(pool.clone());
        let marker_source = format!("ingest-830-rollback-{run}");
        let req = CreateNoteRequest {
            content: "rollback probe".to_string(),
            format: "markdown".to_string(),
            source: marker_source.clone(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
            title: None,
        };
        let result: std::result::Result<Uuid, _> = ctx
            .execute(move |tx| {
                Box::pin(async move {
                    let note_id = notes.insert_tx(tx, req).await?;
                    matric_db::PgEventOutboxRepository::emit_event_tx(
                        tx,
                        matric_db::CreateOutboxEvent::new("", "note", note_id, json!({}), None),
                    )
                    .await?;
                    Ok(note_id)
                })
            })
            .await;
        assert!(
            result.is_err(),
            "an invalid outbox emit must fail the transaction"
        );
        let note_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM note WHERE source = $1")
            .bind(&marker_source)
            .fetch_one(&pool)
            .await
            .expect("count note rows");
        assert_eq!(
            note_rows, 0,
            "the note insert rolled back together with the failed outbox emit"
        );
    }
}

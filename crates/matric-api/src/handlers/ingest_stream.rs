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
//! Blank lines are ignored. Unknown `type` values, malformed JSON, and empty
//! content produce a per-line `error` ack — they never abort the stream.
//!
//! Response: `text/event-stream`:
//! - `ack` — `{"line":N,"status":"ok","note_id":"..."}` or
//!   `{"line":N,"status":"error","error":"..."}` per data line
//! - `done` — `{"total":N,"success":M,"errors":K}` terminal summary
//! - `error` — `{"error":"...","code":"INGEST_FATAL"}` only for a pre-stream
//!   fatal (e.g. an invalid archive schema)
//!
//! ## Scope (#825 foundation — store-only)
//!
//! This is the foundation: parse → `insert_tx` → ack, plus a single post-stream
//! search-cache invalidation so stored notes are FTS-findable. Deliberately
//! deferred:
//! - **NLP enrichment** (embeddings, AI title, linking) — streamed notes are
//!   stored but NOT embedded/titled until a later reprocess. Hardening lands
//!   with per-line validation in #826.
//! - **`event_outbox`** durability/replay — wired in #830 (blocked on #592).
//! - **Per-line auth + rate limit** (#829), **backpressure 429** (#827),
//!   **`X-Ingest-Cursor` resumption** (#828). The bounded per-line buffer and
//!   bounded channel here are correctness floors, not the request-level
//!   backpressure of #827.
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
//!    lingering task. A bounded channel applies backpressure.
//! 4. **Low complexity** — the byte state machine ([`LineSplitter`]) is pure
//!    and unit-tested in isolation from the async I/O and the DB.

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use matric_api::services::SearchCache;
use matric_core::CreateNoteRequest;
use matric_db::{PgNoteRepository, SchemaContext};

use crate::{AppState, ArchiveContext, Auth};

/// Default per-line byte ceiling when `FORTEMI_INGEST_MAX_LINE_BYTES` is unset.
const DEFAULT_INGEST_MAX_LINE_BYTES: usize = 1024 * 1024;

/// Bounded SSE frame channel capacity — applies backpressure between the pump
/// and the client (a slow consumer cannot make the pump buffer without bound).
const INGEST_STREAM_CHANNEL_CAPACITY: usize = 64;

/// SSE keep-alive interval (matches `/chat/stream`).
const INGEST_KEEPALIVE_SECS: u64 = 15;

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

impl IngestFrame {
    /// `ack` frame for a successfully stored line.
    fn ack_ok(line: u64, note_id: Uuid) -> Self {
        Self {
            event: "ack",
            data: json!({ "line": line, "status": "ok", "note_id": note_id }).to_string(),
        }
    }

    /// `ack` frame for a line that failed (parse error, empty content, or a DB
    /// write failure). The stream continues.
    fn ack_error(line: u64, error: &str) -> Self {
        Self {
            event: "ack",
            data: json!({ "line": line, "status": "error", "error": error }).to_string(),
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

    /// Pre-stream fatal frame (e.g. the archive schema could not be resolved).
    /// No `ack`s are emitted in this case.
    fn fatal(error: &str) -> Self {
        Self {
            event: "error",
            data: json!({ "error": error, "code": "INGEST_FATAL" }).to_string(),
        }
    }

    fn into_event(self) -> Event {
        Event::default().event(self.event).data(self.data)
    }
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
/// (`SET LOCAL search_path` + `INSERT` + `COMMIT`), so a single failed line
/// rolls back only itself.
struct DbNoteSink {
    ctx: SchemaContext,
    pool: PgPool,
}

impl DbNoteSink {
    fn new(pool: PgPool, schema: String) -> Result<Self, String> {
        let ctx = SchemaContext::new(pool.clone(), schema).map_err(|e| e.to_string())?;
        Ok(Self { ctx, pool })
    }
}

#[async_trait]
impl NoteSink for DbNoteSink {
    async fn store(&self, req: CreateNoteRequest) -> Result<Uuid, String> {
        let notes = PgNoteRepository::new(self.pool.clone());
        self.ctx
            .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req).await }))
            .await
            .map_err(|e| e.to_string())
    }
}

// =============================================================================
// PUMP (pillar 3 — no-leak; pillar 4 — low complexity via decomposition)
// =============================================================================

/// Drive the NDJSON body through the splitter and sink, emitting one frame per
/// data line and a terminal `done`. Owns `tx`; dropping it on return closes the
/// SSE channel. Stops early (without the cache invalidation or `done`) if the
/// client disconnects — a dropped receiver makes `tx.send` fail.
async fn pump_ingest_stream<B, N>(
    mut body: B,
    tx: mpsc::Sender<IngestFrame>,
    sink: N,
    search_cache: SearchCache,
    max_line_bytes: usize,
) where
    B: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
    N: NoteSink,
{
    let mut splitter = LineSplitter::new(max_line_bytes);
    let mut stats = IngestStats::default();

    while let Some(chunk) = body.next().await {
        // A transport error (client disconnect, truncated body) ends ingestion;
        // already-acked lines stand.
        let Ok(bytes) = chunk else { break };
        for ev in splitter.push(&bytes) {
            if dispatch(ev, &sink, &tx, &mut stats, max_line_bytes)
                .await
                .is_break()
            {
                return; // client gone — abandon without cache work or `done`
            }
        }
    }

    if let Some(ev) = splitter.finish() {
        let _ = dispatch(ev, &sink, &tx, &mut stats, max_line_bytes).await;
    }

    // Single post-stream invalidation so stored notes appear in FTS results
    // (skipped when nothing was stored).
    if stats.success > 0 {
        search_cache.invalidate_all().await;
    }

    let _ = tx.send(IngestFrame::done(&stats)).await;
    // `tx` drops here — the SSE channel closes and the client's stream ends.
}

/// Handle one line-event: build its frame and send it. Returns `Break` only when
/// the receiver is gone (client disconnected), so the pump can stop promptly.
async fn dispatch<N: NoteSink>(
    ev: LineEvent,
    sink: &N,
    tx: &mpsc::Sender<IngestFrame>,
    stats: &mut IngestStats,
    max_line_bytes: usize,
) -> ControlFlow<()> {
    let frame = match ev {
        LineEvent::Overflow => overflow_frame(stats, max_line_bytes),
        LineEvent::Line(bytes) => {
            if is_blank(&bytes) {
                return ControlFlow::Continue(());
            }
            process_line(&bytes, sink, stats).await
        }
    };
    match tx.send(frame).await {
        Ok(()) => ControlFlow::Continue(()),
        Err(_) => ControlFlow::Break(()),
    }
}

/// Parse + store one non-blank line, advancing the counters and producing its
/// `ack` frame.
async fn process_line<N: NoteSink>(raw: &[u8], sink: &N, stats: &mut IngestStats) -> IngestFrame {
    stats.line_no += 1;
    let line = stats.line_no;
    match parse_ingest_line(raw) {
        Ok(req) => match sink.store(req).await {
            Ok(note_id) => {
                stats.success += 1;
                IngestFrame::ack_ok(line, note_id)
            }
            Err(e) => {
                stats.errors += 1;
                IngestFrame::ack_error(line, &e)
            }
        },
        Err(e) => {
            stats.errors += 1;
            IngestFrame::ack_error(line, &e)
        }
    }
}

fn overflow_frame(stats: &mut IngestStats, max_line_bytes: usize) -> IngestFrame {
    stats.line_no += 1;
    stats.errors += 1;
    IngestFrame::ack_error(
        stats.line_no,
        &format!("line exceeds {max_line_bytes} byte limit"),
    )
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
        (status = 200, description = "SSE stream: one `ack` per line, terminal `done` summary"),
        (status = 401, description = "Missing or invalid bearer token"),
    )
)]
pub async fn ingest_stream_handler(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    body: Body,
) -> Response {
    let (tx, rx) = mpsc::channel::<IngestFrame>(INGEST_STREAM_CHANNEL_CAPACITY);
    let pool = state.db.pool.clone();
    let schema = archive_ctx.schema.clone();
    let search_cache = state.search_cache.clone();
    let max_line_bytes = ingest_max_line_bytes();

    tokio::spawn(async move {
        match DbNoteSink::new(pool, schema) {
            Ok(sink) => {
                pump_ingest_stream(
                    body.into_data_stream(),
                    tx,
                    sink,
                    search_cache,
                    max_line_bytes,
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

// =============================================================================
// TESTS — pure logic + pump resource discipline (no DB / no server)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        let f = IngestFrame::ack_ok(3, id);
        assert_eq!(f.event, "ack");
        let j = frame_json(&f);
        assert_eq!(j["line"], 3);
        assert_eq!(j["status"], "ok");
        assert_eq!(j["note_id"], id.to_string());
    }

    #[test]
    fn frame_ack_error_shape() {
        let f = IngestFrame::ack_error(5, "boom");
        assert_eq!(f.event, "ack");
        let j = frame_json(&f);
        assert_eq!(j["line"], 5);
        assert_eq!(j["status"], "error");
        assert_eq!(j["error"], "boom");
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
        let error_owned = String::from("transient failure");
        let f = IngestFrame::ack_error(9, &error_owned);
        drop(error_owned);
        let j = frame_json(&f);
        assert_eq!(j["error"], "transient failure");
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
    async fn drive<B>(stream: B, max: usize) -> Vec<(&'static str, serde_json::Value)>
    where
        B: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
    {
        let (tx, mut rx) = mpsc::channel::<IngestFrame>(64);
        pump_ingest_stream(stream, tx, MockSink, SearchCache::disabled(), max).await;
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
        drive(body_of(chunks), max).await
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
        let frames = drive(stream, 50).await;
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
}

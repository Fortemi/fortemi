# Streaming + Gateway Worklist

Consolidated execution plan for all outstanding streaming and Universal Model
Gateway work. Two release tracks: **Track 1 (streaming, gates v2026.6.0)** and
**Track 2 (gateway, separate release / feature-flagged)**.

Status legend: `[x]` done · `[~]` in progress · `[ ]` not started · `(ext)` cross-repo

Parent: #586 (Streaming Data Capture & Realtime Processing).
Roadmap: `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/incoming-streams-roadmap.md`

---

## Track 1 — Streaming (release gate for v2026.6.0)

v2026.6.0 is **staged but held** until Phase A–D epics (#811/#817/#824/#832) are
all closed.

### Phase A — Streaming chat · EPIC #811 · `P1` — ✅ DONE (closed 2026-06-13)

- [x] **A1** `POST /api/v1/chat/stream` SSE endpoint (drop-oldest backpressure)
- [x] **A3** backpressure metric `chat_stream_dropped_tokens_total` + GPU release (#814)
- [x] **A4** `Last-Event-ID` resumption, 60s Redis cursor TTL (#815)
- [x] **A5** contract tests for `/chat/stream` parallel to `/chat` (#549) — `54113e8`
  (HTTP/SSE boundary suite + async resource-discipline unit tests: Arc leak guard,
  no-dangling-sender, value-independent frame round-trip; clippy cognitive_complexity
  0 findings in `handlers/chat.rs`)
- [ ] **A2** HotM consumer contract (#813) — `(ext)` HotM repo; not a Fortemi blocker

**Done:** #811 closed; A1/A3/A4/A5 landed + pushed to main. #813 (A2) left open as
cross-repo HotM coordination — does not gate the cut.

### Phase B — Webhook receivers · EPIC #817 · `P2`

- [ ] #821 schema-shape registry + server-side validation *(foundation; has design comments)*
- [ ] #822 `Idempotency-Key` dedupe via Redis (24h TTL)
- [ ] #823 contract tests: signature, schema, idempotency

**Sequence:** #821 → #822 → #823 → close #817.
**Reuse:** Redis `ConnectionManager` pattern from `chat_stream_store.rs` / `search_cache.rs`.

### Phase C — Streaming bulk ingest + TUS finish · EPIC #824 · `P2`

- [x] #825 `POST /api/v1/ingest/stream` — NDJSON streaming bulk ingest *(foundation landed: bounded line-by-line parse + per-line `insert_tx` + SSE `ack`/`done`, store-only; outbox → #830, validation hardening → #826)*
- [x] #826 per-line validation + SSE-streamed per-line response codes *(landed: DB-free schema validation — tag depth/length + metadata-object; `progress {processed:N}` every `FORTEMI_INGEST_PROGRESS_INTERVAL`=100; unified `ack` contract retained)*
- [ ] #827 backpressure — bounded buffer + 429 early-warning
- [ ] #828 `X-Ingest-Cursor` resumption (60s TTL) — **reuse chat-stream resumption pattern**
- [ ] #829 per-stream bearer token auth + rate limit
- [ ] #830 wire `/ingest/stream` into `event_outbox` — **verify dep #592 status first**
- [ ] #831 finish TUS resumable upload (closes #544 stub)

**Sequence:** #825 → {#826, #827, #828, #829 parallel} → #830. #831 independent
(parallel any time). Close #824 when all land.

### Phase D — External tech event sources · EPIC #832 · `P3` (Low)

- [ ] #833 `InboundEventSource` connector trait + plug-in scaffold *(foundation)*
- [ ] #834 Redis Stream consumer connector *(first concrete source)*
- [ ] #835 SSE consumer connector (long-lived HTTP)
- [ ] #836 Kafka consumer connector (feature-flagged, high-end tier)

**Sequence:** #833 → {#834, #835, #836 parallel} → close #832.

### → Cut v2026.6.0

When #811, #817, #824, #832 are all closed: bump CalVer `2026.6.0`, tag `v2026.6.0`,
update CHANGELOG, let CI publish (release jobs un-skip on tag). **Full release only**
(no RC) unless explicitly requested.

---

## Track 2 — Universal Model Gateway · EPIC #863 · separate release

Single control point for all model ops: any protocol in, any backend out, max
token value, precise cost tracking. Provider secrets never leave Fortemi.
Default-off (`FORTEMI_BRIDGE_ENABLED`). 17 children (#864–#880).

### Wave 1 — Foundations (unlocks everything)

- [ ] **B10** #873 protocol-adapter framework + canonical request/response model `P1` `effort/large`
- [ ] **B1** #864 OpenAI `/v1/chat/completions` (non-streaming) + per-key auth `P1`
- [ ] **B14** #877 token + cost accounting: usage, pricing tables, persistent ledger `P1`

### Wave 2 — Breadth (protocols + backends)

- [ ] **B2** #865 OpenAI streaming (SSE) — reuse chat pump + resumption `P1`
- [ ] **B3** #866 `/v1/models` + qualified-slug routing
- [ ] **B6** #869 `/v1/embeddings` passthrough
- [ ] **B11** #874 Anthropic Messages API (in + out)
- [ ] **B12** #875 Google Gemini API (in + out) `P3`
- [ ] **B13** #876 vLLM / LiteLLM / Azure OpenAI / AWS Bedrock outbound `P3`

### Wave 3 — Control + value

- [ ] **B7** #870 per-consumer policy on API keys (allowed models/routes, quotas)
- [ ] **B4** #867 routing protocols (single/fallback/round-robin) via `X-Fortemi-Route`
- [ ] **B15** #878 cost/utilization reporting endpoints + Prometheus metrics
- [ ] **B16** #879 budgets & spend alerts (soft/hard caps)
- [ ] **B17** #880 token-value optimization: response caching + cost/quality-aware routing

### Wave 4 — Pipeline

- [ ] **B5** #868 session-logging toggle → `bridge-session` Note (streaming-safe tee)
- [ ] **B8** #871 analytics worker: session notes → create/update Gitea issues

### Continuous

- [ ] **B9** #872 docs (`provider-bridge.md`), OpenAPI, contract tests — author incrementally

### → Cut gateway release (v2026.7.0 or feature-flag earlier)

Exit: any-protocol client + Fortemi key completes streaming/non-streaming against
any backend (secret never client-side); every request metered; budget caps enforce;
cache hit serves at $0; bridge off by default.

---

## Critical path (shortest route to both cuts)

1. **A5** (#811 close) → Phase A complete.
2. **B/C/D** epics in parallel by priority: B (P2) and C (P2) before D (P3).
   - Phase C foundation #825 unblocks 4 children; do it early.
   - #828 (ingest resumption) directly reuses #815 work — cheap win.
3. Close #811/#817/#824/#832 → **cut v2026.6.0**.
4. Gateway Wave 1 (#873/#864/#877) → Wave 2–4 → **cut v2026.7.0**.

## Deferred (post-backlog)

- **#881** — EPIC: Project standards audit (async / sanitization / leak / complexity
  compliance across the Rust codebase). `blocked` until Track 1 (#817/#824/#832) and
  Track 2 (#863) are closed. New issues already meet the bar; #881 retrofits the
  pre-existing surface (incl. ~54 `cognitive_complexity` findings + `main.rs`
  decomposition). Standard: `memory/rust-implementation-standard.md`.

## Open dependencies to verify

- #830 depends on **#592** (event_outbox) — **VERIFIED STILL OPEN (2026-06-13)**: outbox
  helpers not yet implemented. #830 stays blocked until #592 lands. The #825 foundation
  (note-create + SSE acks) proceeds without it; outbox wiring is #830's scope.
- #813 (A2 HotM) lands in the HotM repo, not here — keep open as coordination only.

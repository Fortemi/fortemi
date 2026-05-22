# Incoming Streams — Phased Roadmap

**Parent epic:** [Fortemi/fortemi#586 — Streaming Data Capture & Realtime Processing](https://git.integrolabs.net/Fortemi/fortemi/issues/586)
**Sibling docs:** [`feature-plan.md`](./feature-plan.md) (outbound event bus), [`risk-register.md`](./risk-register.md), [`poc-plan.md`](./poc-plan.md)
**Status:** Draft for review
**Drafted:** 2026-05-21

## 1. Scope and Framing

Epic #586 covers two halves of the streaming problem:

- **Outbound** (the existing 19-child plan #590–#606): events flow OUT of Fortemi through Redis Streams + outbox to SSE, WebSocket, webhooks, and reactive consumers.
- **Inbound** (this roadmap): events and data flow INTO Fortemi from external sources — webhook receivers, long-lived ingest connections, bulk push, external event-source consumers, resumable uploads.

The two halves share infrastructure (the outbox is the integration boundary — inbound events land in the outbox first, then flow out via the existing Phase 1 plan). This roadmap defines the inbound-specific work.

### 1.1 Why incoming streams first

Three reasons to attack the inbound side before completing #586 Phase 1:

1. **The outbox is the integration seam** — every inbound stream eventually writes to the outbox. Defining inbound shapes before finalizing outbox schema (#591) lets us design the schema once.
2. **User-visible value lands faster** — a streaming `/chat`, a streaming bulk-ingest, or a webhook receiver delivers visible product value before the internal event bus is complete.
3. **Backpressure semantics surface early** — incoming streams force us to confront flow control, idempotency, and authentication concerns that the outbound-only plan defers.

### 1.2 Out of scope (intentional)

- File scan-and-ingest (#741 epic) — separate workstream.
- Bidirectional WebSocket protocol design (would warrant a full ADR; this roadmap stays REST/SSE/HTTP-stream-shaped).
- Multi-tenant ingest auth — current REQUIRE_AUTH=false default works as-is; supplementary auth is a separate concern.

## 2. Inventory — What Exists Today

### 2.1 Inbound surface (existing)

| Endpoint | Shape | Status |
|---|---|---|
| `POST /api/v1/notes` | One-shot create | Production |
| `POST /api/v1/notes/bulk` | Batch create (request body holds the batch) | Production |
| `POST /api/v1/notes/reprocess` | Bulk reprocess trigger | Production |
| `POST /api/v1/attachments` (multipart) | File upload | Production |
| `POST /api/v1/attachments` (raw body) | File upload (alt path) | Production |
| `POST /api/v1/inference/stream` | Inference call returning SSE stream — *server streams out*, client streams in token-by-token from response | Production (REF #628) |
| `POST /api/v1/chat` | One-shot chat request/response | Production (#549) |
| Audio job pipeline | Background chunked transcription (`AudioChunkTranscription` job type) | Production |
| TUS upload finalization | Partial — `GET` handler exists per #544 | Stub |

### 2.2 Outbound surface (existing)

| Endpoint | Shape | Status |
|---|---|---|
| `GET /api/v1/events` (SSE) | Server-sent events to subscribers | Production |
| `GET /api/v1/ws` (WebSocket) | Server-push WebSocket | Production (push-only today) |
| Webhook dispatcher | Fortemi POSTs to subscriber URLs | Production |

Note: WebSocket is currently push-only. Bidirectional WS for command/data input is a separate proposal, deferred.

### 2.3 Gap analysis

Missing inbound surfaces, ordered by user-visible impact:

1. **Streaming chat (`/chat/stream`)** — client opens a connection, sends user input, receives token-by-token assistant reply via SSE. Today `/chat` is one-shot; chat clients (HotM) cannot show typing.
2. **Webhook receivers** — third-party systems POST events to Fortemi (today only the reverse exists).
3. **Streaming bulk ingest (NDJSON POST)** — agents emit a stream of notes/events as newline-delimited JSON; server processes incrementally with backpressure. Current bulk is whole-batch.
4. **External event-source consumers** — Fortemi pulls from upstream Redis Stream, Kafka topic, or SSE source. Today only the internal redis-rs surface exists.
5. **Resumable upload (TUS finish)** — #544 left a stub; resumable upload for large media (multi-GB recordings) is incomplete.
6. **Streaming MCP tool calls** — long-running MCP tools emit progress events. Today MCP responses are one-shot.

## 3. Phased Roadmap

Four phases, ordered by dependency and user impact. Each phase ships a usable increment; later phases are not gated on completion of all earlier work unless explicitly noted.

### Phase A — Streaming Chat (visible product value)

**Goal:** First incoming-stream-shaped endpoint with end-to-end streaming UX. Highest user impact per unit of effort.

| Item | Description | Estimated complexity |
|---|---|---|
| A1 | `POST /api/v1/chat/stream` — SSE-streamed assistant tokens; reuses #628 inference stream backend | S — wire-up against existing `/api/v1/inference/stream` |
| A2 | Client contract update: HotM agent-proxy consumes the new endpoint | S — separate repo, separate ticket |
| A3 | Backpressure: drop oldest token if client lags, recorded in metric `chat_stream_dropped_tokens_total` | S |
| A4 | `Last-Event-ID` resumption — client can reconnect mid-stream within N seconds and skip to where it left off | M |
| A5 | Tests: HotM consumer contract covering streamed `/chat/stream` parallel to existing `/chat` tests (#549) | S |

**Exit criteria:**
- HotM UI shows typing-style token-by-token output
- p99 first-token latency < 800 ms (matches existing `/api/v1/inference/stream`)
- Drop-token metric is observable in `/api/v1/health/streaming` (depends on #596 — coordinate)

**Dependencies on #586 main path:** None. Builds on existing `/api/v1/inference/stream`.

### Phase B — Webhook Receivers (bidirectional integration)

**Goal:** Allow external systems to push events INTO Fortemi, completing the symmetric webhook story.

| Item | Description | Estimated complexity |
|---|---|---|
| B1 | `POST /api/v1/webhooks/incoming/{slug}` — generic receiver; payload + metadata land in `event_outbox` (depends on #591) | M |
| B2 | Receiver registration: `POST /api/v1/webhooks/incoming` to create a slug with HMAC secret + expected schema reference | M |
| B3 | HMAC signature verification (mirrors outbound dispatcher's signing format) | S |
| B4 | Schema-shape registry: per-slug expected JSON schema, validated server-side before outbox insert | M |
| B5 | Idempotency keys (`Idempotency-Key` header) — dedupe via short-window seen-set in Redis | S |
| B6 | Receiver tests: signature verification, schema rejection, idempotency replay | M |

**Exit criteria:**
- External system can register a receiver, push events via HMAC-signed POSTs, and see those events flow through the outbox → consumers
- Replay of the same `Idempotency-Key` within 24 hours returns 200 without duplicate outbox row
- Schema-violating payload returns 400 with descriptive error

**Dependencies on #586 main path:** Requires #591 (event_outbox table). If Phase B starts before #591 lands, it can write to a stub `incoming_webhook_log` table instead and migrate forward.

### Phase C — Streaming Bulk Ingest (agent-driven push)

**Goal:** Long-running agents push many notes/events as a stream rather than batched POST bodies.

| Item | Description | Estimated complexity |
|---|---|---|
| C1 | `POST /api/v1/ingest/stream` — `Content-Type: application/x-ndjson` body; chunked transfer encoding | M |
| C2 | Per-line validation + persistence; per-line response codes streamed back as SSE counter events | M |
| C3 | Backpressure: server applies bounded-buffer + `429` early-warning frames; client backs off | M |
| C4 | Resumption via stream cursor (`X-Ingest-Cursor` header) — client can reconnect, server tells it where to resume | L |
| C5 | Authentication: bearer-token-per-stream + per-stream rate limit (composes with global rate-limit middleware) | M |
| C6 | Integration with outbox — each ingested item writes a single outbox row in the same PG transaction as the note | S — depends on #592 |
| C7 | Tests: million-row stream load test, mid-stream connection drop, deliberate slow consumer | L |

**Exit criteria:**
- Streaming ingest sustains 5 000 notes/sec at p99 < 200 ms per-line ack on the mid-tier hardware reference rig
- Resumption from mid-stream cursor produces zero duplicates
- Outbox row count = ingested-item count (invariant)

**Dependencies on #586 main path:** Heavy — requires #591 (outbox table) and #592 (insert helpers). Sequencing: start B (above) which forces #591 to land; then C falls into place.

### Phase D — External Tech Event Sources (Fortemi as consumer)

**Goal:** Fortemi pulls from upstream technical event sources (external Redis Stream, upstream SSE, optionally Kafka) and processes them through the same pipeline as native events.

**Scope note (2026-05-21):** This phase is intentionally limited to **lightweight technical-source connectors**. Real-time provider integrations (Twilio Programmable Voice, WebRTC/SIP, Twilio Messaging, recording providers, live video ingest) are tracked in a **separate epic** because they involve media servers, codec negotiation, RTP, and a fundamentally different ingest pipeline. Cross-link: see real-time provider integration epic (filed separately).

| Item | Description | Estimated complexity |
|---|---|---|
| D1 | Event-source connector plug-in trait (`InboundEventSource`) — Rust trait, single method `async fn next_event() -> Result<InboundEvent>` | M |
| D2 | Redis Stream consumer connector — subscribe to external Redis Stream, normalize to `InboundEvent`, write to outbox | M |
| D3 | SSE consumer connector — long-lived HTTP client connection to upstream SSE source | M |
| D4 | Kafka consumer connector (high-end tier only, behind feature flag) | L — gated on hardware tier |
| D5 | Connector registration / management endpoints (`POST/GET/DELETE /api/v1/inbound-sources`) | M |
| D6 | Failure handling: dead-letter for malformed upstream events; backoff for connector errors | M |
| D7 | Tests: chaos test (kill upstream mid-stream), schema-mismatch DLQ behavior, connector restart resume | L |

**Exit criteria:**
- Fortemi can ingest from at least one external Redis Stream and one external SSE source
- Connector restart resumes from last-committed offset with no event loss
- Kafka connector is feature-flagged `INBOUND_KAFKA_ENABLED=false` by default on edge tier (per #586 cost-gate principle)

**Dependencies on #586 main path:** Requires Phase 1 (#590–#596) to be substantially complete — connectors write to the outbox using the same helpers consumers use.

## 4. Cross-Cutting Concerns

### 4.1 Backpressure strategy

Every inbound surface MUST implement one of:
- **Bounded buffer + 429** (default): server caps queued items; returns `429 Too Many Requests` with `Retry-After` once full. Used by Phase B and C.
- **Drop-oldest** (real-time only): server discards oldest queued item when full. Used by Phase A token streams where stale tokens are useless.
- **Block sender** (TCP-level): server stops reading the client socket. Used by Phase C resumable streams.

Backpressure policy is per-endpoint, documented in OpenAPI, and observable via per-endpoint metrics.

### 4.2 Idempotency

All non-streaming POSTs (Phase B receivers, Phase C per-line ingest) accept `Idempotency-Key` header. Server stores `(key, body_hash) → response` for 24h in Redis. Repeat key + matching body returns the original response; repeat key + different body returns `409 Conflict`.

Phase A streams do NOT need idempotency (each stream connection is unique by definition).

### 4.3 Authentication

Inbound auth follows the existing project decision (REQUIRE_AUTH default false per ADR-094; flip to true for shared deployments). Streaming endpoints additionally support:
- **Bearer tokens** (matching `REQUIRE_AUTH=true` semantics) for Phase B/C/D
- **HMAC signatures** for Phase B incoming webhooks (mirrors outbound webhook signing)
- **Stream tokens** (one-time, scoped to a single stream session) for Phase C/D

No new auth mechanism is introduced; we compose existing ones.

### 4.4 Outbox integration

The integration boundary between inbound and #586's outbound plan:

```
Inbound surface (Phase B/C/D) → outbox_insert(tx, OutboxEvent) → event_outbox row
                                                                  ↓
                                                       Publisher task (#593)
                                                                  ↓
                                                       Redis Stream
                                                                  ↓
                                                       Existing consumers (#594/595/596)
```

This means: **every inbound event becomes an outbound event automatically.** Consumers don't need to know whether an event originated locally or from an external source — the outbox is the canonical event store.

### 4.5 Cost-gate alignment

Per the cost-gate on #586 Phase 1 kickoff, each phase below MUST document its standby/runtime cost before merging the first PR:

| Phase | Standby cost when no events flow | Cost source |
|---|---|---|
| A — streaming chat | Zero (per-request connection) | One outbound SSE channel per active chat |
| B — webhook receiver | Low — endpoint listener + Redis dedupe set TTL | Redis memory for idempotency set |
| C — streaming bulk ingest | Zero (per-request connection) | Per-stream auth token state in Redis |
| D — external event-source consumers | **Non-trivial** — long-lived connections to upstream sources, even when no events flow | Connection count + connector task scheduling |

Phase D is the cost-gate concern. Default `INBOUND_EXTERNAL_SOURCES_ENABLED=false` on edge.

## 5. Sequencing Recommendation

```
Phase A (streaming chat)      ────────────────►  ship independently, no #586 dependency
                                                  │
                                                  │
Phase B (webhook receivers)   ─────►  starts when #591 (event_outbox) is ready
                                       │
                                       │
Phase C (streaming bulk)      ─────►   starts when #592 (outbox helpers) is ready
                                        │
                                        │
Phase D (external sources)    ─────►    starts when #593 (publisher) is ready
                                                                        │
                                                                        │
                                                              (cost-gate per Phase D)
```

**Recommended order:**
1. Open Phase A as a small standalone epic with 3–5 child issues. Ship first; visible to users.
2. Resolve #586 cost gate (the prerequisite for Phase 1 of the outbound plan).
3. Start #591 (event_outbox migration) — Phase B can start immediately after.
4. #592 → Phase C can begin (Phase B and C parallelize from this point).
5. #593 → Phase D unblocks; gate the Kafka connector behind a feature flag.

## 6. Decisions (resolved 2026-05-21)

All prior open questions have been answered by the operator. Decisions recorded below as the authoritative answer; filing of child issues proceeds against these.

1. **`/chat/stream` as a sibling** to `/chat`. Both endpoints coexist; consumers migrate at their own pace. Existing `/chat` contract (#549) and tests stay intact.
2. **Webhook receiver HMAC required per receiver.** Mirrors the outbound dispatcher signing format symmetrically. No bearer-token fallback for receivers; bearer tokens remain available for other inbound endpoints.
3. **Finish #544's TUS stub** as part of Phase C. Resumable upload for large media is a real HotM use case; the stub is completed rather than removed.
4. **Phase D connector order:** Redis Stream → SSE → Kafka. Kafka behind `INBOUND_KAFKA_ENABLED=false` feature flag on edge tier (per cost-gate principle).
5. **Filing strategy:** all four phases filed up front as sub-epics under #586 with their children. Phase A can ship in parallel with #586 Phase 1 cost-gate resolution.
6. **Backpressure policy** stays per-phase as drafted in §4.1: chat (drop-oldest), webhooks/bulk (bounded buffer + 429), long-stream bulk (block-sender / TCP backpressure).
7. **Resumption TTL: 60 seconds** for all cursored streams (Phase A `Last-Event-ID`, Phase C `X-Ingest-Cursor`). Short window covers transient network blips; client must reconnect fast. Low Redis memory cost.
8. **Issue labels:** P1/P2/P3 per existing repo convention. Phase A = P1, Phase B/C = P2, Phase D = P3. No milestone.

### Scope split: real-time provider integration is a separate epic

Operator-introduced scope: Twilio Programmable Voice (live audio), WebRTC/SIP, recording providers, and live video ingest are valuable but are a **different domain** from the technical-source connectors in Phase D. Decision: file as a **separate epic** with its own scoping research as the first child. That epic will:

- Leverage the SDLC framework's research → inception → elaboration discovery process
- Reference the internal `roctinam/research-papers` corpus
- Search public sources (Twilio Media Streams docs, WebRTC RFCs, SIP/RTP, LiveKit/Agora docs, vendor benchmarks)
- File INDUCT issues in `roctinam/research-papers` for any new papers or key references discovered
- Output: scoped epic body, ADR list, child issue plan

Cross-linked from #586 but tracked independently.

## 7. Filing Strategy (resolved)

Decision: file all four phases as sub-epics under #586 with their children, plus a separate real-time provider epic.

| Sub-epic | Children | Priority |
|---|---|---|
| Phase A — streaming chat | A1–A5 (5) | P1 |
| Phase B — webhook receivers | B1–B6 (6) | P2 |
| Phase C — streaming bulk ingest + TUS finish | C1–C7 (7) | P2 |
| Phase D — external tech event sources | D1–D4 (4) | P3 |
| Real-time provider integration (separate epic) | Scoping research (1) — implementation children TBD | P2 |

Children carry P1/P2/P3 labels per existing repo convention. No milestone.

## 8. Artifacts to Produce After Approval

- ADR-005: Inbound streaming surface — endpoint shapes, backpressure policy, auth composition
- OpenAPI updates: new `/chat/stream`, `/webhooks/incoming/*`, `/ingest/stream`, `/inbound-sources/*` paths
- Risk register entries: backpressure correctness, idempotency window, connector restart correctness
- Test plan: load test fixtures for Phase C (million-row NDJSON stream), chaos scenarios for Phase D

## References

- Parent epic: [#586](https://git.integrolabs.net/Fortemi/fortemi/issues/586) — Streaming Data Capture & Realtime Processing
- Outbound feature plan: [`feature-plan.md`](./feature-plan.md)
- ADRs (accepted): `.aiwg/architecture/streaming-realtime/ADR-001..004.md`
- Research synthesis: `../research-complete/working/streaming-realtime/research-synthesis.md`
- HotM chat consumer contract: [#549](https://git.integrolabs.net/Fortemi/fortemi/issues/549) (closed)
- BT6 bollard contract: [`docs/deployment/bt6-bollard.md`](../../../../docs/deployment/bt6-bollard.md)

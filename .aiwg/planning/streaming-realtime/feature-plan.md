# Feature Plan: Streaming Data Capture & Realtime Processing

**Date**: 2026-04-09 (updated)
**Track**: fortemi/streaming-realtime
**Phase**: Elaboration (Architecture Decisions Accepted)
**Research**: `.aiwg/research/streaming-realtime/research-synthesis.md`
**ADRs**: `.aiwg/architecture/streaming-realtime/ADR-001..004`

---

## 1. Problem Statement

Fortemi processes knowledge through batch job queues (SKIP LOCKED). When a note is created or updated, extraction, embedding, linking, and graph maintenance run as discrete, polled jobs. This introduces latency, prevents live search updates, and underutilizes the existing event infrastructure (SSE, WebSocket, webhooks).

The goal is to add streaming data capture and realtime processing that:
- Works within existing hardware tier constraints (6-8GB edge through 24GB+ high-end)
- Leverages existing infrastructure (PostgreSQL, Redis, tokio, EventBus)
- Enables incremental AI pipeline processing (not full batch reruns)
- Scales feature availability based on hardware profile

## 2. Hardware Tier Strategy

### Principle: Progressive Enhancement

Not all features are available on all tiers. Lower-spec systems get the core benefits (event-driven wake-up, streaming notifications) while higher-spec systems get full realtime processing.

| Capability | Edge (6-8GB) | Mid (12-16GB) | High-End (24GB+) |
|------------|:---:|:---:|:---:|
| Event-driven job wake-up | Y | Y | Y |
| Redis Streams event fan-out | Y | Y | Y |
| SSE/WebSocket from Redis Stream | Y | Y | Y |
| FTS index updates (realtime) | Y | Y | Y |
| Incremental embedding | - | Y | Y |
| Incremental graph linking | - | Y | Y |
| Streaming NER/extraction | - | - | Y |
| Live semantic search updates | - | - | Y |
| Dynamic GPU batching | - | Y | Y |
| CDC (full change capture) | - | Optional | Y |

### Configuration

```bash
# .env — streaming feature tier
STREAMING_TIER=edge          # edge | mid | high
# Or auto-detect from COMPOSE_PROFILES
# edge → STREAMING_TIER=edge
# gpu-12gb → STREAMING_TIER=mid
# gpu-24gb → STREAMING_TIER=high
```

## 3. Architecture

### 3.1 Data Flow (Target State)

```
Note Created/Updated
        |
        v
  [PostgreSQL] ──NOTIFY──> [PgListener] ──> [Job Worker wake-up]
        |
        v (after commit)
  [Redis Stream XADD] ──> [Consumer Group]
        |                       |
        |                  [Processing Pipeline]
        |                       |
        |              ┌────────┼────────┐
        |              v        v        v
        |         [Embed]  [Extract]  [Link]
        |              |        |        |
        |              v        v        v
        |         [pgvector] [concepts] [graph]
        |
        v
  [SSE/WebSocket Fan-out]
        |
        v
  [Connected Clients]
```

### 3.2 Key Components (Decisions Accepted)

**Stream Processing** (ADR-001: Direct redis-rs):
- Direct redis-rs usage via raw `redis::cmd(...)` stream commands unless a
  typed command materially simplifies the remaining outbound bus implementation
- No abstraction layer — YAGNI for single-node architecture
- Consumer group management via XREADGROUP/XACK
- Hardware-tier-aware feature gating
- Metrics and health reporting

**Event Outbox** (ADR-002: Manual outbox pattern):
- `event_outbox` table for durable event capture
- Application-level write in same PG transaction as data change
- Background worker reads outbox, publishes to Redis Stream, marks consumed
- Cleanup: DELETE consumed rows older than retention window

**Message Broker** (ADR-003: Redis Streams):
- Existing Redis instance — zero new infrastructure
- Stream key: `fortemi:events` (configurable)
- MAXLEN ~10000 trimming (configurable via `REDIS_STREAM_MAX_LEN`)
- PG outbox is source of truth — Redis can be rebuilt from outbox on data loss

**Edge Realtime Scope** (ADR-004: Selective realtime):
- Realtime: event-driven wake-up + SSE/WebSocket + FTS index
- Batch: embedding, linking, extraction, graph maintenance
- FTS is CPU-only (PG-native on INSERT) — zero GPU cost

**Incremental Processing**:
- Change-aware embedding: only re-embed notes where content changed
- Change-aware linking: only recompute links for affected nodes
- Change-aware extraction: run NER only on new/modified content

## 4. Implementation Roadmap

### Phase 0: Foundation — Event-Driven Wake-up — COMPLETE
**Status**: Already implemented (Issue #417, discovered 2026-04-09)

The worker already uses `tokio::Notify` for instant (<1ms) event-driven wake-up:
- [x] `PgJobRepository` holds `Arc<Notify>` (`jobs.rs:20`)
- [x] `queue()` and `queue_deduplicated()` call `notify.notify_waiters()` after INSERT
- [x] Worker loop uses `tokio::select!` with notify + 60s safety-net poll + shutdown
- [x] Wake latency: <1ms (in-process signaling, no network round trip)

**Note**: `tokio::Notify` is superior to `PgListener` for single-process. PgListener adds network overhead and the `NotifyQueueLock` global lock risk. Issues #588, #589 closed.

### Phase 1: Redis Streams Event Bus
**Scope**: Enable Redis Streams. Replace tokio broadcast for cross-process fan-out.

**Tasks**:
- [ ] Enable `streams` feature on redis crate in Cargo.toml
- [ ] Create `event_outbox` table (migration)
- [ ] Implement outbox publisher: after PG commit, XADD to Redis Stream
- [ ] SSE handler: consume from Redis Stream XREADGROUP instead of tokio broadcast
- [ ] WebSocket handler: same Redis Stream consumption
- [ ] Webhook dispatcher: consume from Redis Stream
- [ ] Stream trimming: MAXLEN ~10000 (configurable via `REDIS_STREAM_MAX_LEN`)
- [ ] Consumer group initialization on startup
- [ ] Health endpoint: expose stream lag metrics

**Completion criteria**: SSE/WebSocket events delivered from Redis Stream; works across multiple API instances

### Phase 2: Incremental AI Pipeline
**Scope**: Change-aware processing. Requires mid-tier hardware.

**Tasks**:
- [ ] Change tracking: hash-based dirty detection for note content
- [ ] Incremental embedding: skip re-embed if content hash unchanged
- [ ] Incremental linking: recompute only links involving changed notes
- [ ] Dynamic GPU batching: replace fixed semaphore with memory-aware admission
- [ ] Streaming extraction: trigger NER on note create/update event
- [ ] Batch coalescing: group rapid edits (debounce) before processing

**Completion criteria**: Embedding/linking cost proportional to changes, not corpus size

### Phase 3: Advanced Streaming
**Scope**: CDC, live search, model upgrades. High-end tier features.

**Tasks**:
- [ ] CDC integration (evaluate pgwire-replication or Sequin if outbox proves insufficient)
- [ ] Live search: streaming vector index updates via VectraFlow patterns
- [ ] Drift-Adapter: zero-downtime embedding model migration
- [ ] Streaming graph construction: iText2KG-inspired incremental KG

**Completion criteria**: Full realtime pipeline on high-end; search results update within seconds of note creation

## 5. Dependencies and Risks

| Risk | Impact | Mitigation | Status |
|------|--------|------------|--------|
| Redis Streams adds memory pressure on edge | High | MAXLEN trimming, small buffer sizes, monitor RSS | Open |
| GPU VRAM contention with streaming processing | High | Strict tier gating; CPU-only streaming on edge (ADR-004) | Mitigated |
| LISTEN/NOTIFY global lock at scale | Low | Use as wake-up signal only, not primary stream | Mitigated |
| ~~pgwire-replication immaturity~~ | ~~Medium~~ | ~~Fork and maintain~~ | Retired (ADR-002: chose outbox) |
| ~~SeaStreamer limited community~~ | ~~Medium~~ | ~~Evaluate before committing~~ | Retired (ADR-001: chose direct redis-rs) |

## 6. Success Metrics

- **Latency**: Job processing starts within 100ms of insertion (vs current poll interval)
- **Throughput**: Event fan-out supports 100+ concurrent SSE clients from Redis Stream
- **Efficiency**: Embedding/linking cost reduced by 80%+ through incremental processing
- **Availability**: Streaming features degrade gracefully per hardware tier
- **Reliability**: At-least-once event delivery via outbox pattern

## 7. Related Issues

- Research sources filed as `[INDUCT]` issues in `section9/research-papers`
- Feature issues to be filed in `fortemi/fortemi` after planning approval
- Research track: `fortemi/streaming-realtime`

## 8. References

- Research synthesis: `.aiwg/research/streaming-realtime/research-synthesis.md`
- Existing EventBus: `crates/matric-core/src/events.rs`
- Existing job queue: `crates/matric-db/src/jobs.rs`
- Redis dependency: `crates/matric-api/Cargo.toml` (redis 0.27)
- Hardware profiles: `docker-compose.bundle.yml` (COMPOSE_PROFILES)

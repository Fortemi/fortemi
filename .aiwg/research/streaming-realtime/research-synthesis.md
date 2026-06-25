# Streaming Data Capture & Realtime Processing — Research Synthesis

**Date**: 2026-04-08
**Track**: fortemi/streaming-realtime
**Status**: Research Complete, Planning In Progress

## Executive Summary

This document synthesizes research across 51 sources (academic papers, industry frameworks, production architectures) on streaming data capture and realtime processing for Fortemi. The research covers four domains: stream processing foundations, industry frameworks, realtime AI/ML processing, and PostgreSQL/Redis streaming patterns.

**Key finding**: Fortemi already has production-ready event *output* (SSE, WebSocket, webhooks, 46+ event types). The gap is streaming data *input* — capturing changes, processing them in realtime, and feeding the AI pipeline incrementally rather than through batch job queues.

**Recommended architecture**: A hardware-tiered approach that starts with zero new dependencies (PgListener + Redis Streams) on edge, scaling to dedicated stream processors on high-end systems.

---

## 1. Current State Assessment

### What Fortemi Already Has (Production-Ready)
- **SSE endpoint** (`/api/v1/events`) with filtering, replay (1024-event ring buffer), coalescing, auth
- **WebSocket endpoint** (`/api/v1/ws`) for legacy HotM compatibility
- **Webhook system** with HMAC signing, delivery tracking, auto-disable after 10 failures
- **EventBus** (tokio broadcast) with 46+ event types, 256 subscriber capacity
- **Background job worker** using PostgreSQL SKIP LOCKED queue
- **OpenAI-compatible streaming** for inference responses
- **AsyncAPI 3.0 spec** auto-generated from code
- **Redis** in Docker bundle (currently used for caching only)

### The Gap: Streaming Input & Realtime Processing
1. **No CDC**: Changes to notes/tags/embeddings are not streamed — consumers must poll
2. **Batch-oriented AI pipeline**: Extraction, embedding, linking run as discrete batch jobs
3. **No incremental graph updates**: `POST /api/v1/graph/maintenance` is a full batch operation
4. **Polling-based job worker**: Workers poll the job queue instead of being event-driven
5. **No streaming search**: Search results are point-in-time, not live-updating

---

## 2. Research Findings by Domain

### 2.1 Stream Processing Foundations (13 Academic Sources)

**Seminal works**:
- **The Dataflow Model** (Akidau et al., 2015) — Unified batch+streaming via windowing, watermarks, triggers, accumulation. Defines *what/where/when/how* decomposition.
- **MillWheel** (Google, 2013) — Exactly-once with persistent versioned state. Basis for processing guarantees.
- **Chandy-Lamport** (1985) + **ABS** (Carbone et al., 2015) — Checkpoint/recovery without stalling ingestion.

**Directly applicable**:
- **One SQL to Rule Them All** (Begoli et al., SIGMOD 2019) — Extends SQL for streaming via time-varying relations. Enables streaming queries over PostgreSQL without abandoning sqlx.
- **Watermarks** (Begoli et al., VLDB 2021) — Formal semantics for "when is it safe to process this batch?"

**Edge-critical**:
- **Count-Min Sketch** + **HyperLogLog** — Sublinear-space data structures for tracking frequencies and cardinalities on 6-8GB hardware without full materialization.
- **Edge Learning Survey** (2025) — Continual learning on resource-constrained devices; concept drift detection.

**Knowledge graph streaming**:
- **Stream Reasoning + KG Survey** (Springer, 2025) — Core taxonomy for the intersection of streaming and knowledge graphs.
- **RDF Stream Processing** (VLDB Journal, 2025) — Windowed graph queries and continuous query semantics.

### 2.2 Industry Frameworks (14+ Sources)

**Already in stack (zero new deps)**:
| Tool | Fit | Notes |
|------|-----|-------|
| Redis Streams | Edge through high-end | Already deployed. Enable `streams` feature on redis crate. Consumer groups, XADD/XREADGROUP. |
| PG LISTEN/NOTIFY | Edge (signaling only) | Via sqlx PgListener. At-most-once, 8KB limit. Use as wake-up signal, not primary stream. |
| tokio channels | All tiers | Already the runtime. broadcast/mpsc/watch for in-process routing. |

**Low-effort additions**:
| Tool | Fit | Notes |
|------|-----|-------|
| SeaStreamer | All tiers | Backend-agnostic Rust streaming. Write once, swap Redis↔Kafka at deploy time. **Strategic choice.** |
| Sequin | Mid through high-end | PostgreSQL CDC in single Docker container. 6.8x faster than Debezium. Streams to Redis Streams. |
| pgwire-replication | Edge (in-process) | Pure Rust WAL consumer. Embed CDC in matric-jobs. Immature but functional. |
| async-nats / NATS | All tiers | 30-50MB footprint. Step-up from Redis Streams for stronger guarantees. |

**High-end only**:
| Tool | Fit | Notes |
|------|-----|-------|
| Redpanda | High-end | Kafka-compatible, no JVM. Single binary. |
| Kafka + rdkafka | High-end | Industry standard. 4-6GB RAM minimum. |
| RisingWave | High-end | Streaming SQL. PG wire-compatible. Native CDC. |

**Avoid on edge**:
- Any JVM-based tool (Kafka, Debezium, Flink) — VRAM is consumed by LLM, RAM budget <500MB for streaming

### 2.3 Realtime AI/ML Processing (13 Sources)

**Streaming inference**:
- **StreamingLLM** (ICLR 2024) — Attention sinks enable infinite-length streaming with 22.2x speedup. Applicable to llama.cpp server configuration.
- **BucketServe** (2025) — Dynamic batching by sequence length, 3.58x throughput. Replace fixed GPU semaphore.
- **Memory-Aware Batching** (2025) — Dynamic VRAM-aware admission controller instead of fixed concurrency.
- **Aegaeon** (SOSP 2025) — GPU model swapping between embedding and generation models. Critical for edge.

**Incremental knowledge graph**:
- **iText2KG** (WISE 2024) — Zero-shot incremental KG construction. Modules: Document Distiller, Incremental Entities Extractor, Incremental Relations Extractor, Graph Integrator. "Blueprints" align with DocumentType registry.
- **IncRML** (Semantic Web Journal, 2024) — Incremental KG updates via CDC. 11-57x faster than full regeneration. Publishes changes as Linked Data Event Streams.

**Streaming search**:
- **VectraFlow** (CIDR 2025) — First system for vector processing in streaming context. Two-stage sparse-then-dense. Enables "live search" that updates as notes are created.
- **Drift-Adapter** (EMNLP 2025) — Zero-downtime embedding model upgrades. 95-99% recall recovery with <10us overhead. 100x cost reduction vs full re-embedding.

**Edge optimization**:
- **Quantized LLM evaluation** (ACM ToIT, 2025) — Per-task quantization: Q4 for NER/extraction, Q5/Q6 for revision.
- **DVFS for edge inference** (USENIX ATC 2025) — Per-layer frequency scaling to reduce thermal throttling.

**Streaming RAG**:
- **StreamingRAG** (2025) — Temporal knowledge graphs from streaming data. 5-6x throughput. Validates Fortemi's multi-model cascade approach.

### 2.4 PostgreSQL + Redis Patterns (11 Sources)

**Critical findings**:
- **LISTEN/NOTIFY does NOT scale** at high concurrency (Recall.ai post-mortem, 2025). Global `NotifyQueueLock` serializes all commits. Safe at Fortemi's current scale, but not as primary stream backbone.
- **Outbox pattern** is the recommended approach: write events to PG table + NOTIFY as wake-up signal. At-least-once delivery.
- **Redis Streams as hot delivery path**: PG is source of truth, Redis Streams is the real-time fan-out layer. Pattern validated at Stripe scale (Brandur).
- **Partitioned event tables**: RudderStack achieved 100K events/sec with 100K-row partitions + DROP TABLE for completed partitions. pg_partman automates this.
- **Multi-tenant queue isolation**: Hatchet's per-tenant partitioning maps to Fortemi's per-archive (multi-memory) isolation.

**Concrete next steps already validated**:
1. Add `PgListener` wake-up to job worker (~20 lines of code)
2. Enable `streams` feature on redis crate (zero new infra)
3. Use outbox pattern: write event row + NOTIFY, consume from Redis Stream

---

## 3. Hardware-Tiered Architecture Recommendation

### Edge Tier (6-8GB VRAM — RTX 3060 8GB, 4060, 5060)

**Constraints**: VRAM entirely consumed by LLM (qwen3.5:9b). Streaming must be CPU-only, <200MB RAM.

| Component | Implementation |
|-----------|---------------|
| Internal routing | tokio broadcast/mpsc (existing) |
| Change notifications | PG LISTEN/NOTIFY via sqlx PgListener |
| Message broker | Redis Streams (existing Redis) |
| Stream processing | SeaStreamer with Redis backend |
| CDC | pgwire-replication (in-process, no external deps) |
| Processing mode | Selective: realtime for search index + SSE, batch for embedding/linking |
| GPU scheduling | Memory-aware admission (replace fixed semaphore) |
| Quantization | Q4_K_M for extraction, Q5_K_M for revision |

### Mid Tier (12-16GB VRAM — RTX 3060 12GB, 4070, 5070)

| Component | Implementation |
|-----------|---------------|
| Internal routing | tokio broadcast/mpsc |
| Change notifications | PG LISTEN/NOTIFY |
| Message broker | Redis Streams or NATS JetStream |
| Stream processing | SeaStreamer with Redis backend |
| CDC | Sequin (single Docker container) |
| Processing mode | Realtime extraction + embedding, batch for full graph maintenance |
| GPU scheduling | Dynamic batching (BucketServe approach) |

### High-End Tier (24GB+ VRAM — RTX 3090, 4090, 5090)

| Component | Implementation |
|-----------|---------------|
| Internal routing | tokio broadcast/mpsc |
| Change notifications | PG LISTEN/NOTIFY |
| Message broker | Redpanda or NATS JetStream |
| Stream processing | SeaStreamer with Kafka backend, or RisingWave |
| CDC | Sequin or RisingWave native CDC |
| Processing mode | Full realtime pipeline |
| GPU scheduling | Aegaeon-style model pooling + dynamic batching |

---

## 4. Implementation Phases

### Phase 0: Foundation (Zero New Dependencies)
- Add `PgListener` to job worker for event-driven wake-up
- Enable `streams` feature on redis crate
- Implement outbox pattern: event table + NOTIFY + Redis Stream XADD
- Replace polling in job worker with XREADGROUP consumer

### Phase 1: Streaming Event Bus
- New `matric-stream` crate or extend `matric-core` events
- SeaStreamer integration for backend-agnostic stream processing
- Consumer groups for parallel processing across worker instances
- Stream-driven SSE/WebSocket (consume from Redis Stream instead of tokio broadcast)

### Phase 2: Incremental AI Pipeline
- Incremental embedding computation (only re-embed changed notes)
- Incremental graph linking (IncRML-inspired change detection)
- Streaming NER/concept extraction on note creation/update
- Dynamic GPU batching (BucketServe approach)

### Phase 3: Advanced Streaming
- CDC via Sequin or pgwire-replication for full change data capture
- VectraFlow-inspired live search (results update as notes are created)
- Drift-Adapter for zero-downtime embedding model upgrades
- Streaming knowledge graph construction (iText2KG patterns)

### Phase 4: High-End Features
- Redpanda/Kafka integration via SeaStreamer's Kafka backend
- RisingWave for streaming SQL analytics
- Aegaeon-style GPU pooling for multi-model concurrent serving
- Full temporal knowledge graph with StreamingRAG patterns

---

## 5. Strategic Decisions Required

1. **SeaStreamer vs custom tokio streams**: SeaStreamer provides backend abstraction but is a new dependency. Custom tokio streams are lighter but lock in to a single backend.

2. **CDC approach**: pgwire-replication (in-process, immature) vs Sequin (container, mature) vs manual outbox (proven, no CDC).

3. **Graph maintenance**: Keep batch `POST /api/v1/graph/maintenance` and add incremental path, or replace entirely?

4. **Redis Streams vs NATS**: Redis is already deployed. NATS adds stronger guarantees and replay but is a new service.

5. **Scope for edge tier**: What is "realtime" on edge? Just search index + SSE notifications? Or also extraction?

---

## 6. Source Index

### Academic (13 sources)
| # | Title | Year | Key Concept |
|---|-------|------|-------------|
| A1 | The Dataflow Model | 2015 | Windowing, watermarks, triggers |
| A2 | MillWheel | 2013 | Exactly-once processing |
| A3 | Lightweight Asynchronous Snapshots (ABS) | 2015 | Barrier snapshotting |
| A4 | Chandy-Lamport Distributed Snapshots | 1985 | Global state recording |
| A5 | Processing Flows of Information (CEP Survey) | 2012 | Complex event processing taxonomy |
| A6 | Bridging the Gap: CEP on Stream Processing | 2024 | CEP-to-stream operator mapping |
| A7 | One SQL to Rule Them All | 2019 | SQL streaming extensions |
| A8 | Watermarks in Stream Processing | 2021 | Watermark semantics |
| A9 | Count-Min Sketch | 2005 | Sublinear frequency estimation |
| A10 | HyperLogLog | 2007 | Cardinality estimation |
| A11 | On-Device Edge Learning Survey | 2025 | Edge stream processing |
| A12 | Stream Reasoning + KG Survey | 2025 | Streaming knowledge graphs |
| A13 | RDF Stream Processing | 2025 | Windowed graph queries |

### Industry (14 sources)
| # | Title | Type | Key Value |
|---|-------|------|-----------|
| I1 | Redis Streams vs Kafka vs NATS 2026 | Comparison | Decision framework |
| I2 | Sequin CDC | Tool | 6.8x faster than Debezium |
| I3 | All Ways to CDC in Postgres | Guide | CDC taxonomy |
| I4 | SeaStreamer | Framework | Backend-agnostic Rust streaming |
| I5 | pgwire-replication | Library | Pure Rust WAL consumer |
| I6 | async-nats | Library | Rust NATS client |
| I7 | Redpanda vs Kafka 2026 | Comparison | No-JVM Kafka alternative |
| I8 | RisingWave | Database | Streaming SQL in Rust |
| I9 | Arroyo / Cloudflare | Framework | SQL stream processing in Rust |
| I10 | Fluvio | Framework | Rust-native, WASM transforms |
| I11 | rdkafka | Library | Rust Kafka client |
| I12 | NATS vs Kafka vs Redis | Comparison | Simplicity analysis |
| I13 | AI Hardware 2026 | Guide | VRAM budgeting |
| I14 | Hybrid Cloud-Edge AI | Guide | Tiered architecture patterns |

### AI/ML (13 sources)
| # | Title | Year | Key Technique |
|---|-------|------|--------------|
| M1 | StreamingLLM | 2024 | Attention sinks, 22.2x speedup |
| M2 | Incremental Timeline Summarization | 2024 | Incremental document processing |
| M3 | Recursive Summarization for Dialogue | 2025 | Hierarchical memory compression |
| M4 | iText2KG | 2024 | Incremental KG construction |
| M5 | IncRML | 2024 | CDC-driven KG updates, 11-57x faster |
| M6 | BucketServe | 2025 | Dynamic batching, 3.58x throughput |
| M7 | Memory-Aware Dynamic Batching | 2025 | VRAM-aware admission |
| M8 | Aegaeon GPU Pooling | 2025 | Model swapping |
| M9 | Quantized LLM Edge Evaluation | 2025 | Per-task quantization |
| M10 | DVFS Edge Inference | 2025 | Per-layer power scaling |
| M11 | VectraFlow | 2025 | Streaming vector search |
| M12 | Drift-Adapter | 2025 | Zero-downtime model upgrades |
| M13 | StreamingRAG | 2025 | Temporal KG + streaming RAG |

### PostgreSQL/Redis (11 sources)
| # | Title | Key Finding |
|---|-------|-------------|
| P1 | PG LISTEN/NOTIFY scaling (Recall.ai) | Global lock hazard at high concurrency |
| P2 | Scaling LISTEN/NOTIFY (PgDog) | Proxy-based decoupling |
| P3 | Postgres as Message Bus | Outbox pattern recommendation |
| P4 | ElectricSQL Durable Streams | Shapes for multi-memory |
| P5 | Scaling PG Queues to 100K (RudderStack) | Partition strategy |
| P6 | pg_partman | Automatic partition management |
| P7 | Multi-tenant Queues (Hatchet) | Per-archive isolation |
| P8 | redis-rs Streams API | Already in Fortemi's crate |
| P9 | Redis Streams Docs | Consumer groups architecture |
| P10 | Redis Streams Unified Log (Brandur) | PG+Redis hybrid pattern |
| P11 | Redis Event Sourcing (Learning.com) | 1-year production validation |

---

## 7. Cross-References

- **Fortemi Issue Tracking**: Issues filed in `section9/research-papers` with `[INDUCT]` prefix
- **Research Track**: `fortemi/streaming-realtime`
- **AIWG Planning**: `.aiwg/planning/streaming-realtime/`
- **Existing Fortemi SSE/WebSocket**: `crates/matric-core/src/events.rs`, `crates/matric-api/src/main.rs`
- **Existing Job Queue**: `crates/matric-db/src/jobs.rs` (SKIP LOCKED)
- **Redis Dependency**: `crates/matric-api/Cargo.toml` line 52 (redis 0.27, tokio-comp + connection-manager only)

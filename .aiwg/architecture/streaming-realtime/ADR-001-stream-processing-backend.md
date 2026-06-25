# ADR-001: Stream Processing Backend

**Status**: Accepted
**Date**: 2026-04-09
**Track**: fortemi/streaming-realtime
**Decision**: Direct redis-rs streams API (no abstraction layer)

## Context

Fortemi needs a stream processing backend for event fan-out (SSE, WebSocket, webhooks) and consumer group processing. The system is single-node across all hardware tiers (edge 6-8GB, mid 12-16GB, high-end 24GB+).

Three options were evaluated:
1. **SeaStreamer** — Backend-agnostic trait abstraction (Redis/Kafka swap at deploy time)
2. **Direct redis-rs** — Use the redis crate's streams API directly
3. **Thin internal trait** — Custom `StreamBackend` trait in matric-core

## Decision

**Direct redis-rs streams API.** No abstraction layer.

## Rationale

- Fortemi is single-node across all tiers. The Redis↔Kafka swap scenario is speculative and may never materialize.
- The redis crate (v0.27) is already a dependency. Enabling the `streams` feature adds zero new deps.
- SeaStreamer adds dependency coupling and abstraction overhead for a swap that doesn't currently serve any deployment scenario.
- YAGNI: if a high-end tier ever needs Kafka, rdkafka can be added at that point with a focused integration.
- Fewer moving parts = fewer things to debug on edge hardware.

## Consequences

- Stream processing code will use `redis::streams::*` types directly.
- No backend swap without code changes — accepted tradeoff for simplicity.
- If Kafka is needed in the future, it will require a new integration path (not a config swap).

## Implementation

- Enable `streams` feature on `redis` crate in `crates/matric-api/Cargo.toml`
- Use `XADD`, `XREADGROUP`, `XACK`, `XTRIM` via redis-rs
- Consumer group initialization on startup
- Stream trimming via `MAXLEN ~10000` (configurable)

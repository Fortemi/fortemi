# ADR-004: Edge Tier Realtime Scope

**Status**: Accepted
**Date**: 2026-04-09
**Track**: fortemi/streaming-realtime
**Decision**: Selective realtime (event-driven wake-up + SSE/WebSocket + FTS index)

## Context

Edge tier hardware (6-8GB VRAM — RTX 3060 8GB, 4060, 5060) has VRAM entirely consumed by the LLM (qwen3.5:9b). Stream processing must be CPU-only with <200MB RAM budget. The question is which operations should be realtime vs batch.

Three scopes were evaluated:
1. **Minimal** — Realtime notifications only, everything else batch
2. **Selective** — Realtime notifications + FTS index, embedding/linking batch
3. **Aggressive** — Everything realtime except graph maintenance

## Decision

**Selective realtime.** Edge tier gets:
- **Realtime**: Event-driven job wake-up, SSE/WebSocket event delivery, FTS index updates
- **Batch**: Embedding computation, semantic linking, graph maintenance, extraction

## Rationale

- FTS index updates are CPU-only and handled natively by PostgreSQL on INSERT/UPDATE — no additional resource cost.
- Users see new notes in full-text search immediately after creation, which is the most common search path.
- Semantic search (vector similarity) lags slightly until the batch embedding job runs — acceptable tradeoff since FTS covers the immediate need.
- Embedding and extraction require GPU inference, which competes with the LLM for VRAM. Batch processing with the existing job queue keeps GPU usage predictable.
- Aggressive realtime would cause GPU contention between streaming extraction and user-facing chat/revision on 6-8GB hardware.

## Consequences

- New notes are immediately searchable via FTS but not via semantic search until embedding completes.
- Edge users may notice a delay between note creation and semantic search results appearing — this is the expected behavior, not a bug.
- Mid and high-end tiers progressively enable more realtime operations (see feature plan).

## Hardware Tier Matrix (Final)

| Capability | Edge (6-8GB) | Mid (12-16GB) | High-End (24GB+) |
|------------|:---:|:---:|:---:|
| Event-driven job wake-up | Y | Y | Y |
| SSE/WebSocket event delivery | Y | Y | Y |
| FTS index updates (realtime) | Y | Y | Y |
| Incremental embedding | - | Y | Y |
| Incremental graph linking | - | Y | Y |
| Streaming NER/extraction | - | - | Y |
| Live semantic search updates | - | - | Y |
| Dynamic GPU batching | - | Y | Y |
| Full CDC | - | Optional | Y |

# ADR-003: Message Broker Selection

**Status**: Accepted
**Date**: 2026-04-09
**Track**: fortemi/streaming-realtime
**Decision**: Redis Streams (existing infrastructure)

## Context

Fortemi needs a message broker for event fan-out from the outbox to SSE, WebSocket, webhook, and processing consumers. The system already runs Redis in the Docker bundle for caching.

Two options were evaluated:
1. **Redis Streams** — Already deployed, zero new infrastructure
2. **NATS JetStream** — Stronger guarantees, new service

## Decision

**Redis Streams.** Use the existing Redis instance.

## Rationale

- Redis is already running in the Docker bundle. Zero new infrastructure, zero new services to monitor.
- With PG outbox as the durable source of truth, Redis Streams' at-least-once delivery is sufficient. If Redis loses data (restart without AOF), the outbox can be replayed.
- Consumer groups provide parallel processing, message acknowledgment, and dead consumer recovery (XPENDING/XCLAIM).
- NATS JetStream has stronger guarantees but adds operational overhead (deploy, configure, monitor, debug) that doesn't pay off when Redis is already there and PG is the durability layer.
- Edge hardware (6-8GB VRAM) has tight resource budgets — adding another service is undesirable.

## Consequences

- Single Redis instance is the fan-out layer. If Redis is unavailable, event delivery pauses but no data is lost (outbox retains events).
- No multi-node pub/sub or cross-datacenter replication — accepted for single-node architecture.
- NATS remains an option if Fortemi ever moves to multi-node deployments.
- Redis AOF should be enabled in production for better persistence (reduces replay window on restart).

## Configuration

```bash
# .env
REDIS_STREAM_MAX_LEN=10000        # MAXLEN ~ trimming
REDIS_STREAM_NAME=fortemi:events  # Stream key
REDIS_CONSUMER_GROUP=api          # Consumer group name
```

# Scope Boundaries: Eventing, Streaming & Telemetry Track

**Track**: Eventing, Streaming & Telemetry
**Issues**: #38-#46
**Version**: 1.0
**Last Updated**: 2026-02-05

## Executive Summary

This document defines clear boundaries for the Eventing, Streaming & Telemetry development track. The track implements a unified event system for real-time notifications across WebSocket, SSE, and webhook channels, bridging job worker events with client consumption patterns.

## IN SCOPE

### Core Event Infrastructure (#38)

- `ServerEvent` unified event type with 7 message variants:
  - `JobQueued`, `JobStarted`, `JobProgress`, `JobCompleted`, `JobFailed`
  - `NoteUpdated`, `QueueStatus`
- `EventBus` implementation using `tokio::sync::broadcast`
- Event routing and distribution logic
- Type-safe event serialization/deserialization

### WebSocket Endpoint (#39)

- HTTP endpoint at `/api/v1/ws`
- WebSocket upgrade handling
- Bidirectional message protocol
- Client message handling (ping/pong, subscription management)
- JSON message format with type discrimination

### Worker Event Bridge (#40)

- `WorkerEvent` → `ServerEvent` translation layer
- Integration with existing job queue system
- Event emission from worker completion/failure paths
- Job metadata propagation (job_id, note_id, progress)

### CRUD Event Emission (#41)

- `NoteUpdated` event emission from note CRUD handlers
- Integration points: create, update, delete operations
- Event payload design (note_id, user_id, operation type)
- Coordination with existing transaction boundaries

### Connection Management (#42)

- WebSocket connection lifecycle tracking
- Heartbeat/ping-pong mechanism (30s interval)
- Connection registry for active clients
- Graceful disconnection handling
- Optional authentication integration (auth check if present, no enforcement)
- Connection metrics (count, duration)

### SSE Endpoint for MCP Clients (#43)

- HTTP endpoint at `/api/v1/events`
- Server-Sent Events (SSE) protocol implementation
- Event stream formatting (`event:`, `data:`, `id:`)
- Long-lived connection management
- Reconnection support via Last-Event-ID
- Same event types as WebSocket

### Outbound Webhook System (#44)

- Webhook registration API (CRUD operations)
- HTTP POST delivery to configured URLs
- HMAC-SHA256 signature generation (`X-Matric-Signature` header)
- Event filtering by type
- Retry logic with exponential backoff
- Delivery status tracking
- Webhook configuration storage in PostgreSQL

### Telemetry Mirror (#45)

- Structured tracing events for all ServerEvent emissions
- Metrics extraction (event counts, connection stats, queue depth)
- Integration with existing tracing infrastructure
- Performance impact monitoring
- Log aggregation compatibility (JSON format)

### Integration & Documentation (#46)

- Integration tests for all endpoints
- WebSocket client test harness
- SSE client test harness
- Webhook delivery verification tests
- API documentation updates
- Architecture decision records (ADRs)
- Example client code (Rust, JavaScript)

## OUT OF SCOPE

### Frontend/UI Changes

- HotM UI modifications (client already complete)
- New UI components or views
- Dashboard visualizations
- Client-side state management beyond existing implementation

### Extended Message Types

- Custom event types beyond the 7 defined variants
- Plugin/extension event system
- User-defined event schemas
- Event type versioning

### Authentication System Implementation

- New authentication mechanisms
- Authorization policy engine
- User permission checks for event subscription
- OAuth/JWT token generation
- **Note**: Authentication is optional - connections work with or without auth, deferred to separate security track

### Event Persistence

- Event storage/archival
- Event replay functionality
- Event sourcing patterns
- Audit log generation from events
- **Note**: Events are ephemeral via `tokio::sync::broadcast` channel

### Multi-Node Distribution

- Event distribution across multiple API instances
- Redis/NATS pub-sub integration
- Cluster coordination
- Load balancing for WebSocket connections
- **Note**: Single-node deployment only, horizontal scaling deferred

### Rate Limiting Infrastructure

- Per-connection rate limits
- Event throttling mechanisms
- Backpressure handling beyond channel capacity
- **Note**: Rate limiting exists separately in API layer

### Advanced Webhook Features

- Webhook templating/transformation
- Batch event delivery
- Webhook circuit breakers
- Delivery analytics dashboard
- Third-party webhook service integration

### Performance Optimization

- Event batching/coalescing
- Compression for WebSocket/SSE streams
- Connection pooling optimizations
- Memory profiling/tuning

## Boundary Justifications

### Why Optional Auth?

Authentication integration is designed to be optional to:
1. Avoid blocking development on separate auth track
2. Support both public and authenticated deployment models
3. Enable testing without auth infrastructure
4. Allow gradual migration to authenticated endpoints

### Why Ephemeral Events?

Event persistence is out of scope because:
1. Initial use cases require real-time delivery only
2. Persistence adds significant storage/query complexity
3. Audit logging is a separate concern with different requirements
4. Broadcast channel provides natural backpressure via buffer limits

### Why Single-Node Only?

Multi-node distribution is deferred because:
1. Current deployment model is single-instance
2. Adds significant architectural complexity (Redis/NATS)
3. Horizontal scaling is a future optimization, not MVP requirement
4. Load testing will determine if/when clustering is needed

## Dependencies

### Internal Dependencies

- Existing job queue system (`matric-jobs` crate)
- Note CRUD handlers (`matric-api` crate)
- Database connection pool
- Tracing/telemetry infrastructure

### External Dependencies

- `tokio::sync::broadcast` for event bus
- `axum::extract::ws` for WebSocket support
- `tower-http::sse` for SSE implementation
- `hmac` + `sha2` for webhook signatures
- `reqwest` for webhook HTTP delivery

## Success Criteria

- All 7 event types flow through EventBus
- WebSocket endpoint handles 100+ concurrent connections
- SSE endpoint provides MCP-compatible event stream
- Webhook delivery achieves >95% success rate
- No performance degradation >5% on existing endpoints
- Integration tests achieve >90% coverage
- Documentation complete for all public APIs

## Risk Mitigation

### Performance Risk

- **Risk**: EventBus broadcast overhead impacts request latency
- **Mitigation**: Measure baseline performance, add async event emission, monitor telemetry

### Connection Stability Risk

- **Risk**: Long-lived WebSocket/SSE connections cause resource exhaustion
- **Mitigation**: Implement connection limits, heartbeat timeouts, memory monitoring

### Webhook Delivery Risk

- **Risk**: Slow/failing webhook endpoints block event processing
- **Mitigation**: Async delivery with timeout, bounded retry attempts, dead-letter handling

## Timeline Estimates

- Core event infrastructure (#38): 2-3 days
- WebSocket endpoint (#39): 2-3 days
- Worker event bridge (#40): 1-2 days
- CRUD event emission (#41): 1 day
- Connection management (#42): 2 days
- SSE endpoint (#43): 1-2 days
- Webhook system (#44): 3-4 days
- Telemetry mirror (#45): 1-2 days
- Integration tests (#46): 2-3 days

**Total estimated effort**: 15-22 days (3-4 weeks with single developer)

## Open Questions

1. Should EventBus buffer size be configurable via environment variable?
2. What is the maximum acceptable connection count before requiring clustering?
3. Should webhook signatures use a per-webhook secret or global secret?
4. What retry schedule for webhook delivery (3 retries? 5 retries?)?
5. Should SSE support event filtering via query parameters?

## References

- Issues: #38, #39, #40, #41, #42, #43, #44, #45, #46
- Related: HotM UI implementation (already complete)
- Architecture: Single-node deployment model
- Technology: Axum, tokio, broadcast channels

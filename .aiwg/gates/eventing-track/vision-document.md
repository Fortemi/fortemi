# Vision Document: Eventing, Streaming & Telemetry Track

**Feature Track**: Real-time Event Infrastructure
**Project**: Fortemi/matric-memory
**Epic**: fortemi/fortemi#37
**Created**: 2026-02-05
**Status**: Planning

## Executive Summary

Restore real-time WebSocket eventing capabilities from the original HotM platform, enabling the HotM UI to receive live updates for background jobs and note changes without modification. This track delivers backward-compatible event streaming infrastructure that extends beyond WebSocket to support SSE for MCP clients, webhooks for third-party integrations, and telemetry mirroring for observability.

## Problem Statement

### Current State

The HotM UI's real-time features are broken following the port to Fortemi. The UI maintains a complete WebSocket client (`ui/src/services/websocket.ts`) expecting to connect to `/api/v1/ws` and receive 7 message types:

- QueueStatus
- JobQueued
- JobStarted
- JobProgress
- JobCompleted
- JobFailed
- NoteUpdated

When the UI attempts to connect, nginx correctly proxies WebSocket upgrade requests, but the matric-api server returns 503 (service unavailable) because the endpoint does not exist. This breaks:

- Real-time job progress indicators in the UI
- Live notification of note updates during collaborative editing
- Background task monitoring and queue status visibility

### Root Cause

The matric-api crate lacks:
1. WebSocket endpoint handler (`/api/v1/ws`)
2. Event bus to route internal events to connected clients
3. Integration layer to bridge `matric-jobs` worker events to WebSocket clients
4. Note change event emission from CRUD operations

### Impact

**User Experience**
- Users must manually refresh to see job progress
- No awareness of when long-running operations (embedding generation, AI revision) complete
- Collaborative workflows lack real-time feedback
- Degraded UX compared to original HotM deployment

**Technical Debt**
- UI polls inefficiently or shows stale data
- Infrastructure exists (nginx WebSocket support, `tokio::sync::broadcast` in worker) but remains disconnected
- MCP server has SSE transport capability unused for event streaming

## Target Users

### Primary Users

1. **HotM UI Users**
   - Personas: Knowledge workers, researchers, documentation maintainers
   - Needs: Immediate feedback on background jobs, awareness of note updates from other users/systems
   - Pain points: Uncertainty about job status, stale UI state, no collaborative awareness

2. **MCP/AI Clients**
   - Personas: Claude Code users, AI automation workflows, agent systems
   - Needs: Server-sent events for note changes, job completion notifications for AI revision flows
   - Pain points: Polling for job results introduces latency, cannot react to external note updates

3. **External Integrators**
   - Personas: DevOps engineers, integration platform users (Zapier, n8n), custom webhook consumers
   - Needs: Outbound webhooks for note changes, job events for workflow triggers
   - Pain points: No push notification mechanism, must poll API or miss events

### Secondary Users

4. **Operations Teams**
   - Personas: SREs, platform engineers
   - Needs: Telemetry mirror for event observability, metrics on event delivery latency
   - Pain points: Limited visibility into real-time event flows, cannot correlate events with traces

## Success Metrics

### Functional Success

1. **WebSocket Compatibility**
   - HotM UI connects to `/api/v1/ws` without code changes
   - All 7 message types (QueueStatus, JobQueued, JobStarted, JobProgress, JobCompleted, JobFailed, NoteUpdated) delivered successfully
   - UI real-time features function identically to original HotM deployment

2. **Event Delivery Performance**
   - Event propagation latency (source → client) < 100ms at p95
   - Support 100+ concurrent WebSocket connections per instance
   - No event loss for connected clients under normal load

3. **Webhook Reliability**
   - Webhook delivery success rate > 99%
   - HMAC-SHA256 signature validation on all outbound requests
   - Exponential backoff retry for failed deliveries (up to 3 attempts)

### Operational Success

4. **System Stability**
   - Zero impact on existing API endpoints (backward compatibility)
   - Graceful degradation if event bus unavailable (events logged, not dropped)
   - Connection lifecycle management prevents resource leaks

5. **Observability**
   - All events mirrored to telemetry system with event type, latency, delivery status
   - Metrics exposed: `ws_connections_active`, `ws_events_sent`, `webhook_deliveries_total`, `event_latency_ms`

## Constraints & Assumptions

### Constraints

1. **Backward Compatibility**
   - MUST maintain exact HotM WebSocket protocol (message format, event types)
   - NO breaking changes to existing REST API endpoints
   - NO changes required to HotM UI codebase

2. **Infrastructure**
   - Nginx already configured with WebSocket upgrade headers (no infra changes)
   - Must work within existing Axum API server architecture
   - Cannot require external message brokers (Redis, Kafka) - use in-process channels

3. **Security**
   - WebSocket connections MUST authenticate using existing OAuth token validation
   - Webhook signatures MUST use HMAC-SHA256 with per-webhook secret
   - Event payloads MUST respect tag-based access control (no cross-tenant leaks)

### Assumptions

1. **Existing Infrastructure**
   - `matric-jobs` worker already emits events via `tokio::sync::broadcast`
   - MCP server SSE transport can be adapted for event streaming
   - Note repository operations can be instrumented to emit change events

2. **Usage Patterns**
   - Most users connect 1-2 WebSocket clients concurrently
   - Webhook consumers tolerate up to 5-second delivery delay
   - Event volume remains < 1000 events/second per instance (no event aggregation needed)

3. **Deployment**
   - Single-instance deployment initially (no distributed event bus)
   - Docker bundle includes event infrastructure by default
   - Horizontal scaling deferred to future work (acknowledged limitation)

## Scope

### In-Scope

**Phase 1: Event Bus Foundation** (#38)
- Internal event bus using `tokio::sync::broadcast`
- Event type definitions (matches HotM protocol)
- Shared `AppState` integration for event distribution

**Phase 2: WebSocket Endpoint** (#39)
- `/api/v1/ws` route handler in `matric-api`
- OAuth token authentication for upgrade requests
- Bidirectional WebSocket frame handling (JSON message serialization)

**Phase 3: Job Event Bridge** (#40)
- Subscribe to `matric-jobs` worker events
- Transform `WorkerEvent` → WebSocket protocol messages
- Broadcast to all authenticated WebSocket clients

**Phase 4: Note Event Emission** (#41)
- Instrument note repository CRUD operations
- Emit `NoteUpdated` events with change metadata
- Tag-based filtering to prevent cross-tenant leaks

**Phase 5: Connection Management** (#42)
- Client registry tracking active connections
- Heartbeat/ping-pong for keepalive
- Graceful shutdown and reconnection handling

**Phase 6: MCP Event Forwarding** (#43)
- SSE endpoint for MCP clients (`/api/v1/events/stream`)
- Event filtering by type/scope
- Reuse authentication from existing MCP integration

**Phase 7: Webhook System** (#44)
- Webhook subscription CRUD API
- Outbound HTTP POST delivery with HMAC signatures
- Retry logic with exponential backoff
- Delivery status tracking

**Phase 8: Telemetry Mirror** (#45)
- Trace all events to OpenTelemetry spans
- Metrics for event counts, latency, delivery success
- Structured logging for event payloads (debug builds only)

**Phase 9: Integration Tests** (#46)
- End-to-end WebSocket client tests
- HotM UI protocol compatibility validation
- Webhook delivery and retry tests
- Load tests for concurrent connections

### Out-of-Scope

**Explicitly Excluded**
- New event types beyond the 7 HotM protocol messages (future feature)
- UI changes to HotM client (must work with existing code)
- Distributed event bus for multi-instance deployments (single-instance only)
- Event persistence/replay (ephemeral events only)
- Rate limiting per client (rely on nginx request limits)
- Custom event filtering per client (all authenticated clients receive all events)

**Deferred to Future Work**
- Pub/sub topics for selective event subscription
- Event schemas with versioning
- Binary protocol alternatives (MessagePack, Protobuf)
- Event log persistence for audit trails
- Horizontal scaling with shared event bus (Redis Pub/Sub)

## Technical Approach

### Architecture Principles

1. **Layered Event Flow**
   ```
   Source (Job Worker / Note Repository)
     → Internal Event Bus (broadcast channel)
     → Transport Layer (WebSocket / SSE / Webhook)
     → Client
   ```

2. **Backward Compatibility First**
   - WebSocket message format matches HotM protocol exactly
   - Use newtype wrappers for internal events to decouple from transport serialization
   - Version detection for future protocol extensions (currently v1 only)

3. **Fail-Safe Degradation**
   - Event bus unavailable → log events, continue request processing
   - WebSocket client disconnects → clean up resources, no error propagation
   - Webhook delivery fails → retry with backoff, log failure, mark subscription degraded

### Key Design Decisions

**Event Bus Choice**: `tokio::sync::broadcast`
- Already used in `matric-jobs` worker
- Efficient for fan-out to multiple receivers
- In-process only (acceptable for single-instance constraint)

**Authentication Strategy**: OAuth Token in WebSocket Upgrade
- Reuse existing JWT validation from REST API
- Token passed as query parameter (`/api/v1/ws?token=xxx`) or `Sec-WebSocket-Protocol` header
- Same tag-based authorization model as REST endpoints

**Webhook Signatures**: HMAC-SHA256
- Industry standard (GitHub, Stripe compatibility)
- Secret stored per webhook subscription
- Signature in `X-Matric-Signature` header

## Validation Plan

### Pre-Launch Validation

1. **HotM UI Integration Test**
   - Deploy matric-api with WebSocket support
   - Run HotM UI in dev mode pointing to test instance
   - Verify all 7 event types display correctly in UI
   - Confirm no UI errors or protocol mismatches

2. **Load Testing**
   - Simulate 100 concurrent WebSocket connections
   - Generate 100 events/second across all types
   - Measure event delivery latency (target: <100ms p95)
   - Monitor memory/CPU usage (must remain < 500MB / 50% CPU)

3. **Failure Scenarios**
   - WebSocket client abrupt disconnect (no close frame)
   - Webhook endpoint returns 500 (verify retry)
   - Event bus channel full (verify backpressure handling)

### Post-Launch Metrics

Track via OpenTelemetry/Prometheus:
- `matric_ws_connections_active` (gauge)
- `matric_ws_events_sent_total` (counter, labeled by event_type)
- `matric_webhook_deliveries_total` (counter, labeled by status: success/retry/failed)
- `matric_event_latency_ms` (histogram, p50/p95/p99)

Success threshold:
- Event delivery p95 latency < 100ms for first 30 days
- Webhook success rate > 99% (excluding client errors 4xx)
- Zero reports of broken UI real-time features from HotM users

## Risks & Mitigations

### Technical Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| HotM protocol mismatch (breaking changes in UI expectations) | High | Medium | Upstream review of Fortemi/HotM#27, UI code inspection, integration tests |
| Event bus memory leak (unbounded broadcast channel) | High | Low | Bounded channel (capacity 1000), monitor `ws_connections_active`, load tests |
| Webhook endpoint DDoS (malicious subscriptions) | Medium | Medium | Rate limit subscription creation, require OAuth admin scope for webhooks |
| Tag isolation failure (cross-tenant event leak) | Critical | Low | Mandatory unit tests for tag filtering, audit log all event emissions |

### Organizational Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Scope creep (requests for new event types) | Medium | High | Strict adherence to 7 HotM event types, document deferred features, gate new events behind feature flags |
| HotM UI codebase divergence (future UI changes break assumptions) | Low | Medium | Pin UI version in integration tests, monitor HotM repo for WebSocket client changes |

## Dependencies & Coordination

### Internal Dependencies

- **matric-jobs** (`crates/matric-jobs`): Already provides `WorkerEvent` enum (#40)
- **matric-api** (`crates/matric-api`): Must add WebSocket route and event bus integration (#39)
- **matric-db** (`crates/matric-db`): Must instrument note repository for change events (#41)
- **mcp-server** (`mcp-server/`): SSE transport reuse for MCP event forwarding (#43)

### External Dependencies

- **HotM UI** (`Fortemi/HotM`): Upstream WebSocket client protocol definition (issue #27)
- **Nginx**: Already configured with WebSocket upgrade headers (no changes needed)
- **OAuth**: Existing token validation reused for WebSocket auth

### Coordination Points

- **Product Strategist**: Validate webhook feature priority, confirm no UI changes acceptable
- **System Analyst**: Review event schema alignment with existing data models
- **Project Manager**: Sequence phases to unblock HotM UI first (#38-#42), defer MCP/webhook (#43-#44)

## Open Questions & Assumptions Log

### Outstanding Questions

1. **Event Retention**: Do we need to persist events for late-joining clients?
   - **Status**: Open
   - **Owner**: Vision Owner
   - **Target Date**: 2026-02-12 (pre-Phase 1 kickoff)
   - **Current Assumption**: No persistence needed (ephemeral events only)

2. **Multi-Instance Scaling**: When will horizontal scaling become required?
   - **Status**: Open
   - **Owner**: Product Strategist
   - **Target Date**: 2026-03-01 (post-Phase 9 review)
   - **Current Assumption**: Single-instance deployment sufficient for 6-12 months

3. **MCP Event Schema**: Should SSE events use different format than WebSocket?
   - **Status**: Open
   - **Owner**: Requirements Reviewer
   - **Target Date**: 2026-02-15 (pre-Phase 6)
   - **Current Assumption**: Reuse same JSON schema, wrap in SSE data field

### Validated Assumptions

1. **HotM Protocol Stability**: UI expects exact 7 event types, no additions
   - **Validation Method**: Code inspection of `ui/src/services/websocket.ts`
   - **Validated By**: Vision Owner
   - **Date**: 2026-02-05
   - **Evidence**: TypeScript enum defines exactly 7 message types, no version field

2. **Nginx WebSocket Config**: Already supports `Upgrade` and `Connection` headers
   - **Validation Method**: Deployment config review
   - **Validated By**: Project Manager (assumed from context)
   - **Date**: Prior deployment
   - **Evidence**: Nginx returns 503 (backend unavailable), not 400 (bad request)

## Success Criteria Summary

**Minimum Viable Success** (Phase 1-5 Complete)
- HotM UI connects without code changes
- All 7 event types delivered with <100ms latency
- 100 concurrent connections supported
- Zero breaking changes to existing API

**Full Feature Success** (All Phases Complete)
- MCP clients receive SSE event streams
- Webhook system delivers with >99% success rate
- Telemetry provides full event observability
- Integration tests validate HotM protocol compatibility

**Stretch Goals** (Future Enhancements)
- Event persistence for replay
- Selective event subscriptions (topic-based filtering)
- Multi-instance support with distributed event bus

---

**Approvals**

- Vision Owner: [Pending]
- Product Strategist: [Pending]
- System Analyst: [Pending]
- Requirements Reviewer: [Pending]

**Next Steps**

1. Review vision with stakeholders (target: 2026-02-07)
2. Resolve open questions before Phase 1 kickoff
3. Create detailed technical design for Event Bus Foundation (#38)
4. Begin Phase 1 implementation (target start: 2026-02-10)

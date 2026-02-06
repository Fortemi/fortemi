# Use Case Briefs: Eventing, Streaming & Telemetry Track

**Track**: Eventing, Streaming & Telemetry
**Issues**: #38-#46
**Version**: 1.0
**Last Updated**: 2026-02-05

## Overview

This document provides concise use case briefs for the primary actors and scenarios in the Eventing, Streaming & Telemetry track. Detailed specifications are maintained in the referenced GitHub issues.

---

## UC-001: HotM UI Receives Real-Time Job Status

**Actor**: HotM UI WebSocket client
**Goal**: Display live queue status, job progress, and completion notifications to end users
**Priority**: Critical
**References**: #39, #40, #42

### Preconditions

- User has HotM UI open in browser
- WebSocket connection established to `/api/v1/ws`
- Optional: User authenticated (if auth enabled)

### Main Flow

1. HotM UI initiates WebSocket connection
2. Connection manager accepts and tracks connection
3. UI receives initial `QueueStatus` event with current queue depth
4. User submits a job (e.g., AI revision) via API
5. Worker emits `WorkerEvent::Queued` → translated to `ServerEvent::JobQueued`
6. UI receives `JobQueued` event, displays "Job queued" status
7. Worker picks up job → emits `WorkerEvent::Started` → `ServerEvent::JobStarted`
8. UI receives `JobStarted` event, displays "Processing..." with spinner
9. Worker emits progress updates → `ServerEvent::JobProgress` with percentage
10. UI receives `JobProgress` events, updates progress bar
11. Worker completes → emits `WorkerEvent::Completed` → `ServerEvent::JobCompleted`
12. UI receives `JobCompleted` event, displays success notification

### Alternative Flows

- **Job Failure**: Worker emits `JobFailed` event, UI displays error message
- **Connection Lost**: UI detects disconnection, attempts reconnect with exponential backoff
- **Heartbeat Timeout**: Server closes stale connection after 90s of no ping response

### Success Criteria

- End-to-end latency <500ms for event delivery
- UI updates within 1 second of worker state change
- Graceful handling of connection interruptions
- Progress updates at least every 5 seconds during long jobs

---

## UC-002: Note Auto-Refresh After AI Revision

**Actor**: HotM UI
**Goal**: Automatically refresh note content when AI revision completes, eliminating manual reload
**Priority**: High
**References**: #40, #41

### Preconditions

- User is viewing a note in HotM UI
- WebSocket connection active
- User triggers "Revise with AI" action

### Main Flow

1. UI sends revision request via API
2. Job queued and processed (see UC-001 for job flow)
3. Worker completes revision, updates note in database
4. Note CRUD handler emits `ServerEvent::NoteUpdated` with `note_id`
5. UI receives `NoteUpdated` event
6. UI checks if currently displayed note matches `note_id`
7. If match, UI fetches updated note content via GET `/api/v1/notes/{id}`
8. UI re-renders note with updated content
9. UI displays toast notification: "Note updated by AI revision"

### Alternative Flows

- **User Navigated Away**: UI receives event but ignores (not viewing that note)
- **Concurrent Edit**: UI detects version conflict, prompts user to choose version
- **Fetch Fails**: UI retries fetch, falls back to manual refresh prompt

### Success Criteria

- Note content updates without user action
- No duplicate fetches (event deduplication)
- Smooth UI transition without flicker
- Works for all note update sources (API, worker, etc.)

---

## UC-003: MCP/AI Agent Monitors Events

**Actor**: Claude Code or AI agent via MCP
**Goal**: React to note changes and job completions to enable AI-driven workflows
**Priority**: High
**References**: #43

### Preconditions

- MCP server configured with matric-memory connection
- SSE client connected to `/api/v1/events`
- Agent has appropriate permissions (if auth enabled)

### Main Flow

1. MCP client establishes SSE connection to `/api/v1/events`
2. Server sends initial connection confirmation event
3. MCP client enters event listening loop
4. User creates/updates note via HotM UI
5. API handler emits `NoteUpdated` event
6. SSE endpoint formats event as `event: NoteUpdated\ndata: {...}\nid: 12345\n\n`
7. MCP client receives event, parses JSON payload
8. Agent analyzes note content change
9. Agent decides to take action (e.g., generate related notes, update tags)
10. Agent executes action via MCP tools

### Alternative Flows

- **Connection Interrupted**: Client reconnects with `Last-Event-ID` header for event replay
- **Event Filtering**: Client subscribes to specific event types via query param `?events=NoteUpdated,JobCompleted`
- **Batch Processing**: Agent accumulates events for 5 seconds before processing batch

### Success Criteria

- SSE stream stays connected for hours without interruption
- Events delivered in order with unique IDs
- Reconnection recovers missed events (best effort within buffer window)
- Compatible with standard SSE clients (EventSource API, curl)

---

## UC-004: External System Receives Webhook Notifications

**Actor**: External system (CI/CD pipeline, Slack bot, custom integration tool)
**Goal**: Get notified when specific events occur to trigger downstream automation
**Priority**: Medium
**References**: #44

### Preconditions

- External system registered webhook via POST `/api/v1/webhooks`
- Webhook configured with target URL and event filter
- HMAC secret stored for signature verification

### Main Flow

1. Admin registers webhook:
   ```json
   POST /api/v1/webhooks
   {
     "url": "https://example.com/webhook",
     "events": ["JobCompleted", "NoteUpdated"],
     "secret": "shared-secret-key"
   }
   ```
2. System stores webhook configuration in database
3. Event occurs (e.g., `JobCompleted`)
4. EventBus broadcasts event
5. Webhook delivery service receives event
6. Service filters: event type matches webhook subscription
7. Service generates HMAC-SHA256 signature over JSON payload
8. Service sends HTTP POST to webhook URL:
   ```
   POST https://example.com/webhook
   X-Matric-Signature: sha256=abc123...
   Content-Type: application/json

   {"event":"JobCompleted","data":{...}}
   ```
9. External system receives request, verifies signature
10. External system processes event (e.g., deploys updated docs)
11. External system responds with 200 OK
12. Webhook service marks delivery successful

### Alternative Flows

- **Delivery Failure**: Service retries with exponential backoff (1s, 5s, 25s, 125s)
- **Permanent Failure**: After 4 retries, mark webhook as failed, notify admin
- **Signature Mismatch**: External system rejects request, logs security incident
- **Timeout**: Service aborts request after 30s, schedules retry

### Success Criteria

- Webhook delivery within 5 seconds of event emission
- >95% successful delivery rate under normal conditions
- Signature validation prevents spoofed requests
- Failed webhooks don't block event processing

---

## UC-005: Operations Team Monitors System Health

**Actor**: Operations/SRE team
**Goal**: Track job throughput, queue depth, connection counts, and system performance
**Priority**: Medium
**References**: #45

### Preconditions

- Telemetry mirror enabled (default)
- Log aggregation system configured (e.g., Loki, Elasticsearch)
- Dashboards configured to query structured logs

### Main Flow

1. EventBus emits `ServerEvent::JobCompleted`
2. Telemetry mirror intercepts event
3. Mirror emits structured tracing event:
   ```rust
   tracing::info!(
       event.type = "JobCompleted",
       event.job_id = %job_id,
       event.duration_ms = 1234,
       event.note_id = %note_id,
       "Job completed successfully"
   );
   ```
4. Tracing subscriber serializes to JSON:
   ```json
   {
     "timestamp": "2026-02-05T10:30:45Z",
     "level": "INFO",
     "fields": {
       "event.type": "JobCompleted",
       "event.job_id": "job_abc123",
       "event.duration_ms": 1234,
       "event.note_id": "note_xyz789"
     },
     "message": "Job completed successfully"
   }
   ```
5. Log aggregation system ingests JSON log
6. Ops team queries metrics:
   - Job throughput: `sum(rate(event.type="JobCompleted"[5m]))`
   - Queue depth: `last(event.queue_depth) where event.type="QueueStatus"`
   - Connection count: `sum(event.type="ConnectionOpened") - sum(event.type="ConnectionClosed")`
7. Dashboards visualize trends, alert on anomalies

### Alternative Flows

- **Performance Profiling**: Filter events by duration, identify slow jobs
- **Error Analysis**: Query `event.type="JobFailed"` with error details
- **Connection Debugging**: Trace individual connection lifecycle via connection_id

### Success Criteria

- All event types emit structured telemetry
- Performance overhead <1% (measured via benchmarks)
- Metrics available within 10 seconds of event emission
- Compatible with standard tracing subscribers (JSON, Jaeger, etc.)

---

## Cross-Cutting Concerns

### Authentication

All use cases operate under the principle of **optional authentication**:
- If auth system is enabled, events include user context
- If auth system is disabled, events are delivered to all connected clients
- No use case is blocked by lack of auth implementation

### Error Handling

All use cases must handle:
- Network interruptions (reconnect logic)
- Malformed events (log and skip)
- Resource exhaustion (connection/memory limits)

### Performance

Target metrics across all use cases:
- Event emission latency: <10ms (p99)
- Event delivery latency: <500ms (p95)
- Concurrent connections: 100+ without degradation
- Event throughput: 1000+ events/sec

### Security

All external-facing interfaces must implement:
- WebSocket: Origin validation (if auth enabled)
- SSE: Same origin policy enforcement
- Webhooks: HMAC signature verification
- Telemetry: No PII in structured logs

---

## Summary

These use cases represent the core scenarios for the Eventing, Streaming & Telemetry track. Each use case is designed to be independently testable with clear success criteria. Detailed specifications, API contracts, and implementation details are maintained in the referenced GitHub issues (#38-#46).

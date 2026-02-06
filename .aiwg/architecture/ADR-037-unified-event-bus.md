# ADR-037: Unified Event Bus with Multi-Transport Delivery

**Status:** Accepted
**Date:** 2026-02-05
**Deciders:** roctinam
**Technical Story:** WebSocket and SSE eventing support for real-time updates

## Context

The Fortemi/matric-memory project needs real-time event delivery to support multiple client types:

- **HotM Web Client**: Expects WebSocket updates for job status and note changes
- **MCP Server**: Node.js process needs SSE feed for event monitoring
- **Future Webhooks**: Outbound HTTP notifications for integrations
- **Telemetry**: System monitoring needs event stream access

### Current State

The job worker (`crates/matric-jobs/src/worker.rs`) already uses `tokio::sync::broadcast` for internal event distribution:

```rust
pub enum WorkerEvent {
    JobStarted { job_id: Uuid, job_type: JobType },
    JobProgress { job_id: Uuid, percent: i32, message: Option<String> },
    JobCompleted { job_id: Uuid, job_type: JobType },
    JobFailed { job_id: Uuid, job_type: JobType, error: String },
    WorkerStarted,
    WorkerStopped,
}
```

This channel has a 100-slot buffer and serves events only within the job worker crate.

### Limitations

| Limitation | Impact | Issue |
|------------|--------|-------|
| Job-only events | Note CRUD changes not published | #39, #43 |
| Worker-scoped | No access from API layer for WS/SSE | #39, #43 |
| Tight coupling | `WorkerEvent` tied to job worker semantics | - |
| No bridging | Can't extend to other event sources | - |

### Problem Statement

Clients need unified event access across:
1. Job lifecycle events (already exists)
2. Note CRUD operations (create, update, delete)
3. Collection changes (future)
4. Search index updates (future)

Multiple transport protocols must consume from the same event stream:
- **WebSocket**: Bi-directional, connection-per-client
- **SSE**: Unidirectional, long-lived HTTP
- **Webhooks**: Fire-and-forget HTTP POST
- **Telemetry**: Metrics aggregation

## Decision

Adopt a **unified event bus architecture using tokio::sync::broadcast as the central distribution mechanism**.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Event Sources                         │
├─────────────────────────────────────────────────────────┤
│  JobWorker  │  NoteRepository  │  CollectionRepository  │
└──────┬──────┴──────────┬───────┴──────────┬─────────────┘
       │                 │                  │
       └─────────────────┼──────────────────┘
                         ▼
              ┌─────────────────────┐
              │     EventBus        │
              │  (broadcast, 256)   │
              └──────────┬──────────┘
                         │
         ┌───────────────┼───────────────┐
         │               │               │
         ▼               ▼               ▼
    ┌─────────┐   ┌──────────┐   ┌──────────┐
    │   WS    │   │   SSE    │   │ Webhook  │
    │ Handler │   │ Handler  │   │  Worker  │
    └─────────┘   └──────────┘   └──────────┘
         │               │               │
         ▼               ▼               ▼
    WebSocket       Server-Sent      HTTP POST
     Clients          Events       (external)
```

### Core Components

**1. ServerEvent Enum (matric-core)**

Single source of truth for all system events:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    // Job events
    JobQueued { job_id: Uuid, job_type: JobType },
    JobStarted { job_id: Uuid, job_type: JobType },
    JobProgress { job_id: Uuid, percent: i32, message: Option<String> },
    JobCompleted { job_id: Uuid, job_type: JobType },
    JobFailed { job_id: Uuid, job_type: JobType, error: String },

    // Note events
    NoteCreated { note_id: Uuid, title: String },
    NoteUpdated { note_id: Uuid, title: String },
    NoteDeleted { note_id: Uuid },

    // Queue status
    QueueStatus { pending: i64, active: i64 },
}
```

**2. EventBus Struct (matric-core)**

Wraps `tokio::sync::broadcast` with ergonomic API:

```rust
pub struct EventBus {
    tx: broadcast::Sender<ServerEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn emit(&self, event: ServerEvent) {
        let _ = self.tx.send(event); // Ignore send errors (no receivers)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}
```

Default capacity: **256 slots** (balances memory vs. lagging protection).

**3. Event Bridge (matric-jobs)**

Spawned task bridges `WorkerEvent` → `ServerEvent`:

```rust
// In JobWorker::start()
let event_bus = app_state.event_bus.clone();
let mut worker_rx = worker_handle.events();

tokio::spawn(async move {
    while let Ok(event) = worker_rx.recv().await {
        let server_event = match event {
            WorkerEvent::JobStarted { job_id, job_type } =>
                ServerEvent::JobStarted { job_id, job_type },
            WorkerEvent::JobProgress { job_id, percent, message } =>
                ServerEvent::JobProgress { job_id, percent, message },
            // ... etc
        };
        event_bus.emit(server_event);
    }
});
```

**4. Transport Handlers (matric-api)**

Each transport subscribes independently:

| Transport | Protocol | Location | Handler |
|-----------|----------|----------|---------|
| WebSocket | WS | `/ws` | `axum::extract::ws::WebSocket` |
| SSE | HTTP | `/events` | `axum::response::Sse` |
| Webhooks | HTTP POST | Background worker | `reqwest::Client` |
| Telemetry | Internal | Metrics task | Prometheus counters |

All handlers call `event_bus.subscribe()` and loop on `recv()`.

### Lagged Receiver Handling

When a receiver falls behind by 256 events, `recv()` returns `RecvError::Lagged(n)`:

```rust
match rx.recv().await {
    Ok(event) => handle_event(event),
    Err(RecvError::Lagged(n)) => {
        warn!("Missed {} events, resyncing", n);
        // Send QueueStatus to resync client state
        send_queue_status().await;
    }
    Err(RecvError::Closed) => break,
}
```

### Note Events Integration

Repository methods emit events after successful DB operations:

```rust
impl NoteRepository for Database {
    async fn create_note(&self, note: &Note) -> Result<Note> {
        let created = sqlx::query_as(/* ... */).fetch_one(&self.pool).await?;

        self.event_bus.emit(ServerEvent::NoteCreated {
            note_id: created.id,
            title: created.title.clone(),
        });

        Ok(created)
    }
}
```

## Alternatives Considered

### 1. Direct WorkerEvent Extension

Extend `WorkerEvent` with note variants.

**Rejected because:**
- Couples note events to job worker crate (violates separation of concerns)
- `matric-jobs` would depend on `matric-db` internals
- Note operations don't conceptually belong in job worker
- Makes job worker the "event god object"

### 2. Database-Backed Event Queue with Polling

PostgreSQL `LISTEN/NOTIFY` or event table with polling.

**Rejected because:**
- 100-500ms latency minimum (polling overhead)
- Database becomes bottleneck for high-frequency events
- Complex cleanup of old events
- No in-memory performance benefits
- LISTEN/NOTIFY requires persistent connections (complicates pooling)

### 3. Redis Pub/Sub

External Redis instance for pub/sub messaging.

**Rejected because:**
- External dependency for single-node deployment (overkill)
- Network hop adds latency (5-20ms)
- Requires Redis configuration and monitoring
- Incompatible with "Docker bundle" simplicity goal
- No guaranteed delivery (pub/sub fires and forgets)

### 4. Separate Channels Per Transport

Each transport gets its own `broadcast::channel`:

```rust
ws_tx.send(event);
sse_tx.send(event);
webhook_tx.send(event);
```

**Rejected because:**
- Duplicates emit logic across codebase
- Hard to keep event types consistent
- Adding new transport requires touching all event sources
- Memory overhead (N × 256 slots)
- No single source of truth for event schema

## Consequences

### Positive

- **Single source of truth**: `ServerEvent` enum defines all event types
- **Decoupled transports**: WS, SSE, webhooks independently subscribe
- **Zero-copy fanout**: `tokio::sync::broadcast` clones `Arc` internally
- **Backpressure handling**: Lagged receivers get explicit error
- **Simple integration**: `event_bus.emit()` at operation completion
- **No external dependencies**: Pure Tokio primitives
- **Testing-friendly**: Mock event bus for integration tests

### Negative

- **Single point of backpressure**: Slow receiver affects buffer for all
- **Lagged receiver drops events**: No persistent queue for replay
- **Manual bridging required**: `WorkerEvent` → `ServerEvent` task
- **Buffer tuning needed**: 256-slot default may need adjustment
- **No event filtering at source**: All subscribers get all events
- **Memory overhead**: 256 × `sizeof(ServerEvent)` ≈ 64KB per subscriber

### Mitigations

1. **Buffer size**: Monitor `RecvError::Lagged` frequency; adjust capacity per deployment
2. **Event filtering**: Implement client-side filters (WebSocket sends filter spec)
3. **Persistent log**: Optional event journal to PostgreSQL for replay
4. **Telemetry**: Track emit/recv rates, lag counts via Prometheus metrics
5. **Graceful degradation**: SSE reconnects automatically; WS clients implement exponential backoff

## Implementation

**Code Location:**
- `crates/matric-core/src/events.rs` - `ServerEvent` enum and `EventBus` struct
- `crates/matric-api/src/routes/ws.rs` - WebSocket handler
- `crates/matric-api/src/routes/sse.rs` - SSE handler
- `crates/matric-jobs/src/bridge.rs` - `WorkerEvent` → `ServerEvent` bridge
- `crates/matric-db/src/notes.rs` - Note repository event emissions

**Key Changes:**

1. Add `EventBus` to `AppState` (shared via `Arc`)
2. Emit `ServerEvent::NoteUpdated` after successful note updates
3. Bridge job worker events via spawned task
4. Implement WebSocket handler with JSON-over-WS protocol
5. Implement SSE handler with `text/event-stream` response
6. Add telemetry mirror task for Prometheus metrics

**Message Format (WebSocket/SSE):**

```json
{
  "type": "job_progress",
  "job_id": "123e4567-e89b-12d3-a456-426614174000",
  "percent": 45,
  "message": "Processing chunk 3/7"
}
```

**HotM Client Expectations:**

The existing HotM client expects these specific message types:
- `QueueStatus` - Queue depth and active job count
- `JobQueued` - New job added to queue
- `JobStarted` - Job execution began
- `JobProgress` - Progress updates with percent and message
- `JobCompleted` - Job finished successfully
- `JobFailed` - Job execution failed with error
- `NoteUpdated` - Note was created/updated/deleted

The bridge layer will map `ServerEvent` to these expected types.

## References

- Issue #37: Event delivery epic
- Issue #38: Unified event bus implementation
- Issue #39: WebSocket real-time updates
- Issue #43: SSE event stream for MCP
- Issue #44: Webhook delivery system
- Issue #45: Telemetry mirror for metrics
- `tokio::sync::broadcast` documentation: https://docs.rs/tokio/latest/tokio/sync/broadcast/
- ADR-015: Workspace crate structure (domain separation)
- ADR-004: Unified error types (consistency pattern)

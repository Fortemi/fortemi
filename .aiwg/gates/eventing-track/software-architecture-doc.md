# Software Architecture Document: Eventing, Streaming & Telemetry Extension

**Project**: Fortemi/matric-memory
**Track**: Eventing, Streaming & Telemetry
**Epic**: fortemi/fortemi#37
**Issues**: #38-#46
**Status**: Active Development
**Created**: 2026-02-05
**Authors**: Architecture Team

---

## Executive Summary

This document specifies the architecture for adding real-time event distribution capabilities to the existing matric-memory Rust API server. This is an **extension architecture document** covering the eventing system that overlays the existing single-instance Axum HTTP server. The architecture restores backward-compatible WebSocket functionality for the HotM UI while extending event delivery to SSE clients (MCP server), outbound webhooks, and telemetry systems.

**Key Architectural Decisions:**
- ADR-037: Unified event bus using `tokio::sync::broadcast`
- In-process event distribution (no external message brokers)
- Bridge pattern for `WorkerEvent` → `ServerEvent` translation
- Fan-out to multiple transport handlers (WebSocket, SSE, webhooks)
- Optional authentication model (deferred enforcement to separate track)

**System Boundaries:**
- Single-node deployment (no distributed event bus)
- Ephemeral events only (no persistence/replay)
- 7 message types matching HotM protocol
- 100+ concurrent connections target

---

## 1. Architectural Drivers

### 1.1 Quality Attributes

The eventing system must satisfy these quality attribute requirements:

| Quality Attribute | Target | Measurement | Priority |
|-------------------|--------|-------------|----------|
| **Latency** | Event delivery <100ms (p95) | Source emission → WS client receive | Critical |
| **Throughput** | 1000 events/sec sustained | Broadcast channel utilization <80% | High |
| **Connection Scale** | 100+ concurrent WebSocket clients | No memory leak, stable latency | Critical |
| **Availability** | 99.9% uptime for event stream | Graceful degradation, auto-reconnect | High |
| **Backward Compatibility** | Zero HotM UI changes | Exact message format match | Critical |
| **Observability** | All events traced/logged | Structured tracing, metrics export | Medium |

### 1.2 Design Constraints

**Existing Infrastructure:**
- Axum 0.7 web framework (features: json, tower-log, multipart)
- Single-instance Docker bundle deployment
- PostgreSQL 16 with pgvector for data persistence
- Nginx reverse proxy with WebSocket upgrade support
- `tokio::sync::broadcast` already in use for worker events (100-slot buffer)

**Must-Have Requirements:**
- No breaking changes to existing REST API endpoints
- No external dependencies (Redis Pub/Sub, Kafka) for MVP
- Compatible with current `tower-http` middleware stack (CORS, tracing, rate limiting)
- Reuse existing OAuth token validation infrastructure

**Architectural Constraints:**
- Events are ephemeral (buffer-based, not persisted)
- Single-node coordination only (no clustering in scope)
- Authentication is optional (connections work without auth)
- Connection limits enforced at application layer (not load balancer)

### 1.3 Graceful Degradation Strategy

**Failure Mode Handling:**

| Failure Scenario | Degradation Behavior | Recovery Path |
|------------------|----------------------|---------------|
| Event bus unavailable | Emit log warning, continue request processing | Restart event bus on next startup |
| WebSocket upgrade fails | Return 503, client retries with backoff | Check middleware stack compatibility |
| Slow receiver lags | Disconnect client with 1011 code | Client reconnects, UI polls fallback |
| Webhook delivery timeout | Retry with exponential backoff (3 attempts) | Mark webhook degraded, alert admin |
| Broadcast buffer full | Drop oldest events, emit `RecvError::Lagged` | Increase buffer size via tuning |

---

## 2. Component Decomposition

### 2.1 Crate Structure

The eventing system spans multiple crates following the existing workspace architecture:

```
crates/
├── matric-core/          [EXTENDED]
│   └── src/
│       └── events.rs     [NEW] ServerEvent enum, EventBus struct
│
├── matric-api/           [EXTENDED]
│   └── src/
│       ├── routes/
│       │   ├── ws.rs     [NEW] WebSocket handler (/api/v1/ws)
│       │   ├── sse.rs    [NEW] SSE handler (/api/v1/events)
│       │   └── webhooks.rs [NEW] Webhook CRUD API
│       ├── connection.rs [NEW] Connection registry & lifecycle
│       └── main.rs       [MODIFIED] Add EventBus to AppState
│
├── matric-jobs/          [EXTENDED]
│   └── src/
│       ├── worker.rs     [EXISTING] WorkerEvent enum (line 59-82)
│       └── bridge.rs     [NEW] WorkerEvent → ServerEvent bridge task
│
├── matric-db/            [EXTENDED]
│   └── src/
│       ├── notes.rs      [MODIFIED] Emit NoteUpdated events
│       └── webhooks.rs   [NEW] Webhook subscription repository
│
└── matric-inference/     [NO CHANGES]
```

### 2.2 Core Components

**2.2.1 ServerEvent Enum (matric-core)**

Central event type matching HotM protocol:

```rust
// crates/matric-core/src/events.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    // Job lifecycle events
    JobQueued { job_id: Uuid, job_type: JobType },
    JobStarted { job_id: Uuid, job_type: JobType },
    JobProgress { job_id: Uuid, percent: i32, message: Option<String> },
    JobCompleted { job_id: Uuid, job_type: JobType },
    JobFailed { job_id: Uuid, job_type: JobType, error: String },

    // Note change events
    NoteUpdated { note_id: Uuid, title: String },

    // System status events
    QueueStatus { pending: i64, active: i64 },
}
```

**Design Rationale:**
- Tagged JSON format (`"type": "job_started"`) matches HotM client expectations
- `Clone` required for broadcast channel fan-out
- `Serialize`/`Deserialize` for WebSocket JSON, SSE data field, webhook body

**2.2.2 EventBus Struct (matric-core)**

Wrapper around `tokio::sync::broadcast` with ergonomic API:

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

**Key Properties:**
- Capacity: 256 slots (increased from worker's 100 to handle burst traffic)
- Zero-copy fan-out via `Arc` internally
- Non-blocking emit (send failures logged but don't block producers)
- Thread-safe via `Arc<EventBus>` in AppState

**2.2.3 AppState Integration (matric-api)**

Add EventBus to shared application state:

```rust
// crates/matric-api/src/main.rs (existing at line 141)

#[derive(Clone)]
struct AppState {
    db: Database,
    search: Arc<HybridSearchEngine>,
    issuer: String,
    rate_limiter: Option<Arc<GlobalRateLimiter>>,
    tag_resolver: TagResolver,
    search_cache: SearchCache,
    event_bus: Arc<EventBus>,  // [NEW]
}
```

**Initialization (main.rs ~line 450):**

```rust
let event_bus = Arc::new(EventBus::new(256));

let app_state = AppState {
    db: db.clone(),
    search: Arc::new(search_engine),
    issuer,
    rate_limiter,
    tag_resolver,
    search_cache,
    event_bus: event_bus.clone(),
};
```

**2.2.4 Event Bridge Task (matric-jobs)**

Spawned task translates worker events to server events:

```rust
// crates/matric-jobs/src/bridge.rs

pub fn spawn_event_bridge(
    worker_handle: &WorkerHandle,
    event_bus: Arc<EventBus>,
) -> tokio::task::JoinHandle<()> {
    let mut worker_rx = worker_handle.events();

    tokio::spawn(async move {
        loop {
            match worker_rx.recv().await {
                Ok(WorkerEvent::JobStarted { job_id, job_type }) => {
                    event_bus.emit(ServerEvent::JobStarted { job_id, job_type });
                }
                Ok(WorkerEvent::JobProgress { job_id, percent, message }) => {
                    event_bus.emit(ServerEvent::JobProgress { job_id, percent, message });
                }
                Ok(WorkerEvent::JobCompleted { job_id, job_type }) => {
                    event_bus.emit(ServerEvent::JobCompleted { job_id, job_type });
                }
                Ok(WorkerEvent::JobFailed { job_id, job_type, error }) => {
                    event_bus.emit(ServerEvent::JobFailed { job_id, job_type, error });
                }
                Ok(WorkerEvent::WorkerStarted) => {
                    // Emit QueueStatus on worker start
                    // (query pending count from database)
                }
                Ok(WorkerEvent::WorkerStopped) => {
                    tracing::info!("Worker stopped, event bridge terminating");
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Event bridge lagged by {} events", n);
                }
            }
        }
    })
}
```

**Bridge Location:** Spawned in `main.rs` after worker start (line ~500):

```rust
let _bridge_handle = spawn_event_bridge(&worker_handle, app_state.event_bus.clone());
```

---

## 3. Data Flow Architecture

### 3.1 Event Flow Diagram

```
┌──────────────────────── Event Sources ────────────────────────┐
│                                                                 │
│  ┌─────────────┐        ┌──────────────┐                      │
│  │ JobWorker   │        │ Note CRUD    │                      │
│  │  (existing) │        │  Handlers    │                      │
│  └──────┬──────┘        └──────┬───────┘                      │
│         │ WorkerEvent           │ Direct emit                  │
│         ▼                       ▼                              │
│  ┌──────────────┐        ┌──────────────┐                     │
│  │ Event Bridge │───────▶│  EventBus    │                     │
│  │  (new task)  │        │ (broadcast)  │                     │
│  └──────────────┘        └──────┬───────┘                     │
│                                  │                             │
└──────────────────────────────────┼─────────────────────────────┘
                                   │
                 ┌─────────────────┼─────────────────┐
                 │                 │                 │
                 ▼                 ▼                 ▼
         ┌────────────┐    ┌────────────┐   ┌────────────┐
         │ WS Handler │    │ SSE Handler│   │  Webhook   │
         │ (Axum ws)  │    │(Axum sse)  │   │  Worker    │
         └──────┬─────┘    └──────┬─────┘   └──────┬─────┘
                │                 │                 │
                ▼                 ▼                 ▼
         ┌────────────┐    ┌────────────┐   ┌────────────┐
         │  HotM UI   │    │ MCP Server │   │  External  │
         │  Browser   │    │ (Node.js)  │   │  Systems   │
         └────────────┘    └────────────┘   └────────────┘

         ┌────────────────────────────────────────┐
         │        Telemetry Mirror (trace)        │
         │   (all events → structured logging)    │
         └────────────────────────────────────────┘
```

### 3.2 Event Lifecycle

**Phase 1: Generation**
1. Operation occurs (job status change, note update)
2. Source emits event:
   - Worker: `WorkerEvent` via `worker.event_tx.send()`
   - Note handler: `ServerEvent` via `app_state.event_bus.emit()`

**Phase 2: Distribution**
1. Event bridge translates `WorkerEvent` → `ServerEvent` (if from worker)
2. `EventBus::emit()` sends to broadcast channel
3. All subscribers receive cloned event via `recv()`

**Phase 3: Delivery**
1. Each transport handler processes event:
   - **WebSocket**: Serialize to JSON, send via `ws.send(axum::extract::ws::Message::Text)`
   - **SSE**: Format as `event: EventType\ndata: {...}\nid: 12345\n\n`, write to response stream
   - **Webhook**: HTTP POST to registered URLs with HMAC signature
   - **Telemetry**: Emit structured `tracing::info!` span

**Phase 4: Consumption**
1. Client receives and parses event
2. Client updates UI or triggers workflow
3. (Optional) Client sends acknowledgment (WebSocket only)

### 3.3 Note CRUD Event Integration

Example: Update note endpoint emits `NoteUpdated` after successful database write:

```rust
// crates/matric-api/src/handlers/notes.rs

async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<Note>, ApiError> {
    let updated = state.db.notes.update_note(id, req).await?;

    // Emit event after successful update
    state.event_bus.emit(ServerEvent::NoteUpdated {
        note_id: updated.id,
        title: updated.title.clone(),
    });

    Ok(Json(updated))
}
```

**Integration Points:**
- `create_note()` - after note creation
- `update_note()` - after content/metadata update
- `delete_note()` - emit with title "(deleted)"
- `restore_note()` - after undelete

---

## 4. Technology Stack Decisions

### 4.1 Core Dependencies

| Component | Technology | Version | Justification |
|-----------|-----------|---------|---------------|
| Event Bus | `tokio::sync::broadcast` | 1.x (tokio) | Already in use, zero-copy fan-out, no external deps |
| WebSocket | `axum::extract::ws` | 0.7 | Integrated with Axum, hyper underneath, proven |
| SSE | `axum::response::sse` | 0.7 | Standard Axum response type, automatic headers |
| HTTP Client | `reqwest` | (workspace) | Already in use for Ollama, async, connection pooling |
| Signatures | `hmac` + `sha2` | Latest | Industry standard for webhook signatures |

**No New External Dependencies Required** - All components use existing workspace dependencies or Axum built-ins.

### 4.2 Middleware Stack Compatibility

Eventing routes must integrate with existing `tower-http` middleware (line 26 of `Cargo.toml`):

```rust
// Middleware order (outer to inner):
Router::new()
    .route("/api/v1/ws", get(ws_handler))
    .route("/api/v1/events", get(sse_handler))
    .layer(ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(MakeRequestUuidV7))  // Request correlation
        .layer(PropagateRequestIdLayer::new(HeaderName::from_static("x-request-id")))
        .layer(CorsLayer::permissive())                    // WebSocket upgrade support
        .layer(TraceLayer::new_for_http())                 // Structured logging
        // NOTE: Skip RequestBodyLimitLayer for WebSocket routes
    )
```

**Key Compatibility Decisions:**
- **CORS**: Apply before WebSocket upgrade (required for browser clients)
- **Body Limits**: Exclude WebSocket routes (long-lived connections)
- **Tracing**: Capture upgrade request, spawn separate span for connection lifecycle
- **Rate Limiting**: Apply via existing `governor` middleware (line 27 Cargo.toml)

### 4.3 Message Format Specifications

**WebSocket (JSON-over-WS):**
```json
{
  "type": "job_progress",
  "job_id": "123e4567-e89b-12d3-a456-426614174000",
  "percent": 45,
  "message": "Processing chunk 3/7"
}
```

**SSE (text/event-stream):**
```
event: JobProgress
data: {"job_id":"123e4567-e89b-12d3-a456-426614174000","percent":45,"message":"Processing chunk 3/7"}
id: 12345

```

**Webhook (HTTP POST):**
```
POST https://external.example.com/webhook
Content-Type: application/json
X-Matric-Signature: sha256=abc123def456...
X-Matric-Timestamp: 2026-02-05T10:30:45Z

{"type":"note_updated","note_id":"...","title":"Updated Note"}
```

---

## 5. Security Architecture

### 5.1 Authentication Model

**Design Principle:** Optional authentication (connections work with or without auth).

**WebSocket Upgrade:**
```rust
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Extract Bearer token if present
    let auth_result = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|token| validate_token(token, &state));

    // If auth present, must be valid; if absent, allow (open mode)
    if let Some(Err(e)) = auth_result {
        return (StatusCode::UNAUTHORIZED, "Invalid token").into_response();
    }

    // Proceed with upgrade
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}
```

**Rationale for Optional Auth:**
- Enables testing without OAuth infrastructure
- Supports both public and authenticated deployment models
- Avoids blocking development on separate auth track (issue scope boundary)

**Future Enhancement:** Add per-connection event filtering based on authenticated user's tag scope.

### 5.2 Webhook Security

**SSRF Prevention (Risk R-EVT-002):**

```rust
fn validate_webhook_url(url: &str) -> Result<(), WebhookError> {
    let parsed = Url::parse(url)?;

    // Scheme allowlist
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(WebhookError::InvalidScheme);
    }

    // Resolve DNS and check IP ranges
    let host = parsed.host_str().ok_or(WebhookError::MissingHost)?;
    let addrs: Vec<IpAddr> = tokio::net::lookup_host(host)
        .await?
        .map(|sa| sa.ip())
        .collect();

    for addr in addrs {
        if is_private_ip(&addr) {
            return Err(WebhookError::PrivateIpDenied);
        }
    }

    Ok(())
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() ||
            v4.is_private() ||
            v4.is_link_local() ||
            v4.octets()[0] == 169 && v4.octets()[1] == 254  // AWS metadata
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() ||
            (v6.segments()[0] & 0xfe00) == 0xfc00  // fc00::/7 ULA
        }
    }
}
```

**HMAC Signature Generation:**

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

fn sign_webhook_payload(payload: &str, secret: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC key length valid");
    mac.update(payload.as_bytes());

    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}
```

### 5.3 Event Payload Filtering

**Principle:** Minimize information leakage in broadcast events.

**Sanitization Strategy:**
- Job events: Include job_id, type, percent; exclude raw error messages
- Note events: Include note_id, title (max 100 chars); exclude content
- System events: Aggregate counts only, no user IDs

**Future Enhancement (Risk R-EVT-003):** Add tenant_id field to all events, filter subscribers by authenticated tenant scope.

---

## 6. Key Architectural Decisions

### 6.1 Reference: ADR-037 (Unified Event Bus)

**Decision:** Use `tokio::sync::broadcast` as central event distribution mechanism.

**Alternatives Rejected:**
- PostgreSQL LISTEN/NOTIFY - 100-500ms latency, connection pooling issues
- Redis Pub/Sub - External dependency, network hop, incompatible with Docker bundle simplicity
- Separate channels per transport - Memory overhead, duplicate emit logic

**Consequences:**
- Single point of backpressure (slow receiver affects all)
- Lagged receivers drop events (no persistent queue)
- Manual bridging required (WorkerEvent → ServerEvent)
- Buffer size tuning needed per deployment

**Mitigation (R-EVT-004):** Per-client buffering via bounded MPSC channels.

### 6.2 Broadcast Channel Capacity

**Decision:** 256-slot buffer (increased from worker's 100).

**Calculation:**
- Target: 1000 events/sec throughput
- Client processing time: ~10ms per event (p95)
- Burst tolerance: 256 events ≈ 256ms of buffer
- At 100 clients, 256 slots = 2.56 events per client (sufficient for bursty traffic)

**Monitoring:** Alert if channel >80% full sustained over 1 minute.

### 6.3 Connection Limits

**Decision:** Global limit of 1000 concurrent WebSocket connections (configurable via env var).

**Rationale:**
- Each connection: ~1 MB (socket buffer + MPSC channel)
- Docker bundle: 2 GB container limit
- Reserve 1 GB for application logic, 1 GB for connections
- 1000 connections × 1 MB = 1 GB total

**Enforcement:**
```rust
static WS_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
const MAX_WS_CONNECTIONS: usize = 1000;

async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    let count = WS_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
    if count >= MAX_WS_CONNECTIONS {
        WS_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
        return (StatusCode::SERVICE_UNAVAILABLE, "Connection limit reached").into_response();
    }

    ws.on_upgrade(move |socket| {
        let result = handle_ws_connection(socket).await;
        WS_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
        result
    })
}
```

### 6.4 Webhook Delivery Strategy

**Decision:** Asynchronous delivery with 3-retry exponential backoff.

**Retry Schedule:**
- Attempt 1: Immediate
- Attempt 2: +5 seconds
- Attempt 3: +25 seconds
- Attempt 4: +125 seconds
- Give up: Mark webhook as failed, emit alert

**Rationale:**
- Balances reliability (99%+ success rate) with timeout tolerance (155s max)
- Prevents blocking event loop on slow webhook endpoints
- Exponential backoff reduces load on failing endpoints

**Implementation:** Spawn Tokio task per webhook delivery, use `tokio::time::timeout(30s)` per HTTP request.

---

## 7. Deployment Architecture

### 7.1 Container Integration

**Docker Bundle:** All-in-one container includes PostgreSQL + API + MCP + EventBus.

**Process Topology:**
```
fortemi-bundle container
├── PostgreSQL (port 5432, internal)
├── matric-api (port 3000)
│   ├── HTTP routes (existing)
│   ├── WebSocket endpoint (/api/v1/ws) [NEW]
│   └── SSE endpoint (/api/v1/events) [NEW]
├── JobWorker (background thread)
│   └── Event bridge task → EventBus
└── MCP server (port 3001, Node.js)
    └── SSE client → /api/v1/events
```

**Nginx Reverse Proxy Configuration:**

```nginx
# WebSocket endpoint
location /api/v1/ws {
    proxy_pass http://localhost:3000;

    # WebSocket upgrade headers
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";

    # Long-lived connection timeouts
    proxy_read_timeout 3600s;  # 1 hour idle
    proxy_send_timeout 300s;   # 5 min send
    proxy_connect_timeout 10s; # Fast fail on backend down

    # Disable buffering (low latency)
    proxy_buffering off;
    proxy_cache off;

    # Forward real client IP
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}

# SSE endpoint (similar config)
location /api/v1/events {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_read_timeout 3600s;
    proxy_buffering off;
    chunked_transfer_encoding on;
}
```

### 7.2 Graceful Shutdown

**Shutdown Sequence (Risk R-EVT-010):**

1. **SIGTERM Received**
   - HTTP server stops accepting new connections
   - `/health` endpoint returns 503 (nginx stops routing)

2. **WebSocket Draining (30s grace period)**
   - Send `CloseFrame(1012, "Server restarting")` to all clients
   - Wait for client acknowledgment or timeout

3. **Worker Shutdown**
   - Signal worker via `worker_handle.shutdown()`
   - Worker completes in-flight jobs, stops polling queue

4. **Database Cleanup**
   - Close connection pool
   - Flush pending queries

5. **Exit**
   - Log shutdown complete, exit process

**Implementation:**
```rust
async fn shutdown_signal(app_state: AppState) {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");

    info!("Shutdown signal received");

    // Close all WebSocket connections
    for conn in app_state.connection_registry.active_connections() {
        conn.send_close(CloseCode::ServiceRestart, "Server restarting").await;
    }

    // Wait for connections to close (30s timeout)
    tokio::time::timeout(
        Duration::from_secs(30),
        app_state.connection_registry.wait_for_all_closed()
    ).await;

    // Shutdown worker
    if let Some(handle) = app_state.worker_handle {
        let _ = handle.shutdown().await;
    }
}
```

---

## 8. Observability & Monitoring

### 8.1 Telemetry Mirror Implementation

**Structured Logging:**
```rust
// Telemetry mirror task subscribes to EventBus
async fn telemetry_mirror(event_bus: Arc<EventBus>) {
    let mut rx = event_bus.subscribe();

    loop {
        match rx.recv().await {
            Ok(ServerEvent::JobCompleted { job_id, job_type }) => {
                tracing::info!(
                    subsystem = "events",
                    event.type = "JobCompleted",
                    event.job_id = %job_id,
                    event.job_type = ?job_type,
                    "Job completed"
                );
            }
            Ok(event) => {
                tracing::debug!(
                    subsystem = "events",
                    event.type = ?event,
                    "Event emitted"
                );
            }
            Err(RecvError::Lagged(n)) => {
                tracing::warn!(
                    subsystem = "events",
                    lagged_count = n,
                    "Telemetry mirror lagged"
                );
            }
            Err(RecvError::Closed) => break,
        }
    }
}
```

### 8.2 Metrics (Future Enhancement)

**Proposed Prometheus Metrics:**
```rust
// Connection metrics
ws_connections_active (gauge)
ws_connections_total{status="success|auth_fail|rate_limit"} (counter)
ws_connection_duration_seconds (histogram)

// Event metrics
events_emitted_total{type} (counter)
events_delivered_total{transport,status} (counter)
event_delivery_latency_seconds (histogram)

// Webhook metrics
webhook_deliveries_total{status="success|retry|failed"} (counter)
webhook_retry_attempts (histogram)

// Backpressure metrics
broadcast_channel_lag_events_total (counter)
broadcast_channel_utilization (gauge)
```

**Implementation Note:** Metrics collection deferred to issue #42 (observability track).

---

## 9. Testing Strategy

### 9.1 Integration Test Requirements

**WebSocket Tests:**
```rust
#[tokio::test]
async fn test_ws_upgrade_and_event_delivery() {
    let app = test_app().await;
    let ws_client = connect_websocket(&app, "/api/v1/ws").await;

    // Trigger event (create job)
    let job = app.db.jobs.queue(None, JobType::Embedding, 5, None).await.unwrap();

    // Verify event received
    let msg = ws_client.recv_json::<ServerEvent>().await.unwrap();
    assert!(matches!(msg, ServerEvent::JobQueued { .. }));
}

#[tokio::test]
async fn test_ws_auth_required_when_enabled() {
    let app = test_app().with_auth_enforced().await;

    // Without token
    let result = connect_websocket(&app, "/api/v1/ws").await;
    assert_eq!(result.status(), StatusCode::UNAUTHORIZED);

    // With valid token
    let ws = connect_websocket(&app, "/api/v1/ws?token=valid_token").await;
    assert!(ws.is_connected());
}
```

**SSE Tests:**
```rust
#[tokio::test]
async fn test_sse_event_stream() {
    let app = test_app().await;
    let mut sse_stream = connect_sse(&app, "/api/v1/events").await;

    // Trigger event
    app.db.notes.update_note(note_id, req).await.unwrap();

    // Verify SSE message
    let event = sse_stream.next().await.unwrap();
    assert_eq!(event.event, Some("NoteUpdated".to_string()));
}
```

**Webhook Tests:**
```rust
#[tokio::test]
async fn test_webhook_delivery_with_retry() {
    let mock_server = MockServer::start().await;

    // First attempt fails
    mock_server.mock(|when, then| {
        when.method(POST).path("/webhook");
        then.status(500).delay(Duration::from_secs(1));
    });

    // Register webhook
    let webhook = app.create_webhook("http://localhost:8080/webhook").await;

    // Trigger event
    app.event_bus.emit(ServerEvent::JobCompleted { .. });

    // Verify retry attempts
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(mock_server.requests().len(), 3); // Initial + 2 retries
}
```

### 9.2 Load Testing Targets

**Scenario 1: Concurrent Connections**
- 100 WebSocket clients connected
- 1000 events/min emitted
- Measure: p95 latency <100ms, zero lags

**Scenario 2: Burst Traffic**
- 50 concurrent clients
- 5000 events emitted in 10 seconds
- Measure: No `RecvError::Lagged`, buffer <80% full

**Scenario 3: Slow Client**
- 1 client with 1s recv delay
- 99 normal clients
- Measure: Slow client disconnected, others unaffected

---

## 10. Risks & Mitigations Summary

**From Risk List Document:**

| Risk ID | Description | Mitigation Strategy | Issue Ref |
|---------|-------------|---------------------|-----------|
| R-EVT-001 | Unauthenticated WS | Optional auth with Bearer token validation | #38, #41 |
| R-EVT-002 | Webhook SSRF | Private IP range validation, DNS rebinding protection | #40, #46 |
| R-EVT-004 | Broadcast backpressure | Per-client MPSC buffer, lag detection | #38, #39 |
| R-EVT-005 | Memory exhaustion | Global connection limit (1000), per-IP rate limiting | #38, #44 |
| R-EVT-009 | Nginx timeout | WS-specific config (3600s read timeout) | #45 |
| R-EVT-010 | Abrupt restart | Graceful shutdown with close frame notification | #45 |

**Security Gate Checklist:**
- All WebSocket endpoints support optional auth
- Webhook URLs validated against SSRF
- Connection limits enforced (1000 global, 10 per IP)
- Event payloads sanitized (no raw errors, no PII)

---

## 11. Implementation Roadmap

### Phase 1: Foundation (Issues #38)
- `ServerEvent` enum and `EventBus` struct in matric-core
- Add `event_bus: Arc<EventBus>` to `AppState`
- Initialize EventBus with 256-slot buffer
- Unit tests for EventBus emit/subscribe

### Phase 2: WebSocket Endpoint (Issue #39)
- Add `ws` feature to Axum in `Cargo.toml`
- Implement `ws_handler` route at `/api/v1/ws`
- Connection registry for lifecycle tracking
- JSON serialization of `ServerEvent`
- Integration test: connect, receive event, close

### Phase 3: Worker Bridge (Issue #40)
- Spawn bridge task in main.rs after worker start
- Translate all `WorkerEvent` variants to `ServerEvent`
- Handle `RecvError::Lagged` with warning log
- Integration test: job completion → WebSocket delivery

### Phase 4: Note Events (Issue #41)
- Emit `NoteUpdated` in note CRUD handlers
- Integration test: update note → WS client receives event
- Verify event payload matches HotM expectations

### Phase 5: Connection Management (Issue #42)
- Heartbeat/ping-pong (30s interval)
- Connection timeout (90s no pong)
- Graceful close on disconnect
- Metrics: connection count, duration

### Phase 6: SSE Endpoint (Issue #43)
- Implement SSE handler at `/api/v1/events`
- Format events as `event: Type\ndata: {...}\n\n`
- Last-Event-ID support for reconnection
- Integration test: MCP client subscribes

### Phase 7: Webhook System (Issue #44)
- Webhook CRUD API
- SSRF validation and HMAC signatures
- Async delivery with retry logic
- PostgreSQL webhook table schema

### Phase 8: Telemetry Mirror (Issue #45)
- Spawn telemetry mirror task
- Structured tracing for all events
- Integration with existing `tracing-subscriber`

### Phase 9: Integration Tests (Issue #46)
- End-to-end HotM UI compatibility test
- Load tests (100 concurrent connections)
- Chaos tests (network partition, slow client)
- Security tests (SSRF, auth bypass)

---

## 12. Appendices

### Appendix A: HotM Protocol Compatibility

**Message Type Mapping:**

| HotM Client Expects | ServerEvent Variant | Notes |
|---------------------|---------------------|-------|
| `QueueStatus` | `QueueStatus { pending, active }` | Emitted on worker start |
| `JobQueued` | `JobQueued { job_id, job_type }` | Not in current WorkerEvent, add to bridge |
| `JobStarted` | `JobStarted { job_id, job_type }` | Direct mapping |
| `JobProgress` | `JobProgress { job_id, percent, message }` | Direct mapping |
| `JobCompleted` | `JobCompleted { job_id, job_type }` | Direct mapping |
| `JobFailed` | `JobFailed { job_id, job_type, error }` | Direct mapping |
| `NoteUpdated` | `NoteUpdated { note_id, title }` | New event from CRUD handlers |

**Example HotM Client Code (TypeScript):**
```typescript
// From ui/src/services/websocket.ts
export enum MessageType {
  QueueStatus = 'queue_status',
  JobQueued = 'job_queued',
  JobStarted = 'job_started',
  JobProgress = 'job_progress',
  JobCompleted = 'job_completed',
  JobFailed = 'job_failed',
  NoteUpdated = 'note_updated',
}
```

### Appendix B: Performance Benchmarks

**Target Performance Characteristics:**

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Event emission latency | <10ms (p99) | Timestamp before/after `emit()` |
| Event delivery latency | <100ms (p95) | Timestamp in event payload vs. client receipt |
| Broadcast channel throughput | 1000 events/sec | Load test with burst emission |
| WebSocket connection overhead | <1 MB per connection | Memory profiling with 100 clients |
| Webhook delivery latency | <5s (p95) | Histogram of HTTP request duration |

**Baseline (No Eventing):** Existing API performance with 0% overhead.

**Target (With Eventing):** <5% performance impact on existing REST endpoints.

### Appendix C: Future Enhancements

**Deferred to Post-MVP:**

1. **Event Persistence** - Store events in PostgreSQL for replay/audit
2. **Event Filtering** - Client-side subscription to specific event types
3. **Distributed Event Bus** - Redis Pub/Sub for multi-instance deployments
4. **Sequence Numbers** - Monotonic event IDs for gap detection
5. **Binary Protocol** - MessagePack/Protobuf for reduced bandwidth
6. **Compression** - Gzip for WebSocket/SSE streams
7. **Event Batching** - Coalesce multiple events into single message
8. **Circuit Breakers** - Disable failing webhooks automatically
9. **Tenant Isolation** - Per-tenant event filtering and quotas

---

## Document Control

**Version History:**

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-02-05 | Architecture Team | Initial version |

**Approval Status:**
- Architecture Review: [Pending]
- Security Review: [Pending]
- Operations Review: [Pending]

**Related Documents:**
- ADR-037: Unified Event Bus
- Vision Document: Eventing Track
- Risk List: R-EVT-001 through R-EVT-011
- Scope Boundaries: Eventing Track
- Use Case Briefs: UC-001 through UC-005

**Next Review Date:** 2026-03-01 (post-Phase 9 completion)

---

**End of Document**

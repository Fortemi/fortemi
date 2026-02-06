# Master Test Plan: Eventing, Streaming & Telemetry Track

**Track**: Eventing, Streaming & Telemetry
**Issues**: #38-#46 (fortemi/fortemi)
**Version**: 1.0
**Last Updated**: 2026-02-05
**Status**: Active

---

## Executive Summary

This Master Test Plan defines the comprehensive testing strategy for implementing real-time eventing capabilities in matric-memory. The plan covers all test levels from unit tests through end-to-end integration, with mandatory coverage thresholds and quality gates that MUST be met before deployment.

**Coverage Mandate**: This is not an aspirational goal. These targets are minimum requirements for merging to main.

---

## Test Strategy Overview

### Test Philosophy

1. **Tests are NOT optional** - No code merges without tests
2. **Coverage targets are MINIMUM thresholds** - Not aspirational goals
3. **Tests MUST pass in CI** - No `#[ignore]` to hide failures
4. **Quality gates are BLOCKING** - Phase transitions cannot happen without passing tests

### Test Levels

| Level | Coverage Target | Blocking | Automation |
|-------|----------------|----------|------------|
| Unit Tests | 90% line coverage | Yes - PR merge | CI-required |
| Integration Tests | 85% endpoint coverage | Yes - PR merge | CI-required |
| E2E Tests | 100% use case coverage | Yes - Release | CI-required |
| Performance Tests | Baseline established | Yes - Release | Scheduled |
| Security Tests | All SSRF/Auth vectors | Yes - Release | Manual + CI |

---

## 1. Test Strategy by Issue

### Issue #38: Event Bus Infrastructure

**Component**: `matric-core::events` module

#### Unit Tests (Coverage: 90%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_eventbus_emit_single_event` | EventBus emits ServerEvent to subscribers | Critical |
| `test_eventbus_multiple_subscribers` | All subscribers receive same event | Critical |
| `test_eventbus_lagged_receiver_handling` | Slow receiver gets RecvError::Lagged | High |
| `test_eventbus_channel_capacity` | Broadcast channel respects buffer size | High |
| `test_eventbus_no_subscribers` | Events can be emitted with zero subscribers | Medium |
| `test_serverevent_serialization` | All 7 event types serialize to JSON | Critical |
| `test_serverevent_deserialization` | JSON deserializes to correct event type | Critical |
| `test_serverevent_round_trip` | Serialize → deserialize produces identical event | High |

**Test Infrastructure**:
```rust
// Use #[tokio::test] with in-memory EventBus
use tokio::sync::broadcast;

#[tokio::test]
async fn test_eventbus_emit_single_event() {
    let (tx, mut rx) = broadcast::channel(100);
    let event = ServerEvent::NoteUpdated { note_id: Uuid::new_v4() };

    tx.send(event.clone()).unwrap();
    let received = rx.recv().await.unwrap();

    assert_eq!(received, event);
}
```

**Test Data Strategy**:
- Synthetic `ServerEvent` instances with generated UUIDs
- All 7 event variants must be tested: JobQueued, JobStarted, JobProgress, JobCompleted, JobFailed, NoteUpdated, QueueStatus
- Use chrono::Utc::now() for timestamps to ensure freshness

---

### Issue #39: WebSocket Endpoint

**Component**: `matric-api::handlers::websocket`

#### Integration Tests (Coverage: 85%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_ws_upgrade_success` | HTTP → WebSocket upgrade with valid token | Critical |
| `test_ws_upgrade_requires_auth` | Missing Bearer token returns 401 | Critical |
| `test_ws_json_event_delivery` | ServerEvent delivered as JSON message | Critical |
| `test_ws_refresh_command` | Client sends "refresh", receives QueueStatus | High |
| `test_ws_close_graceful` | Client close frame handled gracefully | High |
| `test_ws_close_server_initiated` | Server can close connection with reason | High |
| `test_ws_ping_pong` | Heartbeat ping → pong response | Medium |
| `test_ws_invalid_message_ignored` | Malformed JSON logged but doesn't crash | Medium |
| `test_ws_connection_limit_enforced` | 1001st connection rejected with 503 | High |

**Test Infrastructure**:
```rust
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn setup_test_server() -> (String, Database) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let db = setup_test_pool().await;

    // Start test server on ephemeral port
    tokio::spawn(async move {
        axum::serve(listener, app(db.clone())).await
    });

    (format!("ws://127.0.0.1:{}", addr.port()), db)
}

#[tokio::test]
async fn test_ws_json_event_delivery() {
    let (ws_url, db) = setup_test_server().await;
    let token = create_test_oauth_token(&db).await;

    let url = format!("{}/api/v1/ws", ws_url);
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Sec-WebSocket-Key", "test")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .body(())
        .unwrap();

    let (mut ws_stream, _) = connect_async(request).await.unwrap();

    // Emit event via EventBus
    let event = ServerEvent::NoteUpdated { note_id: Uuid::new_v4() };
    db.event_bus.emit(event.clone()).await;

    // Receive WebSocket message
    let msg = ws_stream.next().await.unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();

    assert_eq!(json["type"], "NoteUpdated");
}
```

**Test Data Strategy**:
- Test OAuth tokens created via `create_test_client_and_token` (see existing pattern in `auth_middleware_test.rs`)
- Use ephemeral ports to avoid port conflicts between parallel tests
- Unique connection IDs using `format!("test-{}", chrono::Utc::now().timestamp_millis())`

---

### Issue #40: Job Worker Event Bridge

**Component**: `matric-jobs::bridge`, `matric-core::events`

#### Unit Tests (Coverage: 90%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_worker_event_to_server_event_mapping` | WorkerEvent::JobCompleted → ServerEvent::JobCompleted | Critical |
| `test_bridge_includes_note_id` | Note ID from job metadata included in event | Critical |
| `test_bridge_periodic_queue_status` | QueueStatus emitted every 5 seconds | High |
| `test_bridge_handles_missing_note_id` | Jobs without note_id still emit events | Medium |

#### Integration Tests (Coverage: 80%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_job_completion_emits_server_event` | Worker completes job → ServerEvent broadcast | Critical |
| `test_job_failure_emits_server_event` | Worker fails job → JobFailed event | Critical |
| `test_job_progress_forwarded` | Worker reports progress → JobProgress events | High |

**Test Infrastructure**:
```rust
// Build on existing worker_integration_test.rs patterns

#[tokio::test]
async fn test_job_completion_emits_server_event() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::Embedding))
        .build()
        .await;

    let handle = worker.start();
    let mut server_events = db.event_bus.subscribe();

    // Create job
    let job_id = db.jobs.queue(None, JobType::Embedding, 10, None).await.unwrap();

    // Wait for ServerEvent::JobCompleted
    let mut received_completed = false;
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout && !received_completed {
        if let Ok(ServerEvent::JobCompleted { job_id: id, .. }) = server_events.recv().await {
            if id == job_id {
                received_completed = true;
            }
        }
    }

    assert!(received_completed, "Should receive ServerEvent::JobCompleted");
    handle.shutdown().await.unwrap();
}
```

---

### Issue #41: Note CRUD Event Emission

**Component**: `matric-api::handlers::notes`

#### Integration Tests (Coverage: 85%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_create_note_emits_noteupdated` | POST /notes → NoteUpdated event | Critical |
| `test_update_note_emits_noteupdated` | PUT /notes/{id} → NoteUpdated event | Critical |
| `test_delete_note_emits_event` | DELETE /notes/{id} → event with operation type | High |
| `test_post_job_note_update_emits` | Job updates note → NoteUpdated event | Critical |
| `test_bulk_create_emits_multiple_events` | Bulk create → one event per note | Medium |

**Test Infrastructure**:
```rust
#[tokio::test]
async fn test_create_note_emits_noteupdated() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();

    let mut server_events = db.event_bus.subscribe();

    // Create note via API handler
    let note = db.notes.create("Test note", None, vec![]).await.unwrap();

    // Wait for event
    let event = tokio::time::timeout(
        Duration::from_secs(2),
        async {
            loop {
                if let Ok(ServerEvent::NoteUpdated { note_id }) = server_events.recv().await {
                    if note_id == note.id {
                        return note_id;
                    }
                }
            }
        }
    ).await.unwrap();

    assert_eq!(event, note.id);
}
```

---

### Issue #42: Connection Management

**Component**: `matric-api::websocket::connection_manager`

#### Integration Tests (Coverage: 80%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_heartbeat_ping_pong_30s` | Server sends ping every 30s, expects pong | Critical |
| `test_dead_connection_cleanup_90s` | No pong for 90s → connection closed | Critical |
| `test_connection_tracking` | Active connections counted correctly | High |
| `test_connection_cleanup_on_disconnect` | Disconnect removes from registry | High |
| `test_multiple_connections_same_user` | Same user can have multiple WS connections | Medium |

**Test Infrastructure**:
```rust
#[tokio::test]
async fn test_heartbeat_ping_pong_30s() {
    let (ws_url, db) = setup_test_server().await;
    let token = create_test_oauth_token(&db).await;

    let (mut ws_stream, _) = connect_with_auth(&ws_url, &token).await;

    // Wait for ping (up to 35 seconds to account for timing)
    let timeout = Duration::from_secs(35);
    let start = std::time::Instant::now();
    let mut received_ping = false;

    while start.elapsed() < timeout {
        tokio::select! {
            msg = ws_stream.next() => {
                if let Some(Ok(Message::Ping(_))) = msg {
                    // Send pong response
                    ws_stream.send(Message::Pong(vec![])).await.unwrap();
                    received_ping = true;
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    assert!(received_ping, "Should receive ping within 35 seconds");
}
```

---

### Issue #43: SSE Endpoint for MCP

**Component**: `matric-api::handlers::sse`

#### Integration Tests (Coverage: 85%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_sse_stream_connection` | GET /api/v1/events returns text/event-stream | Critical |
| `test_sse_event_format` | Events formatted as `event: Type\ndata: {...}\nid: N\n\n` | Critical |
| `test_sse_keepalive` | Keepalive comments sent every 30s | High |
| `test_sse_multiple_clients` | Multiple SSE clients receive same events | High |
| `test_sse_event_type_filtering` | Query param `?events=NoteUpdated` filters events | Medium |
| `test_sse_reconnect_last_event_id` | Last-Event-ID header resumes stream | Medium |

**Test Infrastructure**:
```rust
use reqwest::Client;

#[tokio::test]
async fn test_sse_event_format() {
    let (server_url, db) = setup_test_server().await;
    let token = create_test_oauth_token(&db).await;

    let client = Client::new();
    let mut response = client
        .get(format!("{}/api/v1/events", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("content-type").unwrap(), "text/event-stream");

    // Emit event
    let note_id = Uuid::new_v4();
    db.event_bus.emit(ServerEvent::NoteUpdated { note_id }).await;

    // Read SSE stream
    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await.unwrap() {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        if buffer.contains("\n\n") {
            break;
        }
    }

    // Verify SSE format
    assert!(buffer.contains("event: NoteUpdated"));
    assert!(buffer.contains("data: {"));
    assert!(buffer.contains(&note_id.to_string()));
}
```

---

### Issue #44: Outbound Webhooks

**Component**: `matric-api::webhooks`

#### Integration Tests (Coverage: 80%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_webhook_crud_api` | POST/GET/DELETE /api/v1/webhooks works | Critical |
| `test_webhook_hmac_signature` | X-Matric-Signature header contains valid HMAC-SHA256 | Critical |
| `test_webhook_delivery_success` | Event triggers HTTP POST to registered URL | Critical |
| `test_webhook_retry_on_failure` | Failed delivery retries with exponential backoff | High |
| `test_webhook_max_retries` | After 4 retries, mark as permanently failed | High |
| `test_webhook_ssrf_prevention_localhost` | URL with 127.0.0.1 rejected with 400 | Critical |
| `test_webhook_ssrf_prevention_private_ip` | URLs with 10.0.0.0/8, 192.168.0.0/16 rejected | Critical |
| `test_webhook_timeout_30s` | Slow webhook (>30s) times out and retries | Medium |

**Test Infrastructure**:
```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_webhook_hmac_signature() {
    let mock_server = MockServer::start().await;
    let db = setup_test_pool().await;

    let secret = "test-secret-key";

    // Register webhook
    let webhook = db.webhooks.create(
        &mock_server.uri(),
        vec!["NoteUpdated"],
        secret
    ).await.unwrap();

    // Setup mock expectation
    let mock = Mock::given(method("POST"))
        .and(path("/"))
        .and(header("X-Matric-Signature", |sig: &str| {
            sig.starts_with("sha256=")
        }))
        .respond_with(ResponseTemplate::new(200))
        .expect(1);

    mock_server.register(mock).await;

    // Emit event
    let note_id = Uuid::new_v4();
    db.event_bus.emit(ServerEvent::NoteUpdated { note_id }).await;

    // Wait for delivery
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify mock received request
    mock_server.verify().await;
}

#[tokio::test]
async fn test_webhook_ssrf_prevention_localhost() {
    let db = setup_test_pool().await;

    // Attempt to register webhook to localhost
    let result = db.webhooks.create(
        "http://127.0.0.1:5432/webhook",
        vec!["NoteUpdated"],
        "secret"
    ).await;

    assert!(result.is_err(), "Should reject localhost URLs");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("private IP") || err.to_string().contains("SSRF"));
}
```

**SSRF Test Coverage**:
- Localhost: 127.0.0.1, ::1, localhost
- Private IPs: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
- Link-local: 169.254.0.0/16, fe80::/10
- Only http/https schemes allowed (reject file://, ftp://, etc.)

---

### Issue #45: Telemetry Mirror

**Component**: `matric-core::telemetry`

#### Unit Tests (Coverage: 90%)

| Test Case | Validates | Priority |
|-----------|-----------|----------|
| `test_telemetry_structured_tracing_jobcompleted` | JobCompleted emits structured tracing event | Critical |
| `test_telemetry_structured_tracing_noteupdated` | NoteUpdated emits structured tracing event | Critical |
| `test_telemetry_all_event_types` | All 7 ServerEvent types emit telemetry | Critical |
| `test_telemetry_counter_metrics` | Event counters increment correctly | High |
| `test_telemetry_gauge_metrics` | Queue depth gauge updates correctly | High |
| `test_telemetry_no_pii_in_logs` | Error messages scrubbed of sensitive data | High |

**Test Infrastructure**:
```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::test]
async fn test_telemetry_structured_tracing_jobcompleted() {
    // Capture tracing events
    let (subscriber, handle) = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO)
        .build();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    // Emit event
    let job_id = Uuid::new_v4();
    let event = ServerEvent::JobCompleted {
        job_id,
        job_type: JobType::Embedding,
        result: Some(json!({"status": "ok"})),
    };

    // Telemetry mirror intercepts and logs
    telemetry::emit_event_telemetry(&event);

    // Verify structured log contains fields
    let logs = handle.get_logs();
    assert!(logs.iter().any(|log| {
        log.contains("event.type") &&
        log.contains("JobCompleted") &&
        log.contains(&job_id.to_string())
    }));
}
```

---

### Issue #46: End-to-End Integration Tests

**Component**: Full system integration

#### E2E Tests (Coverage: 100% of use cases)

Maps to 5 use cases from `use-case-briefs.md`:

| Test Case | Use Case | Validates | Priority |
|-----------|----------|-----------|----------|
| `test_e2e_hotm_receives_job_status` | UC-001 | Full flow: queue job → WS receives JobQueued → JobStarted → JobProgress → JobCompleted | Critical |
| `test_e2e_note_auto_refresh` | UC-002 | AI revision completes → NoteUpdated event → WS client refreshes note | Critical |
| `test_e2e_mcp_monitors_events` | UC-003 | SSE client receives NoteUpdated when note created via API | Critical |
| `test_e2e_webhook_notification` | UC-004 | Register webhook → trigger event → verify HMAC delivery | Critical |
| `test_e2e_telemetry_pipeline` | UC-005 | Events emit structured logs queryable by ops team | High |

**E2E Test Infrastructure**:
```rust
#[tokio::test]
async fn test_e2e_hotm_receives_job_status() {
    // Setup: Start full server with all components
    let (server_url, db) = setup_full_server().await;
    let token = create_test_oauth_token(&db).await;

    // Step 1: Connect WebSocket
    let (mut ws_stream, _) = connect_with_auth(&server_url, &token).await;

    // Step 2: Submit job via API
    let client = reqwest::Client::new();
    let note = client.post(format!("{}/api/v1/notes", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "content": "Test note for AI revision",
            "tags": []
        }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let note_id = Uuid::parse_str(note["id"].as_str().unwrap()).unwrap();

    // Step 3: Queue AI revision job
    let job_id = db.jobs.queue(Some(note_id), JobType::AiRevision, 10, None)
        .await
        .unwrap();

    // Step 4: Collect WebSocket events
    let mut events = Vec::new();
    let timeout = Duration::from_secs(30);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        tokio::select! {
            msg = ws_stream.next() => {
                if let Some(Ok(Message::Text(text))) = msg {
                    let event: serde_json::Value = serde_json::from_str(&text).unwrap();
                    events.push(event.clone());

                    // Stop when we receive JobCompleted for our job
                    if event["type"] == "JobCompleted" &&
                       event["job_id"] == job_id.to_string() {
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    // Step 5: Verify event sequence
    let job_queued = events.iter().any(|e|
        e["type"] == "JobQueued" && e["job_id"] == job_id.to_string()
    );
    let job_started = events.iter().any(|e|
        e["type"] == "JobStarted" && e["job_id"] == job_id.to_string()
    );
    let job_completed = events.iter().any(|e|
        e["type"] == "JobCompleted" && e["job_id"] == job_id.to_string()
    );

    assert!(job_queued, "Should receive JobQueued event");
    assert!(job_started, "Should receive JobStarted event");
    assert!(job_completed, "Should receive JobCompleted event");

    // Step 6: Verify latency <500ms
    let first_event_time = events[0]["timestamp"].as_str().unwrap();
    let last_event_time = events.last().unwrap()["timestamp"].as_str().unwrap();
    // ... latency validation
}
```

---

## 2. Test Coverage Targets

### By Crate

| Crate | Line Coverage | Branch Coverage | Blocking |
|-------|---------------|-----------------|----------|
| matric-core (events module) | 90% | 85% | Yes - PR merge |
| matric-api (websocket handlers) | 80% | 75% | Yes - PR merge |
| matric-api (sse handlers) | 80% | 75% | Yes - PR merge |
| matric-api (webhook delivery) | 80% | 80% | Yes - PR merge |
| matric-jobs (bridge module) | 85% | 80% | Yes - PR merge |

### By Feature

| Feature | Unit Coverage | Integration Coverage | E2E Coverage |
|---------|---------------|---------------------|--------------|
| EventBus | 90% | N/A | Via integration |
| WebSocket | 85% | 85% | 100% (UC-001, UC-002) |
| SSE | 80% | 85% | 100% (UC-003) |
| Webhooks | 80% | 80% | 100% (UC-004) |
| Telemetry | 90% | N/A | 100% (UC-005) |

### E2E Coverage by Use Case

All 5 use cases from `use-case-briefs.md` MUST have automated E2E tests:

- [x] UC-001: HotM UI Receives Real-Time Job Status
- [x] UC-002: Note Auto-Refresh After AI Revision
- [x] UC-003: MCP/AI Agent Monitors Events
- [x] UC-004: External System Receives Webhook Notifications
- [x] UC-005: Operations Team Monitors System Health

---

## 3. Test Infrastructure

### Test Dependencies

Add to workspace `Cargo.toml`:

```toml
[workspace.dependencies]
# WebSocket client for testing
tokio-tungstenite = "0.21"

# HTTP mock server for webhook tests
wiremock = "0.6"

# Test utilities
assert_matches = "1.5"
```

Add to `matric-api/Cargo.toml` dev-dependencies:

```toml
[dev-dependencies]
tokio-tungstenite = { workspace = true }
wiremock = { workspace = true }
assert_matches = { workspace = true }
```

### Test Database Setup

Following existing patterns from `worker_integration_test.rs`:

```rust
/// Create a test database pool from environment or default.
async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}
```

**IMPORTANT**: Per CLAUDE.md testing standards:
- Use `#[tokio::test]` with manual pool setup
- NEVER use `#[sqlx::test]` for tests involving migrations
- Migrations contain `CREATE INDEX CONCURRENTLY` which cannot run in transactions

### Test Server Setup

For integration tests requiring HTTP/WS endpoints:

```rust
use axum::Router;
use std::net::TcpListener;

async fn setup_test_server() -> (String, Database) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let app = Router::new()
        .route("/api/v1/ws", get(websocket_handler))
        .route("/api/v1/events", get(sse_handler))
        .route("/api/v1/webhooks", post(create_webhook_handler))
        .with_state(db.clone());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (format!("http://127.0.0.1:{}", addr.port()), db)
}
```

### OAuth Test Tokens

Following existing pattern from `auth_middleware_test.rs`:

```rust
async fn create_test_oauth_token(db: &Database) -> String {
    let registration = ClientRegistrationRequest {
        client_name: "Test Client".to_string(),
        redirect_uris: vec!["http://localhost/callback".to_string()],
        grant_types: vec!["client_credentials".to_string()],
        response_types: vec![],
        scope: Some("read write".to_string()),
        // ... other fields
        token_endpoint_auth_method: Some("client_secret_basic".to_string()),
    };

    let client = db.oauth.register_client(registration).await.unwrap();
    let client_secret = client.client_secret.unwrap();

    let lifetime = chrono::Duration::hours(1);
    let (access_token, _, _) = db.oauth
        .create_token_with_lifetime(&client.client_id, "read write", None, false, lifetime)
        .await
        .unwrap();

    access_token
}
```

---

## 4. Test Data Strategy

### Event Data

**Synthetic ServerEvent Instances**:
```rust
// Generate test events with realistic data
fn create_test_note_updated_event() -> ServerEvent {
    ServerEvent::NoteUpdated {
        note_id: Uuid::new_v4(),
    }
}

fn create_test_job_completed_event() -> ServerEvent {
    ServerEvent::JobCompleted {
        job_id: Uuid::new_v4(),
        job_type: JobType::Embedding,
        result: Some(json!({"embeddings_generated": 42})),
    }
}
```

### WebSocket Test Data

**Connection Messages**:
```json
// Client → Server: Refresh command
{"type": "refresh"}

// Server → Client: Event delivery
{
  "type": "NoteUpdated",
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-02-05T12:34:56Z"
}
```

### Webhook Test Data

**Using wiremock for delivery verification**:
```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, body_json, header};

let mock_server = MockServer::start().await;

let mock = Mock::given(method("POST"))
    .and(path("/webhook"))
    .and(header("X-Matric-Signature", |sig: &str| sig.starts_with("sha256=")))
    .and(body_json(json!({
        "event": "NoteUpdated",
        "note_id": "550e8400-e29b-41d4-a716-446655440000"
    })))
    .respond_with(ResponseTemplate::new(200))
    .expect(1);

mock_server.register(mock).await;
```

### Test Isolation

Per CLAUDE.md standards:

1. **Unique Identifiers**: Use timestamps for test isolation
   ```rust
   let connection_id = format!("test-conn-{}", chrono::Utc::now().timestamp_millis());
   ```

2. **Track Created Resources**: Store IDs and verify only those records
   ```rust
   let mut created_webhooks = Vec::new();
   created_webhooks.push(webhook_id);
   // Cleanup in test teardown
   ```

3. **Serial Execution**: Configure CI for serial execution where needed
   ```yaml
   # In .gitea/workflows/test.yml
   - run: cargo test --workspace --test websocket_tests -- --test-threads=1
   ```

---

## 5. CI/CD Integration

### Existing CI Pipeline

Tests run in `.gitea/workflows/test.yml`:

```yaml
name: Test

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    runs-on: matric-builder

    services:
      postgres:
        image: pgvector/pgvector:pg16
        env:
          POSTGRES_USER: matric
          POSTGRES_PASSWORD: matric
          POSTGRES_DB: matric
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v4

      - name: Run migrations
        run: sqlx migrate run
        env:
          DATABASE_URL: postgres://matric:matric@localhost/matric

      - name: Run unit tests
        run: cargo test --workspace --lib

      - name: Run integration tests
        run: cargo test --workspace --tests

      - name: Generate coverage
        run: cargo llvm-cov --workspace --lcov --output-path coverage.lcov

      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          files: coverage.lcov
```

### Coverage Enforcement

Add to CI pipeline:

```yaml
      - name: Check coverage thresholds
        run: |
          cargo llvm-cov report --fail-under-lines 80
          cargo llvm-cov report --json > coverage.json

          # Per-crate thresholds
          python scripts/check_coverage.py coverage.json \
            --crate matric-core --threshold 90 \
            --crate matric-api --threshold 80 \
            --crate matric-jobs --threshold 85
```

### Test Categories

```bash
# Fast tests (unit tests only)
cargo test --workspace --lib

# Integration tests (requires database)
cargo test --workspace --tests

# E2E tests (requires full server setup)
cargo test --workspace --test e2e_tests

# All tests
cargo test --workspace
```

---

## 6. Quality Gates

### Phase Gate: Inception → Elaboration

- [x] Test Strategy Document approved (this document)
- [x] Coverage targets defined (90% unit, 85% integration, 100% E2E)
- [x] Test infrastructure dependencies identified (tokio-tungstenite, wiremock)

### Phase Gate: Elaboration → Construction

- [ ] Master Test Plan approved (this document)
- [ ] Test dependencies added to Cargo.toml
- [ ] CI pipeline updated with coverage checks
- [ ] Baseline coverage established (may be 0% for new code)

### Phase Gate: Construction → Transition

**BLOCKING REQUIREMENTS** (code CANNOT merge without these):

- [ ] All unit test coverage targets met (90% matric-core, 85% matric-api, 85% matric-jobs)
- [ ] All integration test coverage targets met (85% endpoint coverage)
- [ ] All E2E tests passing (100% of 5 use cases)
- [ ] No critical/high defects open
- [ ] Security tests passing (SSRF prevention, auth requirements)
- [ ] Performance baseline established (1000 events/sec, 100 concurrent connections)

### Phase Gate: Transition → Production

- [ ] Load testing complete (1000 concurrent WS, 10k events/sec)
- [ ] Chaos testing complete (container restart, network partition)
- [ ] Security audit complete (penetration testing)
- [ ] Operational runbooks validated
- [ ] All test documentation complete

---

## 7. Testing Standards (from CLAUDE.md)

### CRITICAL: Never Use #[ignore]

**NEVER use `#[ignore]` to skip failing tests.** Fix tests properly instead:

- CI has full test infrastructure (PostgreSQL containers, migrations, etc.)
- If tests need serial execution, configure with `--test-threads=1`
- If tests need isolation, use unique identifiers (timestamps, UUIDs)

### PostgreSQL Migration Compatibility

`#[sqlx::test]` runs migrations in a transaction. Some operations **cannot run in transactions**:
- `CREATE INDEX CONCURRENTLY`
- `ALTER TYPE ... ADD VALUE` (enum values)

**Solution**: Use `#[tokio::test]` with manual pool setup:

```rust
async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url).await.expect("Failed to create pool")
}

#[tokio::test]
async fn test_something() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);
    // Test logic...
}
```

### Test Isolation Strategies

For tests sharing database state without transactional rollback:

1. **Unique identifiers**: `format!("test-{}", chrono::Utc::now().timestamp_millis())`
2. **Track created resources**: Store IDs and verify only those records
3. **Serial execution**: Run with `--test-threads=1` (configured in CI)

---

## 8. Performance Testing

### Baseline Metrics

MUST establish baseline before release:

| Metric | Target | Blocking |
|--------|--------|----------|
| Event emission latency (p99) | <10ms | Yes |
| Event delivery latency (p95) | <500ms | Yes |
| Concurrent WebSocket connections | 100+ without degradation | Yes |
| Event throughput | 1000+ events/sec | Yes |
| WebSocket message rate | 100 msg/sec per connection | No |

### Load Test Scripts

```rust
#[tokio::test]
#[ignore] // Run separately as load test
async fn load_test_concurrent_websockets() {
    let (server_url, db) = setup_test_server().await;

    let mut handles = Vec::new();
    for i in 0..100 {
        let url = server_url.clone();
        let token = create_test_oauth_token(&db).await;

        let handle = tokio::spawn(async move {
            let (mut ws, _) = connect_with_auth(&url, &token).await;

            // Keep connection alive for 5 minutes
            let timeout = Duration::from_secs(300);
            let start = std::time::Instant::now();

            while start.elapsed() < timeout {
                if let Some(Ok(msg)) = ws.next().await {
                    // Process message
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all connections
    for handle in handles {
        handle.await.unwrap();
    }
}
```

---

## 9. Security Testing

### Security Test Matrix

MUST cover all attack vectors from `risk-list.md`:

| Risk ID | Test Case | Validates | Blocking |
|---------|-----------|-----------|----------|
| R-EVT-001 | `test_ws_auth_required` | Unauthenticated WS upgrade rejected | Yes |
| R-EVT-002 | `test_webhook_ssrf_localhost` | Localhost URLs rejected | Yes |
| R-EVT-002 | `test_webhook_ssrf_private_ips` | Private IP ranges rejected | Yes |
| R-EVT-003 | `test_event_payload_no_pii` | Error messages sanitized | Yes |
| R-EVT-008 | `test_connection_limit_dos` | 1001st connection rejected | Yes |

### Penetration Testing

Before production deployment:

1. **Manual testing** of authentication bypass attempts
2. **Automated SSRF fuzzing** with known payloads
3. **DoS simulation** with connection flood
4. **Cross-tenant event leakage** testing

---

## 10. Test Execution Schedule

### Developer Workflow

```bash
# Before committing
cargo fmt --check
cargo clippy -- -D warnings
cargo test --workspace --lib  # Fast unit tests

# Before opening PR
cargo test --workspace         # All tests including integration
cargo llvm-cov --workspace     # Check coverage
```

### CI Workflow

```yaml
on_pull_request:
  - cargo fmt --check
  - cargo clippy -- -D warnings
  - cargo test --workspace
  - cargo llvm-cov --fail-under-lines 80

on_push_to_main:
  - All PR checks
  - E2E tests
  - Performance baseline validation
  - Security scans
```

### Release Workflow

```bash
# Before tagging release
1. Run full test suite: cargo test --workspace
2. Run load tests: cargo test --workspace -- --ignored load_test
3. Run security tests: cargo test --workspace security_tests
4. Verify coverage: cargo llvm-cov report
5. Manual E2E validation against staging environment
6. Security audit review
```

---

## 11. Test Ownership

| Component | Test Owner | Reviewer |
|-----------|------------|----------|
| EventBus (matric-core) | Software Implementer | Test Engineer |
| WebSocket Handler (matric-api) | Software Implementer | Integration Engineer |
| SSE Handler (matric-api) | Software Implementer | Integration Engineer |
| Webhook Delivery (matric-api) | Software Implementer | Security Architect |
| Worker Bridge (matric-jobs) | Software Implementer | Test Engineer |
| E2E Tests | Test Engineer | Test Architect |

---

## 12. Success Criteria

The Test Architect has succeeded when:

1. **Every feature has tests before it reaches main branch**
   - No PR merges without accompanying tests
   - Coverage verified in CI before merge

2. **Coverage never decreases sprint over sprint**
   - Baseline established and enforced
   - Coverage trends monitored in dashboards

3. **No critical bugs escape to production**
   - All E2E use cases validated
   - Security tests comprehensive

4. **Test execution time enables rapid feedback**
   - Unit tests complete in <60 seconds
   - Integration tests complete in <5 minutes
   - E2E tests complete in <10 minutes

5. **Developers write tests naturally as part of development**
   - Test infrastructure is easy to use
   - Examples and patterns documented

---

## 13. Blocking Conditions

**Test Architect MUST escalate if:**

- Coverage targets are set below 80% without documented justification
- Tests are marked as `#[ignore]` instead of being fixed
- Phase transitions happen without test gates passing
- Flaky tests are being skipped instead of fixed
- Test automation is deprioritized or delayed

---

## 14. Deliverables

This Master Test Plan satisfies the Test Architect deliverable requirements:

1. [x] **Test Strategy Document** - Section 1 (Test Strategy by Issue)
2. [x] **Master Test Plan** - This entire document
3. [x] **Test Coverage Matrix** - Section 2 (Test Coverage Targets)
4. [x] **Quality Gates Definition** - Section 6 (Quality Gates)
5. [x] **Automation Roadmap** - Section 5 (CI/CD Integration)

---

## 15. Open Questions

1. **Event Persistence**: Should we test event replay functionality if events are persisted to database?
   - Decision needed: If persistence is added, need replay tests
   - Impact: Additional integration tests for event sourcing

2. **Connection Limits**: What is production target for max concurrent connections?
   - Decision needed: 1000 (current plan) or higher
   - Impact: Load test parameters, infrastructure sizing

3. **Flaky Test Handling**: What is acceptable flake rate in CI?
   - Recommendation: 0% - all flakes must be fixed
   - Impact: Developer time investment in test stability

4. **Test Data Cleanup**: Should tests clean up created resources or rely on database isolation?
   - Current approach: Unique identifiers + manual pool setup
   - Alternative: Transaction rollback (but incompatible with migrations)

---

## 16. References

- **Use Cases**: `.aiwg/gates/eventing-track/use-case-briefs.md`
- **Scope**: `.aiwg/gates/eventing-track/scope-boundaries.md`
- **Risks**: `.aiwg/gates/eventing-track/risk-list.md`
- **Testing Standards**: `/home/roctinam/dev/fortemi/CLAUDE.md` (Testing section)
- **Existing Test Patterns**:
  - `crates/matric-jobs/tests/worker_integration_test.rs`
  - `crates/matric-api/tests/oauth_introspect_test.rs`
  - `crates/matric-api/tests/auth_middleware_test.rs`
- **Issues**: GitHub fortemi/fortemi #38-#46

---

## Appendix A: Test File Structure

```
crates/
├── matric-core/
│   ├── src/
│   │   └── events/
│   │       ├── mod.rs
│   │       ├── event_bus.rs
│   │       └── server_event.rs
│   └── tests/
│       └── event_bus_test.rs          # NEW: EventBus unit tests
│
├── matric-api/
│   ├── src/
│   │   └── handlers/
│   │       ├── websocket.rs
│   │       ├── sse.rs
│   │       └── webhooks.rs
│   └── tests/
│       ├── websocket_test.rs          # NEW: WebSocket integration tests
│       ├── sse_test.rs                # NEW: SSE integration tests
│       ├── webhook_test.rs            # NEW: Webhook integration tests
│       ├── webhook_ssrf_test.rs       # NEW: SSRF security tests
│       └── e2e_eventing_test.rs       # NEW: E2E tests for all 5 use cases
│
└── matric-jobs/
    ├── src/
    │   └── bridge.rs
    └── tests/
        ├── worker_integration_test.rs  # EXISTING
        └── worker_bridge_test.rs       # NEW: Worker → Server event bridge tests
```

---

## Appendix B: Quick Reference Checklist

### Before Opening PR

- [ ] All new code has unit tests
- [ ] Integration tests added for new endpoints
- [ ] Coverage meets minimum thresholds
- [ ] No `#[ignore]` attributes
- [ ] Tests pass locally: `cargo test --workspace`
- [ ] Clippy clean: `cargo clippy -- -D warnings`
- [ ] Formatted: `cargo fmt --check`

### Before Merging PR

- [ ] CI tests passing
- [ ] Coverage checks passing
- [ ] Code review approved
- [ ] No regressions in coverage
- [ ] Security tests passing (if applicable)

### Before Release

- [ ] All E2E tests passing
- [ ] Load tests completed
- [ ] Security audit complete
- [ ] Performance baseline validated
- [ ] Operational runbooks tested

---

**Document Version:** 1.0
**Last Updated:** 2026-02-05
**Status:** Active - Ready for Review
**Next Review:** After Elaboration → Construction transition

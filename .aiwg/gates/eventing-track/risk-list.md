# Risk Assessment: Eventing, Streaming & Telemetry Track

**Date:** 2026-02-05
**Track:** Eventing, Streaming & Telemetry
**Issues:** #38-#46 (Fortemi/fortemi)
**Reviewer:** Security Architect

---

## Executive Summary

This risk assessment covers the addition of real-time eventing capabilities (WebSocket, SSE, webhooks) to the matric-memory Rust Axum API server. The track spans 9 issues addressing WebSocket endpoints, SSE streaming, outbound webhooks, authentication, and observability.

**Overall Risk Level:** HIGH
**Critical Risks:** 3 (Security)
**High Risks:** 5 (Technical + Operational)
**Medium Risks:** 2

**Key Concerns:**
- Authentication gaps creating unauthorized access vectors
- SSRF vulnerabilities in webhook delivery
- WebSocket resource exhaustion without rate limiting
- Operational blind spots in WS connection monitoring

---

## Risk Summary Table

| ID | Risk | Category | Likelihood | Impact | Mitigation Owner | Issue Ref |
|----|------|----------|------------|--------|-----------------|-----------|
| R-EVT-001 | Unauthenticated WebSocket connections | Security | High | Show Stopper | #38, #41 | #38, #41 |
| R-EVT-002 | Webhook SSRF via user-controlled URLs | Security | High | High | #40, #46 | #40, #46 |
| R-EVT-003 | Event payload information leakage | Security | Medium | High | #38, #41 | #38, #41 |
| R-EVT-004 | Broadcast channel backpressure | Technical | High | High | #38, #39 | #38, #39 |
| R-EVT-005 | WebSocket memory exhaustion | Technical | High | High | #38, #44 | #38, #44 |
| R-EVT-006 | Event ordering guarantees | Technical | Medium | Medium | #38, #39 | #38, #39 |
| R-EVT-007 | Axum ws feature compatibility | Technical | Low | Medium | #38 | #38 |
| R-EVT-008 | DoS via connection flooding | Security | High | High | #38, #44 | #38, #44 |
| R-EVT-009 | Nginx proxy timeout config | Operational | High | High | #38, #45 | #38, #45 |
| R-EVT-010 | Container restart with active WS | Operational | Medium | High | #38, #45 | #38, #45 |
| R-EVT-011 | WS vs HTTP observability gaps | Operational | High | Medium | #42, #43 | #42, #43 |

---

## Detailed Risk Cards

### R-EVT-001: Unauthenticated WebSocket Connections

**Category:** Security
**Likelihood:** High
**Impact:** Show Stopper

**Description:**
WebSocket endpoints at `/api/v1/ws` currently lack authentication. The existing Axum middleware stack uses `Bearer` token authentication for HTTP endpoints, but WebSocket upgrade requests may bypass these layers. Without proper auth, any client can connect and receive real-time job events containing potentially sensitive metadata (job types, UUIDs, error messages).

**Current State:**
- Axum 0.7 in use (line 24, `Cargo.toml`)
- `ws` feature NOT yet enabled in dependencies
- No auth middleware documented for WS upgrade paths
- `worker.rs` broadcasts events to ALL subscribers (line 117: `broadcast::channel(100)`)

**Attack Vector:**
1. Attacker connects to `wss://fortemi.com/api/v1/ws` without credentials
2. Receives broadcast events for all jobs (JobStarted, JobProgress, JobFailed)
3. Harvests UUIDs, job types, timing data
4. Correlates with known user activity for reconnaissance

**Mitigation Strategy:**

1. **Auth Middleware** (Issue #41):
   - Extract `Authorization: Bearer <token>` from WS upgrade request headers
   - Validate token via existing OAuth introspection before upgrade
   - Reject upgrade with 401 if token missing/invalid
   - Implementation: Axum extractor in WS handler
   ```rust
   async fn ws_handler(
       ws: WebSocketUpgrade,
       TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
   ) -> Response {
       // Validate auth.token() before ws.on_upgrade()
   }
   ```

2. **Per-User Event Filtering** (Issue #38):
   - Tag events with user_id/tenant_id in `WorkerEvent` enum
   - Filter broadcast subscribers by authenticated identity
   - Prevent cross-tenant event leakage

3. **Fallback Policy**:
   - If auth fails, close connection immediately
   - Log auth failures for security monitoring
   - Return 401 with clear error message

**Success Criteria:**
- [ ] All WS connections require valid Bearer token
- [ ] Token validation logs integrated with security audit trail
- [ ] Integration test: unauthenticated WS upgrade rejected with 401
- [ ] Load test: 1000 concurrent authenticated WS connections stable

**References:**
- Issue #38: WebSocket endpoint implementation
- Issue #41: OAuth2/JWT authentication for WS
- `crates/matric-jobs/src/worker.rs`: lines 111-117 (broadcast channel)
- Axum ws auth pattern: <https://docs.rs/axum/0.7/axum/extract/ws/index.html>

---

### R-EVT-002: Webhook SSRF via User-Controlled URLs

**Category:** Security
**Likelihood:** High
**Impact:** High

**Description:**
Outbound webhooks (Issue #40) accept user-provided URLs for event delivery. Without validation, an attacker can register webhooks pointing to internal network resources (127.0.0.1, 169.254.169.254 AWS metadata, internal databases), enabling Server-Side Request Forgery (SSRF) attacks.

**Current State:**
- No webhook delivery infrastructure exists yet
- `reqwest` HTTP client available (workspace dependency, line 52)
- No URL validation or allow/deny list documented
- PostgreSQL on `localhost` (default config)

**Attack Vector:**
1. Attacker registers webhook: `POST /api/v1/webhooks {"url": "http://127.0.0.1:5432"}`
2. Job event triggers webhook delivery
3. matric-api server makes request to local PostgreSQL
4. Attacker probes internal services, exfiltrates cloud metadata, or attacks localhost services

**Mitigation Strategy:**

1. **URL Validation** (Issue #40, #46):
   - Deny private IP ranges: 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16
   - Deny link-local IPv6: fe80::/10, fc00::/7
   - Deny localhost: 127.0.0.1, ::1, localhost
   - Only allow http/https schemes (no file://, ftp://, etc.)
   - Implement DNS rebinding protection: resolve URL, check IP, then make request

2. **Network-Level Controls** (Issue #46):
   - Run webhook worker in isolated network segment
   - Firewall outbound traffic from webhook worker
   - Use egress proxy with allow-list

3. **Request Headers** (Issue #40):
   - Add `User-Agent: matric-memory-webhook/2026.2.7`
   - Add `X-Webhook-ID: <uuid>` for tracking
   - Strip sensitive headers (Authorization from other requests)

4. **Retry/Rate Limiting** (Issue #40):
   - Max 3 retries with exponential backoff
   - 10s timeout per request
   - Rate limit: 100 webhook deliveries/minute per tenant

**Code Reference:**
```rust
// Validation logic example
fn is_safe_webhook_url(url: &str) -> Result<(), WebhookError> {
    let parsed = Url::parse(url)?;

    // Scheme check
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(WebhookError::InvalidScheme);
    }

    // Host check
    let host = parsed.host_str().ok_or(WebhookError::MissingHost)?;
    let addrs: Vec<IpAddr> = resolve_host(host)?;

    for addr in addrs {
        if is_private_ip(&addr) || is_link_local(&addr) {
            return Err(WebhookError::PrivateIpDenied);
        }
    }

    Ok(())
}
```

**Success Criteria:**
- [ ] All private IP ranges blocked
- [ ] DNS rebinding attack prevented (test with rebind.network)
- [ ] Security test: attempting SSRF returns 400 Bad Request
- [ ] Integration test: valid public URL delivers webhook successfully
- [ ] Documented allow-list override for development/testing

**References:**
- Issue #40: Outbound webhook delivery
- Issue #46: Webhook security hardening
- OWASP SSRF: <https://owasp.org/www-community/attacks/Server_Side_Request_Forgery>
- Workspace dependency: `reqwest` (line 52, `Cargo.toml`)

---

### R-EVT-003: Event Payload Information Leakage

**Category:** Security
**Likelihood:** Medium
**Impact:** High

**Description:**
`WorkerEvent` enum (lines 59-82, `worker.rs`) contains detailed job metadata that may leak sensitive information across tenant boundaries or to unauthorized subscribers. Events include job IDs, job types, error messages, and progress details that could expose business logic, data processing patterns, or system internals.

**Current State:**
- Events broadcast to ALL subscribers without filtering (line 117)
- Error messages included verbatim (line 73: `error: String`)
- No tenant isolation in event delivery
- Progress messages may contain user data (line 68: `message: Option<String>`)

**Information at Risk:**
- Job UUIDs (correlate with user activity)
- Job types (reveal enabled features: Embedding, Linking, AiRevision)
- Error messages (may contain SQL fragments, file paths, stack traces)
- Progress messages (could contain note titles, collection names)
- Timing data (job start/complete duration reveals processing patterns)

**Mitigation Strategy:**

1. **Tenant-Scoped Events** (Issue #38, #41):
   - Add `tenant_id: Uuid` field to all `WorkerEvent` variants
   - Filter broadcast subscribers by authenticated tenant
   - Database isolation: ensure jobs table has tenant_id column

2. **Event Sanitization** (Issue #38):
   - Scrub error messages: remove file paths, SQL, stack traces
   - Generic error categories: "DatabaseError", "InferenceError", "ValidationError"
   - Redact progress messages: only include percentage, not details
   ```rust
   pub enum WorkerEvent {
       JobFailed {
           job_id: Uuid,
           job_type: JobType,
           error_category: ErrorCategory, // Not raw error string
           tenant_id: Uuid,
       },
   }
   ```

3. **Event Access Control** (Issue #41):
   - Per-connection subscription scopes: "jobs:read:own", "jobs:read:all" (admin only)
   - OAuth scope validation before event delivery
   - Audit log: who subscribed to which event streams

4. **Opt-In Verbosity** (Issue #38):
   - Default: minimal events (JobStarted, JobCompleted)
   - Admin scope: detailed events with error messages
   - Debug mode: full verbosity (dev/staging only)

**Success Criteria:**
- [ ] Events include tenant_id and are filtered per connection
- [ ] Error messages sanitized (no SQL, no file paths)
- [ ] Integration test: tenant A cannot see tenant B events
- [ ] Security review: event payloads audited for PII/sensitive data
- [ ] Documentation: event schema and access control model

**References:**
- Issue #38: WebSocket event streaming
- Issue #41: Authentication and authorization
- `crates/matric-jobs/src/worker.rs`: lines 59-82 (WorkerEvent enum)
- `crates/matric-jobs/src/worker.rs`: line 117 (broadcast channel)

---

### R-EVT-004: Broadcast Channel Backpressure

**Category:** Technical
**Likelihood:** High
**Impact:** High

**Description:**
Tokio broadcast channel (line 117, `worker.rs`) uses fixed capacity of 100 messages. When a slow WebSocket client cannot consume events fast enough, the channel lags that receiver. With multiple slow clients, the channel may drop messages or block producers, impacting job worker throughput.

**Current State:**
- Broadcast capacity: 100 messages (line 117: `broadcast::channel(100)`)
- No backpressure handling documented
- No per-client buffering strategy
- Job worker sends events synchronously (lines 214-216, 257-259)

**Failure Modes:**
1. **Receiver Lag**: Slow WS client falls behind, `recv()` returns `Err(RecvError::Lagged(n))`
2. **Message Loss**: Lagged receiver misses events (job progress updates, failures)
3. **Producer Blocking**: If send fails, job worker may retry/block (affects throughput)
4. **Memory Pressure**: Fast producers + slow consumers = unbounded queue growth (if using unbounded channel)

**Mitigation Strategy:**

1. **Per-Client Buffering** (Issue #38, #39):
   - Separate bounded MPSC channel per WS connection (capacity: 1000)
   - Spawn bridge task: reads from broadcast, writes to per-client MPSC
   - Drop slow clients if their buffer fills (send disconnect event)
   ```rust
   async fn ws_event_bridge(
       mut broadcast_rx: broadcast::Receiver<WorkerEvent>,
       client_tx: mpsc::Sender<WorkerEvent>,
   ) {
       while let Ok(event) = broadcast_rx.recv().await {
           if client_tx.try_send(event.clone()).is_err() {
               // Client buffer full - disconnect
               break;
           }
       }
   }
   ```

2. **Lag Recovery** (Issue #39):
   - Detect `RecvError::Lagged(n)` and send synthetic event to client
   - Client reconnects and requests missed events from database (if logged)
   - Emit metric: `websocket_lag_total` (Prometheus counter)

3. **Capacity Tuning** (Issue #38):
   - Increase broadcast capacity: 100 → 1000 (supports burst traffic)
   - Monitor: `broadcast_channel_utilization` gauge
   - Alert if >80% full sustained over 1 minute

4. **Non-Blocking Sends** (Issue #39):
   - Job worker uses `try_send()` instead of blocking `send()`
   - Drop events if broadcast channel full (emit warning metric)
   - Prioritize job processing over event delivery

**Performance Targets:**
- Broadcast channel <50% utilization under normal load
- Zero receiver lags with <100 concurrent WS clients
- P95 event delivery latency <50ms (producer to WS client)

**Success Criteria:**
- [ ] Load test: 500 concurrent WS clients, 1000 jobs/min, zero lags
- [ ] Chaos test: simulate slow client (1s recv delay), verify disconnect
- [ ] Metric dashboards: channel utilization, lag events, client disconnects
- [ ] Documentation: backpressure behavior and client recovery

**References:**
- Issue #38: WebSocket implementation
- Issue #39: Event streaming optimizations
- `crates/matric-jobs/src/worker.rs`: line 117 (broadcast channel)
- Tokio broadcast docs: <https://docs.rs/tokio/latest/tokio/sync/broadcast/>

---

### R-EVT-005: WebSocket Memory Exhaustion

**Category:** Technical
**Likelihood:** High
**Impact:** High

**Description:**
Without connection limits or per-connection memory caps, an attacker or misbehaving client can open thousands of WebSocket connections, exhausting server memory and crashing the API container. Each WS connection consumes memory for: socket buffers, per-client event channels, authentication state, and Axum task overhead.

**Current State:**
- No documented connection limits
- No rate limiting on WS upgrades
- Docker bundle runs API in single container (1-2 GB typical allocation)
- Each WS connection: ~16 KB socket buffer + 1 KB/event × 1000 buffer = ~1 MB/connection

**Attack Vector:**
1. Attacker scripts 10,000 WS connections from distributed IPs
2. Each connection allocates 1 MB → 10 GB memory
3. Container OOM killed, service outage
4. Kubernetes/Docker restart loop if attack sustained

**Mitigation Strategy:**

1. **Global Connection Limit** (Issue #38, #44):
   - Cap total concurrent WS connections: 1000 (adjustable via env var)
   - Reject new connections with 503 Service Unavailable if limit reached
   - Implement via shared `Arc<RwLock<usize>>` counter
   ```rust
   static WS_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
   const MAX_WS_CONNECTIONS: usize = 1000;

   async fn ws_handler(ws: WebSocketUpgrade) -> Response {
       let count = WS_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
       if count >= MAX_WS_CONNECTIONS {
           WS_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
           return StatusCode::SERVICE_UNAVAILABLE.into_response();
       }
       // ... proceed with upgrade
   }
   ```

2. **Per-IP Rate Limiting** (Issue #44):
   - Max 10 WS connections per IP address
   - Use `tower-http` rate limiter middleware
   - Existing `governor` dependency available (line 27, API Cargo.toml)
   - Track connections in LRU cache: `lru = "0.12"` (line 48)

3. **Per-User Quota** (Issue #44):
   - Authenticated users: max 5 concurrent WS connections
   - Prevent single user monopolizing resources
   - Store in Redis: `ws_connections:<user_id>` (TTL: connection lifetime)

4. **Memory Limits** (Issue #45):
   - Docker container memory limit: 2 GB
   - OOM score adjustment: prefer killing WS worker over database
   - Kubernetes: `resources.limits.memory: 2Gi`, `requests.memory: 512Mi`

5. **Connection Draining** (Issue #45):
   - Graceful shutdown: stop accepting new WS upgrades
   - Send close frame to all clients with 1011 code (server restart)
   - Wait up to 30s for clients to disconnect before force close

**Monitoring:**
- Gauge: `websocket_connections_active` (by tenant, by IP)
- Counter: `websocket_connections_rejected_total` (reason: limit, auth, rate)
- Histogram: `websocket_connection_duration_seconds`

**Success Criteria:**
- [ ] Load test: 1000 concurrent WS connections stable under limit
- [ ] Attack test: 10,000 connection attempts → 9000 rejected, container stable
- [ ] Metric validation: connection counters accurate
- [ ] Runbook: procedure for raising limits during high load
- [ ] Alert: fire if connections >80% of limit for >5 minutes

**References:**
- Issue #38: WebSocket endpoint
- Issue #44: Rate limiting and DoS protection
- Issue #45: Operational considerations
- `crates/matric-api/Cargo.toml`: line 27 (governor), line 48 (lru)

---

### R-EVT-006: Event Ordering Guarantees

**Category:** Technical
**Likelihood:** Medium
**Impact:** Medium

**Description:**
Multiple subscribers to the same broadcast channel may receive events in different orders due to async scheduling, network latency, or receiver processing speeds. Clients relying on strict event ordering (e.g., JobStarted before JobProgress before JobCompleted) may see inconsistent state.

**Current State:**
- Broadcast channel delivers in send order to single receiver
- No cross-subscriber ordering guarantees documented
- No sequence numbers in `WorkerEvent` enum
- Clients may process events out of order due to async handlers

**Scenarios:**
1. **Job Lifecycle**: Client expects JobStarted → JobProgress → JobCompleted, but sees JobProgress first (if it reconnects mid-job)
2. **Multi-Worker**: Two workers process same job (race condition) → duplicate JobCompleted events
3. **Network Reordering**: SSE events arrive out of order over HTTP/2 multiplexing

**Mitigation Strategy:**

1. **Sequence Numbers** (Issue #39):
   - Add monotonic sequence number to `WorkerEvent`
   ```rust
   pub struct WorkerEvent {
       pub seq: u64,
       pub event_type: EventType,
       // ... other fields
   }
   ```
   - Use `AtomicU64` counter in worker
   - Clients detect gaps and request replay from database

2. **Event Timestamps** (Issue #39):
   - Add `timestamp: chrono::DateTime<Utc>` to all events
   - Clients sort by timestamp if order matters
   - Allows reconstruction of timeline from unordered stream

3. **Per-Job Ordering** (Issue #38):
   - Events for same job_id are causally ordered
   - Use job_id as partition key for ordering guarantee
   - Document: global ordering not guaranteed across jobs

4. **Replay from Database** (Issue #39):
   - Store job events in `job_events` table (optional feature)
   - Clients query `/api/v1/jobs/{id}/events` for authoritative history
   - SSE/WS used for low-latency notifications only

**Trade-offs:**
- Strict ordering requires blocking sends → reduced throughput
- Sequence numbers add 8 bytes per event
- Event storage in database increases I/O load

**Success Criteria:**
- [ ] Document event ordering guarantees (per-job only)
- [ ] Add sequence numbers to events
- [ ] Integration test: verify JobStarted arrives before JobCompleted for same job
- [ ] Client library: example showing gap detection and replay logic
- [ ] Decision: event persistence (database storage) - ADR required

**References:**
- Issue #38: WebSocket events
- Issue #39: Event streaming reliability
- `crates/matric-jobs/src/worker.rs`: lines 59-82 (WorkerEvent enum)

---

### R-EVT-007: Axum ws Feature Compatibility

**Category:** Technical
**Likelihood:** Low
**Impact:** Medium

**Description:**
Axum 0.7 `ws` feature must be enabled and integrated with existing middleware stack (CORS, auth, rate limiting, tracing). Potential conflicts with current Axum configuration or tower middleware layers could block WebSocket upgrades or break existing HTTP routes.

**Current State:**
- Axum 0.7 installed (line 24, API Cargo.toml)
- Features enabled: `json`, `tower-log`, `multipart` (NOT `ws`)
- Existing middleware: tower-http CORS, trace, catch-panic, request-id, limit (line 26)
- No WS-specific middleware documented

**Potential Conflicts:**
1. **CORS Preflight**: WS upgrade requires `Connection: Upgrade` header, may conflict with CORS preflight handling
2. **Request Size Limits**: `tower-http::limit` may reject WS frames (binary messages)
3. **Tracing**: WS connections are long-lived, may spam trace logs
4. **Panic Handling**: `catch-panic` middleware may not cover WS task panics

**Mitigation Strategy:**

1. **Feature Flag** (Issue #38):
   - Add `ws` to Axum features: `axum = { version = "0.7", features = ["json", "tower-log", "multipart", "ws"] }`
   - Verify cargo build succeeds with no version conflicts

2. **Middleware Ordering** (Issue #38):
   - Apply CORS before WS upgrade
   - Apply auth middleware after CORS, before upgrade
   - Skip request size limits for WS routes
   ```rust
   let app = Router::new()
       .route("/api/v1/ws", get(ws_handler))
       .layer(ServiceBuilder::new()
           .layer(CorsLayer::permissive())
           .layer(AuthLayer::new())
       )
       .route("/api/v1/notes", get(list_notes))
       .layer(ServiceBuilder::new()
           .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
       );
   ```

3. **WS-Specific Middleware** (Issue #38):
   - Custom `ws::Message` size limit: 1 MB (reject larger frames)
   - Ping/pong interval: 30s (detect dead connections)
   - Idle timeout: 5 minutes (close inactive connections)

4. **Integration Testing** (Issue #38):
   - Test: WS upgrade succeeds through full middleware stack
   - Test: CORS headers present on WS upgrade response
   - Test: Auth failure rejects upgrade before socket creation
   - Test: Existing HTTP routes unaffected by WS feature

**Success Criteria:**
- [ ] Cargo build succeeds with `ws` feature
- [ ] All existing API tests pass (no regressions)
- [ ] WS upgrade test passes through CORS + auth middleware
- [ ] Load test: 100 concurrent HTTP + WS requests, no conflicts
- [ ] Documentation: middleware ordering and WS-specific config

**References:**
- Issue #38: WebSocket implementation
- `crates/matric-api/Cargo.toml`: lines 24-26 (Axum deps)
- Axum WS example: <https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs>

---

### R-EVT-008: DoS via Connection Flooding

**Category:** Security
**Likelihood:** High
**Impact:** High

**Description:**
Attackers can flood the server with connection requests (HTTP or WS) to exhaust CPU, memory, or file descriptors, causing service degradation or outage. Unlike established connection limits (R-EVT-005), this targets the connection setup phase (SYN flood, TLS handshake, HTTP upgrade).

**Current State:**
- No documented rate limiting on connection attempts
- Nginx reverse proxy in front of API (CLAUDE.md mentions proxy for MCP)
- No SYN cookie config or connection backlog tuning mentioned
- Docker bundle deployment: single container (no load balancer redundancy)

**Attack Vectors:**
1. **SYN Flood**: Exhaust TCP backlog queue (kernel-level)
2. **TLS Handshake**: Slow TLS handshake (Slowloris-style) ties up worker threads
3. **HTTP Upgrade Spam**: Rapid WS upgrade requests consume HTTP connection pool
4. **Distributed Attack**: Bypass IP-based rate limiting via botnet

**Mitigation Strategy:**

1. **Nginx Rate Limiting** (Issue #44, #45):
   - `limit_req_zone $binary_remote_addr zone=ws_limit:10m rate=10r/s;`
   - `limit_req zone=ws_limit burst=20 nodelay;`
   - Apply to `/api/v1/ws` route specifically
   - Return 429 Too Many Requests with Retry-After header

2. **TCP/Kernel Hardening** (Issue #45):
   - Enable SYN cookies: `net.ipv4.tcp_syncookies = 1`
   - Increase backlog: `net.core.somaxconn = 4096`
   - Reduce SYN timeout: `net.ipv4.tcp_synack_retries = 2` (default 5)

3. **Connection Limits** (Issue #44):
   - Nginx: `limit_conn_zone $binary_remote_addr zone=conn_limit:10m;`
   - `limit_conn conn_limit 100;` (max 100 connections per IP)
   - Apply globally to prevent exhaustion

4. **TLS Session Resumption** (Issue #45):
   - Enable TLS session tickets (reduce handshake overhead)
   - Nginx: `ssl_session_cache shared:SSL:10m;`
   - `ssl_session_timeout 10m;`

5. **DDoS Mitigation Service** (Issue #46):
   - Cloudflare, AWS Shield, or similar in front of nginx
   - WAF rules: block known bot signatures
   - GeoIP blocking: restrict to expected regions
   - Challenge-response for suspicious IPs (CAPTCHA, JS challenge)

**Monitoring:**
- Counter: `http_requests_rejected_total{reason="rate_limit"}`
- Counter: `tcp_syn_cookies_sent_total` (kernel metric)
- Counter: `nginx_limit_req_exceeded_total`
- Alert: >1000 req/s to WS endpoint (baseline: <100 req/s)

**Success Criteria:**
- [ ] Nginx rate limiting configured and tested
- [ ] Kernel tuning applied to production hosts
- [ ] Load test: 10,000 req/s to WS endpoint → service remains responsive
- [ ] Attack simulation: Slowloris attack mitigated (connections timeout)
- [ ] Runbook: DDoS response procedure (enable stricter limits, engage CDN)

**References:**
- Issue #44: Rate limiting
- Issue #45: Operational readiness
- Issue #46: Security hardening
- Nginx rate limiting: <https://www.nginx.com/blog/rate-limiting-nginx/>

---

### R-EVT-009: Nginx Proxy Timeout Configuration

**Category:** Operational
**Likelihood:** High
**Impact:** High

**Description:**
Nginx reverse proxy requires specific timeout configuration for long-lived WebSocket connections. Default HTTP timeouts (60s) will prematurely close idle WS connections, causing client disconnects and poor UX. Incorrect timeout values can also cause resource leaks or delayed error detection.

**Current State:**
- Nginx reverse proxy documented for MCP server (CLAUDE.md: "Configure nginx to proxy to the container")
- No specific WS timeout config mentioned
- MCP server uses SSE transport (long-lived connections)
- Default nginx timeout: 60s (`proxy_read_timeout`)

**Failure Modes:**
1. **Idle Timeout**: WS connection idle for 60s → nginx closes → client sees unexpected disconnect
2. **Send Timeout**: Large message takes >60s to send → nginx aborts → partial message loss
3. **Buffering**: nginx buffers WS frames → increased latency, memory pressure
4. **Upgrade Delay**: nginx delays Upgrade response → client timeout

**Mitigation Strategy:**

1. **WS-Specific Timeouts** (Issue #45):
   ```nginx
   location /api/v1/ws {
       proxy_pass http://localhost:3000;

       # WebSocket upgrade headers
       proxy_http_version 1.1;
       proxy_set_header Upgrade $http_upgrade;
       proxy_set_header Connection "upgrade";

       # Timeouts for long-lived connections
       proxy_read_timeout 3600s;  # 1 hour idle timeout
       proxy_send_timeout 300s;   # 5 min send timeout
       proxy_connect_timeout 10s; # Fast fail on backend down

       # Disable buffering (low latency)
       proxy_buffering off;
       proxy_cache off;

       # Forward real client IP
       proxy_set_header X-Real-IP $remote_addr;
       proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
   }
   ```

2. **SSE Route Config** (Issue #39, #45):
   - Similar config for `/api/v1/events` (SSE endpoint)
   - Set `Content-Type: text/event-stream`
   - Disable gzip compression (breaks SSE streaming)

3. **Health Check Exclusion** (Issue #45):
   - `/health` endpoint uses short timeout (10s)
   - Load balancer health checks don't affect WS routes

4. **Timeout Monitoring** (Issue #43):
   - Log WS connection duration on close
   - Alert if median duration <60s (indicates premature closes)
   - Metric: `websocket_connection_duration_seconds` histogram

**Testing:**
1. **Idle Test**: Open WS, send no messages for 2 hours → connection stays alive
2. **Burst Test**: Send 1000 messages/sec for 5 min → no timeouts
3. **Large Message**: Send 10 MB binary frame → succeeds (or rejects with clear error)
4. **Backend Restart**: Restart API container → nginx detects and closes WS gracefully

**Success Criteria:**
- [ ] Nginx config file updated with WS-specific timeouts
- [ ] Config tested in staging environment
- [ ] Load test: 100 concurrent WS, idle for 1 hour, zero disconnects
- [ ] Documentation: nginx config template with comments
- [ ] Deployment: ansible/docker-compose includes nginx config
- [ ] Monitoring: dashboard showing WS connection lifetimes

**References:**
- Issue #45: Operational deployment
- Issue #38: WebSocket implementation
- Issue #39: SSE streaming
- CLAUDE.md: Nginx reverse proxy section
- Nginx WS proxying: <https://nginx.org/en/docs/http/websocket.html>

---

### R-EVT-010: Container Restart with Active WebSocket Connections

**Category:** Operational
**Likelihood:** Medium
**Impact:** High

**Description:**
Docker container restarts (deployments, crashes, manual restart) abruptly close all active WebSocket connections without graceful client notification. Clients see broken sockets and must implement retry logic. Frequent restarts during deployments cause poor UX and event loss.

**Current State:**
- Docker bundle deployment (`docker-compose.bundle.yml`)
- No graceful shutdown documented for WS connections
- No zero-downtime deployment strategy mentioned
- Kubernetes not in use (single-container Docker Compose)

**Impact:**
1. **Event Loss**: In-flight events dropped when container stops
2. **Client Confusion**: Abrupt disconnect without error message
3. **Reconnect Storm**: 1000 clients reconnect simultaneously after restart
4. **Database Load**: Mass reconnection triggers auth validation spike
5. **User Experience**: Real-time updates pause during deployments

**Mitigation Strategy:**

1. **Graceful Shutdown** (Issue #45):
   - Intercept SIGTERM signal (Docker stop)
   - Stop accepting new WS upgrades (return 503)
   - Send close frame to all connected clients with code 1012 (Service Restart)
   - Wait up to 30s for clients to acknowledge close
   - Force close remaining connections
   - Shutdown worker and database pools
   ```rust
   async fn graceful_shutdown(ws_server: WsServer) {
       ws_server.stop_accepting_connections();

       for client in ws_server.active_connections() {
           client.send_close(CloseCode::ServiceRestart, "Server restarting").await;
       }

       tokio::time::timeout(
           Duration::from_secs(30),
           ws_server.wait_for_connections_closed()
       ).await;

       ws_server.force_close_all();
   }
   ```

2. **Client Reconnect Logic** (Issue #38):
   - Document expected client behavior
   - Exponential backoff: 1s, 2s, 4s, 8s, max 30s
   - Jitter: randomize retry timing (avoid thundering herd)
   - Max retries: 10 attempts before giving up
   - Show "Reconnecting..." UI state

3. **Zero-Downtime Deployment** (Issue #45):
   - **Blue-Green**: Run new container, migrate traffic, stop old
   - **Rolling Update**: If Kubernetes, use `maxUnavailable: 0`
   - **Connection Draining**: nginx marks backend as draining, waits for WS close

4. **Health Check Coordination** (Issue #45):
   - `/health` returns 503 during shutdown grace period
   - Load balancer stops sending new traffic
   - Existing connections complete gracefully

5. **Event Replay** (Issue #39):
   - Clients track last received event sequence number
   - After reconnect, request missed events: `GET /api/v1/events?since=<seq>`
   - Server sends buffered events or queries database

**Operational Procedures:**
1. **Deployment SOP**:
   - Schedule during low-traffic window (if possible)
   - Notify users: "Brief interruption expected"
   - Monitor: connection count, reconnect rate, error logs
   - Rollback plan: revert to previous container image

2. **Emergency Restart**:
   - Use `docker stop --time=30` (allow grace period)
   - NOT `docker kill` (immediate termination)

**Success Criteria:**
- [ ] Graceful shutdown implemented and tested
- [ ] Integration test: send SIGTERM → all WS clients receive close frame
- [ ] Load test: 1000 active WS → restart → all reconnect within 60s
- [ ] Metrics: `websocket_connections_closed_total{reason="shutdown"}`
- [ ] Documentation: deployment runbook with WS considerations
- [ ] Client library: example reconnect logic

**References:**
- Issue #45: Operational readiness
- Issue #38: WebSocket implementation
- Issue #39: Event streaming reliability
- Docker graceful shutdown: <https://docs.docker.com/engine/reference/commandline/stop/>

---

### R-EVT-011: WebSocket vs HTTP Observability Gaps

**Category:** Operational
**Likelihood:** High
**Impact:** Medium

**Description:**
Existing observability stack (tracing, metrics, logs) is optimized for HTTP request/response patterns. WebSocket connections lack visibility into: active connection count, message rate, error conditions, client latency, and resource consumption. This creates operational blind spots for troubleshooting and capacity planning.

**Current State:**
- `tracing` and `tracing-subscriber` in use (workspace deps, lines 72-74)
- `tower-http` trace middleware for HTTP (line 26, API Cargo.toml)
- No WS-specific instrumentation documented
- No metrics collection (Prometheus/StatsD) mentioned

**Observability Gaps:**
1. **Connection Lifecycle**: Can't see: when clients connect, duration, disconnect reasons
2. **Message Throughput**: Missing: messages/sec per connection, message sizes
3. **Error Rate**: No tracking: auth failures, rate limit hits, malformed messages
4. **Latency**: Unknown: time from event generation to client delivery
5. **Resource Usage**: Invisible: memory per connection, CPU for message serialization

**Mitigation Strategy:**

1. **Structured Logging** (Issue #43):
   - Log events: WS upgrade, auth success/failure, client disconnect
   - Include context: client IP, user_id, connection_id, duration
   ```rust
   tracing::info!(
       client_ip = %addr,
       user_id = %user.id,
       connection_id = %conn_id,
       "WebSocket connection established"
   );
   ```

2. **Prometheus Metrics** (Issue #42, #43):
   - Gauge: `websocket_connections_active{tenant_id}`
   - Counter: `websocket_connections_total{status="success|auth_fail|rate_limit"}`
   - Counter: `websocket_messages_sent_total`, `websocket_messages_received_total`
   - Histogram: `websocket_connection_duration_seconds`
   - Histogram: `websocket_message_size_bytes`
   - Histogram: `websocket_event_delivery_latency_seconds` (generation to send)

3. **Distributed Tracing** (Issue #43):
   - OpenTelemetry spans for WS lifecycle
   - Trace ID propagation: client sends trace header, correlate with job events
   - Parent span: WS connection → child spans: auth, event sends
   ```rust
   let span = tracing::info_span!(
       "websocket_connection",
       connection_id = %conn_id,
       user_id = %user.id
   );
   ```

4. **Health Dashboard** (Issue #42):
   - Grafana dashboard: WS connections over time, by tenant
   - Panels: auth failure rate, message throughput, error types
   - Alerts: connections >1000, auth failures >100/min, avg latency >1s

5. **Client-Side Metrics** (Issue #42):
   - Client reports: connection state, reconnect count, message receive rate
   - Send via separate HTTP endpoint: `POST /api/v1/telemetry`
   - Aggregate for end-to-end visibility

**Implementation:**
- Use `metrics` crate: <https://docs.rs/metrics/latest/metrics/>
- Prometheus exporter endpoint: `GET /metrics`
- Existing `tracing` integration: emit metrics via tracing events

**Success Criteria:**
- [ ] Metrics endpoint `/metrics` returns WS metrics in Prometheus format
- [ ] Grafana dashboard deployed with 8 WS-specific panels
- [ ] Alert rules defined for critical conditions (connections, errors)
- [ ] Log aggregation: WS events queryable in Loki/Elasticsearch
- [ ] Documentation: metric definitions and alert thresholds
- [ ] Runbook: how to debug "clients not receiving events"

**References:**
- Issue #42: Metrics and observability
- Issue #43: Distributed tracing
- `Cargo.toml`: lines 72-74 (tracing)
- Metrics crate: <https://docs.rs/metrics/>
- Axum metrics example: <https://github.com/tokio-rs/axum/blob/main/examples/prometheus-metrics/src/main.rs>

---

## Mitigation Roadmap

### Critical Path (Must complete before GA)

1. **R-EVT-001: WS Authentication** (Issue #41)
   - Effort: 3 days
   - Blocks: All WS features (security gate)

2. **R-EVT-002: Webhook SSRF Protection** (Issue #40, #46)
   - Effort: 2 days
   - Blocks: Webhook GA

3. **R-EVT-005: Connection Limits** (Issue #44)
   - Effort: 1 day
   - Blocks: Production deployment

4. **R-EVT-009: Nginx Timeouts** (Issue #45)
   - Effort: 0.5 days
   - Blocks: Stable WS connections

### High Priority (Should complete for beta)

5. **R-EVT-004: Backpressure Handling** (Issue #38, #39)
   - Effort: 3 days
   - Improves: Reliability under load

6. **R-EVT-008: DoS Protection** (Issue #44)
   - Effort: 2 days
   - Improves: Security posture

7. **R-EVT-010: Graceful Shutdown** (Issue #45)
   - Effort: 2 days
   - Improves: Deployment UX

### Medium Priority (Nice to have)

8. **R-EVT-003: Event Sanitization** (Issue #38)
   - Effort: 2 days
   - Improves: Privacy

9. **R-EVT-006: Event Ordering** (Issue #39)
   - Effort: 2 days
   - Improves: Client simplicity

10. **R-EVT-011: Observability** (Issue #42, #43)
    - Effort: 3 days
    - Improves: Operations

### Low Priority (Can defer)

11. **R-EVT-007: Axum ws Feature** (Issue #38)
    - Effort: 0.5 days
    - Risk: Low (proven integration pattern)

---

## Security Gate Checklist

Before merging to main, the following MUST be addressed:

- [ ] **R-EVT-001**: All WS endpoints require authentication (Bearer token)
- [ ] **R-EVT-002**: Webhook URLs validated against SSRF (deny private IPs)
- [ ] **R-EVT-003**: Event payloads scrubbed of sensitive data
- [ ] **R-EVT-008**: Rate limiting configured (nginx + application layer)
- [ ] Security review: threat model updated with WS/webhook attack surface
- [ ] Penetration test: simulate auth bypass, SSRF, DoS attacks
- [ ] Security audit: external review of webhook delivery and WS auth code

---

## Testing Requirements

### Integration Tests
- WS connection with valid/invalid auth tokens
- WS message exchange (client → server → broadcast → client)
- SSE event stream subscription and delivery
- Webhook delivery with SSRF attempt (rejected)
- Connection limit enforcement (1001st connection rejected)

### Load Tests
- 1000 concurrent WS connections, 1000 jobs/min processing
- 10,000 HTTP req/s with 500 active WS connections
- Webhook delivery: 100 deliveries/sec sustained for 10 min

### Chaos Tests
- API container restart during active WS connections
- Network partition between API and database
- Slow webhook endpoint (10s response time)
- Malformed WS messages (invalid JSON, oversized frames)

### Security Tests
- Auth bypass attempts (missing token, expired token, forged token)
- SSRF payloads (localhost, private IPs, cloud metadata endpoints)
- DoS simulation (connection flood, message spam)
- Cross-tenant event leakage (tenant A sees tenant B events)

---

## Operational Readiness Review (ORR)

Before production deployment, verify:

1. **Runbooks**:
   - [ ] "WebSocket clients not connecting" troubleshooting guide
   - [ ] "Webhook delivery failing" diagnosis steps
   - [ ] "API container restart" procedure with WS considerations
   - [ ] "Rate limit tuning" guide for scaling

2. **Monitoring**:
   - [ ] Grafana dashboard: WS connections, message rates, errors
   - [ ] Alerts configured: connection limit, auth failure rate, latency
   - [ ] Log aggregation: WS events indexed and queryable
   - [ ] SLO defined: 99.9% WS uptime, P95 latency <100ms

3. **Capacity Planning**:
   - [ ] Benchmark: max concurrent WS connections per container
   - [ ] Load test results: documented breaking points
   - [ ] Scaling plan: when to add containers/increase limits
   - [ ] Cost estimate: infrastructure for 10k active WS clients

4. **Incident Response**:
   - [ ] On-call training: WS-specific failure modes
   - [ ] Escalation path: when to engage security team (auth issues)
   - [ ] Disaster recovery: procedure if all WS connections lost

---

## Issue Mapping

| Issue | Title | Related Risks |
|-------|-------|---------------|
| #38 | WebSocket /api/v1/ws endpoint | R-EVT-001, R-EVT-004, R-EVT-005, R-EVT-007 |
| #39 | SSE /api/v1/events endpoint | R-EVT-004, R-EVT-006, R-EVT-009 |
| #40 | Outbound webhook delivery | R-EVT-002 |
| #41 | OAuth2/JWT authentication for WS/SSE | R-EVT-001, R-EVT-003 |
| #42 | Prometheus metrics for eventing | R-EVT-011 |
| #43 | Distributed tracing for events | R-EVT-011 |
| #44 | Rate limiting for WS/webhooks | R-EVT-005, R-EVT-008 |
| #45 | Operational deployment considerations | R-EVT-009, R-EVT-010 |
| #46 | Security hardening (SSRF, DoS) | R-EVT-002, R-EVT-008 |

---

## Open Questions for Product/Engineering

1. **Event Persistence**: Should job events be stored in database for replay? (R-EVT-006)
   - Decision needed: event retention policy, storage cost vs reliability
   - Impact: client recovery logic, database schema changes

2. **Multi-Tenancy**: How strict should tenant isolation be for events? (R-EVT-003)
   - Decision needed: shared vs dedicated broadcast channels per tenant
   - Impact: memory usage, event routing complexity

3. **Webhook Retries**: Max retry attempts and backoff strategy? (R-EVT-002)
   - Decision needed: 3 retries (current plan) vs more aggressive
   - Impact: delivery guarantees, infrastructure cost

4. **Connection Limits**: Production capacity target? (R-EVT-005)
   - Decision needed: 1000 (conservative) vs 5000 (aggressive)
   - Impact: infrastructure sizing, load balancer config

5. **Graceful Shutdown**: Zero-downtime deployment requirement? (R-EVT-010)
   - Decision needed: acceptable downtime during deployments
   - Impact: deployment complexity (Kubernetes vs Docker Compose)

---

## References

- **Codebase**: `fortemi/fortemi` (NOT `roctinam/matric`)
- **Issues**: GitHub fortemi/fortemi #38-#46
- **Architecture**: `.aiwg/architecture/ADR-*.md`
- **Existing Eventing**: `crates/matric-jobs/src/worker.rs` (broadcast channel)
- **MCP Server**: `mcp-server/index.js` (SSE transport reference)
- **Dependencies**: `Cargo.toml`, `crates/matric-api/Cargo.toml`
- **Deployment**: `docker-compose.bundle.yml`, CLAUDE.md nginx config

---

**Next Steps:**
1. Prioritize risks with product and engineering leads
2. Assign mitigation owners to issues #38-#46
3. Create ADRs for open questions (event persistence, tenant isolation)
4. Schedule security review for authentication and webhook delivery
5. Define SLOs for eventing track (latency, uptime, throughput)

**Review Cadence:**
- Weekly: track progress on critical risks (R-EVT-001, R-EVT-002, R-EVT-005)
- Bi-weekly: re-assess likelihood/impact as implementation proceeds
- Pre-GA: final security gate review

---

**Document Version:** 1.0
**Last Updated:** 2026-02-05
**Status:** Draft - Awaiting Review

# Real-Time Events

Fortémi provides a real-time event system for monitoring job progress, queue status, and note updates. The system supports three delivery mechanisms: Server-Sent Events (SSE), WebSocket, and Webhooks.

## Overview

The real-time event system was implemented in Issues #38-#46 (see ADR-037) and uses a `tokio::sync::broadcast` channel as the central EventBus. Events are broadcast to all connected clients (SSE/WebSocket) and delivered to registered webhooks with optional HMAC signing.

**EventBus Architecture:**
- Broadcast channel with 256-message capacity (recommended for production)
- Automatic queue status broadcasts every 5 seconds when subscribers exist
- Worker bridge translates job events into server events
- Telemetry mirror for structured tracing at debug level (`fortemi::events`)

## Event Types Reference

All events use tagged JSON serialization with `{"type": "EventName", ...fields}` format.

| Event Type | Description | Fields |
|------------|-------------|--------|
| `QueueStatus` | Periodic queue statistics | `total_jobs` (i64), `running` (i64), `pending` (i64) |
| `JobQueued` | Job added to queue | `job_id` (UUID), `job_type` (String), `note_id` (Option<UUID>) |
| `JobStarted` | Job processing started | `job_id` (UUID), `job_type` (String), `note_id` (Option<UUID>) |
| `JobProgress` | Job progress update | `job_id` (UUID), `note_id` (Option<UUID>), `progress` (i32), `message` (Option<String>) |
| `JobCompleted` | Job finished successfully | `job_id` (UUID), `job_type` (String), `note_id` (Option<UUID>), `duration_ms` (Option<i64>) |
| `JobFailed` | Job failed with error | `job_id` (UUID), `job_type` (String), `note_id` (Option<UUID>), `error` (String) |
| `NoteUpdated` | Note created/updated/refreshed | `note_id` (UUID), `title` (Option<String>), `tags` (Vec<String>), `has_ai_content` (bool), `has_links` (bool) |

### JSON Examples

**QueueStatus:**
```json
{
  "type": "QueueStatus",
  "total_jobs": 42,
  "running": 3,
  "pending": 39
}
```

**JobStarted:**
```json
{
  "type": "JobStarted",
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "job_type": "Embedding",
  "note_id": "123e4567-e89b-12d3-a456-426614174000"
}
```

**JobProgress:**
```json
{
  "type": "JobProgress",
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "note_id": "123e4567-e89b-12d3-a456-426614174000",
  "progress": 65,
  "message": "Processing chunk 13 of 20"
}
```

**JobCompleted:**
```json
{
  "type": "JobCompleted",
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "job_type": "Embedding",
  "note_id": "123e4567-e89b-12d3-a456-426614174000",
  "duration_ms": 1523
}
```

**JobFailed:**
```json
{
  "type": "JobFailed",
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "job_type": "Embedding",
  "note_id": "123e4567-e89b-12d3-a456-426614174000",
  "error": "Embedding model connection timeout"
}
```

**NoteUpdated:**
```json
{
  "type": "NoteUpdated",
  "note_id": "123e4567-e89b-12d3-a456-426614174000",
  "title": "Project Architecture Notes",
  "tags": ["architecture", "backend"],
  "has_ai_content": true,
  "has_links": true
}
```

## SSE (Server-Sent Events)

**Endpoint:** `GET /api/v1/events`

Server-Sent Events provide a unidirectional stream from server to client using standard HTTP. Each event includes both an `event:` type field and `data:` JSON payload.

### Format

```
event: QueueStatus
data: {"type":"QueueStatus","total_jobs":42,"running":3,"pending":39}

event: JobStarted
data: {"type":"JobStarted","job_id":"550e8400-e29b-41d4-a716-446655440000","job_type":"Embedding"}

event: keepalive
data: keepalive
```

Keep-alive messages are sent every 15 seconds to prevent connection timeout.

### curl Example

```bash
curl -N -H "Accept: text/event-stream" http://localhost:3000/api/v1/events
```

### JavaScript EventSource Example

```javascript
const eventSource = new EventSource('http://localhost:3000/api/v1/events');

eventSource.addEventListener('QueueStatus', (event) => {
  const data = JSON.parse(event.data);
  console.log('Queue stats:', data);
});

eventSource.addEventListener('JobStarted', (event) => {
  const data = JSON.parse(event.data);
  console.log('Job started:', data.job_id, data.job_type);
});

eventSource.addEventListener('JobProgress', (event) => {
  const data = JSON.parse(event.data);
  console.log(`Job ${data.job_id}: ${data.progress}%`, data.message);
});

eventSource.addEventListener('JobCompleted', (event) => {
  const data = JSON.parse(event.data);
  console.log('Job completed:', data.job_id, `in ${data.duration_ms}ms`);
});

eventSource.addEventListener('NoteUpdated', (event) => {
  const data = JSON.parse(event.data);
  console.log('Note updated:', data.note_id, data.title);
});

eventSource.onerror = (error) => {
  console.error('SSE error:', error);
  eventSource.close();
};
```

**Important:** SSE uses broadcast semantics. Slow receivers that cannot keep up with the event rate will have events dropped.

## WebSocket

**Endpoint:** `GET /api/v1/ws`

WebSocket provides bidirectional communication. The server sends JSON-encoded ServerEvents as text messages, and clients can send commands.

### Client Commands

- `"refresh"` - Trigger immediate QueueStatus broadcast

### Connection Health

- Ping/pong every 30 seconds
- Connection count tracked atomically
- Multiple concurrent clients supported

### JavaScript WebSocket Example

```javascript
const ws = new WebSocket('ws://localhost:3000/api/v1/ws');

ws.onopen = () => {
  console.log('WebSocket connected');
  // Request immediate queue status
  ws.send('refresh');
};

ws.onmessage = (event) => {
  const serverEvent = JSON.parse(event.data);

  switch (serverEvent.type) {
    case 'QueueStatus':
      console.log('Queue:', serverEvent.total_jobs, 'total,',
                  serverEvent.running, 'running,', serverEvent.pending, 'pending');
      break;

    case 'JobStarted':
      console.log('Job started:', serverEvent.job_id, serverEvent.job_type);
      break;

    case 'JobProgress':
      console.log(`Job ${serverEvent.job_id}: ${serverEvent.progress}%`);
      if (serverEvent.message) {
        console.log('  ', serverEvent.message);
      }
      break;

    case 'JobCompleted':
      console.log('Job completed:', serverEvent.job_id,
                  `in ${serverEvent.duration_ms}ms`);
      break;

    case 'JobFailed':
      console.error('Job failed:', serverEvent.job_id, serverEvent.error);
      break;

    case 'NoteUpdated':
      console.log('Note updated:', serverEvent.note_id, serverEvent.title);
      break;
  }
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('WebSocket closed');
};
```

## Webhooks

Webhooks deliver events to external HTTP endpoints with optional HMAC-SHA256 signing.

### Webhook API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/v1/webhooks` | Create webhook |
| `GET` | `/api/v1/webhooks` | List all webhooks |
| `GET` | `/api/v1/webhooks/:id` | Get specific webhook |
| `PATCH` | `/api/v1/webhooks/:id` | Update webhook (url, events, active, secret) |
| `DELETE` | `/api/v1/webhooks/:id` | Delete webhook |
| `GET` | `/api/v1/webhooks/:id/deliveries` | List delivery logs (with limit param) |
| `POST` | `/api/v1/webhooks/:id/test` | Send test delivery |

### Webhook Features

- **Event filtering:** Subscribe to specific event types
- **HMAC-SHA256 signing:** Optional signature in `X-Fortemi-Signature` header
- **Delivery recording:** Success/failure status tracked
- **Event type header:** `X-Fortemi-Event` header on each delivery
- **Timeout:** 10-second timeout per delivery
- **Concurrency:** Deliveries sent via `tokio::spawn`

### Creating a Webhook

```bash
curl -X POST http://localhost:3000/api/v1/webhooks \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://example.com/fortemi-webhook",
    "events": ["JobCompleted", "JobFailed", "NoteUpdated"],
    "active": true,
    "secret": "my-webhook-secret"
  }'
```

Response:
```json
{
  "id": "a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d",
  "url": "https://example.com/fortemi-webhook",
  "events": ["JobCompleted", "JobFailed", "NoteUpdated"],
  "active": true,
  "created_at": "2026-02-05T10:30:00Z"
}
```

### Listing Webhooks

```bash
curl http://localhost:3000/api/v1/webhooks
```

### Testing a Webhook

```bash
curl -X POST http://localhost:3000/api/v1/webhooks/a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d/test
```

### Viewing Delivery Logs

```bash
# Get last 50 deliveries
curl "http://localhost:3000/api/v1/webhooks/a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d/deliveries?limit=50"
```

### Webhook Payload Format

Each webhook delivery includes:

**Headers:**
- `Content-Type: application/json`
- `X-Fortemi-Event: JobCompleted` (event type)
- `X-Fortemi-Signature: sha256=abc123...` (if secret configured)

**Body:**
```json
{
  "type": "JobCompleted",
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "job_type": "Embedding",
  "note_id": "123e4567-e89b-12d3-a456-426614174000",
  "duration_ms": 1523
}
```

### HMAC Signature Verification

If a webhook has a configured secret, the `X-Fortemi-Signature` header contains the HMAC-SHA256 signature:

**Python example:**
```python
import hmac
import hashlib

def verify_signature(payload, signature_header, secret):
    expected = 'sha256=' + hmac.new(
        secret.encode(),
        payload.encode(),
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature_header)

# In your webhook handler
payload = request.body.decode('utf-8')
signature = request.headers.get('X-Fortemi-Signature')
if verify_signature(payload, signature, 'my-webhook-secret'):
    # Signature valid, process event
    event = json.loads(payload)
```

**Node.js example:**
```javascript
const crypto = require('crypto');

function verifySignature(payload, signatureHeader, secret) {
  const expected = 'sha256=' + crypto
    .createHmac('sha256', secret)
    .update(payload)
    .digest('hex');
  return crypto.timingSafeEqual(
    Buffer.from(expected),
    Buffer.from(signatureHeader)
  );
}

// In your webhook handler
app.post('/fortemi-webhook', (req, res) => {
  const payload = JSON.stringify(req.body);
  const signature = req.headers['x-fortemi-signature'];

  if (verifySignature(payload, signature, 'my-webhook-secret')) {
    // Signature valid, process event
    const event = req.body;
    console.log('Event type:', event.type);
    res.status(200).send('OK');
  } else {
    res.status(401).send('Invalid signature');
  }
});
```

## Architecture

### EventBus Design

```
┌─────────────────────────────────────────────────────────────┐
│                         EventBus                            │
│              (tokio::sync::broadcast channel)               │
│                    Capacity: 256 messages                   │
└───────────────────┬─────────────────────────────────────────┘
                    │
         ┌──────────┼──────────┐
         │          │          │
         ▼          ▼          ▼
    ┌────────┐ ┌────────┐ ┌─────────┐
    │  SSE   │ │   WS   │ │Webhooks │
    │Clients │ │Clients │ │Delivery │
    └────────┘ └────────┘ └─────────┘
```

### Worker Bridge

The `bridge_worker_events` function translates job worker events into server events:

- `WorkerEvent::Started` → `ServerEvent::JobStarted`
- `WorkerEvent::Progress` → `ServerEvent::JobProgress`
- `WorkerEvent::Completed` → `ServerEvent::JobCompleted`
- `WorkerEvent::Failed` → `ServerEvent::JobFailed`

### Telemetry Mirror

All EventBus events are logged at debug level under the `fortemi::events` tracing target:

```
2026-02-05T10:30:15.234Z DEBUG fortemi::events: Broadcasting event type=JobStarted job_id=550e8400-e29b-41d4-a716-446655440000
2026-02-05T10:30:16.789Z DEBUG fortemi::events: Broadcasting event type=JobProgress job_id=550e8400-e29b-41d4-a716-446655440000 progress=50
2026-02-05T10:30:18.123Z DEBUG fortemi::events: Broadcasting event type=JobCompleted job_id=550e8400-e29b-41d4-a716-446655440000 duration_ms=2889
```

## Nginx Configuration

When deploying behind an Nginx reverse proxy, WebSocket connections require special headers:

```nginx
location /api/v1/ws {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;

    # Disable buffering for real-time events
    proxy_buffering off;

    # Increase timeout for long-lived connections
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
}

location /api/v1/events {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;

    # SSE requires chunked transfer encoding
    proxy_buffering off;
    proxy_cache off;
    chunked_transfer_encoding on;

    # Keep connection alive
    proxy_read_timeout 3600s;
}
```

## Troubleshooting

### Connection Drops

**Symptom:** SSE/WebSocket connections close unexpectedly

**Causes:**
- Reverse proxy timeout (check nginx `proxy_read_timeout`)
- Client network timeout
- Server restart or deployment

**Solutions:**
- Increase proxy timeouts to 1 hour (3600s)
- Implement client-side reconnection logic with exponential backoff
- Monitor keep-alive messages (SSE: every 15s, WS ping: every 30s)

### Missing Events

**Symptom:** Some events not received by clients

**Causes:**
- Slow receiver with broadcast channel overflow (capacity: 256)
- Client subscribed after event was broadcast
- Event filtering (webhooks only receive subscribed event types)

**Solutions:**
- For SSE/WebSocket: Events are fire-and-forget, consider webhook delivery if guaranteed delivery is required
- For webhooks: Check delivery logs at `/api/v1/webhooks/:id/deliveries`
- Monitor `QueueStatus` events to detect queue backlog

### Webhook Delivery Failures

**Symptom:** Webhooks not receiving events

**Causes:**
- Webhook URL unreachable (DNS, firewall, SSL certificate issues)
- Webhook endpoint timeout (>10 seconds)
- Webhook marked inactive
- Event type not in webhook's subscribed events list

**Solutions:**
1. Check webhook is active: `GET /api/v1/webhooks/:id`
2. Review delivery logs: `GET /api/v1/webhooks/:id/deliveries?limit=50`
3. Test webhook endpoint: `POST /api/v1/webhooks/:id/test`
4. Verify endpoint responds within 10 seconds
5. Check server logs for delivery errors (`fortemi::events` tracing target)

**Testing webhook endpoint:**
```bash
# Send test event manually
curl -X POST https://example.com/fortemi-webhook \
  -H "Content-Type: application/json" \
  -H "X-Fortemi-Event: JobCompleted" \
  -d '{"type":"JobCompleted","job_id":"test","job_type":"Test"}'
```

## See Also

- [API Reference](api.md) - Complete REST API documentation
- [MCP Server](mcp.md) - Model Context Protocol integration
- [Architecture](architecture.md) - System design and component overview
- [Operations](operations.md) - Deployment and monitoring

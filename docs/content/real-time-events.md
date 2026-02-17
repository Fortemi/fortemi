# Real-Time Events

Fortemi provides a real-time event system for monitoring job progress, queue status, note mutations, and system changes. The system supports three delivery mechanisms: Server-Sent Events (SSE), WebSocket, and Webhooks.

## Overview

The real-time event system uses a `tokio::sync::broadcast` channel as the central EventBus (ADR-037). Events are broadcast to all connected clients (SSE/WebSocket) and delivered to registered webhooks with optional HMAC signing.

**EventBus Architecture:**
- Broadcast channel with 256-message capacity (configurable via `EVENT_BUS_CAPACITY`)
- Replay buffer (1024 events, configurable via `SSE_REPLAY_BUFFER_SIZE`) for `Last-Event-ID` reconnection
- Automatic queue status broadcasts every 5 seconds when subscribers exist
- Worker bridge translates job events into server events
- SSE metrics (connections, throughput, lag) exposed via `/health`

```
┌─────────────────────────────────────────────────────────────┐
│                         EventBus                            │
│              (tokio::sync::broadcast + replay buffer)       │
│                    Capacity: 256 messages                   │
│                    Replay: 1024 events                      │
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

## Event Envelope Schema

All SSE events are wrapped in a versioned `EventEnvelope` — a self-describing wrapper with metadata. This is the contract for all SSE consumers.

```json
{
  "event_id": "019507a3-1234-7000-8000-abcdef012345",
  "event_type": "note.created",
  "occurred_at": "2026-02-17T10:30:15.234Z",
  "memory": "research",
  "actor": {
    "kind": "system",
    "id": null,
    "name": null
  },
  "entity_type": "note",
  "entity_id": "123e4567-e89b-12d3-a456-426614174000",
  "payload_version": 1,
  "payload": {
    "NoteCreated": {
      "note_id": "123e4567-e89b-12d3-a456-426614174000",
      "title": "My Note",
      "tags": ["architecture"]
    }
  }
}
```

### Envelope Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `event_id` | UUID (v7) | Yes | Unique, monotonically increasing event ID |
| `event_type` | String | Yes | Namespaced event type (e.g., `note.created`) |
| `occurred_at` | ISO 8601 | Yes | When the event occurred (UTC) |
| `memory` | String | No | Memory/archive scope. Null for system events |
| `tenant_id` | String | No | Tenant identifier (multi-tenant deployments) |
| `actor` | Object | Yes | Who/what caused the event |
| `actor.kind` | String | Yes | `"system"`, `"user"`, or `"agent"` |
| `actor.id` | String | No | Actor identifier (user ID, API key ID) |
| `actor.name` | String | No | Display name |
| `entity_type` | String | No | Entity this event relates to (e.g., `"note"`, `"job"`) |
| `entity_id` | String | No | Entity identifier |
| `correlation_id` | UUID | No | Links related events across operations |
| `causation_id` | UUID | No | ID of the event that caused this event |
| `payload_version` | Integer | Yes | Schema version for the payload (currently `1`) |
| `payload` | Object | Yes | Domain-specific event data (tagged union) |

### SSE Frame Format

Each SSE frame includes three fields:

```
event: note.created
id: 019507a3-1234-7000-8000-abcdef012345
data: {"event_id":"019507a3-1234-7000-8000-abcdef012345","event_type":"note.created",...}

```

- `event:` — namespaced type for `addEventListener()` filtering
- `id:` — UUIDv7 for `Last-Event-ID` replay on reconnection
- `data:` — full `EventEnvelope` JSON

## Event Catalog

### Event Priority

Events are classified by priority for backpressure decisions:

| Priority | Behavior | Event Types |
|----------|----------|-------------|
| **Critical** | Never dropped or coalesced | All `note.*`, `attachment.*`, `collection.*`, `archive.*` events |
| **Normal** | Delivered in order; dropped only under severe lag | `job.queued`, `job.started`, `job.completed`, `job.failed` |
| **Low** | May be coalesced within time windows | `job.progress`, `queue.status` |

### Note Events

| Event Type | Description | Entity Fields |
|------------|-------------|---------------|
| `note.created` | A new note was created | `note_id`, `title`, `tags` |
| `note.updated` | A note was updated (content, tags, AI refresh) | `note_id`, `title`, `tags`, `has_ai_content`, `has_links` |
| `note.deleted` | A note was soft-deleted | `note_id` |
| `note.archived` | A note was archived | `note_id` |
| `note.restored` | A note was restored from archive/deletion | `note_id` |
| `note.tags.updated` | Tags on a note were changed | `note_id`, `tags` |
| `note.links.updated` | Semantic links updated (by background job) | `note_id` |
| `note.revision.created` | An AI revision was created | `note_id` |

### Attachment Events

| Event Type | Description | Entity Fields |
|------------|-------------|---------------|
| `attachment.created` | A file was uploaded to a note | `attachment_id`, `note_id`, `filename` |
| `attachment.deleted` | An attachment was deleted | `attachment_id`, `note_id` |
| `attachment.extraction.updated` | Extraction metadata updated | `attachment_id`, `note_id` |

### Collection Events

| Event Type | Description | Entity Fields |
|------------|-------------|---------------|
| `collection.created` | A collection was created | `collection_id`, `name` |
| `collection.updated` | A collection was renamed/updated | `collection_id`, `name` |
| `collection.deleted` | A collection was deleted | `collection_id` |
| `collection.membership.changed` | A note was moved into/out of a collection | `collection_id`, `note_id` |

### Archive (Memory) Events

| Event Type | Description | Entity Fields |
|------------|-------------|---------------|
| `archive.created` | A memory archive was created | `name`, `archive_id` |
| `archive.updated` | A memory archive was updated | `name` |
| `archive.deleted` | A memory archive was deleted | `name` |
| `archive.default.changed` | The default memory was changed | `name` |

### Job Events

| Event Type | Description | Entity Fields |
|------------|-------------|---------------|
| `job.queued` | A job was added to the queue | `job_id`, `job_type`, `note_id` |
| `job.started` | A job started processing | `job_id`, `job_type`, `note_id` |
| `job.progress` | Job progress update (coalescable) | `job_id`, `note_id`, `progress`, `message` |
| `job.completed` | A job completed successfully | `job_id`, `job_type`, `note_id`, `duration_ms` |
| `job.failed` | A job failed with error | `job_id`, `job_type`, `note_id`, `error` |

### System Events

| Event Type | Description | Fields |
|------------|-------------|--------|
| `queue.status` | Periodic queue statistics (coalescable) | `total_jobs`, `running`, `pending` |

### Synthetic Events (Server-Generated)

These events are generated by the SSE handler, not by domain operations:

| Event Type | Description | When |
|------------|-------------|------|
| `resync_required` | Client must perform full state refresh | `Last-Event-ID` expired from replay buffer |
| `events.lagged` | Events were dropped for this client | Client too slow to keep up with broadcast |

## SSE Endpoint

**Endpoint:** `GET /api/v1/events`

### Authentication

Browser `EventSource` cannot set custom headers, so authentication is supported via query parameters:

| Method | Example |
|--------|---------|
| Query param (preferred for browsers) | `?token=mm_at_xxx` or `?token=mm_key_xxx` |
| Authorization header | `Authorization: Bearer mm_at_xxx` |

When `REQUIRE_AUTH=true`, one of the above is required. When `REQUIRE_AUTH=false` (default), authentication is optional.

### Memory Scoping

Events can be scoped to a specific memory (archive):

| Method | Example | Behavior |
|--------|---------|----------|
| Query param | `?memory=research` | Only events from "research" memory |
| Header | `X-Fortemi-Memory: research` | Same as query param |
| Neither | (omit) | All events from all memories (admin view) |

System events (no memory scope) pass through all memory filters.

### Type Filtering

Reduce stream noise by filtering to specific event types:

```
GET /api/v1/events?types=note
GET /api/v1/events?types=note.created,note.updated
GET /api/v1/events?types=note,collection
```

- **Prefix matching:** `?types=note` matches `note.created`, `note.updated`, etc.
- **Exact matching:** `?types=note.created` matches only `note.created`
- **Multiple types:** Comma-separated, any match delivers the event
- **Case-insensitive:** `?types=Note` works the same as `?types=note`
- **No filter:** All event types are delivered

### Entity Filtering

Filter events for a specific entity:

```
GET /api/v1/events?entity_id=123e4567-e89b-12d3-a456-426614174000
```

Only events where `envelope.entity_id` matches are delivered.

### Combined Filters

All filters can be combined:

```
GET /api/v1/events?token=mm_at_xxx&memory=research&types=note&entity_id=abc-123
```

### Replay (Last-Event-ID)

The SSE endpoint supports automatic reconnection via the standard `Last-Event-ID` mechanism:

1. Browser `EventSource` automatically sends the `Last-Event-ID` header on reconnect
2. The server replays all buffered events since that ID (up to 1024 events retained)
3. Replay events are delivered before the live stream begins (no gap)
4. If the event ID has expired from the buffer, a `resync_required` event is sent

**Delivery semantics:** At-least-once. During the replay-to-live transition, some events may be delivered twice. Clients should deduplicate by `event_id`.

**Manual replay via curl:**
```bash
curl -N -H "Last-Event-ID: 019507a3-1234-7000-8000-abcdef012345" \
     http://localhost:3000/api/v1/events
```

### Backpressure and Coalescing

**Broadcast backpressure:** The broadcast channel has a fixed capacity (256 messages). When a slow consumer falls behind, it receives an `events.lagged` notification with the number of dropped events.

**Coalescing:** Low-priority events (`job.progress`, `queue.status`) are debounced per stream. Events with the same coalescing key within the window (default 500ms, configurable via `SSE_COALESCE_WINDOW_MS`) are deduplicated — only the first event in each window is delivered. Set to `0` to disable.

Critical events (note, attachment, collection, archive mutations) are never coalesced.

### Keep-Alive

Keep-alive messages are sent every 15 seconds to prevent connection timeouts:

```
: keepalive

```

### curl Example

```bash
# Basic connection
curl -N http://localhost:3000/api/v1/events

# With auth and memory scope
curl -N "http://localhost:3000/api/v1/events?token=mm_at_xxx&memory=research"

# Filtered to note events only
curl -N "http://localhost:3000/api/v1/events?types=note"
```

### JavaScript EventSource Example

```javascript
// Basic connection with type filter
const url = new URL('http://localhost:3000/api/v1/events');
url.searchParams.set('types', 'note,collection');
url.searchParams.set('memory', 'research');
// url.searchParams.set('token', 'mm_at_xxx');  // if auth required

const eventSource = new EventSource(url);

// Listen to specific event types (namespaced names)
eventSource.addEventListener('note.created', (event) => {
  const envelope = JSON.parse(event.data);
  console.log('Note created:', envelope.payload.NoteCreated.note_id);
});

eventSource.addEventListener('note.updated', (event) => {
  const envelope = JSON.parse(event.data);
  console.log('Note updated:', envelope.payload.NoteUpdated.note_id);
});

eventSource.addEventListener('collection.created', (event) => {
  const envelope = JSON.parse(event.data);
  console.log('Collection:', envelope.payload.CollectionCreated.name);
});

// Handle server-generated events
eventSource.addEventListener('resync_required', (event) => {
  const data = JSON.parse(event.data);
  console.warn('Resync required:', data.reason);
  // Perform full state refresh from REST API
});

eventSource.addEventListener('events.lagged', (event) => {
  const data = JSON.parse(event.data);
  console.warn('Events dropped:', data.dropped_count);
  // Consider adding type filters to reduce stream volume
});

// EventSource auto-reconnects with Last-Event-ID
eventSource.onerror = (error) => {
  console.error('SSE connection lost, auto-reconnecting...');
};
```

## SSE Metrics

The `/health` endpoint includes SSE subsystem metrics:

```json
{
  "sse": {
    "connections_total": 42,
    "disconnections_total": 38,
    "active_connections": 4,
    "events_emitted": 12847,
    "events_delivered": 51293,
    "events_coalesced": 1024,
    "events_lagged": 0,
    "replays_success": 7,
    "replays_expired": 1
  }
}
```

| Metric | Description |
|--------|-------------|
| `connections_total` | Total SSE connections opened since startup |
| `disconnections_total` | Total SSE disconnections since startup |
| `active_connections` | Current active SSE connections |
| `events_emitted` | Total events emitted to broadcast bus |
| `events_delivered` | Total events delivered to SSE clients (after filtering) |
| `events_coalesced` | Total events skipped by coalescing |
| `events_lagged` | Total events dropped due to slow consumers |
| `replays_success` | Successful `Last-Event-ID` replays |
| `replays_expired` | Expired replay cursor attempts |

## Migration from Legacy Format

### Breaking Changes (v2026.2)

The SSE subsystem was overhauled in Epic #450. Key changes:

1. **Envelope wrapping:** Events are now wrapped in `EventEnvelope` instead of bare `ServerEvent` JSON. The payload is under the `payload` key.

2. **Namespaced event types:** Event types changed from PascalCase to dot-notation:
   - `NoteUpdated` → `note.updated`
   - `JobStarted` → `job.started`
   - `QueueStatus` → `queue.status`

3. **SSE `id:` field:** Each SSE frame now includes an `id:` line with the UUIDv7 event_id.

4. **New event types:** 18 new event types for note lifecycle, attachments, collections, and archives.

### Migration Guide

**Before (legacy):**
```javascript
eventSource.addEventListener('NoteUpdated', (event) => {
  const data = JSON.parse(event.data);
  console.log('Note:', data.note_id);
});
```

**After (v2026.2+):**
```javascript
eventSource.addEventListener('note.updated', (event) => {
  const envelope = JSON.parse(event.data);
  console.log('Note:', envelope.payload.NoteUpdated.note_id);
  console.log('Memory:', envelope.memory);
  console.log('Event ID:', envelope.event_id);
});
```

**WebSocket:** The WebSocket endpoint (`/api/v1/ws`) still uses the legacy `ServerEvent` format for backward compatibility. Clients that need the envelope schema should migrate to SSE.

## WebSocket

**Endpoint:** `GET /api/v1/ws`

WebSocket provides bidirectional communication. The server sends JSON-encoded ServerEvents as text messages (legacy format), and clients can send commands.

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
  ws.send('refresh');
};

ws.onmessage = (event) => {
  const serverEvent = JSON.parse(event.data);
  // Note: WebSocket uses legacy format (no envelope wrapping)
  switch (serverEvent.type) {
    case 'QueueStatus':
      console.log('Queue:', serverEvent.total_jobs, 'total');
      break;
    case 'JobStarted':
      console.log('Job started:', serverEvent.job_id);
      break;
    case 'NoteUpdated':
      console.log('Note updated:', serverEvent.note_id);
      break;
  }
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

### HMAC Signature Verification

If a webhook has a configured secret, the `X-Fortemi-Signature` header contains the HMAC-SHA256 signature:

```python
import hmac, hashlib

def verify_signature(payload, signature_header, secret):
    expected = 'sha256=' + hmac.new(
        secret.encode(), payload.encode(), hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature_header)
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `EVENT_BUS_CAPACITY` | `256` | Broadcast channel capacity |
| `SSE_REPLAY_BUFFER_SIZE` | `1024` | Events retained for `Last-Event-ID` replay |
| `SSE_COALESCE_WINDOW_MS` | `500` | Coalescing window for low-priority events (0 to disable) |
| `MATRIC_WEBHOOK_TIMEOUT_SECS` | `10` | Webhook delivery timeout |

## Nginx Configuration

When deploying behind an Nginx reverse proxy:

```nginx
location /api/v1/events {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;

    # SSE requires no buffering
    proxy_buffering off;
    proxy_cache off;
    chunked_transfer_encoding on;

    # Keep connection alive
    proxy_read_timeout 3600s;
}

location /api/v1/ws {
    proxy_pass http://localhost:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_buffering off;
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
}
```

## Troubleshooting

### Connection Drops

**Symptom:** SSE connections close unexpectedly

**Solutions:**
- Increase proxy timeouts to 1 hour (3600s)
- Implement client-side reconnection (EventSource does this automatically)
- Monitor keep-alive messages (every 15s)
- Check `/health` for SSE metrics (active connections, disconnect rate)

### Missing Events

**Symptom:** Some events not received

**Causes & Solutions:**
- **Slow consumer:** Check for `events.lagged` frames. Add `?types=` filters to reduce volume.
- **Memory scope:** Events from other memories are filtered. Use admin view (no `?memory=`) to see all.
- **Coalescing:** Low-priority events may be deduplicated. Set `SSE_COALESCE_WINDOW_MS=0` to disable.
- **Replay gap:** If `Last-Event-ID` is expired, client receives `resync_required`. Perform full REST refresh.

### Webhook Delivery Failures

**Symptom:** Webhooks not receiving events

**Solutions:**
1. Check webhook is active: `GET /api/v1/webhooks/:id`
2. Review delivery logs: `GET /api/v1/webhooks/:id/deliveries?limit=50`
3. Test webhook endpoint: `POST /api/v1/webhooks/:id/test`
4. Verify endpoint responds within 10 seconds

## See Also

- [API Reference](api.md) - Complete REST API documentation
- [Multi-Memory Architecture](multi-memory.md) - Memory scoping and isolation
- [Authentication](authentication.md) - OAuth2 and API key setup
- [Architecture](architecture.md) - System design and component overview
- [Operations](operations.md) - Deployment and monitoring

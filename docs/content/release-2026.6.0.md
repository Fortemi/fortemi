# Fortémi v2026.6.0 Release

*Released: 2026-06-12*

Streaming chat lands. Assistant responses now arrive token-by-token over Server-Sent Events, with the metrics needed to operate the stream and a completed incoming-webhook receiver surface. This release is primarily a **server contract for the HotM client** — the streaming endpoint, its event shapes, and its backpressure semantics are stable and ready to build against.

> **For the HotM team:** the authoritative API surface is below, and the implementing commit is `db547a1`. Validate the server with this document + the git history, then expand the client per the integration guide.

## What's New

### Streaming Chat — `POST /api/v1/chat/stream`

A streaming counterpart to `POST /api/v1/chat`. Same request body, but the response is an SSE token stream instead of one blocking JSON object — the basis for a live-typing chat experience.

**Request** (identical contract to `/api/v1/chat`):

```json
{
  "input": "Summarize my notes on vector search.",
  "model": "qwen3:8b",                       // optional; server default if omitted
  "context": {
    "conversation_history": [                // optional; full multi-turn history
      { "role": "user", "content": "Hi" },
      { "role": "assistant", "content": "Hello!" }
    ]
  }
}
```

**Response** — `Content-Type: text/event-stream`, three event types:

| Event | Data | Meaning |
|-------|------|---------|
| `delta` | `{"content": "<chunk>"}` | One generated content chunk. Concatenate `content` across `delta` events to rebuild the message. |
| `done`  | `{"finish_reason": "stop", "model": "<slug>"}` | Terminal success. No events follow. |
| `error` | `{"type":"https://fortemi.com/problems/provider-failure","title":"Provider Failure","status":502,"detail":"<message>"}` | Terminal failure. No events follow. |

A keep-alive comment is emitted every 15s to hold idle connections open.

**Capacity behavior.** The endpoint holds an *owned* GPU permit for the full stream lifetime and returns **503** (with `retry_after`) immediately when no permit is free — streaming chat never starves background jobs. Pre-stream validation errors (empty input, provider unreachable, model not installed) return a normal JSON error response *before* the stream opens, so a non-200 status is always a plain JSON body, never an SSE stream.

### Streaming Observability — `GET /api/v1/health/streaming`

A new `"chat"` block joins `sse` and `rtp`:

```json
{
  "status": "healthy",
  "chat": {
    "chat_stream_started_total":            { "type": "counter", "value": 0 },
    "chat_stream_completed_total":          { "type": "counter", "value": 0 },
    "chat_stream_errored_total":            { "type": "counter", "value": 0 },
    "chat_stream_client_disconnect_total":  { "type": "counter", "value": 0 },
    "chat_stream_tokens_total":             { "type": "counter", "value": 0 },
    "chat_stream_dropped_tokens_total":     { "type": "counter", "value": 0 }
  }
}
```

`chat_stream_dropped_tokens_total` is the signal to watch: it increments when generated tokens cannot be delivered because a client disconnected mid-stream or stopped draining the buffer (see *Backpressure* below).

### Incoming Webhook Receiver — Completed Surface

- **Generic event capture.** Every accepted `POST /api/v1/webhooks/incoming/{slug}` now records a durable `incoming_webhook.received` event in the shared `event_outbox` (for all providers, not just Twilio), ready for downstream fan-out.
- **Receiver deletion.** `DELETE /api/v1/webhooks/incoming/{slug}` → **204** on success, **404** for an unknown slug. The incoming-webhook receiver now supports full create / list / get / **delete**.

## Integrating with HotM

### Consuming the stream

`/api/v1/chat/stream` is initiated with **POST** (it carries the request body), so the browser `EventSource` API — which is GET-only — cannot be used. Consume it with `fetch()` + a `ReadableStream` reader and a small SSE frame parser:

```ts
async function streamChat(
  baseUrl: string,
  token: string,
  body: unknown,
  onDelta: (text: string) => void,
  onDone: (info: { model: string }) => void,
  onError: (err: { error: string; code: string }) => void,
  signal?: AbortSignal,
) {
  const res = await fetch(`${baseUrl}/api/v1/chat/stream`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
      Accept: "text/event-stream",
    },
    body: JSON.stringify(body),
    signal,
  });

  // Non-200 is always a plain JSON error, not a stream.
  if (!res.ok || !res.body) {
    const detail = await res.json().catch(() => ({}));
    onError({ error: detail.error ?? `HTTP ${res.status}`, code: "REQUEST_FAILED" });
    return;
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });

    // SSE frames are separated by a blank line.
    let sep: number;
    while ((sep = buf.indexOf("\n\n")) !== -1) {
      const frame = buf.slice(0, sep);
      buf = buf.slice(sep + 2);

      let event = "message";
      let data = "";
      for (const line of frame.split("\n")) {
        if (line.startsWith("event:")) event = line.slice(6).trim();
        else if (line.startsWith("data:")) data += line.slice(5).trim();
        // lines starting with ":" are keep-alive comments — ignore.
      }
      if (!data) continue;

      const payload = JSON.parse(data);
      if (event === "delta") onDelta(payload.content);
      else if (event === "done") return onDone({ model: payload.model });
      else if (event === "error") return onError(payload);
    }
  }
}
```

### Client contract notes

- **Rebuild the message** by concatenating `delta.content` in arrival order. The full assistant turn is the concatenation up to the `done` event.
- **Always treat `done` and `error` as terminal.** Nothing follows either.
- **Model selection passes through** exactly as in `/api/v1/chat`: omit `model` for the server default, or pass an installed slug (validated server-side; an invalid slug returns a pre-stream 400). Use `GET /api/v1/chat/models` to populate a model picker.
- **Capacity:** a `503` before the stream opens means all GPU permits are busy — honor `retry_after` and retry, do not treat it as a hard failure.
- **Abort cleanly:** dropping the connection (e.g. `AbortController`) releases the server's GPU permit. The server counts the undelivered remainder in `chat_stream_dropped_tokens_total` — expected and harmless on user-initiated cancel.

### Backpressure

The server buffers up to 256 events per stream and gives each send a window (`CHAT_STREAM_SEND_TIMEOUT_SECS`, default 30s). A client that stalls past the window has its remaining tokens **shed** (counted as dropped) rather than pinning the GPU. Under normal UI pacing this never triggers; it exists to protect the server from a wedged client.

## Breaking Changes

None. `POST /api/v1/chat` is unchanged; `/chat/stream` is additive. The `/api/v1/health/streaming` response gains a `chat` key (additive).

## Completed / Fixed

- Incoming-webhook receivers now durably capture every accepted delivery to the event outbox, not only Twilio voice calls (#818).
- Incoming-webhook receiver registrations can be deleted (#819), completing the receiver CRUD surface; HMAC verification on the receiver path confirmed complete (#820).

## Upgrade Notes

```bash
# Pull the latest images and restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

No configuration changes are required. Optional tuning:

- `CHAT_STREAM_SEND_TIMEOUT_SECS` (default `30`) — per-chunk send window before a stalled client's tokens are shed.

## Validation Checklist (HotM team)

- [ ] `POST /api/v1/chat/stream` with a minimal `{ "input": "..." }` yields `delta` events then a single `done`.
- [ ] Concatenated `delta.content` matches the non-streamed `/api/v1/chat` response for the same prompt/model.
- [ ] Invalid model slug returns a pre-stream **400** JSON body (not an SSE stream).
- [ ] At capacity, the endpoint returns **503** with `retry_after` before opening a stream.
- [ ] `GET /api/v1/health/streaming` exposes the `chat` counter block; `chat_stream_completed_total` increments after a clean run.
- [ ] Client cancel (AbortController) ends the stream and bumps `chat_stream_client_disconnect_total`.

## Not Yet Included

- `Last-Event-ID` resumption for interrupted streams (#815) — planned next Phase A increment.
- HotM-side stream consumer — moved to HotM#242; this release is the server side it builds against.

## Full Changelog

See [CHANGELOG.md](../../CHANGELOG.md) for the complete list of changes. The v2026.6.0 entry covers this release; the implementing commit is `db547a1`.

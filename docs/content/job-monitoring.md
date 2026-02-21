# Job Monitoring Guide

Fortemi processes notes through a multi-stage NLP pipeline in the background. This guide shows how to monitor job progress using SSE (real-time), REST (polling), and MCP (AI agents).

## Job Lifecycle

Every job follows a linear state machine:

```
pending → running → completed
                  → failed
```

Each transition emits a corresponding SSE event:

| State Transition | SSE Event | Description |
|-----------------|-----------|-------------|
| Created | `job.queued` | Job added to queue |
| Claimed by worker | `job.started` | Worker began processing |
| Progress update | `job.progress` | Intermediate status (0–100%) |
| Finished | `job.completed` | Job succeeded |
| Error | `job.failed` | Job failed with error message |

## The Processing Pipeline

When a note is created or updated, Fortemi queues a cascade of jobs. Each phase completes before the next begins:

```
Note Create/Update
│
├─ Phase 1 (parallel, queued by API)
│   ├─ ConceptTagging (tier-0 GLiNER → tier-1 fast → tier-2 standard)
│   ├─ TitleGeneration (tier-1 fast → tier-2 standard)
│   ├─ ReferenceExtraction (tier-0 GLiNER → tier-1 fast → tier-2 standard)
│   ├─ MetadataExtraction (tier-1 fast → tier-2 standard)
│   └─ DocumentTypeInference
│
├─ Phase 2 (queued by ConceptTagging handler)
│   └─ RelatedConceptInference (tier-1 fast → tier-2 standard)
│
└─ Phase 3 (queued by RelatedConceptInference handler)
    ├─ Embedding
    └─ Linking → GraphMaintenance
```

For attachment uploads, an `Extraction` job runs first to extract text/metadata from the file, then queues the same Phase 1–3 pipeline for the parent note.

```
Attachment Upload
│
├─ Extraction (adapter-specific: PDF, image, audio, video, 3D)
│   └─ On success, queues Phase 1 jobs for the parent note:
│       ├─ Embedding
│       ├─ Linking
│       ├─ ConceptTagging
│       └─ TitleGeneration
│
└─ ExifExtraction (parallel, for images)
```

**Tier escalation:** NLP handlers use a tiered cost model. If a fast model produces insufficient results, the handler queues a new job at the next tier. Each escalation emits a `job.queued` event.

## Monitoring via SSE

SSE is the primary monitoring method. Connect once and receive events in real time.

### Basic: All Job Events

```bash
curl -N "http://localhost:3000/api/v1/events?types=job"
```

This streams all `job.*` events. Each line is a JSON `EventEnvelope`:

```
data: {"event_id":"...","event_type":"job.queued","payload":{"JobQueued":{"job_id":"...","job_type":"Embedding","note_id":"..."}}}

data: {"event_id":"...","event_type":"job.started","payload":{"JobStarted":{"job_id":"...","job_type":"Embedding","note_id":"..."}}}

data: {"event_id":"...","event_type":"job.progress","payload":{"JobProgress":{"job_id":"...","progress":50,"message":"Processing...","note_id":"..."}}}

data: {"event_id":"...","event_type":"job.completed","payload":{"JobCompleted":{"job_id":"...","job_type":"Embedding","note_id":"...","duration_ms":1234}}}
```

### Filtered: Jobs for a Specific Note

```bash
curl -N "http://localhost:3000/api/v1/events?types=job&entity_id=NOTE_UUID"
```

This filters to only events related to a specific note, which is the most useful pattern for tracking a single note's pipeline progress.

### JavaScript: EventSource with Job Tracking

```javascript
const noteId = 'YOUR_NOTE_UUID';
const url = `http://localhost:3000/api/v1/events?types=job&entity_id=${noteId}`;
const es = new EventSource(url);

const jobs = new Map(); // job_id → { type, status, progress }

es.onmessage = (event) => {
  const envelope = JSON.parse(event.data);
  const type = envelope.event_type;
  const payload = envelope.payload[Object.keys(envelope.payload)[0]];

  switch (type) {
    case 'job.queued':
      jobs.set(payload.job_id, {
        type: payload.job_type, status: 'queued', progress: 0
      });
      break;
    case 'job.started':
      if (jobs.has(payload.job_id)) {
        jobs.get(payload.job_id).status = 'running';
      }
      break;
    case 'job.progress':
      if (jobs.has(payload.job_id)) {
        const job = jobs.get(payload.job_id);
        job.progress = payload.progress;
        job.message = payload.message;
      }
      break;
    case 'job.completed':
      if (jobs.has(payload.job_id)) {
        jobs.get(payload.job_id).status = 'completed';
        jobs.get(payload.job_id).progress = 100;
      }
      checkPipelineComplete();
      break;
    case 'job.failed':
      if (jobs.has(payload.job_id)) {
        jobs.get(payload.job_id).status = 'failed';
      }
      break;
  }

  console.log([...jobs.values()]);
};

function checkPipelineComplete() {
  const allDone = [...jobs.values()].every(
    j => j.status === 'completed' || j.status === 'failed'
  );
  if (allDone && jobs.size > 0) {
    console.log('Pipeline complete for note', noteId);
    es.close();
  }
}
```

### Python: sseclient

```python
import json
import sseclient  # pip install sseclient-py
import requests

note_id = 'YOUR_NOTE_UUID'
url = f'http://localhost:3000/api/v1/events?types=job&entity_id={note_id}'

response = requests.get(url, stream=True)
client = sseclient.SSEClient(response)

for event in client.events():
    envelope = json.loads(event.data)
    event_type = envelope['event_type']
    payload_key = list(envelope['payload'].keys())[0]
    payload = envelope['payload'][payload_key]

    if event_type == 'job.progress':
        print(f"  [{payload.get('job_id', '')[:8]}] {payload.get('progress', 0)}% - {payload.get('message', '')}")
    elif event_type == 'job.completed':
        print(f"  [{payload.get('job_id', '')[:8]}] completed in {payload.get('duration_ms', '?')}ms")
    elif event_type == 'job.failed':
        print(f"  [{payload.get('job_id', '')[:8]}] FAILED: {payload.get('error', 'unknown')}")
    else:
        print(f"  {event_type}: {payload.get('job_type', '?')}")
```

## Monitoring via REST API

Use REST endpoints for polling or when SSE is unavailable.

### Active Jobs for a Note

```bash
curl "http://localhost:3000/api/v1/jobs?note_id=NOTE_UUID&status=running"
```

Returns a list of currently running jobs for the note, with progress:

```json
[
  {
    "id": "...",
    "note_id": "...",
    "job_type": "Embedding",
    "status": "running",
    "progress_percent": 50,
    "progress_message": "Generating embeddings...",
    "created_at": "2026-02-21T10:00:00Z",
    "started_at": "2026-02-21T10:00:01Z"
  }
]
```

### Single Job Detail

```bash
curl "http://localhost:3000/api/v1/jobs/JOB_UUID"
```

### Queue Statistics

```bash
curl "http://localhost:3000/api/v1/jobs/stats"
```

Returns overall queue health:

```json
{
  "total": 42,
  "pending": 5,
  "processing": 3,
  "completed": 30,
  "failed": 4
}
```

### Pending Jobs for a Note

To check if a note's pipeline is still running:

```bash
curl "http://localhost:3000/api/v1/jobs?note_id=NOTE_UUID&status=pending,running" | jq length
```

A count of `0` means all jobs are finished.

## Monitoring via MCP

For AI agents using Fortemi's MCP server, the `manage_jobs` tool provides job monitoring:

```
manage_jobs action=list note_id=NOTE_UUID status=running
manage_jobs action=get job_id=JOB_UUID
manage_jobs action=stats
manage_jobs action=extraction_stats
```

The `extraction_stats` action returns per-strategy success rates and average durations, useful for diagnosing extraction pipeline health.

## Progress Percentages

Job handlers report progress at different granularities:

### Extraction Jobs

| Progress | Stage |
|----------|-------|
| 5% | Resolving attachment and strategy |
| 10% | Starting extraction adapter |
| 20–80% | Adapter-specific processing (chunked updates for large files) |
| 85% | Content persisted to note |
| 95% | Downstream NLP jobs queued |
| 100% | Done |

### NLP Pipeline Jobs (ConceptTagging, TitleGeneration, etc.)

| Progress | Stage |
|----------|-------|
| 10% | Loading note content |
| 20–30% | Preparing prompt / running NER |
| 50% | AI model processing |
| 80–90% | Persisting results |
| 95% | Queuing downstream jobs |
| 100% | Done |

### Embedding and Linking

| Progress | Stage |
|----------|-------|
| 10% | Loading note content |
| 50% | Computing embeddings / finding links |
| 90% | Persisting results |
| 100% | Done |

## Backpressure and Reconnection

### Event Coalescing

`job.progress` events are coalesced with a 500ms window (configurable via `SSE_COALESCE_WINDOW_MS`). If a job reports progress faster than this, intermediate values are skipped. The latest progress value is always delivered.

### Reconnection with Last-Event-ID

SSE connections can be resumed after disconnection:

```bash
curl -N -H "Last-Event-ID: 019507a3-1234-7000-8000-abcdef012345" \
  "http://localhost:3000/api/v1/events?types=job"
```

The server replays events from the replay buffer (1024 events, configurable via `SSE_REPLAY_BUFFER_SIZE`). If the requested ID has expired, the server sends a `resync_required` event — the client should perform a full REST refresh.

### Handling Lag

If the EventBus overflows (client too slow), the server sends:

```json
{"event_type": "events.lagged", "payload": {"missed": 5}}
```

Mitigations:
- Use `?types=job` filter to reduce event volume
- Use `?entity_id=NOTE_UUID` to scope to a single note
- Increase `EVENT_BUS_CAPACITY` for high-throughput deployments

## Building a Job Dashboard

Combine SSE for real-time updates with REST for initial state:

1. **On page load:** `GET /api/v1/jobs?status=pending,running` to populate the current job list
2. **Connect SSE:** `GET /api/v1/events?types=job` for live updates
3. **On `job.queued`:** Add the job to the UI
4. **On `job.progress`:** Update the progress bar
5. **On `job.completed` / `job.failed`:** Move the job to the finished section
6. **On `resync_required`:** Re-fetch from REST and reconnect SSE

### Queue Status Heartbeat

The server emits `queue.status` every 5 seconds when SSE clients are connected:

```json
{
  "event_type": "queue.status",
  "payload": {
    "QueueStatus": {
      "total_jobs": 42,
      "running": 3,
      "pending": 5
    }
  }
}
```

Use this as a dashboard-level health indicator.

## Troubleshooting

### Missing `job.queued` Events

All jobs — both user-initiated and handler-initiated downstream jobs — emit `job.queued` events. If you don't see them:

- Verify the SSE filter includes job events: `?types=job`
- Check you connected before the job was queued (events are not replayed indefinitely)
- Use `Last-Event-ID` for reconnection to avoid gaps

### Jobs Stuck in "pending"

Possible causes:

1. **Worker disabled:** Check `JOB_WORKER_ENABLED` is not `false`
2. **Worker paused:** Check `GET /api/v1/jobs/pause` for global or per-archive pause state
3. **Model unavailable:** Tier-1/tier-2 jobs need Ollama models loaded. Check `GET /health` for model availability
4. **Concurrency limit:** Default `JOB_MAX_CONCURRENT=4`. Increase for faster throughput.

### Jobs Failing Repeatedly

Check the job error message:

```bash
curl "http://localhost:3000/api/v1/jobs?status=failed&limit=10" | jq '.[].error_message'
```

Common causes:
- **Model timeout:** Increase `JOB_TIMEOUT_SECS` (default: 300s)
- **Missing Ollama model:** Run `ollama pull <model>` on the host
- **Database connection issues:** Check PostgreSQL connectivity

### No Progress Events for a Job

Some handlers don't emit granular progress. Extraction jobs have the most detailed progress reporting. Simple handlers (Embedding, Linking) may jump from 0% to 100%.

## See Also

- [Real-Time Events](real-time-events.md) — Full SSE/WebSocket/Webhook documentation
- [Extraction Pipeline Design](extraction-pipeline-design.md) — Architecture of the extraction system
- [Operations](operations.md) — Deployment and monitoring

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
│   ├─ On success:
│   │   ├─ MP4 faststart optimization (video only)
│   │   ├─ Thumbnail persisted as derived attachment (video/audio)
│   │   ├─ Transcript files persisted — VTT, SRT, TXT (audio/video)
│   │   ├─ SpeakerDiarization queued (audio/video, when DIARIZATION_BASE_URL set)
│   │   └─ Queues Phase 1 jobs for the parent note:
│   │       ├─ Embedding
│   │       ├─ Linking
│   │       ├─ ConceptTagging
│   │       └─ TitleGeneration
│
└─ ExifExtraction (parallel, for images)
```

Speaker diarization produces a speaker configuration block in the note content. When a user edits speaker names and saves, a `SpeakerRelabel` job is queued:

```
User edits speaker config block in note
│
└─ SpeakerRelabel
    ├─ Reads speaker map from note content (or API payload)
    ├─ Applies name mapping to transcript segments
    ├─ Updates attachment metadata
    └─ Re-renders caption files (VTT, SRT, TXT)
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
| 82% | Optimizing video for streaming (MP4 faststart, video only) |
| 83% | Persisting thumbnail as derived attachment (video/audio only) |
| 84% | Persisting transcript files — VTT, SRT, plain text (audio/video only) |
| 85% | Content persisted to note |
| 95% | Downstream NLP jobs queued |
| 100% | Done |

### Speaker Diarization Jobs

Queued automatically after audio/video extraction when `DIARIZATION_BASE_URL` is set.

| Progress | Stage |
|----------|-------|
| 5% | Loading attachment metadata |
| 10% | Retrieving audio file |
| 20% | Running speaker diarization (pyannote) |
| 60% | Aligning speakers with transcript segments |
| 70% | Updating attachment metadata with speaker labels |
| 80% | Re-rendering caption files (VTT, SRT, TXT) with speaker labels |
| 95% | Diarization complete |
| 100% | Done |

### Speaker Relabel Jobs

Triggered when a user edits the speaker configuration block in a note, or via the API with a `speaker_map` payload.

| Progress | Stage |
|----------|-------|
| 10% | Loading speaker config |
| 20% | Loading attachment metadata |
| 40% | Applying speaker labels |
| 50% | Updating attachment metadata |
| 70% | Re-rendering caption files (VTT, SRT, TXT) |
| 95% | Relabel complete |
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

## Tracking Multi-Chunk Long-Running Jobs

Some jobs process content in multiple chunks — for example, embedding large notes with many semantic sections, or extraction jobs that must process video keyframes sequentially. These jobs emit progress events at each stage boundary. Here's how to build robust monitoring for them.

### Understanding Chunked Progress

Multi-stage jobs report progress at these granularities:

| Job Type | Chunk Behavior | Progress Events |
|----------|---------------|-----------------|
| **Extraction** | 15+ stages (resolve → extract → persist → queue downstream) | Every 5–10% |
| **ConceptTagging** | Tiered: GLiNER → fast LLM → standard LLM, with potential escalation | Every 10–20% |
| **ReferenceExtraction** | GLiNER entities + LLM extraction + concept resolution | Every 10–20% |
| **EmbeddingHandler** | Chunk → embed → store (large notes have many chunks) | 10%, 30%, 50%, 70%, 100% |
| **GraphMaintenance** | 4-step pipeline (normalize → SNN → PFNET → diagnostics) | 5%, 20%, 30%, 55%, 80%, 100% |
| **AiRevision** | Fetch → generate → save → queue contextual | Every 10–20% |
| **ExifExtraction** | 10+ stages (resolve → download → parse → provenance → persist) | Every 5–10% |

### Complete Pipeline State Machine

When monitoring a full note processing pipeline, you need to track cascading jobs across phases. Here's a complete state machine implementation:

```javascript
class PipelineTracker {
  constructor(noteId, baseUrl) {
    this.noteId = noteId;
    this.jobs = new Map();      // job_id → { type, status, progress, message, startedAt }
    this.phases = {
      phase1: new Set(), // ConceptTagging, TitleGeneration, ReferenceExtraction, MetadataExtraction, DocumentTypeInference
      phase2: new Set(), // RelatedConceptInference
      phase3: new Set(), // Embedding, Linking
    };
    this.callbacks = { onProgress: null, onPhaseComplete: null, onPipelineComplete: null, onError: null };

    // Classify job types into phases
    this.PHASE_MAP = {
      ConceptTagging: 'phase1', TitleGeneration: 'phase1',
      ReferenceExtraction: 'phase1', MetadataExtraction: 'phase1',
      DocumentTypeInference: 'phase1',
      RelatedConceptInference: 'phase2',
      Embedding: 'phase3', Linking: 'phase3',
    };

    this.url = `${baseUrl}/api/v1/events?types=job&entity_id=${noteId}`;
    this.es = null;
  }

  start(token) {
    const url = new URL(this.url);
    if (token) url.searchParams.set('token', token);

    this.es = new EventSource(url);

    // Use the generic onmessage handler since all events come through data:
    this.es.onmessage = (event) => {
      const envelope = JSON.parse(event.data);
      this._handleEvent(envelope);
    };

    // Also listen to specific named events (SSE uses event: field)
    for (const type of ['job.queued', 'job.started', 'job.progress', 'job.completed', 'job.failed']) {
      this.es.addEventListener(type, (event) => {
        const envelope = JSON.parse(event.data);
        this._handleEvent(envelope);
      });
    }

    this.es.addEventListener('events.lagged', (event) => {
      const data = JSON.parse(event.data);
      console.warn(`Events lagged: ${data.dropped_count} dropped. Refreshing state...`);
      this._refreshFromRest(token);
    });

    this.es.addEventListener('resync_required', () => {
      console.warn('Resync required. Refreshing state...');
      this._refreshFromRest(token);
    });

    this.es.onerror = () => {
      // EventSource auto-reconnects with Last-Event-ID
      console.warn('SSE connection lost, auto-reconnecting...');
    };

    return this;
  }

  _handleEvent(envelope) {
    const eventType = envelope.event_type;
    // Extract payload — it's a tagged union: { "type": "JobQueued", "job_id": "...", ... }
    const payload = envelope.payload;

    switch (eventType) {
      case 'job.queued': {
        const jobId = payload.job_id;
        const jobType = payload.job_type;
        this.jobs.set(jobId, {
          type: jobType, status: 'queued', progress: 0,
          message: null, startedAt: null
        });
        const phase = this.PHASE_MAP[jobType];
        if (phase) this.phases[phase].add(jobId);
        break;
      }
      case 'job.started': {
        const job = this.jobs.get(payload.job_id);
        if (job) {
          job.status = 'running';
          job.startedAt = new Date();
        }
        break;
      }
      case 'job.progress': {
        const job = this.jobs.get(payload.job_id);
        if (job) {
          job.progress = payload.progress;
          job.message = payload.message || job.message;
        }
        this.callbacks.onProgress?.(this.getStatus());
        break;
      }
      case 'job.completed': {
        const job = this.jobs.get(payload.job_id);
        if (job) {
          job.status = 'completed';
          job.progress = 100;
          job.durationMs = payload.duration_ms;
        }
        this._checkPhaseCompletion();
        this._checkPipelineCompletion();
        break;
      }
      case 'job.failed': {
        const job = this.jobs.get(payload.job_id);
        if (job) {
          job.status = 'failed';
          job.error = payload.error;
        }
        this.callbacks.onError?.(payload.job_id, payload.error);
        this._checkPipelineCompletion();
        break;
      }
    }
  }

  _checkPhaseCompletion() {
    for (const [phase, jobIds] of Object.entries(this.phases)) {
      if (jobIds.size === 0) continue;
      const allDone = [...jobIds].every(id => {
        const job = this.jobs.get(id);
        return job && (job.status === 'completed' || job.status === 'failed');
      });
      if (allDone) {
        this.callbacks.onPhaseComplete?.(phase, this.getPhaseStatus(phase));
      }
    }
  }

  _checkPipelineCompletion() {
    if (this.jobs.size === 0) return;
    const allDone = [...this.jobs.values()].every(
      j => j.status === 'completed' || j.status === 'failed'
    );
    if (allDone) {
      this.callbacks.onPipelineComplete?.(this.getStatus());
      this.es?.close();
    }
  }

  async _refreshFromRest(token) {
    const headers = token ? { Authorization: `Bearer ${token}` } : {};
    const res = await fetch(
      `${this.url.split('/api/v1/events')[0]}/api/v1/jobs?note_id=${this.noteId}&status=pending,running`,
      { headers }
    );
    const jobs = await res.json();
    for (const j of jobs) {
      this.jobs.set(j.id, {
        type: j.job_type, status: j.status,
        progress: j.progress_percent || 0,
        message: j.progress_message, startedAt: j.started_at
      });
    }
  }

  getStatus() {
    const jobs = [...this.jobs.values()];
    return {
      total: jobs.length,
      queued: jobs.filter(j => j.status === 'queued').length,
      running: jobs.filter(j => j.status === 'running').length,
      completed: jobs.filter(j => j.status === 'completed').length,
      failed: jobs.filter(j => j.status === 'failed').length,
      overallProgress: jobs.length > 0
        ? Math.round(jobs.reduce((sum, j) => sum + j.progress, 0) / jobs.length)
        : 0,
      jobs: Object.fromEntries(this.jobs),
    };
  }

  getPhaseStatus(phase) {
    const jobIds = this.phases[phase];
    return [...jobIds].map(id => ({ id, ...this.jobs.get(id) }));
  }

  stop() {
    this.es?.close();
  }
}

// Usage:
const tracker = new PipelineTracker('NOTE_UUID', 'http://localhost:3000');
tracker.callbacks.onProgress = (status) => {
  console.log(`Pipeline ${status.overallProgress}% — ${status.running} running, ${status.queued} queued`);
};
tracker.callbacks.onPhaseComplete = (phase, jobs) => {
  console.log(`Phase ${phase} complete:`, jobs.map(j => `${j.type}: ${j.status}`));
};
tracker.callbacks.onPipelineComplete = (status) => {
  console.log(`Pipeline done! ${status.completed}/${status.total} succeeded`);
};
tracker.callbacks.onError = (jobId, error) => {
  console.error(`Job ${jobId} failed: ${error}`);
};
tracker.start(/* 'mm_at_xxx' */);
```

### Tier Escalation Events

NLP handlers use a tiered cost model (GLiNER → fast LLM → standard LLM). When a lower tier produces insufficient results, the handler queues a new job at the next tier. This means:

1. You may see **multiple `job.queued` events** for the same job type and note ID
2. Each tier escalation is a **new job** with its own lifecycle
3. The previous job completes successfully (it produced *some* results, just not enough)

```
job.queued    ConceptTagging (tier-0 GLiNER)
job.started   ConceptTagging
job.progress  ConceptTagging 10% — "Fetching note content..."
job.progress  ConceptTagging 25% — "Mapping GLiNER entities to concepts..."
job.progress  ConceptTagging 95% — "Escalating to higher tier — phase-2 deferred"
job.completed ConceptTagging          ← tier-0 done, found 2 concepts (target: 5)
job.queued    ConceptTagging (tier-1)  ← escalation! new job queued
job.started   ConceptTagging
job.progress  ConceptTagging 30% — "Running fast LLM concept extraction..."
job.completed ConceptTagging          ← tier-1 found enough concepts
```

To track this, use `entity_id` (note ID) filtering — all tier escalations share the same `note_id`.

### Coalescing Behavior

`job.progress` events are classified as `Low` priority and coalesced with a default 500ms window. This means:

- If a job reports progress at 10%, 15%, 20% within 500ms, only the 10% event is delivered
- The next progress event after the window expires delivers the latest value
- Set `SSE_COALESCE_WINDOW_MS=0` to disable coalescing and receive every progress update

For long-running jobs (Extraction, GraphMaintenance), coalescing has minimal impact since progress stages are seconds apart. For fast jobs (DocumentTypeInference), you may only see the start and completion.

### Long-Running Job Patterns

Some jobs can take minutes or longer:

| Job Type | Typical Duration | Long-Running Scenario |
|----------|-----------------|----------------------|
| **Extraction** (video) | 30s–5min | Large video files with scene detection + transcription |
| **Extraction** (audio) | 10s–3min | Long audio files transcribed via Whisper |
| **GraphMaintenance** | 5s–2min | Large graphs with SNN + PFNET computation |
| **ReEmbedAll** | 1min–30min | Bulk re-embedding all notes in an archive |
| **AiRevisionContextual** | 10s–2min | Gathering context from related notes + LLM generation |

For these, monitor the progress message field — it describes the current stage:

```bash
# Watch progress messages for a specific job
curl -N "http://localhost:3000/api/v1/events?types=job.progress&entity_id=JOB_UUID" | \
  while IFS= read -r line; do
    if [[ "$line" == data:* ]]; then
      echo "$line" | sed 's/^data: //' | jq -r '.payload.message // empty'
    fi
  done
```

### Downstream Job Tracking

When a handler queues downstream jobs (e.g., Extraction → Embedding + Linking), those jobs emit `job.queued` events with the same `note_id`. This is how you know the pipeline is extending:

```
Extraction started for note abc-123
  → job.queued Embedding (note: abc-123)     ← handler queued downstream
  → job.queued Linking (note: abc-123)        ← handler queued downstream
  → job.queued ConceptTagging (note: abc-123) ← handler queued downstream
  → job.queued TitleGeneration (note: abc-123)
Extraction completed

Embedding started...
Linking started...
ConceptTagging started...
```

If you're tracking pipeline completion, new `job.queued` events for your `entity_id` mean the pipeline isn't done yet — reset your completion check.

## Event Emission Completeness

The following table shows which job lifecycle events each handler actually emits via `report_progress()`:

| Handler | `job.queued` | `job.started` | `job.progress` | `job.completed` | `job.failed` |
|---------|:-----------:|:-------------:|:--------------:|:---------------:|:------------:|
| EmbeddingHandler | Auto | Auto | 10%, 30%, 50%, 70%, 100% | Auto | Auto |
| LinkingHandler | Auto | Auto | 10%, 20%, 40%, 60%, 100% | Auto | Auto |
| ConceptTaggingHandler | Auto | Auto | 10%, 20-30%, 50%, 60%, 80-95%, 100% | Auto | Auto |
| TitleGenerationHandler | Auto | Auto | 10%, 20%, 80%, 100% | Auto | Auto |
| ReferenceExtractionHandler | Auto | Auto | 10%, 20%, 30-50%, 60%, 100% | Auto | Auto |
| MetadataExtractionHandler | Auto | Auto | 10%, 20%, 60%, 80%, 100% | Auto | Auto |
| DocumentTypeInferenceHandler | Auto | Auto | 10%, 30%, 80%, 100% | Auto | Auto |
| AiRevisionHandler | Auto | Auto | 10%, 40%, 80%, 90%, 95%, 100% | Auto | Auto |
| AiRevisionContextualHandler | Auto | Auto | 10%, 30%, 40%, 60%, 80%, 90%, 100% | Auto | Auto |
| ExtractionHandler | Auto | Auto | 10%, 20%, 80%, 82%, 83%, 84%, 85%, 95%, 100% | Auto | Auto |
| ExifExtractionHandler | Auto | Auto | 5%, 10%, 30%, 50%, 60%, 70%, 80%, 90%, 100% | Auto | Auto |
| GraphMaintenanceHandler | Auto | Auto | 5%, 20%, 30%, 55%, 80%, 100% | Auto | Auto |
| ReEmbedAllHandler | Auto | Auto | 5%, 10%, per-note updates, 100% | Auto | Auto |
| RelatedConceptHandler | Auto | Auto | 10%, 30%, 60%, 70%, 98%, 100% | Auto | Auto |
| PurgeNoteHandler | Auto | Auto | 10%, 30%, 50%, 80%, 100% | Auto | Auto |
| ContextUpdateHandler | Auto | Auto | 20%, 40%, 60%, 80%, 100% | Auto | Auto |
| RefreshEmbeddingSetHandler | Auto | Auto | 10%, 20%, 50%, 100% | Auto | Auto |
| SpeakerDiarizationHandler | Auto | Auto | 5%, 10%, 20%, 60%, 70%, 80%, 95%, 100% | Auto | Auto |
| SpeakerRelabelHandler | Auto | Auto | 10%, 20%, 40%, 50%, 70%, 95%, 100% | Auto | Auto |

"Auto" means the worker framework emits these events automatically for every job — handlers don't need to emit them explicitly.

## See Also

- [Real-Time Events](real-time-events.md) — Full SSE/WebSocket/Webhook documentation
- [Extraction Pipeline Design](extraction-pipeline-design.md) — Architecture of the extraction system
- [Operations](operations.md) — Deployment and monitoring

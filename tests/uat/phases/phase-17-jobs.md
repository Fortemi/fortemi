# Phase 17: Jobs & Queue

**Duration**: ~8 minutes
**Tools Tested**: 7 tools
**Dependencies**: Phase 0 (preflight), Phase 2 (CRUD)

---

## Overview

The job queue manages background processing tasks like AI revision, embedding generation, semantic linking, and title generation. This phase tests job creation, monitoring, and queue statistics.

---

## Job Types

| Job Type | Priority | Description |
|----------|----------|-------------|
| `ai_revision` | 8 | AI-enhanced content revision |
| `embedding` | 5 | Generate vector embeddings |
| `linking` | 3 | Semantic link discovery |
| `title_generation` | 2 | Auto-generate note title |
| `context_update` | 1 | Update context for related notes |
| `concept_tagging` | 4 | SKOS auto-tagging |
| `re_embed_all` | 1 | Batch re-embedding |

---

## Test Cases

### Queue Statistics

#### JOB-001: Get Queue Stats

**Tool**: `get_queue_stats`

```javascript
get_queue_stats()
```

**Expected**:
```json
{
  "pending": <n>,
  "processing": <n>,
  "completed_last_hour": <n>,
  "failed_last_hour": <n>,
  "total": <n>,
  "by_type": {
    "ai_revision": <n>,
    "embedding": <n>,
    "linking": <n>,
    ...
  }
}
```

**Pass Criteria**: Valid statistics returned

**Store**: `initial_pending_count`

---

### Job Listing

#### JOB-002: List Jobs (All)

**Tool**: `list_jobs`

```javascript
list_jobs({
  limit: 20
})
```

**Expected**:
```json
{
  "jobs": [
    {
      "id": "<uuid>",
      "job_type": "embedding",
      "note_id": "<uuid>",
      "status": "completed",
      "priority": 5,
      "created_at": "<timestamp>",
      "started_at": "<timestamp>",
      "completed_at": "<timestamp>"
    },
    ...
  ],
  "total": <n>
}
```

**Pass Criteria**: Returns job list (may be empty)

---

#### JOB-003: List Jobs by Status

**Tool**: `list_jobs`

```javascript
list_jobs({
  status: "completed",
  limit: 10
})
```

**Expected**: Only completed jobs returned

---

#### JOB-004: List Jobs by Type

**Tool**: `list_jobs`

```javascript
list_jobs({
  job_type: "embedding",
  limit: 10
})
```

**Expected**: Only embedding jobs returned

---

#### JOB-005: List Jobs for Note

**Tool**: `list_jobs`

```javascript
// Use a note from seed data
list_jobs({
  note_id: "<seed_note_id>",
  limit: 10
})
```

**Expected**: Only jobs for that note

---

### Job Creation

#### JOB-006: Create Embedding Job

**Tool**: `create_job`

```javascript
// Create a test note first
const note = create_note({
  content: "# Job Test Note\n\nContent for testing job queue.",
  tags: ["uat/jobs"],
  revision_mode: "none"  // Don't auto-queue jobs
})

// Manually queue embedding job
create_job({
  note_id: note.id,
  job_type: "embedding",
  priority: 5
})
```

**Expected**:
```json
{
  "id": "<uuid>",
  "status": "pending"
}
```

**Store**: `embedding_job_id`, `job_test_note_id`

---

#### JOB-007: Create Linking Job

**Tool**: `create_job`

```javascript
create_job({
  note_id: job_test_note_id,
  job_type: "linking",
  priority: 3
})
```

**Store**: `linking_job_id`

---

#### JOB-008: Create Title Generation Job

**Tool**: `create_job`

```javascript
create_job({
  note_id: job_test_note_id,
  job_type: "title_generation",
  priority: 2
})
```

**Store**: `title_job_id`

---

#### JOB-009: Verify Queue Stats Updated

**Tool**: `get_queue_stats`

```javascript
get_queue_stats()
```

**Expected**: `pending` >= initial_pending_count (may be processed already)

---

#### JOB-010: Create AI Revision Job

**Tool**: `create_job`

```javascript
create_job({
  note_id: job_test_note_id,
  job_type: "ai_revision",
  priority: 8
})
```

**Store**: `revision_job_id`

---

#### JOB-011: Verify High Priority Ordering

**Tool**: `list_jobs`

```javascript
list_jobs({
  status: "pending",
  limit: 10
})
```

**Expected**: Higher priority jobs appear first (ai_revision before linking)

---

### Re-embedding

#### JOB-012: Trigger Re-embed All

**Tool**: `reembed_all`

```javascript
reembed_all({
  force: false  // Only re-embed notes without embeddings
})
```

**Expected**: Batch job queued

---

#### JOB-013: Re-embed Specific Set

**Tool**: `reembed_all`

```javascript
reembed_all({
  embedding_set_slug: "default",
  force: true  // Re-embed all notes in set
})
```

**Expected**: Set-specific re-embedding queued

---

### Job Completion Monitoring

#### JOB-014: Monitor Job Progress

**Tool**: `list_jobs`

```javascript
// Wait briefly, then check
list_jobs({
  note_id: job_test_note_id,
  limit: 10
})
```

**Expected**: Jobs transition from pending → processing → completed

**Pass Criteria**: At least some jobs completed or processing

---

#### JOB-015: Verify Failed Jobs

**Tool**: `list_jobs`

```javascript
list_jobs({
  status: "failed",
  limit: 10
})
```

**Expected**: Failed jobs (if any) have error information

---

### Edge Cases

#### JOB-016: Create Job for Non-Existent Note

**Tool**: `create_job`

```javascript
create_job({
  note_id: "00000000-0000-0000-0000-000000000000",
  job_type: "embedding"
})
```

**Expected**: Error - note not found

**Pass Criteria**: Graceful error handling

---

#### JOB-017: Create Invalid Job Type

**Tool**: `create_job`

```javascript
create_job({
  note_id: job_test_note_id,
  job_type: "invalid_type"
})
```

**Expected**: Error - invalid job type

---

#### JOB-018: Create Duplicate Job

**Tool**: `create_job`

```javascript
// Create same job type for same note
create_job({
  note_id: job_test_note_id,
  job_type: "embedding"
})
```

**Expected**:
- Either succeeds (duplicate allowed)
- Or returns existing job
- Or errors (no duplicates)

**Pass Criteria**: Defined behavior

---

## Cleanup

```javascript
// Delete test note (jobs should be cleaned up)
delete_note({ id: job_test_note_id })

// Verify jobs cleaned up
list_jobs({ note_id: job_test_note_id })  // Should be empty
```

---

## Success Criteria

| Test | Status | Notes |
|------|--------|-------|
| JOB-001 | | Get queue stats |
| JOB-002 | | List all jobs |
| JOB-003 | | List by status |
| JOB-004 | | List by type |
| JOB-005 | | List for note |
| JOB-006 | | Create embedding job |
| JOB-007 | | Create linking job |
| JOB-008 | | Create title job |
| JOB-009 | | Verify stats updated |
| JOB-010 | | Create AI revision job |
| JOB-011 | | Priority ordering |
| JOB-012 | | Re-embed all |
| JOB-013 | | Re-embed specific set |
| JOB-014 | | Monitor progress |
| JOB-015 | | Failed jobs info |
| JOB-016 | | Non-existent note error |
| JOB-017 | | Invalid job type error |
| JOB-018 | | Duplicate job handling |
| JOB-019 | | Get job by ID |
| JOB-020 | | Get pending jobs count |
| JOB-021 | | Reprocess note |
| JOB-022 | | Reprocess note all ops |

**Pass Rate Required**: 95% (21/22)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `get_queue_stats` | JOB-001, JOB-009 |
| `list_jobs` | JOB-002, JOB-003, JOB-004, JOB-005, JOB-011, JOB-014, JOB-015 |
| `create_job` | JOB-006, JOB-007, JOB-008, JOB-010, JOB-016, JOB-017, JOB-018 |
| `reembed_all` | JOB-012, JOB-013 |
| `get_job` | JOB-019 |
| `get_pending_jobs_count` | JOB-020 |
| `reprocess_note` | JOB-021, JOB-022 |

**Coverage**: 7/7 job tools (100%)

---

## Individual Job Operations

#### JOB-019: Get Job by ID

**Tool**: `get_job`

```javascript
get_job({ id: embedding_job_id })
```

**Expected**:
```json
{
  "id": "<uuid>",
  "job_type": "embedding",
  "note_id": "<uuid>",
  "status": "completed",
  "priority": 5,
  "created_at": "<timestamp>",
  "started_at": "<timestamp>",
  "completed_at": "<timestamp>",
  "error": null
}
```

**Pass Criteria**: Returns full job details including timestamps

---

#### JOB-020: Get Pending Jobs Count

**Tool**: `get_pending_jobs_count`

```javascript
get_pending_jobs_count()
```

**Expected**:
```json
{
  "pending": <n>,
  "by_type": {
    "embedding": <n>,
    "linking": <n>,
    "ai_revision": <n>,
    ...
  }
}
```

**Pass Criteria**: Returns quick pending count (faster than full stats)

---

## Note Reprocessing

#### JOB-021: Reprocess Note

**Tool**: `reprocess_note`

```javascript
reprocess_note({
  id: job_test_note_id,
  operations: ["embedding", "linking", "title_generation"]
})
```

**Expected**:
```json
{
  "jobs_created": [
    { "id": "<uuid>", "job_type": "embedding" },
    { "id": "<uuid>", "job_type": "linking" },
    { "id": "<uuid>", "job_type": "title_generation" }
  ]
}
```

**Pass Criteria**: Creates specified jobs for note

---

#### JOB-022: Reprocess Note - All Operations

**Tool**: `reprocess_note`

```javascript
reprocess_note({
  id: job_test_note_id
  // No operations = reprocess all
})
```

**Pass Criteria**: Creates jobs for all applicable operations

---

# UAT Phase 16: Observability

**Duration**: ~10 minutes
**Tools Tested**: 7 tools
**Dependencies**: Phase 0 (preflight), Phase 1 (seed data), Phase 2 (CRUD)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Overview

This phase tests knowledge health monitoring, timeline/activity views, and observability tools for understanding and maintaining the knowledge base.

---

## Knowledge Health

### OBS-001: Get Knowledge Health Overview

**MCP Tool**: `get_knowledge_health`

```javascript
get_knowledge_health()
```

**Expected Response Structure**:
```
{
  "health_score": <integer 0-100>,
  "metrics": {
    "total_notes": <integer >= 0>,
    "orphan_tags": <integer >= 0>,
    "stale_notes": <integer >= 0>,
    "unlinked_notes": <integer >= 0>,
    "missing_embeddings": <integer >= 0>,
    "broken_links": <integer >= 0>
  },
  "recommendations": [<array of objects with type, severity, message fields>]
}
```

**Pass Criteria**: Returns object with `health_score` (integer 0-100), `metrics` object containing all six keys as non-negative integers, and `recommendations` array

---

### OBS-002: Get Orphan Tags

**MCP Tool**: `get_orphan_tags`

```javascript
get_orphan_tags()
```

**Expected Response Structure**:
```
{
  "orphan_tags": [<array of objects, each with tag (string), created_at (timestamp), last_used (timestamp)>],
  "total": <integer >= 0>
}
```

**Pass Criteria**: Returns object with `orphan_tags` array (each entry has `tag` string, `created_at` timestamp, `last_used` timestamp) and `total` integer >= 0

---

### OBS-003: Get Stale Notes

**MCP Tool**: `get_stale_notes`

```javascript
get_stale_notes({
  days: 90,
  limit: 20
})
```

**Expected Response Structure**:
```
{
  "stale_notes": [<array of objects, each with id (UUID), title (string), last_modified (timestamp), days_stale (integer >= days param), tags (string array)>],
  "total": <integer >= 0>
}
```

**Pass Criteria**: Returns object with `stale_notes` array (each entry has valid UUID `id`, non-empty `title`, `last_modified` timestamp, `days_stale` integer >= 90, `tags` array) and `total` integer >= 0

---

### OBS-004: Get Unlinked Notes

**MCP Tool**: `get_unlinked_notes`

```javascript
get_unlinked_notes({
  limit: 20
})
```

**Expected Response Structure**:
```
{
  "unlinked_notes": [<array of objects, each with id (UUID), title (string), created_at (timestamp), tags (string array)>],
  "total": <integer >= 0>
}
```

**Pass Criteria**: Returns object with `unlinked_notes` array (each entry has valid UUID `id`, non-empty `title`, `created_at` timestamp, `tags` array) and `total` integer >= 0

---

### OBS-005: Get Tag Co-occurrence

**MCP Tool**: `get_tag_cooccurrence`

```javascript
get_tag_cooccurrence({
  min_count: 2,
  limit: 20
})
```

**Expected Response Structure**:
```
{
  "cooccurrences": [<array of objects, each with tag_a (string), tag_b (string), count (integer >= min_count), correlation (float 0.0-1.0)>]
}
```

**Pass Criteria**: Returns object with `cooccurrences` array where each entry has `tag_a` (non-empty string), `tag_b` (non-empty string, different from tag_a), `count` (integer >= `min_count` parameter), and `correlation` (number between 0.0 and 1.0)

---

## Timeline & Activity

### OBS-006: Get Notes Timeline

**MCP Tool**: `get_notes_timeline`

```javascript
get_notes_timeline({
  start_date: "2026-01-01",
  end_date: "2026-02-02",
  granularity: "day"
})
```

**Expected Response Structure**:
```
{
  "timeline": [<array of objects, each with date (ISO 8601 date), created (integer >= 0), modified (integer >= 0), deleted (integer >= 0)>],
  "summary": {
    "total_created": <integer >= 0>,
    "total_modified": <integer >= 0>,
    "total_deleted": <integer >= 0>,
    "most_active_day": <ISO 8601 date string or null>
  }
}
```

**Pass Criteria**: Returns object with `timeline` array (each entry has `date` as ISO 8601 date string, `created`/`modified`/`deleted` as non-negative integers) and `summary` object with totals and `most_active_day`

---

### OBS-007: Get Notes Timeline - Weekly Granularity

**MCP Tool**: `get_notes_timeline`

```javascript
get_notes_timeline({
  start_date: "2025-12-01",
  end_date: "2026-02-02",
  granularity: "week"
})
```

**Expected Response Structure**:
```
{
  "timeline": [<array of objects with period: "week", date as ISO 8601 week start>],
  "summary": { ... }
}
```

**Pass Criteria**: Timeline entries have `period: "week"` (not "day"), dates correspond to week boundaries (Mondays), and the number of entries is approximately `(date range in days) / 7` rather than one entry per day

---

### OBS-008: Get Notes Activity

**MCP Tool**: `get_notes_activity`

```javascript
get_notes_activity({
  limit: 20
})
```

**Expected Response Structure**:
```
{
  "recent_activity": [<array of objects, each with note_id (UUID), title (string), action (enum), timestamp (ISO 8601), user (string)>]
}
```

**Pass Criteria**: Returns object with `recent_activity` array where each entry has valid UUID `note_id`, non-empty `title`, `action` (one of "created", "updated", "deleted", "restored", "tagged", "linked"), `timestamp` (ISO 8601), and `user` (non-empty string)

---

### OBS-009: Get Notes Activity - Filtered

**MCP Tool**: `get_notes_activity`

```javascript
get_notes_activity({
  action: "created",
  limit: 10
})
```

**Pass Criteria**: Returns only creation events

---

## Health-Based Recommendations

### OBS-010: Act on Orphan Tags

**MCP Tool**: `get_orphan_tags`

```javascript
// Get orphan tags
const orphans = get_orphan_tags()

// If orphan tags exist, verify they can be cleaned up
if (orphans.orphan_tags.length > 0) {
  // These would normally be cleaned via delete_tag or similar
  console.log("Orphan tags identified for cleanup")
}
```

**Pass Criteria**: Orphan tag workflow demonstrated

---

### OBS-011: Act on Stale Notes

**MCP Tool**: `get_stale_notes`

```javascript
// Get stale notes
const stale = get_stale_notes({ days: 365, limit: 5 })

// If stale notes exist, verify they can be archived or reviewed
if (stale.stale_notes.length > 0) {
  // Could tag with "needs-review" or archive
  console.log("Stale notes identified for review")
}
```

**Pass Criteria**: Stale note workflow demonstrated

---

### OBS-012: Knowledge Health After Operations

**MCP Tool**: `get_knowledge_health`

```javascript
// Perform some operations
create_note({
  content: "# Observability Test\n\nTest note for health tracking.",
  tags: ["uat/observability"]
})

// Check health updated
get_knowledge_health()
```

**Pass Criteria**: Health metrics reflect recent changes

---

## Success Criteria

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| OBS-001 | Knowledge Health Overview | `get_knowledge_health` | |
| OBS-002 | Orphan Tags | `get_orphan_tags` | |
| OBS-003 | Stale Notes | `get_stale_notes` | |
| OBS-004 | Unlinked Notes | `get_unlinked_notes` | |
| OBS-005 | Tag Co-occurrence | `get_tag_cooccurrence` | |
| OBS-006 | Notes Timeline | `get_notes_timeline` | |
| OBS-007 | Timeline Weekly | `get_notes_timeline` | |
| OBS-008 | Notes Activity | `get_notes_activity` | |
| OBS-009 | Activity Filtered | `get_notes_activity` | |
| OBS-010 | Orphan Tag Workflow | `get_orphan_tags` | |
| OBS-011 | Stale Note Workflow | `get_stale_notes` | |
| OBS-012 | Health After Operations | `get_knowledge_health` | |

**Pass Rate Required**: 100% (12/12)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `get_knowledge_health` | OBS-001, OBS-012 |
| `get_orphan_tags` | OBS-002, OBS-010 |
| `get_stale_notes` | OBS-003, OBS-011 |
| `get_unlinked_notes` | OBS-004 |
| `get_tag_cooccurrence` | OBS-005 |
| `get_notes_timeline` | OBS-006, OBS-007 |
| `get_notes_activity` | OBS-008, OBS-009 |

**Coverage**: 7/7 observability tools (100%)

---

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

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

**Expected Response**:
```json
{
  "health_score": 85,
  "metrics": {
    "total_notes": 150,
    "orphan_tags": 3,
    "stale_notes": 12,
    "unlinked_notes": 25,
    "missing_embeddings": 5,
    "broken_links": 0
  },
  "recommendations": [
    {
      "type": "cleanup",
      "severity": "low",
      "message": "3 tags have no associated notes"
    }
  ]
}
```

**Pass Criteria**: Returns health score and metrics

---

### OBS-002: Get Orphan Tags

**MCP Tool**: `get_orphan_tags`

```javascript
get_orphan_tags()
```

**Expected Response**:
```json
{
  "orphan_tags": [
    {
      "tag": "old/unused",
      "created_at": "<timestamp>",
      "last_used": "<timestamp>"
    }
  ],
  "total": 3
}
```

**Pass Criteria**: Returns tags with no associated notes

---

### OBS-003: Get Stale Notes

**MCP Tool**: `get_stale_notes`

```javascript
get_stale_notes({
  days: 90,
  limit: 20
})
```

**Expected Response**:
```json
{
  "stale_notes": [
    {
      "id": "<uuid>",
      "title": "Old Note",
      "last_modified": "<timestamp>",
      "days_stale": 95,
      "tags": ["archive"]
    }
  ],
  "total": 12
}
```

**Pass Criteria**: Returns notes not modified in specified days

---

### OBS-004: Get Unlinked Notes

**MCP Tool**: `get_unlinked_notes`

```javascript
get_unlinked_notes({
  limit: 20
})
```

**Expected Response**:
```json
{
  "unlinked_notes": [
    {
      "id": "<uuid>",
      "title": "Isolated Note",
      "created_at": "<timestamp>",
      "tags": ["standalone"]
    }
  ],
  "total": 25
}
```

**Pass Criteria**: Returns notes with no semantic links

---

### OBS-005: Get Tag Co-occurrence

**MCP Tool**: `get_tag_cooccurrence`

```javascript
get_tag_cooccurrence({
  min_count: 2,
  limit: 20
})
```

**Expected Response**:
```json
{
  "cooccurrences": [
    {
      "tag_a": "machine-learning",
      "tag_b": "python",
      "count": 15,
      "correlation": 0.72
    },
    {
      "tag_a": "api",
      "tag_b": "rest",
      "count": 8,
      "correlation": 0.65
    }
  ]
}
```

**Pass Criteria**: Returns tag pairs that frequently appear together

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

**Expected Response**:
```json
{
  "timeline": [
    {
      "date": "2026-01-15",
      "created": 5,
      "modified": 12,
      "deleted": 0
    },
    {
      "date": "2026-01-16",
      "created": 3,
      "modified": 8,
      "deleted": 1
    }
  ],
  "summary": {
    "total_created": 150,
    "total_modified": 320,
    "total_deleted": 5,
    "most_active_day": "2026-01-20"
  }
}
```

**Pass Criteria**: Returns chronological activity data

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

**Pass Criteria**: Returns weekly aggregated data

---

### OBS-008: Get Notes Activity

**MCP Tool**: `get_notes_activity`

```javascript
get_notes_activity({
  limit: 20
})
```

**Expected Response**:
```json
{
  "recent_activity": [
    {
      "note_id": "<uuid>",
      "title": "Recent Note",
      "action": "created",
      "timestamp": "<timestamp>",
      "user": "api"
    },
    {
      "note_id": "<uuid>",
      "title": "Updated Note",
      "action": "modified",
      "timestamp": "<timestamp>",
      "user": "api"
    }
  ]
}
```

**Pass Criteria**: Returns recent note activity feed

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

**Pass Rate Required**: 90% (11/12)

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

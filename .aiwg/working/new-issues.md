# New Issues to Create

Generated from MCP integration feedback (2026-01-05)

---

## Issue: Add Job Queue Monitoring Tools to MCP

**Labels**: `enhancement`, `mcp`, `priority:high`
**Milestone**: v0.2.0

### Description

The MCP surface exposes `create_job` but lacks tools to monitor job status. Without visibility into the job queue, users cannot:
- Know when embeddings are complete (semantic search becomes available)
- Monitor AI revision progress
- Debug failed jobs

### Acceptance Criteria

- [ ] Add `list_jobs` MCP tool - list jobs with filtering by status, type
- [ ] Add `get_job_status` MCP tool - get detailed status of a specific job
- [ ] Include job progress/completion percentage if available
- [ ] Return meaningful error messages for failed jobs

### Implementation Notes

The API already has `/api/v1/jobs` and `/api/v1/jobs/:id` endpoints. MCP tools just need to expose these.

---

## Issue: Hybrid Search Graceful Degradation

**Labels**: `bug`, `search`, `priority:high`
**Milestone**: v0.1.1

### Description

When `mode=hybrid` is specified but embeddings don't exist for notes (fresh notes, embedding job pending), the search returns empty results instead of falling back to FTS.

### Expected Behavior

Hybrid search should:
1. Attempt semantic search
2. If no embeddings exist, gracefully degrade to FTS-only
3. Optionally indicate in response that semantic results were unavailable

### Acceptance Criteria

- [ ] Hybrid search returns FTS results when embeddings are missing
- [ ] Response includes metadata indicating search mode actually used
- [ ] No errors thrown when embedding table is empty

---

## Issue: Fix Title Field Inconsistency

**Labels**: `bug`, `api`, `priority:medium`
**Milestone**: v0.1.1

### Description

The `title` field is inconsistent between endpoints:
- `GET /api/v1/notes` (list) returns `"title": "MCP Memory Module Test"`
- `GET /api/v1/notes/:id` (single) returns `title: null`

### Expected Behavior

Both endpoints should return the same title value for the same note.

### Acceptance Criteria

- [ ] Single note endpoint returns extracted/generated title
- [ ] Title extraction logic is consistent across endpoints

---

## Issue: Auto-parse Wiki-style Links

**Labels**: `enhancement`, `api`, `priority:medium`
**Milestone**: v0.2.0

### Description

Notes can contain `[[wiki-style]]` links in content, but the `links` array in the response is empty. Links should be auto-detected and populated.

### Acceptance Criteria

- [ ] `[[note-title]]` syntax detected in note content
- [ ] Links resolved to note UUIDs where possible
- [ ] Unresolved links returned with target text for future resolution
- [ ] Links array populated on note creation/update

### Implementation Notes

Consider creating a job type `linking` that runs after note creation to populate links asynchronously.

---

## Issue: Add Bulk Operations to MCP

**Labels**: `enhancement`, `mcp`, `priority:low`
**Milestone**: v0.2.0

### Description

MCP surface lacks bulk operations for common workflows:
- Tag multiple notes at once
- Batch delete/archive
- Bulk status updates

### Acceptance Criteria

- [ ] Add `bulk_set_tags` tool - apply tags to multiple note IDs
- [ ] Add `bulk_update_status` tool - archive/star/delete multiple notes
- [ ] Include summary of operations performed in response

---

## Issue: Add Collection Management to MCP

**Labels**: `enhancement`, `mcp`, `priority:low`
**Milestone**: v0.2.0

### Description

The database schema includes a `collection` table but no MCP tools expose collection management. Collections would allow organizing notes into groups.

### Acceptance Criteria

- [ ] Add `list_collections` tool
- [ ] Add `create_collection` tool
- [ ] Add `add_to_collection` tool
- [ ] Add `remove_from_collection` tool
- [ ] Add collection filter to `list_notes` and `search_notes`

---

## Issue: Add Search Within Tags Filter

**Labels**: `enhancement`, `search`, `priority:low`
**Milestone**: v0.2.0

### Description

The search endpoint should support filtering results by tags, enabling queries like "search for 'authentication' within notes tagged 'security'".

### Acceptance Criteria

- [ ] Add `tags` parameter to search endpoint
- [ ] Support multiple tags (AND logic)
- [ ] Expose in MCP `search_notes` tool

---

## Summary

| Issue # | Title | Priority | Milestone | Labels |
|---------|-------|----------|-----------|--------|
| #46 | Job Queue Monitoring Tools | HIGH | v0.2.0 | feature |
| #47 | Hybrid Search Degradation | HIGH | - | search |
| #48 | Title Field Inconsistency | MEDIUM | - | api |
| #49 | Wiki-style Link Parsing | MEDIUM | v0.2.0 | api, feature |
| #50 | Bulk Operations | LOW | v0.2.0 | feature |
| #51 | Collection Management | LOW | v0.2.0 | feature |
| #52 | Search Within Tags | LOW | v0.2.0 | search, feature |

**Created**: 2026-01-05

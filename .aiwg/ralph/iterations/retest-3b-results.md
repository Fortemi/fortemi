# Phase 3B Retest Results - Spatial-Temporal Memory Search

**Date**: 2026-02-10
**Version**: v2026.2.8
**API**: https://memory.integrolabs.net
**Executor**: Claude Agent (automated)

## Pre-Check Summary

### Attachment Status

All three city notes have JPEG attachments from the previous run:

| Note | City | Attachment ID | Filename | Status | extracted_metadata |
|------|------|--------------|----------|--------|--------------------|
| `019c44c1-7850-...` | Paris | `019c44c3-0389-...` | paris-eiffel-tower.jpg | uploaded | **null** |
| `019c44c1-7ff1-...` | New York | `019c44c3-09db-...` | newyork-statue-liberty.jpg | uploaded | **null** |
| `019c44c1-8744-...` | Tokyo | `019c44c3-1281-...` | tokyo-shibuya.jpg | uploaded | **null** |
| `019c44c1-7850-...` | Paris | `019c44c4-9be9-...` | dated-2020-01-01.jpg | uploaded | **null** |
| `019c44c1-8744-...` | Tokyo | `019c44c4-a2a1-...` | dated-2025-12-31.jpg | uploaded | **null** |

- Fresh re-upload of Paris JPEG (ID `019c453f-1bdf-...`) also shows `extracted_metadata: null` after 30-second wait
- JPEG files confirmed to contain EXIF data (Exif marker present in file header)
- No background extraction jobs are created for attachments

### Provenance Status

All three notes have **empty provenance** on both endpoints:

| Endpoint | Paris | New York | Tokyo |
|----------|-------|----------|-------|
| `get_note_provenance` | 0 activities, 0 edges | 0 activities, 0 edges | 0 activities, 0 edges |
| `get_memory_provenance` | 0 files | 0 files | 0 files |

### Root Cause Analysis

**Two compounding issues prevent all Phase 3B tests from passing:**

1. **EXIF extraction not triggered** (Gitea #278 - closed but NOT fixed in deployed v2026.2.8):
   - JPEG uploads persist attachment records but never enqueue EXIF extraction background jobs
   - `extracted_metadata` remains null indefinitely
   - Attachment status stays "uploaded" (never transitions to "processed")

2. **Provenance creation API endpoints not deployed** (NEW finding):
   - MCP server code references `POST /api/v1/provenance/locations` (and `/devices`, `/files`, `/notes`)
   - These endpoints return **HTTP 404** on the deployed server
   - The provenance creation MCP tools (`create_provenance_location`, `create_provenance_device`, `create_file_provenance`, `create_note_provenance`) exist in `mcp-server/tools.js` and `mcp-server/index.js` but are NOT discoverable via MCP tool search
   - Without provenance creation endpoints, there is no way to populate spatial-temporal data (neither automatically via EXIF nor manually via API)

---

## Test Results

### Spatial Search (4 tests)

#### UAT-3B-001: Paris spatial search
- **Tool**: `search_memories_by_location(lat=48.8584, lon=2.2945, radius=10000)`
- **Expected**: Find Paris note
- **Actual**: `{"count": 0, "mode": "location", "results": []}`
- **Result**: **FAIL** - No provenance data exists; EXIF extraction not implemented (#278); provenance creation endpoints return 404
- **Blocking Issue**: #278 (EXIF extraction), provenance API not deployed

#### UAT-3B-005: New York spatial search
- **Tool**: `search_memories_by_location(lat=40.6892, lon=-74.0445, radius=10000)`
- **Expected**: Find New York note
- **Actual**: `{"count": 0, "mode": "location", "results": []}`
- **Result**: **FAIL** - Same root cause as UAT-3B-001
- **Blocking Issue**: #278, provenance API not deployed

#### UAT-3B-SPATIAL-3: Tokyo spatial search
- **Tool**: `search_memories_by_location(lat=35.6595, lon=139.7004, radius=10000)`
- **Expected**: Find Tokyo note
- **Actual**: `{"count": 0, "mode": "location", "results": []}`
- **Result**: **FAIL** - Same root cause as UAT-3B-001
- **Blocking Issue**: #278, provenance API not deployed

#### UAT-3B-SPATIAL-FAR: Null Island negative test
- **Tool**: `search_memories_by_location(lat=0, lon=0, radius=1000)`
- **Expected**: 0 results (no notes near 0,0)
- **Actual**: `{"count": 0, "mode": "location", "results": []}`
- **Result**: **PASS** - Correctly returns empty results. Note: this passes vacuously since no provenance data exists at all, but the API behavior is correct regardless.

### Temporal Search (2 tests)

#### UAT-3B-010: 2019-2021 temporal search
- **Tool**: `search_memories_by_time(start="2019-01-01T00:00:00Z", end="2021-01-01T00:00:00Z")`
- **Expected**: Find dated-2020-01-01.jpg attachment
- **Actual**: `{"count": 0, "mode": "time", "results": []}`
- **Result**: **FAIL** - No temporal provenance exists; EXIF DateTimeOriginal never extracted from dated-2020-01-01.jpg
- **Blocking Issue**: #278, provenance API not deployed

#### UAT-3B-011: 2025 temporal search
- **Tool**: `search_memories_by_time(start="2025-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
- **Expected**: Find dated-2025-12-31.jpg attachment
- **Actual**: `{"count": 0, "mode": "time", "results": []}`
- **Result**: **FAIL** - Same root cause as UAT-3B-010
- **Blocking Issue**: #278, provenance API not deployed

### Combined Search (2 tests)

#### UAT-3B-012: Paris combined spatial-temporal search
- **Tool**: `search_memories_combined(lat=48.8584, lon=2.2945, radius=50000, start="2019-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
- **Expected**: Find Paris note
- **Actual**: `{"count": 0, "mode": "combined", "results": []}`
- **Result**: **FAIL** - No provenance data to intersect
- **Blocking Issue**: #278, provenance API not deployed

#### UAT-3B-013: Null Island + 2020 negative combined test
- **Tool**: `search_memories_combined(lat=0, lon=0, radius=1000, start="2020-01-01T00:00:00Z", end="2020-12-31T00:00:00Z")`
- **Expected**: 0 results
- **Actual**: `{"count": 0, "mode": "combined", "results": []}`
- **Result**: **PASS** - Correctly returns empty. Vacuously true (no provenance exists), but API behavior is correct.

### Provenance Verification (3 tests)

#### UAT-3B-003: Paris note provenance
- **Tool**: `get_note_provenance(id="019c44c1-7850-7bb1-9642-99e26dbbec1b")`
- **Expected**: Spatial/temporal data from EXIF
- **Actual**: `{"all_activities": [], "all_edges": [], "current_chain": {"activity": null, "edges": []}, "derived_count": 0}`
- **Result**: **FAIL** - Empty provenance chain; EXIF data never extracted from attachments
- **Blocking Issue**: #278, provenance API not deployed

#### UAT-3B-004: Memory (file) provenance for all 3 cities
- **Tool**: `get_memory_provenance` for each note
- **Expected**: File provenance with GPS coordinates, device info, capture timestamps
- **Actual**: All three return `{"files": [], "note_id": "..."}`
  - Paris: `files: []`
  - New York: `files: []`
  - Tokyo: `files: []`
- **Result**: **FAIL** - No file provenance records exist; attachment upload does not create provenance
- **Blocking Issue**: #278, provenance API not deployed

#### UAT-3B-021: Provenance chains with location data
- **Expected**: All 3 city notes have provenance chains with location data
- **Actual**: All 3 notes have completely empty provenance (0 activities, 0 edges, 0 files)
- **Result**: **FAIL** - No provenance chains exist at all
- **Blocking Issue**: #278, provenance API not deployed

---

## Summary

| Category | Tests | Pass | Fail | Pass Rate |
|----------|-------|------|------|-----------|
| Spatial Search | 4 | 1 | 3 | 25% |
| Temporal Search | 2 | 0 | 2 | 0% |
| Combined Search | 2 | 1 | 1 | 50% |
| Provenance | 3 | 0 | 3 | 0% |
| **Total** | **11** | **2** | **9** | **18%** |

**Passes**: 2 (both negative tests that expect empty results - vacuously true)
**Fails**: 9 (all positive tests blocked by missing provenance data)

### Blocking Issues

1. **Gitea #278** (closed, NOT FIXED in v2026.2.8): EXIF metadata extraction not triggered on JPEG upload. Attachments stay in "uploaded" status with `extracted_metadata: null` indefinitely.

2. **NEW - Provenance API endpoints not deployed**: The MCP server code (`mcp-server/index.js` lines 243-325) references REST endpoints at `/api/v1/provenance/{locations,devices,files,notes}` that return HTTP 404 on the deployed server. The corresponding MCP tools (`create_provenance_location`, `create_provenance_device`, `create_file_provenance`, `create_note_provenance`) are defined in `mcp-server/tools.js` but are not discoverable via MCP tool search, suggesting the deployed MCP server binary may be older than the codebase.

### Recommendation

**REOPEN Gitea #278** with updated evidence showing the fix was either not deployed or is incomplete. Additionally, file a new issue for the missing provenance creation API endpoints since this blocks manual provenance data entry as a workaround for the EXIF extraction gap.

Until both issues are resolved, all 9 positive Phase 3B tests will continue to fail. The spatial-temporal search feature is non-functional in v2026.2.8.

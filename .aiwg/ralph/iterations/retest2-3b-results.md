# Phase 3B Retest 2 Results

**Date**: 2026-02-09 (UTC: 2026-02-10 ~04:10)
**System**: https://memory.integrolabs.net
**Tester**: Claude Opus 4.6 (automated)
**Method**: MCP tools (`mcp__fortemi__*`)

## Summary

| Metric | Value |
|--------|-------|
| Total Tests | 9 |
| PASS | 0 |
| FAIL | 9 |
| Pass Rate | 0% |

**Root Cause**: No provenance data (location/temporal/EXIF) is being extracted from uploaded JPEG attachments. All attachments show `status: "uploaded"` with null `document_type_name`, null `detected_document_type_name`, and no extracted metadata. The provenance creation REST endpoint (`POST /api/v1/provenance/locations`) returns 404. No `create_note_provenance` MCP tool exists. Without provenance data, all spatial search, temporal search (with EXIF dates), combined search, and provenance verification tests fail.

## Pre-Check Results

### Provenance Status

| Check | Result | Detail |
|-------|--------|--------|
| `get_note_provenance` (Paris) | Empty | `all_activities: []`, `all_edges: []`, no activity data |
| `get_memory_provenance` (Paris) | Empty | `files: []` -- no file provenance chain |
| `list_attachments` (Paris) | 3 files exist | `paris-eiffel-tower.jpg` (x2), `dated-2020-01-01.jpg` -- all `status: "uploaded"`, no `extracted_metadata` |
| REST `POST /api/v1/provenance/locations` | **404** | Endpoint does not exist |
| REST `POST /api/v1/notes/{id}/provenance` | **405** | Method not allowed (GET-only) |
| REST `POST /api/v1/attachments/{id}/provenance` | **404** | Endpoint does not exist |

### Temporal Search Behavior

The `search_memories_by_time` tool does return results when the time range covers the note creation date (2026-02-09). However, results have `provenance_id: null` and `attachment_id: null`, meaning the search is falling back to `user_created_at` timestamps rather than EXIF capture dates. This is why:
- Range `2019-01-01 to 2021-01-01` returns 0 (notes created in 2026)
- Range `2025-01-01 to 2026-01-01` returns 0 (notes created 2026-02-09, after end date)
- Range `2026-02-01 to 2026-02-28` returns 10 results (all notes in system, by creation date)

## Results Table

| Test ID | Test | Expected | Actual | Status |
|---------|------|----------|--------|--------|
| UAT-3B-001 | `search_memories_by_location(48.8584, 2.2945, 10km)` | Find Paris note | `count: 0, results: []` | **FAIL** |
| UAT-3B-005 | `search_memories_by_location(40.6892, -74.0445, 10km)` | Find New York note | `count: 0, results: []` | **FAIL** |
| UAT-3B-SPATIAL-3 | `search_memories_by_location(35.6595, 139.7004, 10km)` | Find Tokyo note | `count: 0, results: []` | **FAIL** |
| UAT-3B-010 | `search_memories_by_time(2019-01-01, 2021-01-01)` | Find results with EXIF dates in range | `count: 0, results: []` | **FAIL** |
| UAT-3B-011 | `search_memories_by_time(2025-01-01, 2026-01-01)` | Find results with EXIF dates in range | `count: 0, results: []` | **FAIL** |
| UAT-3B-012 | `search_memories_combined(48.8584, 2.2945, 50km, 2019-2026)` | Find Paris note | `count: 0, results: []` | **FAIL** |
| UAT-3B-003 | `get_note_provenance` (Paris) | Spatial data in provenance | `all_activities: [], all_edges: []` | **FAIL** |
| UAT-3B-004 | `get_memory_provenance` (all 3 cities) | File provenance populated | `files: []` for all 3 notes | **FAIL** |
| UAT-3B-021 | Provenance chains for all 3 cities | Location/temporal data | All empty: no activities, no edges, no provenance | **FAIL** |

## Detailed Evidence

### UAT-3B-001: Spatial Search - Paris (FAIL)

**Call**: `search_memories_by_location(lat=48.8584, lon=2.2945, radius=10000)`
**Response**:
```json
{ "count": 0, "mode": "location", "results": [] }
```
Also tested with 1,000,000m radius -- still 0 results. No location data exists in the system.

### UAT-3B-005: Spatial Search - New York (FAIL)

**Call**: `search_memories_by_location(lat=40.6892, lon=-74.0445, radius=10000)`
**Response**:
```json
{ "count": 0, "mode": "location", "results": [] }
```

### UAT-3B-SPATIAL-3: Spatial Search - Tokyo (FAIL)

**Call**: `search_memories_by_location(lat=35.6595, lon=139.7004, radius=10000)`
**Response**:
```json
{ "count": 0, "mode": "location", "results": [] }
```

### UAT-3B-010: Temporal Search 2019-2021 (FAIL)

**Call**: `search_memories_by_time(start="2019-01-01T00:00:00Z", end="2021-01-01T00:00:00Z")`
**Response**:
```json
{ "count": 0, "mode": "time", "results": [] }
```
Notes were created 2026-02-09, so they fall outside this range. No EXIF capture dates are stored.

### UAT-3B-011: Temporal Search 2025-2026 (FAIL)

**Call**: `search_memories_by_time(start="2025-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
**Response**:
```json
{ "count": 0, "mode": "time", "results": [] }
```
End date `2026-01-01` is before note creation `2026-02-09`. No EXIF dates stored.

### UAT-3B-012: Combined Search - Paris (FAIL)

**Call**: `search_memories_combined(lat=48.8584, lon=2.2945, radius=50000, start="2019-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
**Response**:
```json
{ "count": 0, "mode": "combined", "results": [] }
```

### UAT-3B-003: Note Provenance - Paris (FAIL)

**Call**: `get_note_provenance(id="019c44c1-7850-7bb1-9642-99e26dbbec1b")`
**Response**:
```json
{
  "all_activities": [],
  "all_edges": [],
  "current_chain": {
    "activity": null,
    "edges": [],
    "note_id": "019c44c1-7850-7bb1-9642-99e26dbbec1b",
    "revision_id": "019c44c1-785b-7491-912b-8cf2a84279db"
  },
  "derived_count": 0,
  "derived_notes": [],
  "note_id": "019c44c1-7850-7bb1-9642-99e26dbbec1b"
}
```
No spatial data, no activities, no edges.

### UAT-3B-004: Memory Provenance - All 3 Cities (FAIL)

All three notes return empty file provenance:

| Note | `files` |
|------|---------|
| Paris (`019c44c1-7850-7bb1-9642-99e26dbbec1b`) | `[]` |
| New York (`019c44c1-7ff1-7ee3-8ad0-4b216fbf19a6`) | `[]` |
| Tokyo (`019c44c1-8744-72f3-8723-b3ce01dd2b1d`) | `[]` |

Despite each note having uploaded JPEG attachments:
- Paris: 3 attachments (134KB, 197KB, 134KB)
- New York: 1 attachment (181KB)
- Tokyo: 2 attachments (199KB, 202KB)

### UAT-3B-021: Provenance Chains - All 3 Cities (FAIL)

All three notes have identical empty provenance:
- `all_activities: []`
- `all_edges: []`
- `current_chain.activity: null`
- `derived_count: 0`

No location data, no temporal capture data, no device data in any provenance chain.

## Root Cause Analysis

The fundamental issue is that **JPEG EXIF metadata is not being extracted on upload**. The upload pipeline stores the file (`status: "uploaded"`) but does not:

1. Extract GPS coordinates from EXIF to populate `note_provenance.location`
2. Extract capture dates from EXIF to populate `note_provenance.captured_at`
3. Create provenance records linking attachments to spatial-temporal data
4. Populate `extracted_metadata` on the attachment record

Additionally:
- The `POST /api/v1/provenance/locations` REST endpoint returns 404 (does not exist)
- No `create_note_provenance` MCP tool is available for manual provenance creation
- The `POST /api/v1/notes/{id}/provenance` returns 405 (GET-only, no POST handler)

This means there is **no way** (neither automatic nor manual) to add location/temporal provenance data to notes or attachments, making all spatial and temporal search features non-functional.

## Comparison to Previous Run

All 9 tests produced identical results to the previous UAT run. No change detected after the system update.

## Recommendations

1. **Implement EXIF extraction pipeline**: On JPEG upload, extract GPS, capture date, device info from EXIF headers and create provenance records automatically
2. **Add provenance creation API**: Either REST (`POST /api/v1/notes/{id}/provenance`) or MCP tool (`create_note_provenance`) for manual provenance injection
3. **Re-upload test images**: Once the pipeline is fixed, re-upload the city JPEG files to trigger provenance extraction
4. **Blocked issue reference**: These failures relate to existing Gitea issue #252 (attachment phantom write / metadata not persisted) and the broader provenance pipeline gap

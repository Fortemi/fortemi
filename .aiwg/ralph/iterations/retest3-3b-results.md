# Phase 3B Retest 3 Results

**Date**: 2026-02-10 (UTC)
**System**: https://memory.integrolabs.net
**Tester**: Claude Opus 4.6 (automated)
**Method**: MCP tools (`mcp__fortemi__*`) + curl for REST checks

## Summary

| Metric | Value |
|--------|-------|
| Total Tests | 9 |
| PASS | 0 |
| FAIL | 9 |
| Pass Rate | 0% |

**Root Cause**: All JPEG attachments previously uploaded to the 3 city notes have been lost (all `list_attachments` calls return `[]`). Without attachments, no file provenance data exists, and all spatial/temporal/combined searches return empty results. The note provenance chains are similarly empty with no activities or edges.

## Pre-Check Results

### What Changed Since Retest 2

| Check | Retest 2 | Retest 3 | Change |
|-------|----------|----------|--------|
| `list_attachments` (Paris) | 3 files (134KB, 197KB, 134KB) | `[]` (empty) | **REGRESSION** - attachments lost |
| `list_attachments` (New York) | 1 file (181KB) | `[]` (empty) | **REGRESSION** - attachments lost |
| `list_attachments` (Tokyo) | 2 files (199KB, 202KB) | `[]` (empty) | **REGRESSION** - attachments lost |
| `get_note_provenance` (Paris) | `all_activities: []` | `all_activities: []` | No change |
| `get_memory_provenance` (Paris) | `files: []` | `files: []` | No change |
| REST `POST /api/v1/provenance/locations` | **404** (not found) | **422 -> 200** (endpoint works!) | **FIX** - endpoint now deployed |

### REST Provenance Endpoint Discovery

The `POST /api/v1/provenance/locations` endpoint is now live (previously returned 404). Testing revealed:

1. Field names are `latitude`/`longitude` (not `lat`/`lon`)
2. `source` field required -- valid values: `gps_exif`, `device_api`, `user_manual`, `geocoded`, `ai_estimated`, `unknown`
3. `confidence` field required -- valid values include `high`
4. Successfully created a test location:

```bash
curl -s https://memory.integrolabs.net/api/v1/provenance/locations \
  -X POST -H "Content-Type: application/json" \
  -d '{"latitude":48.8584,"longitude":2.2945,"name":"Paris Test","source":"gps_exif","confidence":"high"}'
# Response: {"id":"019c4610-9a2b-7d13-8f77-0abcc99b1000"}
```

This is a positive infrastructure improvement, but without attachments to link provenance to, the spatial/temporal search tests still cannot pass.

## Results Table

| Test ID | Test | Expected | Actual | Status |
|---------|------|----------|--------|--------|
| UAT-3B-001 | `search_memories_by_location(48.8584, 2.2945, 10km)` | Find Paris note | `count: 0, results: []` | **FAIL** |
| UAT-3B-005 | `search_memories_by_location(40.6892, -74.0445, 10km)` | Find NY note | `count: 0, results: []` | **FAIL** |
| UAT-3B-SPATIAL-3 | `search_memories_by_location(35.6595, 139.7004, 10km)` | Find Tokyo note | `count: 0, results: []` | **FAIL** |
| UAT-3B-010 | `search_memories_by_time(2019-01-01, 2021-01-01)` | Find EXIF-dated results | `count: 0, results: []` | **FAIL** |
| UAT-3B-011 | `search_memories_by_time(2025-01-01, 2026-01-01)` | Find EXIF-dated results | `count: 0, results: []` | **FAIL** |
| UAT-3B-012 | `search_memories_combined(48.8584, 2.2945, 50km, 2019-2026)` | Find Paris note | `count: 0, results: []` | **FAIL** |
| UAT-3B-003 | `get_note_provenance` (Paris) | Spatial data in provenance | `all_activities: []` | **FAIL** |
| UAT-3B-004 | `get_memory_provenance` (all 3 cities) | File provenance populated | `files: []` for all 3 | **FAIL** |
| UAT-3B-021 | Provenance chains (all 3 cities) | Location/temporal data | All empty | **FAIL** |

## Detailed Evidence

### UAT-3B-001: Spatial Search - Paris (FAIL)

**Call**: `search_memories_by_location(lat=48.8584, lon=2.2945, radius=10000)`
**Response**:
```json
{ "count": 0, "mode": "location", "results": [] }
```

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

### UAT-3B-011: Temporal Search 2025-2026 (FAIL)

**Call**: `search_memories_by_time(start="2025-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
**Response**:
```json
{ "count": 0, "mode": "time", "results": [] }
```

Note: In retest 2, temporal search at least returned results for 2026-02 (based on note `user_created_at`). In retest 3, even that fallback may have been affected by the attachment loss, since the prov_file_provenance table is empty.

### UAT-3B-012: Combined Search - Paris (FAIL)

**Call**: `search_memories_combined(lat=48.8584, lon=2.2945, radius=50000, start="2019-01-01T00:00:00Z", end="2026-12-31T00:00:00Z")`
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
  "current_chain": null,
  "derived_count": 0,
  "derived_notes": [],
  "note_id": "019c44c1-7850-7bb1-9642-99e26dbbec1b"
}
```
No spatial data, no activities, no edges. Note: `current_chain` is now `null` (was previously an object with null activity in retest 2 -- minor response format change).

### UAT-3B-004: Memory Provenance - All 3 Cities (FAIL)

| Note | `files` |
|------|---------|
| Paris (`019c44c1-7850-7bb1-9642-99e26dbbec1b`) | `[]` |
| New York (`019c44c1-7ff1-7ee3-8ad0-4b216fbf19a6`) | `[]` |
| Tokyo (`019c44c1-8744-72f3-8723-b3ce01dd2b1d`) | `[]` |

All three notes have zero attachments (previously had 6 total), so no file provenance is possible.

### UAT-3B-021: Provenance Chains - All 3 Cities (FAIL)

All three notes have identical empty provenance:
- `all_activities: []`
- `all_edges: []`
- `current_chain: null`
- `derived_count: 0`

No location data, no temporal capture data, no device data in any provenance chain.

## Comparison to Retest 2 (2026-02-10 ~04:10 UTC)

| Aspect | Retest 2 | Retest 3 | Verdict |
|--------|----------|----------|---------|
| Test results | 0/9 PASS | 0/9 PASS | No change |
| Attachments exist | Yes (6 JPEG files across 3 notes) | **No** (all `[]`) | **Regression** |
| REST provenance endpoint | 404 | **200** (functional) | **Improvement** |
| `current_chain` format | Object with null activity | `null` | Minor format change |
| Temporal fallback to creation date | Yes (2026-02 range returned results) | Not tested (no prov data at all) | -- |

### Key Differences

1. **Attachment data loss**: The most significant regression. All 6 JPEG attachments previously uploaded to the city notes are gone. This could indicate a database volume reset, a cleanup operation, or a deployment that dropped the attachment storage.

2. **Provenance REST endpoint now live**: `POST /api/v1/provenance/locations` now works with the correct schema (`latitude`, `longitude`, `source`, `confidence`, `name`). This is a positive step -- the infrastructure is being built. However, the full provenance pipeline (upload -> EXIF extract -> create location -> create file provenance -> link to note) is still not automated.

## Root Cause Analysis

There are now **two blocking issues**:

1. **Attachment data loss** (new): The 6 JPEG attachments that were uploaded in the previous test run are no longer present. Without attachments, there is nothing to attach provenance to.

2. **No automatic EXIF extraction** (unchanged): Even if attachments were re-uploaded, the system does not automatically extract GPS coordinates or capture dates from EXIF headers to create provenance records.

The provenance REST endpoint being live is progress, but the end-to-end pipeline still requires:
- Attachments to be present on notes
- EXIF metadata to be extracted from JPEG uploads
- Provenance records (location, device, file_provenance) to be created automatically or manually
- File provenance to be linked to notes for spatial/temporal search indexing

## Recommendations

1. **Investigate attachment loss**: Determine why all JPEG attachments disappeared. Check if a database volume was reset or if a deployment wiped attachment storage.
2. **Re-upload test JPEG files**: Once the root cause of attachment loss is resolved, re-upload the city JPEG files with EXIF GPS data.
3. **Complete provenance pipeline**: The REST endpoint works for location creation. Next steps:
   - Implement `POST /api/v1/provenance/devices` (if not already done)
   - Implement `POST /api/v1/provenance/files` to link attachments to locations
   - Or: implement automatic EXIF extraction on JPEG upload
4. **Blocked issue reference**: These failures relate to Gitea issues #252 (attachment phantom write) and the broader provenance pipeline gap.

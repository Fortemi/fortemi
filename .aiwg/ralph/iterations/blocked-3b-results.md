# Phase 3B Memory Search Tests - Blocked Test Execution Results

**Date**: 2026-02-09T23:35Z
**Version**: v2026.2.8+
**Executor**: Agent (Claude)
**API**: https://memory.integrolabs.net

## Summary

| Metric | Count |
|--------|-------|
| Total tests | 15 |
| PASS | 2 |
| FAIL | 12 |
| BLOCKED | 1 |
| Pass rate | 13.3% |

**Root cause**: Attachment uploads succeed (HTTP 200, attachment record created) but EXIF extraction background job never triggers. All attachments remain in `status: "uploaded"` indefinitely. No provenance data (GPS coordinates, capture timestamps) is populated from JPEG EXIF metadata. This blocks all spatial, temporal, and combined memory searches.

## Seed Notes

| City | Note ID |
|------|---------|
| Paris | `019c44c1-7850-7bb1-9642-99e26dbbec1b` |
| New York | `019c44c1-7ff1-7ee3-8ad0-4b216fbf19a6` |
| Tokyo | `019c44c1-8744-72f3-8723-b3ce01dd2b1d` |

---

## Step 1: Upload Photos to Notes

### UAT-3B-UPLOAD-1: Upload paris-eiffel-tower.jpg to Paris note
- **Result**: PASS
- **Attachment ID**: `019c44c3-0389-7d80-bae1-17614df1a4d5`
- **Status**: HTTP 200, attachment created with `status: "uploaded"`, `extraction_strategy: "vision"`
- **Note**: Attachment record persisted correctly. No extraction job queued.

### UAT-3B-UPLOAD-2: Upload newyork-statue-liberty.jpg to New York note
- **Result**: PASS
- **Attachment ID**: `019c44c3-09db-7201-916a-9022bbd8cdf9`
- **Status**: HTTP 200, attachment created with `status: "uploaded"`, `extraction_strategy: "vision"`

### UAT-3B-UPLOAD-3: Upload tokyo-shibuya.jpg to Tokyo note
- **Result**: FAIL
- **Attachment ID**: `019c44c3-1281-7261-9f6d-a8ef62175ed9`
- **Status**: HTTP 200, attachment created. However, this is marked FAIL because the end-to-end expectation is that upload triggers EXIF extraction which populates provenance. The extraction never occurred.
- **Observation**: Attachment `status` field remains `"uploaded"` (never transitions to `"processing"` or `"processed"`). `extracted_metadata` remains `null`. No background job of type `exif_extraction` or similar appears in the job queue.
- **Clarification**: Uploads 1 and 2 marked PASS narrowly (upload succeeded), but the same EXIF extraction failure applies to all three.

---

## Step 2: Verify Provenance Created

### UAT-3B-003: Get note provenance for Paris note
- **Result**: FAIL
- **Expected**: Provenance data with location (lat ~48.86, lon ~2.29) from EXIF GPS tags
- **Actual**: `get_note_provenance` returns empty structure: `all_activities: [], all_edges: [], derived_count: 0`
- **get_memory_provenance**: `files: []` (empty)
- **Root cause**: No EXIF extraction job triggered on attachment upload. Attachment `extracted_metadata` is `null`.
- **Waited**: 25+ seconds total (two retries) before confirming.

### UAT-3B-004: Get memory provenance (all notes)
- **Result**: FAIL
- **Expected**: Provenance records for all three city notes with GPS and timestamp data
- **Actual**: All three notes return `files: []` from `get_memory_provenance`:
  - Paris: `{ files: [], note_id: "019c44c1-7850-..." }`
  - New York: `{ files: [], note_id: "019c44c1-7ff1-..." }`
  - Tokyo: `{ files: [], note_id: "019c44c1-8744-..." }`
- **Job queue check**: 0 pending, 0 processing. 58 completed (all are note creation jobs: embedding, linking, title_generation, concept_tagging). 16 failed (all are "Note not found" errors from previously deleted test notes). No EXIF extraction jobs present.

---

## Step 3: Spatial Search

### UAT-3B-001: Search near Eiffel Tower (Paris)
- **Result**: FAIL
- **Query**: `search_memories_by_location(lat=48.8584, lon=2.2945, radius=10000)`
- **Expected**: Paris note found in results
- **Actual**: `{ count: 0, mode: "location", results: [] }`
- **Root cause**: No provenance/location data populated from EXIF extraction.

### UAT-3B-005: Search near Statue of Liberty (New York)
- **Result**: FAIL
- **Query**: `search_memories_by_location(lat=40.6892, lon=-74.0445, radius=10000)`
- **Expected**: New York note found in results
- **Actual**: `{ count: 0, mode: "location", results: [] }`

### UAT-3B-SPATIAL-3: Search near Shibuya (Tokyo)
- **Result**: FAIL
- **Query**: `search_memories_by_location(lat=35.6595, lon=139.7004, radius=10000)`
- **Expected**: Tokyo note found in results
- **Actual**: `{ count: 0, mode: "location", results: [] }`

### UAT-3B-SPATIAL-FAR: Search middle of ocean (negative test)
- **Result**: PASS (vacuously)
- **Query**: `search_memories_by_location(lat=0, lon=0, radius=1000)`
- **Expected**: 0 results
- **Actual**: `{ count: 0, mode: "location", results: [] }`
- **Note**: This passes but is vacuously true since no provenance data exists at all. The negative test cannot be considered meaningful without positive tests also passing.

---

## Step 4: Temporal Search

### UAT-3B-009: Upload dated photos
- **Result**: FAIL (upload OK, extraction missing)
- **Dated-2020 upload**: Attachment `019c44c4-9be9-7822-8d35-4ec7b363177d` to Paris note. Status: `uploaded`, `extracted_metadata: null`.
- **Dated-2025 upload**: Attachment `019c44c4-a2a1-7f13-8c41-13c632420959` to Tokyo note. Status: `uploaded`, `extracted_metadata: null`.
- Same issue: no EXIF extraction triggered.

### UAT-3B-010: Temporal search 2019-2021
- **Result**: FAIL
- **Query**: `search_memories_by_time(start="2019-01-01T00:00:00Z", end="2021-01-01T00:00:00Z")`
- **Expected**: Results including dated-2020-01-01.jpg attachment
- **Actual**: `{ count: 0, mode: "time", results: [] }`

### UAT-3B-011: Temporal search 2025
- **Result**: FAIL
- **Query**: `search_memories_by_time(start="2025-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
- **Expected**: Results including dated-2025-12-31.jpg attachment
- **Actual**: `{ count: 0, mode: "time", results: [] }`

---

## Step 5: Combined Search

### UAT-3B-012: Combined search near Paris in broad time range
- **Result**: FAIL
- **Query**: `search_memories_combined(lat=48.8584, lon=2.2945, radius=50000, start="2019-01-01T00:00:00Z", end="2026-01-01T00:00:00Z")`
- **Expected**: Paris note in results
- **Actual**: `{ count: 0, mode: "combined", results: [] }`

### UAT-3B-013: Combined search impossible location + time (negative test)
- **Result**: PASS (vacuously)
- **Query**: `search_memories_combined(lat=0, lon=0, radius=1000, start="2020-01-01T00:00:00Z", end="2020-12-31T00:00:00Z")`
- **Expected**: 0 results
- **Actual**: `{ count: 0, mode: "combined", results: [] }`
- **Note**: Vacuously true -- no provenance data exists anywhere.

---

## Step 6: Note-Level Provenance

### UAT-3B-021: Verify provenance structure for all 3 city notes
- **Result**: FAIL
- **Expected**: Provenance chains with spatial (GPS lat/lon) and temporal (capture date) data from EXIF extraction
- **Actual**: All three notes return minimal provenance structure with no spatial/temporal data:
  ```json
  {
    "all_activities": [],
    "all_edges": [],
    "current_chain": { "activity": null, "edges": [] },
    "derived_count": 0,
    "derived_notes": []
  }
  ```
- **Note**: `get_note_provenance` returns W3C PROV chain (creation/revision history) but contains no spatial-temporal data. The `get_memory_provenance` endpoint which should return file-level provenance (GPS, device, capture time) returns `files: []` for all notes.

---

## Root Cause Analysis

### Primary Issue: EXIF Extraction Pipeline Not Triggering

The attachment upload flow creates the attachment record and blob successfully but does **not** enqueue an EXIF metadata extraction background job. Evidence:

1. **Attachment status**: All 5 uploaded JPEGs remain in `status: "uploaded"` with `extracted_metadata: null` after 25+ seconds
2. **Job queue**: No EXIF extraction jobs appear. The only jobs are note-level jobs (embedding, linking, title_generation, concept_tagging) triggered by note creation
3. **Extraction strategy**: Set to `"vision"` (AI-based) rather than `"exif"` or `"metadata"`. The vision extraction may be designed for content description rather than EXIF parsing
4. **Provenance tables empty**: Both `get_note_provenance` and `get_memory_provenance` return empty results

### Relationship to Known Issues

- **#252 (Attachment phantom write)**: Previously identified as "upload 200 but data not persisted." This test confirms uploads DO persist (attachment records exist, blob IDs assigned) but the downstream EXIF extraction pipeline is missing or broken.
- This represents a **new aspect** of the attachment pipeline: the metadata extraction stage (EXIF GPS/timestamp parsing -> provenance table population) is not implemented or not triggered.

### Impact

This single root cause blocks **12 of 15 tests** (all positive spatial, temporal, combined, and provenance verification tests). Only the 2 negative tests (no data at impossible locations) and 1 upload test pass meaningfully.

---

## Recommendations

1. **File new issue**: EXIF extraction pipeline does not trigger on JPEG attachment upload. No background job is enqueued to parse GPS coordinates, capture timestamps, or device metadata from EXIF tags.
2. **Implementation needed**:
   - On attachment upload of image/jpeg or image/tiff, enqueue an `exif_extraction` background job
   - Job should parse EXIF data (GPS lat/lon, DateTimeOriginal, Make/Model)
   - Populate `attachments.extracted_metadata` with parsed EXIF data
   - Create corresponding entries in the provenance/memory_provenance table
   - Update attachment `status` from `"uploaded"` to `"processed"`
3. **Workaround check**: Investigate if there is a manual way to trigger EXIF extraction (e.g., `reprocess_note` or direct API call)
4. **Re-test**: Once EXIF extraction pipeline is functional, all 15 tests should be re-run

---

## Attachment Reference

| File | Note | Attachment ID | Status |
|------|------|---------------|--------|
| paris-eiffel-tower.jpg | Paris | `019c44c3-0389-7d80-bae1-17614df1a4d5` | uploaded |
| newyork-statue-liberty.jpg | New York | `019c44c3-09db-7201-916a-9022bbd8cdf9` | uploaded |
| tokyo-shibuya.jpg | Tokyo | `019c44c3-1281-7261-9f6d-a8ef62175ed9` | uploaded |
| dated-2020-01-01.jpg | Paris | `019c44c4-9be9-7822-8d35-4ec7b363177d` | uploaded |
| dated-2025-12-31.jpg | Tokyo | `019c44c4-a2a1-7f13-8c41-13c632420959` | uploaded |

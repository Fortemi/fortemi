# Retest 3 - Phase 3B: Spatial-Temporal Search Pipeline (Fresh Data)

**Date**: 2026-02-10T05:44-05:50 UTC
**Version**: v2026.2.8+
**Tester**: Claude (automated)
**Purpose**: Re-test spatial-temporal search pipeline with freshly created notes and EXIF-embedded JPEGs after confirming EXIF extraction is working.

---

## Seed Data

### Notes Created

| City | Note ID | Title | Tags |
|------|---------|-------|------|
| Paris | `019c4614-bc6a-75f0-95fc-c2667274bca5` | Paris Travel 2024 | travel/paris, photos/geotagged |
| New York | `019c4614-c631-7d01-9497-1441e566e4f9` | New York Visit 2024 | travel/newyork, photos/geotagged |
| Tokyo | `019c4614-cf7f-7700-8147-a1ac098bc760` | Tokyo Trip 2024 | travel/tokyo, photos/geotagged |

### Attachments Uploaded

| City | Attachment ID | Filename | EXIF GPS | EXIF DateTime |
|------|---------------|----------|----------|---------------|
| Paris | `019c4615-09da-77d1-9096-0ba273a96b4f` | paris-eiffel-tower.jpg | 48.8584, 2.2945 | 2024:07:14 10:30:00 |
| New York | `019c4615-0fd1-7e12-a52a-f92bd55db82b` | newyork-statue-liberty.jpg | 40.6892, -74.0445 | 2024:07:14 14:00:00 |
| Tokyo | `019c4615-1570-7c61-b6c6-517ac6966c4d` | tokyo-shibuya.jpg | 35.6595, 139.7004 | 2024:07:14 18:00:00 |

**Image creation**: Synthetic 100x100 JPEG images created with ImageMagick `convert`, EXIF GPS and DateTimeOriginal injected via `exiftool`.

---

## EXIF Extraction Verification

All 3 attachments reached `status: "completed"` within 30 seconds. Extracted metadata confirmed:

| City | extracted_metadata.exif.gps | extracted_metadata.exif.datetime.original | extracted_metadata.exif.camera |
|------|---------------------------|------------------------------------------|-------------------------------|
| Paris | `{ latitude: 48.8584, longitude: 2.2945 }` | `2024:07:14 10:30:00` | TestCamera / Paris-Cam |
| New York | `{ latitude: 40.6892, longitude: -74.0445 }` | `2024:07:14 14:00:00` | TestCamera / NYC-Cam |
| Tokyo | `{ latitude: 35.6595, longitude: 139.7004 }` | `2024:07:14 18:00:00` | TestCamera / Tokyo-Cam |

**Result**: EXIF extraction pipeline is fully functional for GPS and datetime.

---

## Provenance Verification (Pre-Test)

File provenance was automatically created for all 3 notes via EXIF extraction:

| City | Provenance ID | Location Source | Lat/Lon | capture_time_start | capture_time_end |
|------|--------------|-----------------|---------|-------------------|-----------------|
| Paris | `019c4615-24b6-73d0-b9a8-b5580587f400` | gps_exif | 48.8584, 2.2945 | **null** | **null** |
| New York | `019c4615-26d1-75ec-9b15-bd61cbdba800` | gps_exif | 40.6892, -74.0445 | **null** | **null** |
| Tokyo | `019c4615-28ec-7658-b747-efb310743c00` | gps_exif | 35.6595, 139.7004 | **null** | **null** |

**FINDING**: EXIF datetime is extracted into `extracted_metadata.exif.datetime.original` on the attachment record, and `time_source: "exif"` / `time_confidence: "high"` are set on the provenance record, BUT `capture_time_start` and `capture_time_end` are NOT populated. This means the EXIF-to-provenance pipeline extracts GPS coordinates correctly but does NOT map `DateTimeOriginal` to the temporal fields.

---

## Test Results Summary

| Test ID | Description | Result | Notes |
|---------|-------------|--------|-------|
| UAT-3B-001 | Spatial search near Paris (48.8584, 2.2945, r=10km) | **PASS** | Found 2 results (1 from this test, 1 from prior run) |
| UAT-3B-005 | Spatial search near New York (40.6892, -74.0445, r=10km) | **PASS** | Found 1 result matching our NY note |
| UAT-3B-SPATIAL-3 | Spatial search near Tokyo (35.6595, 139.7004, r=10km) | **PASS** | Found 1 result matching our Tokyo note |
| UAT-3B-010 | Temporal search 2024 range | **FAIL** | 0 results - capture_time_start/end not populated from EXIF |
| UAT-3B-011 | Temporal search 2019-2020 (negative) | **PASS** | 0 results as expected (trivial pass - no temporal data at all) |
| UAT-3B-012 | Combined Paris + 2024 | **FAIL** | 0 results - temporal component fails due to null capture times |
| UAT-3B-003 | Note provenance for Paris | **FAIL** | all_activities empty (by design: revision_mode=none) |
| UAT-3B-004 | Memory provenance for all cities | **PASS** | All 3 notes have non-empty files array with location/device |
| UAT-3B-021 | Provenance chains with location/temporal | **PARTIAL** | Location data present on all 3; temporal data null on all 3 |

---

## Detailed Evidence

### UAT-3B-001: Spatial Search Near Paris - PASS

**Input**: `search_memories_by_location(lat=48.8584, lon=2.2945, radius=10000)`
**Output**: `count: 2`
- Result 1: attachment `019c4610-d976-71a1-9c6f-1c8dc823d86d` (prior test run), note `019c4610-a3a7-7b21-839b-41abb1ff9445`, distance 0m
- Result 2: attachment `019c4615-09da-77d1-9096-0ba273a96b4f` (this test), note `019c4614-bc6a-75f0-95fc-c2667274bca5`, distance 0m

Our freshly created Paris note was found. **PASS**.

### UAT-3B-005: Spatial Search Near New York - PASS

**Input**: `search_memories_by_location(lat=40.6892, lon=-74.0445, radius=10000)`
**Output**: `count: 1`
- Result: attachment `019c4615-0fd1-7e12-a52a-f92bd55db82b`, note `019c4614-c631-7d01-9497-1441e566e4f9`, distance 0m

Our freshly created NY note was found. **PASS**.

### UAT-3B-SPATIAL-3: Spatial Search Near Tokyo - PASS

**Input**: `search_memories_by_location(lat=35.6595, lon=139.7004, radius=10000)`
**Output**: `count: 1`
- Result: attachment `019c4615-1570-7c61-b6c6-517ac6966c4d`, note `019c4614-cf7f-7700-8147-a1ac098bc760`, distance 0m

Our freshly created Tokyo note was found. **PASS**.

### UAT-3B-010: Temporal Search 2024 - FAIL

**Input**: `search_memories_by_time(start="2024-01-01T00:00:00Z", end="2025-01-01T00:00:00Z")`
**Output**: `count: 0, results: []`

**Root cause**: The EXIF extraction pipeline populates `extracted_metadata.exif.datetime.original` on the attachment record and sets `time_source: "exif"` on the file provenance, but does NOT copy the datetime value into `capture_time_start` / `capture_time_end` on the `file_provenance` table. The temporal search query filters on these columns, so it finds nothing.

**Evidence**:
- Attachment metadata: `"datetime": { "original": "2024:07:14 10:30:00" }` (present)
- File provenance: `"capture_time_start": null, "capture_time_end": null` (missing)

**FAIL** - Bug in EXIF-to-provenance pipeline: datetime not mapped to temporal fields.

### UAT-3B-011: Temporal Search 2019-2020 (Negative) - PASS

**Input**: `search_memories_by_time(start="2019-01-01T00:00:00Z", end="2020-01-01T00:00:00Z")`
**Output**: `count: 0, results: []`

0 results as expected. **PASS** (trivially, since no provenance has temporal data populated at all).

### UAT-3B-012: Combined Paris + 2024 - FAIL

**Input**: `search_memories_combined(lat=48.8584, lon=2.2945, radius=50000, start="2024-01-01T00:00:00Z", end="2025-01-01T00:00:00Z")`
**Output**: `count: 0, results: []`

Same root cause as UAT-3B-010. The spatial component would match (proven by UAT-3B-001), but the temporal AND condition fails because `capture_time_start`/`capture_time_end` are null. **FAIL**.

### UAT-3B-003: Note Provenance for Paris - FAIL

**Input**: `get_note_provenance(id="019c4614-bc6a-75f0-95fc-c2667274bca5")`
**Output**: `all_activities: [], all_edges: []`

Notes created with `revision_mode=none` have no AI revision activity, so the W3C PROV chain is empty. This is by design -- the test expectation of "non-empty all_activities" is only met when the note goes through AI revision. **FAIL** (expected behavior given revision_mode=none, but the test criteria was not met).

### UAT-3B-004: Memory Provenance for All Cities - PASS

All 3 notes have non-empty `files` arrays:
- Paris: 1 file with location (48.8584, 2.2945), device (TestCamera/Paris-Cam)
- New York: 1 file with location (40.6892, -74.0445), device (TestCamera/NYC-Cam)
- Tokyo: 1 file with location (35.6595, 139.7004), device (TestCamera/Tokyo-Cam)

**PASS** - All 3 have file provenance with spatial data.

### UAT-3B-021: Provenance Chains with Location/Temporal - PARTIAL PASS

All 3 notes have location data in their file provenance:
- Paris: lat=48.8584, lon=2.2945, source=gps_exif
- New York: lat=40.6892, lon=-74.0445, source=gps_exif
- Tokyo: lat=35.6595, lon=139.7004, source=gps_exif

However, ALL 3 have `capture_time_start: null` and `capture_time_end: null`.

**PARTIAL PASS** - Location data present and correct; temporal data missing.

---

## Bug Report

### EXIF DateTimeOriginal Not Mapped to Provenance Temporal Fields

**Severity**: Medium-High
**Component**: EXIF extraction pipeline (file provenance creation)
**Affects**: UAT-3B-010, UAT-3B-012, UAT-3B-021

**Description**: When EXIF GPS data is extracted from a JPEG and file provenance is auto-created, the GPS coordinates are correctly mapped to `provenance_locations.latitude`/`longitude`. However, the EXIF `DateTimeOriginal` value (stored in `extracted_metadata.exif.datetime.original`) is NOT mapped to `file_provenance.capture_time_start` / `capture_time_end`.

**Expected behavior**: The EXIF `DateTimeOriginal` should be parsed and stored as `capture_time_start` (and optionally `capture_time_end = capture_time_start`) on the file provenance record, enabling temporal search via `search_memories_by_time`.

**Observed behavior**:
- `extracted_metadata.exif.datetime.original` = `"2024:07:14 10:30:00"` (correctly extracted)
- `file_provenance.time_source` = `"exif"` (correctly set)
- `file_provenance.time_confidence` = `"high"` (correctly set)
- `file_provenance.capture_time_start` = `null` (BUG - should be `2024-07-14T10:30:00Z`)
- `file_provenance.capture_time_end` = `null` (BUG - should be `2024-07-14T10:30:00Z`)

**Impact**: `search_memories_by_time` and `search_memories_combined` return 0 results for all EXIF-sourced temporal data.

---

## Overall Results

| Metric | Count |
|--------|-------|
| Total tests | 9 |
| PASS | 5 |
| FAIL | 3 |
| PARTIAL | 1 |
| **Pass rate** | **5/9 (55.6%)** |

### By Category

| Category | Tests | Pass | Fail | Partial |
|----------|-------|------|------|---------|
| Spatial Search | 3 | 3 | 0 | 0 |
| Temporal Search | 2 | 1 | 1 | 0 |
| Combined Search | 1 | 0 | 1 | 0 |
| Provenance | 3 | 1 | 1 | 1 |

### Key Findings

1. **Spatial search is fully functional** (3/3 PASS). EXIF GPS extraction correctly populates provenance locations, and `search_memories_by_location` returns accurate results with distance_m=0 for exact coordinate matches.

2. **Temporal search is broken** due to EXIF datetime not being mapped to `capture_time_start`/`capture_time_end` in the file provenance pipeline. The metadata IS extracted (`exif.datetime.original`), and the provenance record correctly notes `time_source: "exif"` and `time_confidence: "high"`, but the actual timestamp values are not written.

3. **Combined search fails** as a consequence of the temporal search bug -- the temporal AND condition never matches.

4. **Note-level provenance** (W3C PROV activity chain) is empty for notes created with `revision_mode=none`. This is expected behavior but means UAT-3B-003 only passes for AI-revised notes.

5. **File-level provenance** (memory provenance) works correctly for spatial data -- all GPS coordinates, devices, and event types are properly extracted and stored.

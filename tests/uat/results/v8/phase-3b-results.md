# Phase 3B: Memory Search (Temporal-Spatial) — Results

**Date**: 2026-02-13
**Version**: v2026.2.8
**Result**: 26 tests — 22 PASS, 1 FAIL, 1 PARTIAL, 1 BLOCKED (84.6% / 88.5% with partials)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| UAT-3B-001 | Search by Location (Eiffel Tower) | PASS | Returns 1 result at exact coordinates |
| UAT-3B-002 | Search by Location with Radius | PASS | 1km radius captures nearby points |
| UAT-3B-003 | Create Provenance Location | BLOCKED | MCP serializes lat/lon as strings — **#358 filed** |
| UAT-3B-004 | Search No Results | PASS | Remote location returns empty array |
| UAT-3B-005 | Distance Calculation | PASS | distance_m field populated correctly |
| UAT-3B-006 | Search by Time (Recent) | PASS | Returns notes captured in last hour |
| UAT-3B-007 | Search by Time (Range) | PASS | 24h range returns expected results |
| UAT-3B-008 | Search by Time (Old) | PASS | 1990s date returns 0 results |
| UAT-3B-009 | Time Range Validation | PASS | API accepts ISO 8601 timestamps |
| UAT-3B-010 | Combined Search (Location + Time Match) | PASS | Eiffel Tower + recent time = 1 result |
| UAT-3B-011 | Combined Search (Location Mismatch) | FAIL | NYC + recent time returned 4 results with null locations — **#359 filed** |
| UAT-3B-012 | Combined Search (Time Mismatch) | PASS | Eiffel Tower + old time = 0 results |
| UAT-3B-013 | Get Full Provenance Chain | PASS | Returns location, capture_time, event_type |
| UAT-3B-014 | Verify Location Coordinates | PASS | latitude: 48.8584, longitude: 2.2945 |
| UAT-3B-015 | Verify Device Info Present | PARTIAL | device: null but location.source: "gps_exif" indicates context |
| UAT-3B-016 | Verify Capture Time Present | PASS | capture_time_start populated correctly |
| UAT-3B-017 | Invalid Note ID Error | PASS | Returns graceful empty result (files: []) |
| UAT-3B-018 | Invalid Latitude Error | PASS | 400: "Latitude must be between -90 and 90" |
| UAT-3B-019 | Invalid Longitude Error | PASS | 400: "Longitude must be between -180 and 180" |
| UAT-3B-019a | Invalid Timestamp Error | PASS | 400: Clear error with format examples |
| UAT-3B-020 | Empty Results (No Match) | PASS | South Pole search returns {count: 0, results: []} |
| UAT-3B-021 | Create Note-Level Provenance | PASS | provenance_id: 019c5a73-7d9e-7278-9a08-59d19cc62800 |
| UAT-3B-022 | Retrieve Note-Level Provenance | PASS | get_memory_provenance returns note.location + time |
| UAT-3B-023 | Note Provenance with Location | PASS | Location (51.5007, -0.1246) correctly linked |
| UAT-3B-024 | Note Provenance with Time Range | PASS | capture_time_start/end stored correctly |
| UAT-3B-025 | Search Finds Note with Provenance | PASS | Location search returns note_id (no attachment) |

## Issues Filed

### #358: MCP Numeric Serialization Bug
- **Status**: Open
- **Severity**: Medium
- **Description**: `create_provenance_location` serializes latitude/longitude as strings instead of numbers in MCP responses

### #359: Spatial Filter Bug in Combined Search
- **Status**: Open
- **Severity**: High
- **Description**: `search_memories_combined` returns records with null locations when searching ANY coordinates. NYC search (40.7128, -74.006) returned 4 results with `distance_m: 0` and `location_name: null` when it should have returned 0.

## Detailed Results

### UAT-3B-011: Spatial Filter Bug

**Query**:
```json
{
  "lat": 40.7128,
  "lon": -74.006,
  "start": "2026-02-13T00:00:00Z",
  "end": "2026-02-14T00:00:00Z"
}
```

**Expected**: 0 results (no provenance exists at NYC)

**Actual**: 4 results with null location data:
```json
{
  "count": 4,
  "results": [
    {"attachment_id": "...", "distance_m": 0, "location_name": null},
    {"attachment_id": "...", "distance_m": 0, "location_name": null},
    ...
  ]
}
```

**Analysis**: When time filter matches but no spatial provenance exists, API returns all time-matching records with `distance_m: 0` instead of filtering them out.

### UAT-3B-015: Device Info Partial

**Observation**: `device` field is null in provenance record, but `location.source: "gps_exif"` indicates the capture context. This is acceptable behavior for programmatically-created provenance where no actual device was involved.

### Note-Level Provenance Chain Verified

Full workflow tested:
1. `create_note` → note_id: 019c5a73-52d8-7342-9cf7-f95a2ff927f6
2. `create_provenance_location` → location_id: 019c5a73-6482-732f-8335-bd53a71ff400
3. `create_note_provenance` → provenance_id: 019c5a73-7d9e-7278-9a08-59d19cc62800
4. `get_memory_provenance` → Returns full chain with location + time
5. `search_memories_by_location` → Finds note via location search

## Stored IDs

- eiffel_note_id: 019c5a53-acde-7db1-8f49-d6899e2d0b44
- eiffel_attachment_id: 019c5a54-3e56-7cc2-99b1-df9460db81c3
- bigben_note_id: 019c5a73-52d8-7342-9cf7-f95a2ff927f6
- bigben_location_id: 019c5a73-6482-732f-8335-bd53a71ff400
- bigben_provenance_id: 019c5a73-7d9e-7278-9a08-59d19cc62800

## Phase Assessment

**Overall**: 22/26 tests passed (84.6%)

**Critical Issues**:
- #359: Spatial filter bug causes false positives in combined search

**Minor Issues**:
- #358: MCP serialization (workaround: use REST API for location creation)
- Device info null for programmatic provenance (acceptable)

**Note-Level Provenance**: Fully functional - can attach spatial-temporal context directly to notes without requiring file attachments.

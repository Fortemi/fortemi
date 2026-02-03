# UAT Phase 3B: Memory Search (Temporal-Spatial)

**Purpose**: Verify temporal-spatial memory search capabilities for file provenance queries
**Duration**: ~15 minutes
**Prerequisites**: Phase 1 seed data exists, PostGIS extension enabled, W3C PROV schema migrated, test data generated
**Critical**: Yes (100% pass required)

> **Test Data**: GPS-tagged images for provenance testing in `tests/uat/data/provenance/`:
> `paris-eiffel-tower.jpg` (48.8584N, 2.2945E), `newyork-statue-liberty.jpg` (40.6892N, 74.0445W),
> `tokyo-shibuya.jpg` (35.6595N, 139.7004E), `dated-2020-01-01.jpg`, `dated-2025-12-31.jpg`.
> Generate with: `cd tests/uat/data/scripts && ./generate-test-data.sh`

---

## Prerequisites Check

### UAT-3B-000: Verify PostGIS and PROV Schema

**Description**: Ensure PostGIS extension and W3C PROV temporal-spatial schema are available

**Prerequisites**: Database connection

**Steps**:
1. Check PostGIS: `SELECT PostGIS_Version()`
2. Check prov_location table: `SELECT COUNT(*) FROM prov_location`
3. Check file_provenance table: `SELECT COUNT(*) FROM file_provenance`

**Expected Results**:
- PostGIS version returned (e.g., "3.4.0")
- Both tables exist and are queryable
- No SQL errors

**Verification**:
- PostGIS extension installed
- Migration 20260204100000 (W3C PROV schema) applied

---

## Search by Location

### UAT-3B-001: Search Near Location - Basic

**Description**: Search for memories captured near a specific location (Eiffel Tower)

**Prerequisites**:
- Test note with attachment created
- Attachment has provenance record with GPS coordinates (48.8584°N, 2.2945°E)

**Setup**:
1. Create test note: `create_note({ content: "# Paris Trip", tags: ["uat/memory-search"], revision_mode: "none" })`
2. Upload photo: `store_file({ note_id: <note-id>, filename: "eiffel-tower.jpg", content_type: "image/jpeg", data: <gps-photo-bytes> })`
3. Create location: `INSERT INTO prov_location (point, source, confidence) VALUES (ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography, 'exif', 'high')`
4. Create provenance: `INSERT INTO file_provenance (attachment_id, location_id, capture_time, event_type) VALUES (<attachment-id>, <location-id>, tstzrange(NOW(), NOW()), 'photo')`

**Steps**:
1. Search within 1km radius: `search_by_location({ lat: 48.8584, lon: 2.2945, radius_meters: 1000 })`

**Expected Results**:
- Returns array with 1 `MemoryLocationResult`
- `attachment_id` matches uploaded photo
- `distance_m` < 1000.0
- `event_type` is "photo"
- `filename` is "eiffel-tower.jpg"
- `content_type` is "image/jpeg"

**Verification**:
- Spatial query finds attachment within radius

**Store**: `eiffel_attachment_id`, `eiffel_location_id`

---

### UAT-3B-002: Search Near Location - No Results

**Description**: Search far from any known locations and verify empty result

**Prerequisites**: Test data from UAT-3B-001 exists

**Steps**:
1. Search in NYC (far from Paris): `search_by_location({ lat: 40.7128, lon: -74.0060, radius_meters: 1000 })`

**Expected Results**:
- Returns empty array `[]`
- No error or crash
- `total: 0` (if pagination metadata included)

**Verification**:
- Query correctly filters by distance

---

### UAT-3B-003: Search with Large Radius

**Description**: Search with large radius covering multiple locations

**Prerequisites**: Multiple attachments with different locations

**Setup**:
1. Create 3 attachments at different Paris locations:
   - Eiffel Tower (48.8584, 2.2945)
   - Louvre (48.8606, 2.3376)
   - Notre-Dame (48.8530, 2.3499)

**Steps**:
1. Search from Eiffel Tower with 10km radius: `search_by_location({ lat: 48.8584, lon: 2.2945, radius_meters: 10000 })`

**Expected Results**:
- Returns 3 results
- All results have `distance_m` < 10000.0
- Results ordered by distance (ascending)

**Verification**:
- Multi-result spatial query works
- Results ordered correctly

**Store**: `louvre_attachment_id`, `notredame_attachment_id`

---

### UAT-3B-004: Verify Distance Ordering

**Description**: Verify results are ordered by distance from search point (closest first)

**Prerequisites**: 3 attachments from UAT-3B-003

**Steps**:
1. Search from Eiffel Tower: `search_by_location({ lat: 48.8584, lon: 2.2945, radius_meters: 10000 })`
2. Check distances: `results[0].distance_m`, `results[1].distance_m`, `results[2].distance_m`

**Expected Results**:
- First result (Eiffel Tower) has smallest distance (< 100m)
- `results[0].distance_m` <= `results[1].distance_m` <= `results[2].distance_m`
- Distance increases monotonically

**Verification**:
- ORDER BY distance_m works correctly

---

### UAT-3B-005: Search with Named Location

**Description**: Search for memories at a named location (e.g., "Eiffel Tower")

**Prerequisites**:
- Attachment with provenance linked to named_location

**Setup**:
1. Create named location: `INSERT INTO named_location (name, location_type, coordinates) VALUES ('Eiffel Tower', 'landmark', ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography)`
2. Link provenance to named location via prov_location

**Steps**:
1. Search near Eiffel Tower: `search_by_location({ lat: 48.8584, lon: 2.2945, radius_meters: 500 })`

**Expected Results**:
- Results include `location_name: "Eiffel Tower"`
- `location_name` field populated from named_location join

**Verification**:
- Named location data joined correctly

---

## Search by Time Range

### UAT-3B-006: Search by Time Range - Basic

**Description**: Search for memories captured within a time range

**Prerequisites**:
- Attachment with provenance record including capture_time

**Setup**:
1. Create attachment with capture time yesterday: `INSERT INTO file_provenance (attachment_id, capture_time, event_type) VALUES (<id>, tstzrange(NOW() - INTERVAL '1 day', NOW() - INTERVAL '1 day'), 'photo')`

**Steps**:
1. Search last 2 days: `search_by_timerange({ start: <now-2days>, end: <now> })`

**Expected Results**:
- Returns 1+ results
- Result `attachment_id` matches test attachment
- `capture_time_start` is within query range
- `event_type` is "photo"

**Verification**:
- Temporal range query works

**Store**: `yesterday_attachment_id`

---

### UAT-3B-007: Search by Time Range - No Results

**Description**: Search time range with no memories and verify empty result

**Prerequisites**: Test data from UAT-3B-006

**Steps**:
1. Search last year (excluding recent data): `search_by_timerange({ start: <1-year-ago>, end: <11-months-ago> })`

**Expected Results**:
- Returns empty array `[]`
- No error

**Verification**:
- Time range filtering works

---

### UAT-3B-008: Search by Time Range - Ordering

**Description**: Verify results ordered by capture time (earliest first)

**Prerequisites**: Multiple attachments with different capture times

**Setup**:
1. Create 3 attachments with capture times:
   - 3 days ago
   - 2 days ago
   - 1 day ago

**Steps**:
1. Search last 5 days: `search_by_timerange({ start: <5-days-ago>, end: <now> })`

**Expected Results**:
- Returns 3 results
- `results[0].capture_time_start` < `results[1].capture_time_start` < `results[2].capture_time_start`
- Results ordered chronologically (oldest first)

**Verification**:
- ORDER BY capture_time works

---

### UAT-3B-009: Search with Time Range Overlap

**Description**: Verify time range overlaps work with tstzrange

**Prerequisites**: Attachment with time range (not instant)

**Setup**:
1. Create provenance with time range: `tstzrange('2025-01-01 10:00:00+00', '2025-01-01 14:00:00+00')` (4-hour event)

**Steps**:
1. Search partially overlapping: `search_by_timerange({ start: '2025-01-01 12:00:00+00', end: '2025-01-01 16:00:00+00' })`

**Expected Results**:
- Returns the attachment (ranges overlap)
- PostgreSQL `&&` operator correctly detects overlap

**Verification**:
- tstzrange overlap detection works

---

## Combined Location + Time Search

### UAT-3B-010: Search by Location AND Time

**Description**: Search with both spatial and temporal filters

**Prerequisites**:
- Attachment with location AND capture time

**Setup**:
1. Create attachment at Eiffel Tower captured yesterday

**Steps**:
1. Search near Eiffel Tower in last 2 days: `search_by_location_and_time({ lat: 48.8584, lon: 2.2945, radius_meters: 1000, start: <2-days-ago>, end: <now> })`

**Expected Results**:
- Returns 1 result
- Both spatial and temporal filters satisfied

**Verification**:
- Combined query works

---

### UAT-3B-011: Combined Search - No Spatial Match

**Description**: Search with correct time but wrong location

**Prerequisites**: Test data from UAT-3B-010

**Steps**:
1. Search NYC (wrong location) in last 2 days: `search_by_location_and_time({ lat: 40.7128, lon: -74.0060, radius_meters: 1000, start: <2-days-ago>, end: <now> })`

**Expected Results**:
- Returns empty array
- Spatial filter rejects result

---

### UAT-3B-012: Combined Search - No Temporal Match

**Description**: Search with correct location but wrong time

**Prerequisites**: Test data from UAT-3B-010

**Steps**:
1. Search Eiffel Tower last year: `search_by_location_and_time({ lat: 48.8584, lon: 2.2945, radius_meters: 1000, start: <1-year-ago>, end: <11-months-ago> })`

**Expected Results**:
- Returns empty array
- Temporal filter rejects result

---

## Provenance Chain Retrieval

### UAT-3B-013: Get Full Provenance Chain

**Description**: Retrieve complete provenance data for a note (all attachments + metadata)

**Prerequisites**:
- Note with attachment that has full provenance (location, device, time)

**Setup**:
1. Create note with attachment
2. Create device: `INSERT INTO prov_agent_device (device_make, device_model) VALUES ('Apple', 'iPhone 15 Pro')`
3. Create full provenance record linking attachment, location, device, and time

**Steps**:
1. Get provenance: `get_memory_provenance(<note-id>)`

**Expected Results**:
- Returns `MemoryProvenance` object
- `note_id` matches query
- `files` array contains 1 `FileProvenanceRecord`
- File record includes:
  - `attachment_id`
  - `location` (lat, lon, location_name)
  - `device` (make, model)
  - `capture_time_start`, `capture_time_end`
  - `event_type`, `event_title`

**Verification**:
- Full provenance chain retrieved with all relationships

---

### UAT-3B-014: Get Provenance - Multiple Attachments

**Description**: Retrieve provenance for note with multiple attachments

**Prerequisites**:
- Note with 3 attachments, each with different provenance

**Steps**:
1. Get provenance: `get_memory_provenance(<note-id>)`

**Expected Results**:
- `files` array contains 3 elements
- Each file has distinct provenance data

**Verification**:
- Multi-attachment provenance aggregation works

---

### UAT-3B-015: Get Provenance - Partial Data

**Description**: Retrieve provenance when only some fields are present

**Prerequisites**:
- Attachment with location but no device

**Steps**:
1. Get provenance: `get_memory_provenance(<note-id>)`

**Expected Results**:
- Returns provenance with `location` populated
- `device` is None/null
- No error on missing optional data

**Verification**:
- Graceful handling of partial provenance data

---

### UAT-3B-016: Get Provenance - No Attachments

**Description**: Query provenance for note with no attachments

**Prerequisites**:
- Note without attachments

**Steps**:
1. Get provenance: `get_memory_provenance(<note-id>)`

**Expected Results**:
- Returns None/null OR empty `files` array
- No error

---

## Error Handling

### UAT-3B-017: Search with Invalid Coordinates

**Description**: Search with out-of-range latitude/longitude

**Prerequisites**: None

**Steps**:
1. Search with invalid lat: `search_by_location({ lat: 200.0, lon: 0.0, radius_meters: 1000 })`
2. Search with invalid lon: `search_by_location({ lat: 0.0, lon: 300.0, radius_meters: 1000 })`

**Expected Results**:
- Returns error with status 400
- Error message: "Invalid coordinates" or similar
- Latitude must be -90 to 90
- Longitude must be -180 to 180

**Verification**:
- Input validation works

---

### UAT-3B-018: Search with Negative Radius

**Description**: Attempt search with negative radius

**Prerequisites**: None

**Steps**:
1. Search with negative radius: `search_by_location({ lat: 48.8584, lon: 2.2945, radius_meters: -1000 })`

**Expected Results**:
- Returns error with status 400
- Error message: "Radius must be positive"

---

### UAT-3B-019: Search with Invalid Time Range

**Description**: Search with end time before start time

**Prerequisites**: None

**Steps**:
1. Search with inverted range: `search_by_timerange({ start: <now>, end: <yesterday> })`

**Expected Results**:
- Either: Returns empty array OR returns 400 error
- No crash

---

## Empty Result Handling

### UAT-3B-020: Search New Database - No Data

**Description**: Search empty database and verify graceful empty result

**Prerequisites**: Fresh database or database with no provenance records

**Steps**:
1. Search by location: `search_by_location({ lat: 0.0, lon: 0.0, radius_meters: 10000 })`
2. Search by time: `search_by_timerange({ start: <1-year-ago>, end: <now> })`

**Expected Results**:
- Both return empty arrays `[]`
- No SQL errors
- No null pointer exceptions

**Verification**:
- Empty database handled gracefully

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| UAT-3B-000 | Verify PostGIS Schema | |
| UAT-3B-001 | Search Near Location Basic | |
| UAT-3B-002 | Search No Spatial Results | |
| UAT-3B-003 | Search Large Radius | |
| UAT-3B-004 | Verify Distance Ordering | |
| UAT-3B-005 | Search Named Location | |
| UAT-3B-006 | Search Time Range Basic | |
| UAT-3B-007 | Search No Temporal Results | |
| UAT-3B-008 | Search Time Ordering | |
| UAT-3B-009 | Search Time Range Overlap | |
| UAT-3B-010 | Combined Location + Time | |
| UAT-3B-011 | Combined No Spatial Match | |
| UAT-3B-012 | Combined No Temporal Match | |
| UAT-3B-013 | Get Full Provenance Chain | |
| UAT-3B-014 | Get Provenance Multiple Files | |
| UAT-3B-015 | Get Provenance Partial Data | |
| UAT-3B-016 | Get Provenance No Attachments | |
| UAT-3B-017 | Search Invalid Coordinates | |
| UAT-3B-018 | Search Negative Radius | |
| UAT-3B-019 | Search Invalid Time Range | |
| UAT-3B-020 | Search Empty Database | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:
- This phase requires PostGIS extension and W3C PROV schema (migration 20260204100000)
- Some tests require manual creation of test data with GPS coordinates
- Search results are limited to 100 by default
- Distance calculations use PostGIS ST_Distance with geography type (meters)
- Time ranges use PostgreSQL tstzrange with overlap operator (&&)

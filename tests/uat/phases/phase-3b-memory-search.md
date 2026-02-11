# UAT Phase 3B: Memory Search (Temporal-Spatial)

**Purpose**: Verify temporal-spatial memory search capabilities for file provenance queries
**Duration**: ~15 minutes
**Prerequisites**: Phase 1 seed data exists, PostGIS extension enabled, W3C PROV schema migrated, test data generated. If upstream attachment uploads failed, still attempt each test and record failures.
**Critical**: Yes (100% pass required)
**Tools Tested**: `search_memories_by_location`, `search_memories_by_time`, `search_memories_combined`, `get_memory_provenance`, `create_provenance_location`, `create_named_location`, `create_provenance_device`, `create_file_provenance`, `create_note_provenance`, `create_note`, `upload_attachment`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

> **Provenance Creation via MCP**: This phase uses `create_provenance_location`, `create_named_location`, `create_provenance_device`, and `create_file_provenance` MCP tools for provenance test data setup (implemented in [#261](https://git.integrolabs.net/Fortemi/fortemi/issues/261)). No raw SQL is needed.

> **Test Data**: GPS-tagged images for provenance testing in `tests/uat/data/provenance/`:
> `paris-eiffel-tower.jpg` (48.8584N, 2.2945E), `newyork-statue-liberty.jpg` (40.6892N, 74.0445W),
> `tokyo-shibuya.jpg` (35.6595N, 139.7004E), `dated-2020-01-01.jpg`, `dated-2025-12-31.jpg`.
> Generate with: `cd tests/uat/data/scripts && ./generate-test-data.sh`

---

## Prerequisites Check

### UAT-3B-000: Verify PostGIS and PROV Schema

**MCP Tool**: `health_check`, `search_memories_by_location`

**Description**: Verify PostGIS extension and W3C PROV temporal-spatial schema are available via MCP tools

**Prerequisites**: MCP connection active

**Steps**:
1. System health: `health_check()`
2. Spatial query smoke test: `search_memories_by_location({ lat: 0.0, lon: 0.0, radius: 1 })`

**Expected Results**:
- `health_check` returns status without database errors (PostGIS must be loaded for the API to start)
- `search_memories_by_location` returns empty array `[]` (not an error), confirming PostGIS spatial queries and the `prov_location`/`provenance` tables are functional
- No 500 errors or "relation does not exist" failures

**Verification**:
- PostGIS extension installed (implicit: spatial query succeeds)
- W3C PROV schema migrated (implicit: provenance tables queried without error)

## Search by Location

### UAT-3B-001: Search Near Location - Basic

**MCP Tool**: `create_note`, `upload_attachment`, `search_memories_by_location`

**Description**: Search for memories captured near a specific location (Eiffel Tower)

**Prerequisites**:
- Test note with attachment created
- Attachment has provenance record with GPS coordinates (48.8584°N, 2.2945°E)

**Setup**:
1. Create test note: `create_note({ content: "# Paris Trip", tags: ["uat/memory-search"], revision_mode: "none" })`
2. Upload photo: `upload_attachment({ note_id: <note-id>, file_path: "tests/uat/data/provenance/paris-eiffel-tower.jpg", content_type: "image/jpeg" })`
3. Create location: `create_provenance_location({ latitude: 48.8584, longitude: 2.2945, source: "gps_exif", confidence: "high" })`
4. Create provenance: `create_file_provenance({ attachment_id: <attachment-id>, location_id: <location-id>, capture_time_start: "<now>", event_type: "photo" })`

**Steps**:
1. Search within 1km radius: `search_memories_by_location({ lat: 48.8584, lon: 2.2945, radius: 1000 })`

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

**MCP Tool**: `search_memories_by_location`

**Description**: Search far from any known locations and verify empty result

**Prerequisites**: Test data from UAT-3B-001 exists

**Steps**:
1. Search in NYC (far from Paris): `search_memories_by_location({ lat: 40.7128, lon: -74.0060, radius: 1000 })`

**Expected Results**:
- Returns empty array `[]`
- No error or crash
- `total: 0` (if pagination metadata included)

**Verification**:
- Query correctly filters by distance

---

### UAT-3B-003: Search with Large Radius

**MCP Tool**: `search_memories_by_location`

**Description**: Search with large radius covering multiple locations

**Prerequisites**: Multiple attachments with different locations

**Setup**:
1. Create 3 notes with attachments and provenance at different Paris locations using MCP tools:
   - Eiffel Tower: `create_provenance_location({ latitude: 48.8584, longitude: 2.2945, source: "gps_exif", confidence: "high" })` then `create_file_provenance({ attachment_id: <id>, location_id: <loc-id>, event_type: "photo" })`
   - Louvre: `create_provenance_location({ latitude: 48.8606, longitude: 2.3376, source: "gps_exif", confidence: "high" })` then `create_file_provenance({ attachment_id: <id>, location_id: <loc-id>, event_type: "photo" })`
   - Notre-Dame: `create_provenance_location({ latitude: 48.8530, longitude: 2.3499, source: "gps_exif", confidence: "high" })` then `create_file_provenance({ attachment_id: <id>, location_id: <loc-id>, event_type: "photo" })`

**Steps**:
1. Search from Eiffel Tower with 10km radius: `search_memories_by_location({ lat: 48.8584, lon: 2.2945, radius: 10000 })`

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

**MCP Tool**: `search_memories_by_location`

**Description**: Verify results are ordered by distance from search point (closest first)

**Prerequisites**: 3 attachments from UAT-3B-003

**Steps**:
1. Search from Eiffel Tower: `search_memories_by_location({ lat: 48.8584, lon: 2.2945, radius: 10000 })`
2. Check distances: `results[0].distance_m`, `results[1].distance_m`, `results[2].distance_m`

**Expected Results**:
- First result (Eiffel Tower) has smallest distance (< 100m)
- `results[0].distance_m` <= `results[1].distance_m` <= `results[2].distance_m`
- Distance increases monotonically

**Verification**:
- ORDER BY distance_m works correctly

---

### UAT-3B-005: Search with Named Location

**MCP Tool**: `search_memories_by_location`

**Description**: Search for memories at a named location (e.g., "Eiffel Tower")

**Prerequisites**:
- Attachment with provenance linked to named_location

**Setup**:
1. Create named location: `create_named_location({ name: "Eiffel Tower", location_type: "poi", latitude: 48.8584, longitude: 2.2945, locality: "Paris", country: "France" })`
2. Create location linked to named location: `create_provenance_location({ latitude: 48.8584, longitude: 2.2945, source: "gps_exif", confidence: "high", named_location_id: <named-loc-id> })`
3. Create file provenance linking to the location

**Steps**:
1. Search near Eiffel Tower: `search_memories_by_location({ lat: 48.8584, lon: 2.2945, radius: 500 })`

**Expected Results**:
- Results include `location_name: "Eiffel Tower"`
- `location_name` field populated from named_location join

**Verification**:
- Named location data joined correctly

---

## Search by Time Range

### UAT-3B-006: Search by Time Range - Basic

**MCP Tool**: `search_memories_by_time`

**Description**: Search for memories captured within a time range

**Prerequisites**:
- Attachment with provenance record including capture_time

**Setup**:
1. Create note with attachment, then create provenance with capture time yesterday: `create_file_provenance({ attachment_id: <id>, capture_time_start: "<yesterday>", event_type: "photo" })`

**Steps**:
1. Search last 2 days: `search_memories_by_time({ start: <now-2days>, end: <now> })`

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

**MCP Tool**: `search_memories_by_time`

**Description**: Search time range with no memories and verify empty result

**Prerequisites**: Test data from UAT-3B-006

**Steps**:
1. Search last year (excluding recent data): `search_memories_by_time({ start: <1-year-ago>, end: <11-months-ago> })`

**Expected Results**:
- Returns empty array `[]`
- No error

**Verification**:
- Time range filtering works

---

### UAT-3B-008: Search by Time Range - Ordering

**MCP Tool**: `search_memories_by_time`

**Description**: Verify results ordered by capture time (earliest first)

**Prerequisites**: Multiple attachments with different capture times

**Setup**:
1. Create 3 notes with attachments, each with different capture times via `create_file_provenance`:
   - `create_file_provenance({ attachment_id: <id1>, capture_time_start: "<3-days-ago>", event_type: "photo" })`
   - `create_file_provenance({ attachment_id: <id2>, capture_time_start: "<2-days-ago>", event_type: "photo" })`
   - `create_file_provenance({ attachment_id: <id3>, capture_time_start: "<1-day-ago>", event_type: "photo" })`

**Steps**:
1. Search last 5 days: `search_memories_by_time({ start: <5-days-ago>, end: <now> })`

**Expected Results**:
- Returns 3 results
- `results[0].capture_time_start` < `results[1].capture_time_start` < `results[2].capture_time_start`
- Results ordered chronologically (oldest first)

**Verification**:
- ORDER BY capture_time works

---

### UAT-3B-009: Search with Time Range Overlap

**MCP Tool**: `search_memories_by_time`

**Description**: Verify time range overlaps work with tstzrange

**Prerequisites**: Attachment with time range (not instant)

**Setup**:
1. Create provenance with time range (4-hour event): `create_file_provenance({ attachment_id: <id>, capture_time_start: "2025-01-01T10:00:00Z", capture_time_end: "2025-01-01T14:00:00Z", event_type: "recording" })`

**Steps**:
1. Search partially overlapping: `search_memories_by_time({ start: '2025-01-01 12:00:00+00', end: '2025-01-01 16:00:00+00' })`

**Expected Results**:
- Returns the attachment (ranges overlap)
- PostgreSQL `&&` operator correctly detects overlap

**Verification**:
- tstzrange overlap detection works

---

## Combined Location + Time Search

### UAT-3B-010: Search by Location AND Time

**MCP Tool**: `search_memories_combined`

**Description**: Search with both spatial and temporal filters

**Prerequisites**:
- Attachment with location AND capture time

**Setup**:
1. Create note with attachment, location at Eiffel Tower, and provenance with capture time yesterday (reuse data from prior tests or create fresh)

**Steps**:
1. Search near Eiffel Tower in last 2 days: `search_memories_combined({ lat: 48.8584, lon: 2.2945, radius: 1000, start: <2-days-ago>, end: <now> })`

**Expected Results**:
- Returns 1 result
- Both spatial and temporal filters satisfied

**Verification**:
- Combined query works

---

### UAT-3B-011: Combined Search - No Spatial Match

**MCP Tool**: `search_memories_combined`

**Description**: Search with correct time but wrong location

**Prerequisites**: Test data from UAT-3B-010

**Steps**:
1. Search NYC (wrong location) in last 2 days: `search_memories_combined({ lat: 40.7128, lon: -74.0060, radius: 1000, start: <2-days-ago>, end: <now> })`

**Expected Results**:
- Returns empty array
- Spatial filter rejects result

---

### UAT-3B-012: Combined Search - No Temporal Match

**MCP Tool**: `search_memories_combined`

**Description**: Search with correct location but wrong time

**Prerequisites**: Test data from UAT-3B-010

**Steps**:
1. Search Eiffel Tower last year: `search_memories_combined({ lat: 48.8584, lon: 2.2945, radius: 1000, start: <1-year-ago>, end: <11-months-ago> })`

**Expected Results**:
- Returns empty array
- Temporal filter rejects result

---

## Provenance Chain Retrieval

### UAT-3B-013: Get Full Provenance Chain

**MCP Tool**: `get_memory_provenance`

**Description**: Retrieve complete provenance data for a note (all attachments + metadata)

**Prerequisites**:
- Note with attachment that has full provenance (location, device, time)

**Setup**:
1. Create note with attachment
2. Create device: `create_provenance_device({ device_make: "Apple", device_model: "iPhone 15 Pro", device_os: "iOS", device_os_version: "17.2", software: "Camera", has_gps: true })`
3. Create location: `create_provenance_location({ latitude: 48.8584, longitude: 2.2945, source: "gps_exif", confidence: "high" })`
4. Create full provenance: `create_file_provenance({ attachment_id: <id>, location_id: <loc-id>, device_id: <dev-id>, capture_time_start: "<yesterday>", capture_timezone: "Europe/Paris", time_source: "exif", time_confidence: "high", event_type: "photo", event_title: "Eiffel Tower Visit" })`

**Steps**:
1. Get provenance: `get_memory_provenance({ note_id: <note-id> })`

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

**MCP Tool**: `get_memory_provenance`

**Description**: Retrieve provenance for note with multiple attachments

**Prerequisites**:
- Note with 3 attachments, each with different provenance

**Steps**:
1. Get provenance: `get_memory_provenance({ note_id: <note-id> })`

**Expected Results**:
- `files` array contains 3 elements
- Each file has distinct provenance data

**Verification**:
- Multi-attachment provenance aggregation works

---

### UAT-3B-015: Get Provenance - Partial Data

**MCP Tool**: `get_memory_provenance`

**Description**: Retrieve provenance when only some fields are present

**Prerequisites**:
- Attachment with location but no device

**Steps**:
1. Get provenance: `get_memory_provenance({ note_id: <note-id> })`

**Expected Results**:
- Returns provenance with `location` populated
- `device` is None/null
- No error on missing optional data

**Verification**:
- Graceful handling of partial provenance data

---

### UAT-3B-016: Get Provenance - No Attachments

**MCP Tool**: `get_memory_provenance`

**Description**: Query provenance for note with no attachments

**Prerequisites**:
- Note without attachments

**Steps**:
1. Get provenance: `get_memory_provenance({ note_id: <note-id> })`

**Expected Results**:
- Returns None/null OR empty `files` array
- No error

---

## Error Handling

### UAT-3B-017: Search with Invalid Coordinates

**Isolation**: Required — negative test expects error response

**MCP Tool**: `search_memories_by_location`

**Description**: Search with out-of-range latitude/longitude

**Prerequisites**: None

**Steps**:
1. Search with invalid lat: `search_memories_by_location({ lat: 200.0, lon: 0.0, radius: 1000 })`
2. Search with invalid lon: `search_memories_by_location({ lat: 0.0, lon: 300.0, radius: 1000 })`

**Expected Results**:
- Returns error with status 400
- Error message: "Invalid coordinates" or similar
- Latitude must be -90 to 90
- Longitude must be -180 to 180

**Verification**:
- Input validation works

---

### UAT-3B-018: Search with Negative Radius

**Isolation**: Required — negative test expects error response

**MCP Tool**: `search_memories_by_location`

**Description**: Attempt search with negative radius

**Prerequisites**: None

**Steps**:
1. Search with negative radius: `search_memories_by_location({ lat: 48.8584, lon: 2.2945, radius: -1000 })`

**Expected Results**:
- Returns error with status 400
- Error message: "Radius must be positive"

---

### UAT-3B-019a: Invalid Time Range — Return Empty

**MCP Tool**: `search_memories_by_time`

**Description**: Search with end time before start time. Verify API returns empty results.

**Prerequisites**: None

**Steps**:
1. Search with inverted range: `search_memories_by_time({ start: <now>, end: <yesterday> })`

**Pass Criteria**: Returns empty results array. No crash or error. API treats inverted range as valid but matching nothing.

---

## Empty Result Handling

### UAT-3B-020: Search New Database - No Data

**MCP Tool**: `search_memories_by_location`, `search_memories_by_time`

**Description**: Search empty database and verify graceful empty result

**Prerequisites**: Fresh database or database with no provenance records

**Steps**:
1. Search by location: `search_memories_by_location({ lat: 0.0, lon: 0.0, radius: 10000 })`
2. Search by time: `search_memories_by_time({ start: <1-year-ago>, end: <now> })`

**Expected Results**:
- Both return empty arrays `[]`
- No SQL errors
- No null pointer exceptions

**Verification**:
- Empty database handled gracefully

---

## Note-Level Provenance

### UAT-3B-021: Create Note Provenance

**MCP Tool**: `create_note`, `create_provenance_location`, `create_note_provenance`

**Description**: Create spatial-temporal provenance directly on a note (no attachment)

**Prerequisites**: None

**Setup**:
1. Create test note: `create_note({ content: "# Meeting at Paris Office", tags: ["uat/note-provenance"], revision_mode: "none" })`
2. Create location: `create_provenance_location({ latitude: 48.8566, longitude: 2.3522, source: "gps", confidence: "high" })`

**Steps**:
1. Create note provenance: `create_note_provenance({ note_id: <note-id>, location_id: <loc-id>, capture_time_start: "<now>", capture_timezone: "Europe/Paris", time_source: "manual", time_confidence: "exact", event_type: "created", event_title: "Paris office meeting" })`

**Expected Results**:
- Returns `{ id: <provenance-uuid> }` with status 201
- Provenance record created in `provenance` table with `note_id` set and `attachment_id` NULL

**Verification**:
- Note provenance creation works via MCP

**Store**: `note_prov_note_id`, `note_prov_id`

---

### UAT-3B-022: Get Memory Provenance with Note Provenance

**MCP Tool**: `get_memory_provenance`

**Description**: Verify `get_memory_provenance` returns note-level provenance in the `note` field

**Prerequisites**: UAT-3B-021 completed

**Steps**:
1. Get provenance: `get_memory_provenance({ note_id: <note_prov_note_id> })`

**Expected Results**:
- Returns `MemoryProvenance` object
- `files` array is empty (no attachments)
- `note` field is populated with:
  - `note_id` matches
  - `event_type` is "created"
  - `event_title` is "Paris office meeting"
  - `time_source` is "manual"
  - `time_confidence` is "exact"

**Verification**:
- Note provenance included in get_memory_provenance response

---

### UAT-3B-023: Note Provenance Uniqueness

**MCP Tool**: `create_note_provenance`

**Description**: Verify only one provenance record per note (unique index)

**Prerequisites**: UAT-3B-021 completed (note already has provenance)

**Steps**:
1. Attempt second provenance: `create_note_provenance({ note_id: <note_prov_note_id>, event_type: "modified" })`

**Expected Results**:
- Returns error (409 Conflict or 500 with unique violation)
- Original provenance record unchanged

**Verification**:
- Unique index `idx_provenance_note_id` enforced

---

### UAT-3B-024: Note Provenance in Search Results

**MCP Tool**: `search_memories_by_location`

**Description**: Verify note-level provenance appears in spatial search results

**Prerequisites**: UAT-3B-021 completed (note has location provenance)

**Steps**:
1. Search near Paris: `search_memories_by_location({ lat: 48.8566, lon: 2.3522, radius: 1000 })`

**Expected Results**:
- Results include the note from UAT-3B-021
- Result has `note_id` set (from provenance `note_id`)
- `attachment_id` is null for this result
- `event_type` is "created"

**Verification**:
- Note provenance participates in spatial searches

---

### UAT-3B-025: Note Provenance in Time Search

**MCP Tool**: `search_memories_by_time`

**Description**: Verify note-level provenance appears in temporal search results

**Prerequisites**: UAT-3B-021 completed

**Steps**:
1. Search recent time range: `search_memories_by_time({ start: <1-hour-ago>, end: <now> })`

**Expected Results**:
- Results include the note from UAT-3B-021
- `note_id` is set
- `event_type` is "created"

**Verification**:
- Note provenance participates in temporal searches

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| UAT-3B-000 | Verify PostGIS Schema | `health_check`, `search_memories_by_location` | |
| UAT-3B-001 | Search Near Location Basic | `create_note`, `upload_attachment`, `search_memories_by_location` | |
| UAT-3B-002 | Search No Spatial Results | `search_memories_by_location` | |
| UAT-3B-003 | Search Large Radius | `search_memories_by_location` | |
| UAT-3B-004 | Verify Distance Ordering | `search_memories_by_location` | |
| UAT-3B-005 | Search Named Location | `search_memories_by_location` | |
| UAT-3B-006 | Search Time Range Basic | `search_memories_by_time` | |
| UAT-3B-007 | Search No Temporal Results | `search_memories_by_time` | |
| UAT-3B-008 | Search Time Ordering | `search_memories_by_time` | |
| UAT-3B-009 | Search Time Range Overlap | `search_memories_by_time` | |
| UAT-3B-010 | Combined Location + Time | `search_memories_combined` | |
| UAT-3B-011 | Combined No Spatial Match | `search_memories_combined` | |
| UAT-3B-012 | Combined No Temporal Match | `search_memories_combined` | |
| UAT-3B-013 | Get Full Provenance Chain | `get_memory_provenance` | |
| UAT-3B-014 | Get Provenance Multiple Files | `get_memory_provenance` | |
| UAT-3B-015 | Get Provenance Partial Data | `get_memory_provenance` | |
| UAT-3B-016 | Get Provenance No Attachments | `get_memory_provenance` | |
| UAT-3B-017 | Search Invalid Coordinates | `search_memories_by_location` | |
| UAT-3B-018 | Search Negative Radius | `search_memories_by_location` | |
| UAT-3B-019a | Invalid Time Range Empty | `search_memories_by_time` | |
| UAT-3B-020 | Search Empty Database | `search_memories_by_location`, `search_memories_by_time` | |
| UAT-3B-021 | Create Note Provenance | `create_note`, `create_provenance_location`, `create_note_provenance` | |
| UAT-3B-022 | Get Provenance with Note | `get_memory_provenance` | |
| UAT-3B-023 | Note Provenance Uniqueness | `create_note_provenance` | |
| UAT-3B-024 | Note Provenance in Spatial Search | `search_memories_by_location` | |
| UAT-3B-025 | Note Provenance in Time Search | `search_memories_by_time` | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:
- This phase requires PostGIS extension and W3C PROV schema (migration 20260204100000)
- Provenance test data is created via MCP tools (`create_provenance_location`, `create_named_location`, `create_provenance_device`, `create_file_provenance`) — no raw SQL needed
- Search results are limited to 100 by default
- Distance calculations use PostGIS ST_Distance with geography type (meters)
- Time ranges use PostgreSQL tstzrange with overlap operator (&&)
- Device registration deduplicates on (make, model) — same device returns same ID

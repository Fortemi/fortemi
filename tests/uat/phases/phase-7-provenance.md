# UAT Phase 7: Provenance Tracking

**Purpose**: Validate provenance recording for locations, devices, files, and note context through MCP tools.

**Duration**: ~5 minutes

**Prerequisites**:
- Phase 1 completed (notes exist for linking provenance)
- System supports spatial queries (PostGIS enabled)

**Tools Tested**:
- `record_provenance` (5 actions: location, named_location, device, file, note)
- `search` (spatial action, to verify provenance-enriched spatial search)

> **MCP-First Requirement**: Every test in this phase MUST use MCP tool calls exclusively. No direct HTTP requests, no curl commands. This validates the agent-first workflow that real AI assistants will experience.

---

## Test Cases

### PROV-001: Create Location Provenance

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "location",
    latitude: 40.7128,
    longitude: -74.0060,
    source: "user_manual",
    confidence: "medium"
  }
});
```

**Expected Response**:
- Location provenance record created
- Returns location ID
- Latitude/longitude stored correctly

**Pass Criteria**:
- [ ] Response contains `id` field
- [ ] Response includes `latitude` and `longitude`
- [ ] Source and confidence stored correctly
- [ ] Location queryable via spatial search

**Store**: `location_id_nyc` for note linking tests

---

### PROV-002: Create Named Location Provenance

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "named_location",
    name: "UAT Test Place",
    location_type: "poi",
    latitude: 51.5074,
    longitude: -0.1278
  }
});
```

**Expected Response**:
- Named location created with human-readable name
- Location type (POI) recorded
- Coordinates stored

**Pass Criteria**:
- [ ] Response contains `id` and `name` fields
- [ ] Location type is "poi"
- [ ] Coordinates match input
- [ ] Name is searchable/retrievable

**Store**: `location_id_london` for cross-location tests

---

### PROV-003: Create Device Provenance

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "device",
    device_make: "TestCo",
    device_model: "UAT-Model"
  }
});
```

**Expected Response**:
- Device provenance record created
- Device make and model stored

**Pass Criteria**:
- [ ] Response contains `id` field
- [ ] Device make is "TestCo"
- [ ] Device model is "UAT-Model"
- [ ] Record linkable to notes

**Store**: `device_id` for note provenance tests

---

### PROV-004: Create File Provenance

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "file",
    original_filename: "test-photo.jpg",
    mime_type: "image/jpeg",
    file_size: 1024
  }
});
```

**Expected Response**:
- File provenance record created
- Filename, MIME type, and size stored

**Pass Criteria**:
- [ ] Response contains `id` field
- [ ] Filename is "test-photo.jpg"
- [ ] MIME type is "image/jpeg"
- [ ] File size is 1024 bytes

**Store**: `file_provenance_id` for note linkage

---

### PROV-005: Create Note Provenance (Location + Time)

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "note",
    note_id: "<note_id_from_phase_1>",
    location_id: location_id_nyc,
    capture_time_start: "2026-01-15T10:00:00Z",
    time_source: "user_manual",
    time_confidence: "high"
  }
});
```

**Expected Response**:
- Note provenance linking note to location and time
- Temporal and spatial metadata enriched

**Pass Criteria**:
- [ ] Response contains provenance record ID
- [ ] Note ID matches input
- [ ] Location ID matches PROV-001
- [ ] Capture time stored correctly
- [ ] Time source and confidence recorded

**Store**: `provenance_note_id` for spatial search verification

---

### PROV-006: Create Note Provenance with Device

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "note",
    note_id: "<another_note_id_from_phase_1>",
    device_id: device_id,
    capture_time_start: "2026-01-15T14:30:00Z",
    time_source: "device_clock",
    time_confidence: "high"
  }
});
```

**Expected Response**:
- Note provenance with device linkage
- Device metadata enriches note context

**Pass Criteria**:
- [ ] Response contains provenance record ID
- [ ] Device ID matches PROV-003
- [ ] Capture time and source stored
- [ ] Device information queryable via note

---

### PROV-007: Create Note Provenance with File

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "note",
    note_id: "<third_note_id_from_phase_1>",
    file_provenance_id: file_provenance_id,
    capture_time_start: "2026-01-15T16:45:00Z",
    time_source: "exif",
    time_confidence: "medium"
  }
});
```

**Expected Response**:
- Note provenance with file origin metadata
- File details linked to note

**Pass Criteria**:
- [ ] Response contains provenance record ID
- [ ] File provenance ID matches PROV-004
- [ ] Time source is "exif"
- [ ] File metadata accessible via note

---

### PROV-008: Verify Spatial Search Finds Provenance Note

**MCP Tool**: `search` (spatial action)

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "spatial",
    lat: 40.7128,
    lon: -74.0060,
    radius: 1000, // 1km radius
    limit: 10
  }
});
```

**Expected Response**:
- Search results include note from PROV-005
- Spatial filtering works with provenance metadata

**Pass Criteria**:
- [ ] Response contains results array
- [ ] Note with location provenance appears in results
- [ ] Distance calculation is accurate
- [ ] Results sorted by proximity

---

### PROV-009: Invalid Provenance Action

**Isolation**: Required

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "invalid_action"
  }
});
```

**Expected Response**:
- Error indicating invalid action
- Clear error message listing valid actions

**Pass Criteria**:
- [ ] Tool call fails with validation error
- [ ] Error message mentions invalid action
- [ ] No partial record created

---

### PROV-010: Missing Required Fields for Location

**Isolation**: Required

**MCP Tool**: `record_provenance`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "location"
    // Missing latitude and longitude
  }
});
```

**Expected Response**:
- Error indicating missing required fields
- Clear validation message

**Pass Criteria**:
- [ ] Tool call fails with validation error
- [ ] Error message mentions missing latitude/longitude
- [ ] No incomplete record created

---

## Phase Summary

| Metric | Target | Actual |
|--------|--------|--------|
| Tests Executed | 10 | ___ |
| Tests Passed | 10 | ___ |
| Tests Failed | 0 | ___ |
| Duration | ~5 min | ___ |
| Location Provenance Validated | ✓ | ___ |
| Device Provenance Validated | ✓ | ___ |
| File Provenance Validated | ✓ | ___ |
| Note Linkage Validated | ✓ | ___ |
| Spatial Query Integration | ✓ | ___ |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- Provenance enables temporal-spatial queries for context-rich search
- Location provenance powers the spatial search capabilities
- Device and file provenance support forensic traceability
- All provenance records are optional enrichments (notes work without them)
- Time confidence levels: low, medium, high, verified
- Location sources: gps, user_manual, ip_geolocation, exif, inferred

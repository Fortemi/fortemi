# Memory Search API

This guide explains how to search for memories based on temporal and spatial context using file provenance data.

## Overview

Memory search enables temporal-spatial queries on file attachments (photos, videos, documents) based on when and where they were captured. Unlike semantic search which finds content based on meaning, memory search finds content based on **context**:

- **When** was this file created? (temporal search)
- **Where** was this file captured? (spatial search)
- **What device** captured it? (device provenance)
- **What event** was it part of? (event metadata)

Memory search is built on the [W3C PROV](https://www.w3.org/TR/prov-dm/) temporal-spatial extension and uses PostGIS for efficient geographic queries.

### Key Concepts

| Concept | Description |
|---------|-------------|
| **File Provenance** | Complete context for an attachment (location, time, device, event) |
| **Capture Time** | When the file was created (not uploaded), from EXIF or file metadata |
| **Location** | Geographic coordinates (latitude/longitude) with accuracy information |
| **Device** | Information about the device that captured the content |
| **Event** | Semantic context (e.g., "photo", "video", "meeting recording") |

## Use Cases

### Find Photos from a Trip

"Show me all photos I took in Paris during my 2025 vacation"

```bash
# Search within 10km of the Eiffel Tower, January 2025
curl "http://localhost:3000/api/v1/memories/search?lat=48.8584&lon=2.2945&radius=10000&start=2025-01-01T00:00:00Z&end=2025-02-01T00:00:00Z"
```

### Find Nearby Memories

"What photos did I take near my current location?"

```bash
# Search within 1km of home
curl "http://localhost:3000/api/v1/memories/search?lat=40.7128&lon=-74.0060&radius=1000"
```

### Get Memory Timeline

"Show me everything I captured last week"

```bash
# Search by time range
curl "http://localhost:3000/api/v1/memories/search?start=2026-01-24T00:00:00Z&end=2026-01-31T23:59:59Z"
```

### Get Full Provenance Chain

"Where and when was this note's attachment captured?"

```bash
# Get complete provenance for a note
curl "http://localhost:3000/api/v1/notes/{note_id}/memory-provenance"
```

## API Endpoints

### Search by Location

Find memories captured near a geographic point.

```http
GET /api/v1/memories/search?lat={latitude}&lon={longitude}&radius={meters}
```

**Parameters:**

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `lat` | float | Yes | Latitude in decimal degrees (-90 to 90) |
| `lon` | float | Yes | Longitude in decimal degrees (-180 to 180) |
| `radius` | float | Yes | Search radius in meters (e.g., 1000 for 1km) |
| `limit` | integer | No | Max results (default: 100, max: 100) |

**Response:**

```json
{
  "results": [
    {
      "provenance_id": "uuid",
      "attachment_id": "uuid",
      "note_id": "uuid",
      "filename": "IMG_1234.jpg",
      "content_type": "image/jpeg",
      "distance_m": 245.7,
      "capture_time_start": "2026-01-15T14:30:00Z",
      "capture_time_end": "2026-01-15T14:30:00Z",
      "location_name": "Eiffel Tower",
      "event_type": "photo"
    }
  ],
  "total": 1,
  "query": {
    "lat": 48.8584,
    "lon": 2.2945,
    "radius_m": 1000
  }
}
```

**Sorting:** Results are ordered by distance (closest first).

**Example:**

```bash
# Find photos within 5km of the Louvre Museum
curl "http://localhost:3000/api/v1/memories/search?lat=48.8606&lon=2.3376&radius=5000" \
  -H "Authorization: Bearer your_token"
```

### Search by Time Range

Find memories captured within a time window.

```http
GET /api/v1/memories/search?start={iso8601_start}&end={iso8601_end}
```

**Parameters:**

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `start` | datetime | Yes | Start of time range (ISO 8601 format) |
| `end` | datetime | Yes | End of time range (ISO 8601 format) |
| `limit` | integer | No | Max results (default: 100, max: 100) |

**Response:**

```json
{
  "results": [
    {
      "provenance_id": "uuid",
      "attachment_id": "uuid",
      "note_id": "uuid",
      "capture_time_start": "2026-01-15T14:30:00Z",
      "capture_time_end": "2026-01-15T14:30:00Z",
      "event_type": "photo",
      "location_name": "Central Park"
    }
  ],
  "total": 1,
  "query": {
    "start": "2026-01-01T00:00:00Z",
    "end": "2026-01-31T23:59:59Z"
  }
}
```

**Sorting:** Results are ordered by capture time (earliest first).

**Example:**

```bash
# Find all memories from January 2026
curl "http://localhost:3000/api/v1/memories/search?start=2026-01-01T00:00:00Z&end=2026-02-01T00:00:00Z" \
  -H "Authorization: Bearer your_token"
```

### Search by Location and Time

Intersection query combining location and time filters.

```http
GET /api/v1/memories/search?lat={latitude}&lon={longitude}&radius={meters}&start={iso8601_start}&end={iso8601_end}
```

**Parameters:** Combination of location and time parameters (all required).

**Response:** Same as location search (includes distance_m field).

**Sorting:** Results are ordered by distance (closest first).

**Example:**

```bash
# Find photos taken in Paris during January 2025
curl "http://localhost:3000/api/v1/memories/search?lat=48.8584&lon=2.2945&radius=10000&start=2025-01-01T00:00:00Z&end=2025-02-01T00:00:00Z" \
  -H "Authorization: Bearer your_token"
```

### Get Memory Provenance

Retrieve complete provenance chain for a note's file attachments.

```http
GET /api/v1/notes/{note_id}/memory-provenance
```

**Parameters:**

| Param | Type | Required | Description |
|-------|------|----------|-------------|
| `note_id` | uuid | Yes | Note ID (path parameter) |

**Response:**

```json
{
  "note_id": "uuid",
  "files": [
    {
      "id": "provenance_uuid",
      "attachment_id": "attachment_uuid",
      "capture_time_start": "2026-01-15T14:30:00Z",
      "capture_time_end": "2026-01-15T14:30:00Z",
      "capture_timezone": "Europe/Paris",
      "capture_duration_seconds": null,
      "time_source": "exif",
      "time_confidence": "high",
      "location": {
        "id": "location_uuid",
        "latitude": 48.8584,
        "longitude": 2.2945,
        "horizontal_accuracy_m": 10.0,
        "altitude_m": 35.0,
        "vertical_accuracy_m": 5.0,
        "heading_degrees": 180.0,
        "speed_mps": 0.0,
        "named_location_id": "named_loc_uuid",
        "named_location_name": "Eiffel Tower",
        "source": "gps_exif",
        "confidence": "high"
      },
      "device": {
        "id": "device_uuid",
        "device_make": "Apple",
        "device_model": "iPhone 15 Pro",
        "device_os": "iOS",
        "device_os_version": "17.2",
        "software": "Camera",
        "software_version": "17.2",
        "device_name": "My iPhone"
      },
      "event_type": "photo",
      "event_title": "Eiffel Tower Visit",
      "event_description": "Sunset view from Trocadéro",
      "user_corrected": false,
      "created_at": "2026-01-15T14:35:00Z"
    }
  ]
}
```

**Returns:** `null` if the note has no file attachments with provenance data.

**Example:**

```bash
# Get provenance for a note
curl "http://localhost:3000/api/v1/notes/550e8400-e29b-41d4-a716-446655440000/memory-provenance" \
  -H "Authorization: Bearer your_token"
```

## Memory Provenance Fields

### Location Details

| Field | Type | Description |
|-------|------|-------------|
| `latitude` | float | Latitude in decimal degrees (-90 to 90) |
| `longitude` | float | Longitude in decimal degrees (-180 to 180) |
| `horizontal_accuracy_m` | float | GPS accuracy in meters (typically ±5-10m for high confidence) |
| `altitude_m` | float | Altitude in meters above sea level |
| `vertical_accuracy_m` | float | Altitude accuracy in meters |
| `heading_degrees` | float | Compass heading (0-359, 0=North) |
| `speed_mps` | float | Speed in meters per second (for video capture) |
| `named_location_name` | string | Semantic place name (e.g., "Home", "Office", "Paris") |
| `source` | string | `gps_exif`, `device_api`, `user_manual`, `geocoded`, `ai_estimated`, `unknown` |
| `confidence` | string | `high` (GPS ±10m), `medium` (WiFi ±100m), `low` (IP ±1km+), `unknown` |

### Device Information

| Field | Type | Description |
|-------|------|-------------|
| `device_make` | string | Manufacturer (e.g., "Apple", "Canon", "Samsung") |
| `device_model` | string | Model name (e.g., "iPhone 15 Pro", "EOS R5") |
| `device_os` | string | Operating system (e.g., "iOS", "Android") |
| `device_os_version` | string | OS version (e.g., "17.2", "Android 14") |
| `software` | string | Capturing software (e.g., "Camera", "Adobe Lightroom") |
| `software_version` | string | Software version |
| `device_name` | string | User-assigned device name (e.g., "My iPhone") |

### Temporal Context

| Field | Type | Description |
|-------|------|-------------|
| `capture_time_start` | datetime | Start of capture time (instant for photos) |
| `capture_time_end` | datetime | End of capture time (for video/audio duration) |
| `capture_timezone` | string | Original timezone (e.g., "Europe/Paris", "America/New_York") |
| `capture_duration_seconds` | float | Duration in seconds (for video/audio) |
| `time_source` | string | `exif` (EXIF metadata), `file_mtime` (file modification time), `user_manual`, `ai_estimated` |
| `time_confidence` | string | `high` (EXIF with GPS sync), `medium` (file mtime), `low` (estimated), `unknown` |

### Event Metadata

| Field | Type | Description |
|-------|------|-------------|
| `event_type` | string | `photo`, `video`, `audio`, `scan`, `screenshot`, `recording`, `unknown` |
| `event_title` | string | User-assigned or AI-generated event title |
| `event_description` | string | Detailed event description |
| `user_corrected` | boolean | Whether user manually corrected provenance data |

## PostGIS Integration

Memory search uses PostGIS for efficient spatial queries.

### Spatial Index

The `prov_location` table uses a GiST (Generalized Search Tree) index on the `point` geography column:

```sql
CREATE INDEX idx_prov_location_point ON prov_location USING GIST (point);
```

This enables fast nearest-neighbor and radius searches even with millions of locations.

### Coordinate System

All coordinates use **WGS84 (EPSG:4326)**, the standard GPS coordinate system:

- Latitude: -90 to 90 degrees (negative = South, positive = North)
- Longitude: -180 to 180 degrees (negative = West, positive = East)
- Altitude: meters above sea level

### Distance Calculation

Distances are calculated using PostGIS `ST_Distance` with the `geography` type, which accounts for Earth's curvature:

```sql
ST_Distance(
    location_point,
    ST_SetSRID(ST_MakePoint(query_lon, query_lat), 4326)::geography
)
```

This returns accurate distances in meters for any points on Earth.

### Query Performance

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Radius search | O(log N) | Uses GiST index |
| Time range search | O(log N) | Uses GiST index on tstzrange |
| Combined search | O(log N) | Both indexes used |

The GiST indexes provide logarithmic query performance, so search remains fast even with millions of memories.

## API Examples

### Example 1: Find Photos from Paris Trip

Find all photos captured within 5km of central Paris during January 2025:

```bash
curl -X GET "http://localhost:3000/api/v1/memories/search" \
  -H "Authorization: Bearer mm_key_xxx" \
  -G \
  --data-urlencode "lat=48.8566" \
  --data-urlencode "lon=2.3522" \
  --data-urlencode "radius=5000" \
  --data-urlencode "start=2025-01-01T00:00:00Z" \
  --data-urlencode "end=2025-02-01T00:00:00Z"
```

**Response:**

```json
{
  "results": [
    {
      "provenance_id": "550e8400-e29b-41d4-a716-446655440001",
      "attachment_id": "550e8400-e29b-41d4-a716-446655440002",
      "note_id": "550e8400-e29b-41d4-a716-446655440003",
      "filename": "IMG_1234.jpg",
      "content_type": "image/jpeg",
      "distance_m": 1247.3,
      "capture_time_start": "2025-01-15T14:30:00Z",
      "capture_time_end": "2025-01-15T14:30:00Z",
      "location_name": "Eiffel Tower",
      "event_type": "photo"
    },
    {
      "provenance_id": "550e8400-e29b-41d4-a716-446655440004",
      "attachment_id": "550e8400-e29b-41d4-a716-446655440005",
      "note_id": "550e8400-e29b-41d4-a716-446655440006",
      "filename": "IMG_1235.jpg",
      "content_type": "image/jpeg",
      "distance_m": 2134.8,
      "capture_time_start": "2025-01-16T10:15:00Z",
      "capture_time_end": "2025-01-16T10:15:00Z",
      "location_name": "Louvre Museum",
      "event_type": "photo"
    }
  ],
  "total": 2,
  "query": {
    "lat": 48.8566,
    "lon": 2.3522,
    "radius_m": 5000,
    "start": "2025-01-01T00:00:00Z",
    "end": "2025-02-01T00:00:00Z"
  }
}
```

### Example 2: Find Memories Within 1km of Home

Find all memories captured within 1km of your home location:

```bash
curl -X GET "http://localhost:3000/api/v1/memories/search" \
  -H "Authorization: Bearer mm_key_xxx" \
  -G \
  --data-urlencode "lat=37.7749" \
  --data-urlencode "lon=-122.4194" \
  --data-urlencode "radius=1000"
```

### Example 3: Timeline for Last Month

Find all memories from the past month:

```bash
# Calculate date 30 days ago
START_DATE=$(date -u -d '30 days ago' +%Y-%m-%dT%H:%M:%SZ)
END_DATE=$(date -u +%Y-%m-%dT%H:%M:%SZ)

curl -X GET "http://localhost:3000/api/v1/memories/search" \
  -H "Authorization: Bearer mm_key_xxx" \
  -G \
  --data-urlencode "start=$START_DATE" \
  --data-urlencode "end=$END_DATE"
```

### Example 4: Get Full Provenance Chain

Get complete provenance information for a note with attachments:

```bash
NOTE_ID="550e8400-e29b-41d4-a716-446655440000"

curl -X GET "http://localhost:3000/api/v1/notes/$NOTE_ID/memory-provenance" \
  -H "Authorization: Bearer your_token"
```

**Response:**

```json
{
  "note_id": "550e8400-e29b-41d4-a716-446655440000",
  "files": [
    {
      "id": "provenance_uuid",
      "attachment_id": "attachment_uuid",
      "capture_time_start": "2026-01-15T14:30:00Z",
      "capture_time_end": "2026-01-15T14:30:00Z",
      "capture_timezone": "Europe/Paris",
      "time_source": "exif",
      "time_confidence": "high",
      "location": {
        "id": "location_uuid",
        "latitude": 48.8584,
        "longitude": 2.2945,
        "horizontal_accuracy_m": 10.0,
        "altitude_m": 35.0,
        "named_location_name": "Eiffel Tower",
        "source": "gps_exif",
        "confidence": "high"
      },
      "device": {
        "id": "device_uuid",
        "device_make": "Apple",
        "device_model": "iPhone 15 Pro",
        "device_os": "iOS",
        "device_os_version": "17.2"
      },
      "event_type": "photo",
      "event_title": "Eiffel Tower Visit",
      "user_corrected": false,
      "created_at": "2026-01-15T14:35:00Z"
    }
  ]
}
```

### Example 5: Complex Query with jq Processing

Find photos from a specific camera within a region and format results:

```bash
curl -X GET "http://localhost:3000/api/v1/memories/search" \
  -H "Authorization: Bearer mm_key_xxx" \
  -G \
  --data-urlencode "lat=40.7128" \
  --data-urlencode "lon=-74.0060" \
  --data-urlencode "radius=5000" \
  | jq '.results[] | select(.content_type == "image/jpeg") | {filename, distance_m, capture_time: .capture_time_start}'
```

## Advanced Topics

### Named Locations

Named locations provide semantic place names for geographic coordinates. The system automatically resolves coordinates to named locations when available.

**Creating Named Locations:**

Named locations can be created via the API (future feature) or populated from reverse geocoding services.

**Location Types:**
- `home` - User's home address
- `work` - Workplace
- `poi` - Point of interest (landmarks, restaurants, etc.)
- `city` - City or town
- `region` - State, province, or region
- `country` - Country

### Time Confidence Levels

| Confidence | Source | Typical Accuracy |
|------------|--------|------------------|
| **high** | EXIF with GPS time sync | ±1 second |
| **medium** | File modification time | ±1 minute to hours |
| **low** | AI estimation or user guess | ±days to weeks |
| **unknown** | No temporal information | N/A |

### Location Confidence Levels

| Confidence | Source | Typical Accuracy |
|------------|--------|------------------|
| **high** | GPS EXIF or device API | ±5-10 meters |
| **medium** | WiFi triangulation | ±50-100 meters |
| **low** | IP geolocation | ±1-10 kilometers |
| **unknown** | No location information | N/A |

### User Corrections

Users can manually correct provenance data when automatic extraction is incorrect:

1. Original provenance is preserved in `original_capture_time` and `original_location_id`
2. `user_corrected` flag is set to `true`
3. Corrected data replaces the primary fields

This enables learning and improvement of automatic extraction over time.

### EXIF Data Extraction

The system automatically extracts provenance from EXIF metadata:

- **Location:** GPS coordinates (latitude, longitude, altitude)
- **Time:** DateTimeOriginal, CreateDate, GPSDateStamp
- **Device:** Make, Model, Software, LensModel
- **Camera Settings:** ISO, ShutterSpeed, Aperture, FocalLength

**Supported Formats:**
- JPEG, TIFF, HEIC/HEIF (photos)
- MP4, MOV, AVI (videos)
- WAV, MP3, FLAC (audio with recording time)

### Privacy Considerations

Location data is sensitive. The system provides:

1. **Granular Access Control:** Memory search respects note access permissions
2. **Named Locations:** Use semantic names instead of raw coordinates in responses
3. **Configurable Precision:** Round coordinates to reduce precision if needed
4. **Opt-Out:** Disable location extraction for privacy-sensitive users

## Implementation Status

Memory search is **available in v2026.2.0**. All components are fully implemented and tested.

**Available:**
- Database schema (W3C PROV temporal-spatial extension)
- PostGIS spatial indexes with GiST indexing
- Repository layer with spatial/temporal queries
- REST API endpoints (documented above)
- MCP tools (search_memories_by_location, search_memories_by_time, search_memories_combined, get_provenance_chain)
- Complete test coverage

**Database Access:**

Application developers can use memory search directly via the repository:

```rust
use matric_db::Database;

let db = Database::new(pool);

// Search by location
let results = db.memory_search
    .search_by_location(48.8584, 2.2945, 5000.0)
    .await?;

// Search by time
let results = db.memory_search
    .search_by_timerange(start, end)
    .await?;

// Get provenance
let provenance = db.memory_search
    .get_memory_provenance(note_id)
    .await?;
```

See `crates/matric-db/src/memory_search.rs` for full API details.

## Related Documentation

- [API Reference](./api.md) - Full REST API documentation
- [Search Guide](./search-guide.md) - Semantic and full-text search
- [Architecture](./architecture.md) - System architecture overview
- [Backup Guide](./backup.md) - Data backup and migration

---

*Memory search enables you to navigate your knowledge base through time and space, finding content based on when and where it was created, not just what it says.*

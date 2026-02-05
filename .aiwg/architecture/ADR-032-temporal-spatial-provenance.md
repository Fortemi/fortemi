# ADR-032: Temporal and Spatial Provenance System

**Status:** Implemented
**Date:** 2026-02-02
**Deciders:** Architecture team
**Related:** ADR-031 (Intelligent Attachment Processing), Epic #430

## Context

Files and notes can be associated with specific times and places. A photo has EXIF GPS coordinates and capture timestamps. A voice memo was recorded at a meeting location. A video captures a vacation memory at a specific beach on a specific day.

Current limitations:
- Attachments only track `created_at` (upload time), not capture time
- No location data extracted or stored
- No way to search "memories from Paris last December"
- No support for "what happened here?" queries

### Primary Use Case: Personal TikTok Memory System

Users record personal videos on their phones to remember places, people, and events. The system should:
1. Extract when/where from device metadata (EXIF, GPS)
2. Process video/audio to reconstruct detailed memories
3. Enable time-and-place-based retrieval
4. Support "memory map" visualization

## Decision

Implement a **comprehensive temporal-spatial provenance system** using PostgreSQL with PostGIS.

### 1. Core Schema

#### Temporal Events

```sql
CREATE TABLE temporal_event (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Time range (supports instants and durations)
    event_time tstzrange NOT NULL,

    -- Original timezone for display
    original_timezone TEXT,
    original_utc_offset INTEGER,  -- Minutes

    -- Duration for recordings
    duration_seconds REAL,

    -- Provenance
    source TEXT NOT NULL,  -- 'exif', 'device_api', 'user_manual', 'ai_estimated'
    confidence TEXT NOT NULL,  -- 'exact', 'high', 'medium', 'low'
    source_metadata JSONB DEFAULT '{}',

    -- User corrections
    user_corrected BOOLEAN DEFAULT FALSE,
    original_event_time tstzrange,
    correction_note TEXT,

    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- BRIN index for efficient time-series scans
CREATE INDEX idx_temporal_event_brin ON temporal_event
    USING BRIN (event_time) WITH (pages_per_range = 32);

-- GIST index for range overlap queries
CREATE INDEX idx_temporal_event_gist ON temporal_event
    USING GIST (event_time);
```

#### Spatial Locations

```sql
-- Requires: CREATE EXTENSION postgis;

CREATE TABLE spatial_location (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- GPS coordinates (geography type for accurate distance)
    point geography(Point, 4326) NOT NULL,

    -- Accuracy metadata
    horizontal_accuracy_m REAL,
    vertical_accuracy_m REAL,

    -- Altitude
    altitude_m REAL,
    altitude_reference TEXT,  -- 'msl' or 'ellipsoid'

    -- Direction/movement
    heading_degrees REAL,
    speed_mps REAL,

    -- Named location reference
    named_location_id UUID REFERENCES named_location(id),

    -- Indoor positioning (optional)
    building_name TEXT,
    floor_level INTEGER,
    room_identifier TEXT,

    -- Provenance
    source TEXT NOT NULL,
    confidence TEXT NOT NULL,
    source_metadata JSONB DEFAULT '{}',

    -- User corrections
    user_corrected BOOLEAN DEFAULT FALSE,
    original_point geography(Point, 4326),
    correction_note TEXT,

    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- GIST index for spatial queries
CREATE INDEX idx_spatial_location_point ON spatial_location
    USING GIST (point);
```

#### Named Locations (Place Registry)

```sql
CREATE TABLE named_location (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Identity
    name TEXT NOT NULL,
    display_name TEXT,
    slug TEXT NOT NULL UNIQUE,

    -- Type
    location_type TEXT NOT NULL,  -- 'point', 'building', 'region', 'city', 'country'

    -- Geometry
    point geography(Point, 4326),
    boundary geography(Polygon, 4326),  -- For regions/geofences

    -- Address
    address_line TEXT,
    locality TEXT,
    admin_area TEXT,
    country TEXT,
    country_code CHAR(2),
    postal_code TEXT,

    -- Indoor
    building_name TEXT,
    floor_level INTEGER,
    room_identifier TEXT,

    -- Metadata
    timezone TEXT,
    altitude_m REAL,
    metadata JSONB DEFAULT '{}',

    -- Owner (for multi-tenant)
    owner_id UUID,

    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_named_location_point ON named_location USING GIST (point);
CREATE INDEX idx_named_location_boundary ON named_location USING GIST (boundary);
```

#### Attachment Provenance Junction

```sql
CREATE TABLE attachment_provenance (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    attachment_id UUID NOT NULL REFERENCES file_attachment(id),
    note_id UUID REFERENCES note(id),

    -- Temporal/spatial links
    temporal_event_id UUID REFERENCES temporal_event(id),
    spatial_location_id UUID REFERENCES spatial_location(id),

    -- Event metadata
    event_type TEXT,  -- 'photo', 'video_recording', 'voice_memo'
    event_title TEXT,
    event_description TEXT,

    -- Device info
    device_make TEXT,
    device_model TEXT,
    device_os TEXT,
    software TEXT,

    -- File metadata
    original_filename TEXT,
    original_file_date TIMESTAMPTZ,

    -- AI-generated context
    ai_context JSONB DEFAULT '{}',
    ai_processed_at TIMESTAMPTZ,
    ai_model TEXT,

    -- Raw metadata preservation
    raw_metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_attachment_provenance_attachment ON attachment_provenance(attachment_id);
CREATE INDEX idx_attachment_provenance_temporal ON attachment_provenance(temporal_event_id);
CREATE INDEX idx_attachment_provenance_spatial ON attachment_provenance(spatial_location_id);
```

### 2. Metadata Extraction

#### EXIF Parsing (Images)

Extract from JPEG/HEIC/PNG using `kamadak-exif` crate:

```rust
struct ExtractedProvenance {
    // Temporal
    capture_time: Option<DateTime<Utc>>,
    original_timezone: Option<String>,
    duration_seconds: Option<f64>,

    // Spatial
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude_m: Option<f64>,
    gps_accuracy_m: Option<f64>,
    heading_degrees: Option<f64>,
    speed_mps: Option<f64>,

    // Device
    device_make: Option<String>,
    device_model: Option<String>,
    software: Option<String>,

    // Raw
    raw_exif: serde_json::Value,
}
```

#### MediaInfo (Video/Audio)

Use `mediainfo` CLI for container metadata:
- Duration, codec, resolution
- Creation/encoding date
- GPS from XMP/metadata tracks

#### Mobile Device API

Accept metadata from mobile uploads:

```json
{
  "capture_time": "2025-12-25T14:30:00Z",
  "timezone": "Europe/Paris",
  "location": {
    "latitude": 48.8566,
    "longitude": 2.3522,
    "altitude_m": 35.0,
    "horizontal_accuracy_m": 5.0,
    "heading_degrees": 180.0
  },
  "device": {
    "make": "Apple",
    "model": "iPhone 15 Pro",
    "os": "iOS 18.1"
  }
}
```

### 3. Query Patterns

#### Radius Search

```sql
-- Find memories within 5km of Paris center
SELECT ap.*, te.event_time, sl.*
FROM attachment_provenance ap
JOIN spatial_location sl ON ap.spatial_location_id = sl.id
LEFT JOIN temporal_event te ON ap.temporal_event_id = te.id
WHERE ST_DWithin(
    sl.point,
    ST_SetSRID(ST_MakePoint(2.3522, 48.8566), 4326)::geography,
    5000  -- meters
)
ORDER BY te.event_time DESC;
```

#### Time Range Search

```sql
-- Find memories from Christmas 2025
SELECT ap.*, te.event_time
FROM attachment_provenance ap
JOIN temporal_event te ON ap.temporal_event_id = te.id
WHERE te.event_time && tstzrange('2025-12-25', '2025-12-26')
ORDER BY lower(te.event_time);
```

#### Combined Temporal-Spatial

```sql
-- "What happened in Paris during Christmas 2025?"
SELECT ap.*, te.event_time, sl.*
FROM attachment_provenance ap
JOIN temporal_event te ON ap.temporal_event_id = te.id
JOIN spatial_location sl ON ap.spatial_location_id = sl.id
WHERE
    te.event_time && tstzrange('2025-12-24', '2025-12-27')
    AND ST_DWithin(
        sl.point,
        ST_SetSRID(ST_MakePoint(2.3522, 48.8566), 4326)::geography,
        10000
    )
ORDER BY lower(te.event_time);
```

#### "What Happened Here?"

```sql
-- Find all memories at a specific location over time
CREATE FUNCTION find_memories_here(
    p_lat DOUBLE PRECISION,
    p_lon DOUBLE PRECISION,
    p_radius_m DOUBLE PRECISION DEFAULT 100
) RETURNS TABLE(...) AS $$
BEGIN
    RETURN QUERY
    SELECT ...
    FROM attachment_provenance ap
    JOIN spatial_location sl ON ap.spatial_location_id = sl.id
    LEFT JOIN temporal_event te ON ap.temporal_event_id = te.id
    WHERE ST_DWithin(
        sl.point,
        ST_SetSRID(ST_MakePoint(p_lon, p_lat), 4326)::geography,
        p_radius_m
    )
    ORDER BY te.event_time DESC NULLS LAST;
END;
$$ LANGUAGE plpgsql;
```

### 4. Index Strategy

| Index Type | Column | Query Pattern | Performance |
|------------|--------|---------------|-------------|
| GIST | spatial_location.point | Radius search, nearest neighbor | <10ms for 1M rows |
| BRIN | temporal_event.event_time | Time range scans | <50ms for 10M rows |
| GIST | temporal_event.event_time | Range overlap | <20ms |
| GIST | named_location.boundary | Geofence containment | <20ms |
| GIN | attachment_provenance.raw_metadata | JSONB queries | Varies |

### 5. API Endpoints

```
GET /memories/near
  ?lat=48.8566&lon=2.3522&radius=5000
  &from=2025-12-24&to=2025-12-27
  &include_ai_context=true

GET /memories/timeline
  ?lat=48.8566&lon=2.3522&radius=1000
  &group_by=month

GET /memories/here
  ?lat=48.8566&lon=2.3522&radius=100

POST /locations
  { name, display_name, point, boundary, address, ... }

GET /locations/reverse
  ?lat=48.8566&lon=2.3522

PATCH /attachments/{id}/provenance
  { temporal: {...}, spatial: {...}, correction_note: "..." }
```

### 6. MCP Tools

```javascript
// Search memories by location/time
search_memories({
  near: { lat: 48.8566, lon: 2.3522, radius: 5000 },
  from: '2025-12-24',
  to: '2025-12-27',
  include_ai_context: true
})

// "What happened here?"
memories_at_location({
  lat: 48.8566,
  lon: 2.3522,
  radius: 100,
  group_by: 'month'
})

// Create named location
create_location({
  name: 'Home',
  display_name: 'My Home',
  location_type: 'building',
  point: { lat: 48.8566, lon: 2.3522 },
  address: { locality: 'Paris', country: 'France' }
})

// Correct provenance
update_provenance(attachment_id, {
  temporal: { event_start: '2025-12-25T14:30:00Z' },
  spatial: { lat: 48.8584, lon: 2.2945 },
  correction_note: 'GPS was off, this was at Eiffel Tower'
})
```

## Consequences

### Positive

- (+) **Memory reconstruction**: "Show me Paris, December 2025"
- (+) **Spatial awareness**: "What happened here?"
- (+) **User corrections**: Override inaccurate GPS/time
- (+) **Named locations**: Semantic place names (Home, Office)
- (+) **Full preservation**: Raw metadata kept for future use
- (+) **PostGIS power**: Industry-standard spatial queries

### Negative

- (-) **PostGIS dependency**: Requires extension installation
- (-) **Storage overhead**: Provenance adds ~1KB per attachment
- (-) **Complexity**: Multiple joins for full provenance queries
- (-) **Privacy concerns**: Location data is sensitive

### Mitigations

- PostGIS is available on all major PostgreSQL deployments
- Provenance storage is optional (graceful degradation)
- Provide views/functions to simplify common queries
- Add privacy controls (location blur, time-only mode)

## Implementation

### Phase 1: Core Schema (Week 1)
- PostGIS extension
- Temporal/spatial tables
- Named location registry
- Attachment provenance junction

### Phase 2: Extraction (Week 2)
- EXIF parser integration
- MediaInfo integration
- Mobile metadata API
- Processing job handlers

### Phase 3: Queries (Week 3)
- API endpoints
- MCP tools
- Timeline/map views
- Combined search

### Phase 4: AI Enhancement (Week 4)
- Memory reconstruction prompts
- AI context generation
- Nearby memory context
- Memory prompt suggestions

## References

- PostGIS documentation: https://postgis.net/docs/
- EXIF specification: https://www.exif.org/
- W3C PROV-O: https://www.w3.org/TR/prov-o/
- Dublin Core metadata: https://www.dublincore.org/

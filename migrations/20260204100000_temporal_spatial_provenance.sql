-- ============================================================================
-- W3C PROV Temporal-Spatial Extension
-- Issue: #434 - Extends existing W3C PROV system for file capture context
-- ============================================================================
--
-- ALIGNMENT WITH EXISTING W3C PROV-DM:
-- This migration EXTENDS (not replaces) the existing W3C PROV schema:
--   - provenance_edge: Entity relationships (wasDerivedFrom, used, wasInformedBy)
--   - provenance_activity: AI processing operations (with started_at, ended_at)
--
-- NEW W3C PROV CONCEPTS FOR FILES:
--   - prov:Location (prov:atLocation) - spatial context for entities/activities
--   - prov:Agent (prov:wasAttributedTo) - devices that captured files
--   - Enhanced temporal tracking for capture events
--
-- INTEGRATION POINTS:
--   - file_provenance.activity_id → provenance_activity.id (file capture activity)
--   - Follows same pattern: Activity has started_at/ended_at, Location, Agent
-- ============================================================================

-- ============================================================================
-- PART 1: ENABLE POSTGIS EXTENSION
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS postgis;

COMMENT ON EXTENSION postgis IS
    'PostGIS extension for W3C PROV prov:atLocation spatial queries';

-- ============================================================================
-- PART 2: PROV LOCATION (prov:atLocation)
-- Represents spatial location where an entity was created or activity occurred
-- ============================================================================

CREATE TABLE IF NOT EXISTS prov_location (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Spatial data (WGS84/EPSG:4326 for GPS compatibility)
    point geography(Point, 4326) NOT NULL,
    horizontal_accuracy_m REAL,
    altitude_m REAL,
    vertical_accuracy_m REAL,

    -- Movement data (for video capture or activity tracking)
    heading_degrees REAL,
    speed_mps REAL,

    -- Named location reference (for semantic place names)
    named_location_id UUID,  -- FK added after named_location created

    -- Source attribution
    source TEXT NOT NULL DEFAULT 'unknown',  -- 'gps_exif', 'device_api', 'user_manual', 'geocoded', 'ai_estimated'
    confidence TEXT NOT NULL DEFAULT 'medium',  -- 'high', 'medium', 'low', 'unknown'

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT check_heading_degrees CHECK (heading_degrees IS NULL OR (heading_degrees >= 0 AND heading_degrees < 360)),
    CONSTRAINT check_speed_mps CHECK (speed_mps IS NULL OR speed_mps >= 0),
    CONSTRAINT check_confidence CHECK (confidence IN ('high', 'medium', 'low', 'unknown')),
    CONSTRAINT check_source CHECK (source IN ('gps_exif', 'device_api', 'user_manual', 'geocoded', 'ai_estimated', 'unknown'))
);

CREATE INDEX IF NOT EXISTS idx_prov_location_point ON prov_location USING GIST (point);
CREATE INDEX IF NOT EXISTS idx_prov_location_named ON prov_location(named_location_id);

COMMENT ON TABLE prov_location IS 'W3C PROV Location (prov:atLocation) - spatial context for entities and activities';
COMMENT ON COLUMN prov_location.point IS 'Geographic point (WGS84/EPSG:4326) - use ST_SetSRID(ST_MakePoint(lon, lat), 4326)';
COMMENT ON COLUMN prov_location.horizontal_accuracy_m IS 'Horizontal accuracy in meters (GPS typically ±5-10m for high confidence)';
COMMENT ON COLUMN prov_location.source IS 'Source of location data: gps_exif, device_api, user_manual, geocoded, ai_estimated';
COMMENT ON COLUMN prov_location.confidence IS 'Confidence level: high (GPS ±10m), medium (WiFi ±100m), low (IP ±1km+), unknown';

-- ============================================================================
-- PART 3: NAMED LOCATIONS REGISTRY (Semantic Place Names)
-- Extends prov:atLocation with user-defined semantic names
-- ============================================================================

CREATE TABLE IF NOT EXISTS named_location (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Identity
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT,
    location_type TEXT NOT NULL,  -- 'home', 'work', 'poi', 'city', 'region', 'country'

    -- Geographic data (point or polygon)
    point geography(Point, 4326),
    boundary geography(Polygon, 4326),
    radius_m REAL,  -- For point-based matching (e.g., "within 100m of home")

    -- Structured address (for reverse geocoding)
    address_line TEXT,
    locality TEXT,          -- City or town
    admin_area TEXT,        -- State/province
    country TEXT,
    country_code CHAR(2),
    postal_code TEXT,

    -- Context
    timezone TEXT,
    altitude_m REAL,

    -- Ownership and visibility
    owner_id UUID,  -- User who created this location
    is_private BOOLEAN DEFAULT TRUE,

    -- Metadata
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT check_has_geography CHECK (point IS NOT NULL OR boundary IS NOT NULL),
    CONSTRAINT check_location_type CHECK (location_type IN ('home', 'work', 'poi', 'city', 'region', 'country'))
);

CREATE INDEX IF NOT EXISTS idx_named_location_point ON named_location USING GIST (point)
    WHERE point IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_named_location_boundary ON named_location USING GIST (boundary)
    WHERE boundary IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_named_location_type ON named_location(location_type);
CREATE INDEX IF NOT EXISTS idx_named_location_owner ON named_location(owner_id);

-- Add FK to prov_location now that named_location exists
ALTER TABLE prov_location ADD CONSTRAINT fk_prov_location_named
    FOREIGN KEY (named_location_id) REFERENCES named_location(id) ON DELETE SET NULL;

COMMENT ON TABLE named_location IS 'Named location registry for semantic place references (e.g., "home", "Paris", "Eiffel Tower")';
COMMENT ON COLUMN named_location.slug IS 'URL-safe unique identifier (e.g., "my-home", "eiffel-tower")';
COMMENT ON COLUMN named_location.point IS 'Representative point for location (centroid, entrance, GPS coordinate)';
COMMENT ON COLUMN named_location.boundary IS 'Geographic boundary polygon for areas (cities, regions, buildings)';
COMMENT ON COLUMN named_location.radius_m IS 'Matching radius for point-based locations (e.g., "within 100m of home")';

-- Trigger for updated_at
CREATE OR REPLACE FUNCTION update_named_location_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER named_location_updated
    BEFORE UPDATE ON named_location
    FOR EACH ROW
    EXECUTE FUNCTION update_named_location_timestamp();

-- ============================================================================
-- PART 4: PROV AGENT DEVICE (prov:wasAttributedTo)
-- Extends W3C PROV Agent concept for devices that captured content
-- ============================================================================

CREATE TABLE IF NOT EXISTS prov_agent_device (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Device identification
    device_make TEXT,           -- "Apple", "Canon", "Samsung"
    device_model TEXT,          -- "iPhone 15 Pro", "EOS R5", "Galaxy S24"
    device_os TEXT,             -- "iOS 17.2", "Android 14"
    device_os_version TEXT,
    software TEXT,              -- Camera app or editing software
    software_version TEXT,

    -- Device capabilities
    has_gps BOOLEAN,
    has_accelerometer BOOLEAN,
    sensor_metadata JSONB DEFAULT '{}',

    -- Ownership (which user owns this device)
    owner_id UUID,
    device_name TEXT,  -- User-assigned name ("My iPhone", "Work Camera")

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique constraint for device deduplication
    CONSTRAINT device_unique UNIQUE NULLS NOT DISTINCT (device_make, device_model, owner_id)
);

CREATE INDEX IF NOT EXISTS idx_prov_agent_device_owner ON prov_agent_device(owner_id);
CREATE INDEX IF NOT EXISTS idx_prov_agent_device_make ON prov_agent_device(device_make, device_model);

COMMENT ON TABLE prov_agent_device IS 'W3C PROV Agent (prov:wasAttributedTo) - devices that capture content';
COMMENT ON COLUMN prov_agent_device.device_make IS 'Device manufacturer (from EXIF Make or OS API)';
COMMENT ON COLUMN prov_agent_device.device_model IS 'Device model (from EXIF Model or OS API)';
COMMENT ON COLUMN prov_agent_device.sensor_metadata IS 'Device-specific sensor data (EXIF, motion sensors, etc.)';

-- ============================================================================
-- PART 5: FILE PROVENANCE (W3C PROV for Attachments)
-- Links attachments to temporal, spatial, and device context
-- Integrates with existing provenance_activity table
-- ============================================================================

CREATE TABLE IF NOT EXISTS file_provenance (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Entity reference (the attachment is a W3C PROV Entity)
    attachment_id UUID NOT NULL REFERENCES attachment(id) ON DELETE CASCADE,

    -- Temporal context (when content was captured, not uploaded)
    -- Using tstzrange for efficient overlap queries
    capture_time tstzrange,  -- Time range for capture (instant or duration for video)
    capture_timezone TEXT,
    capture_duration_seconds REAL,  -- For video/audio

    -- Temporal source and confidence
    time_source TEXT,  -- 'exif', 'file_mtime', 'user_manual', 'ai_estimated'
    time_confidence TEXT DEFAULT 'medium',

    -- Spatial context (prov:atLocation)
    location_id UUID REFERENCES prov_location(id) ON DELETE SET NULL,

    -- Agent attribution (prov:wasAttributedTo)
    device_id UUID REFERENCES prov_agent_device(id) ON DELETE SET NULL,

    -- Activity link (prov:wasGeneratedBy)
    -- Links to existing provenance_activity table for file capture/upload activity
    activity_id UUID,  -- References provenance_activity(id) - no FK to allow flexibility

    -- Event description
    event_type TEXT,  -- 'photo', 'video', 'audio', 'scan', 'screenshot', 'recording'
    event_title TEXT,
    event_description TEXT,

    -- Raw extraction data (full EXIF/XMP/IPTC as JSONB)
    raw_metadata JSONB DEFAULT '{}',

    -- AI enhancement (context extraction from image/video content)
    ai_context JSONB DEFAULT '{}',
    ai_processed_at TIMESTAMPTZ,
    ai_model TEXT,

    -- User corrections (prov:wasInfluencedBy user)
    user_corrected BOOLEAN DEFAULT FALSE,
    original_capture_time tstzrange,  -- Before user correction
    original_location_id UUID REFERENCES prov_location(id) ON DELETE SET NULL,
    correction_note TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT check_time_source CHECK (time_source IN ('exif', 'file_mtime', 'user_manual', 'ai_estimated')),
    CONSTRAINT check_time_confidence CHECK (time_confidence IN ('high', 'medium', 'low', 'unknown')),
    CONSTRAINT check_event_type CHECK (event_type IN ('photo', 'video', 'audio', 'scan', 'screenshot', 'recording', 'unknown'))
);

CREATE INDEX IF NOT EXISTS idx_file_provenance_attachment ON file_provenance(attachment_id);
CREATE INDEX IF NOT EXISTS idx_file_provenance_time ON file_provenance USING GIST (capture_time);
CREATE INDEX IF NOT EXISTS idx_file_provenance_location ON file_provenance(location_id);
CREATE INDEX IF NOT EXISTS idx_file_provenance_device ON file_provenance(device_id);
CREATE INDEX IF NOT EXISTS idx_file_provenance_activity ON file_provenance(activity_id);

-- GIN index for JSONB metadata queries
CREATE INDEX IF NOT EXISTS idx_file_provenance_metadata ON file_provenance USING GIN (raw_metadata);

COMMENT ON TABLE file_provenance IS 'W3C PROV provenance for file attachments - links entities to temporal, spatial, device, and activity context';
COMMENT ON COLUMN file_provenance.attachment_id IS 'The attachment (W3C PROV Entity)';
COMMENT ON COLUMN file_provenance.capture_time IS 'Capture time range (tstzrange) - when content was created, not uploaded';
COMMENT ON COLUMN file_provenance.location_id IS 'W3C PROV prov:atLocation - where content was captured';
COMMENT ON COLUMN file_provenance.device_id IS 'W3C PROV prov:wasAttributedTo - device that captured content';
COMMENT ON COLUMN file_provenance.activity_id IS 'W3C PROV prov:wasGeneratedBy - references provenance_activity (file_capture, file_upload)';
COMMENT ON COLUMN file_provenance.raw_metadata IS 'Raw EXIF/XMP/IPTC metadata as JSONB for advanced queries';

-- ============================================================================
-- PART 6: QUERY FUNCTIONS - SPATIAL
-- ============================================================================

-- Find memories near a location
CREATE OR REPLACE FUNCTION find_memories_near(
    p_lat DOUBLE PRECISION,
    p_lon DOUBLE PRECISION,
    p_radius_m DOUBLE PRECISION DEFAULT 5000,
    p_from TIMESTAMPTZ DEFAULT NULL,
    p_to TIMESTAMPTZ DEFAULT NULL,
    p_limit INTEGER DEFAULT 50
)
RETURNS TABLE(
    provenance_id UUID,
    attachment_id UUID,
    note_id UUID,
    filename TEXT,
    content_type TEXT,
    distance_m DOUBLE PRECISION,
    capture_time tstzrange,
    location_name TEXT,
    event_type TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        fp.id as provenance_id,
        fp.attachment_id,
        a.note_id,
        a.filename,
        ab.content_type,
        ST_Distance(pl.point, ST_SetSRID(ST_MakePoint(p_lon, p_lat), 4326)::geography) as distance_m,
        fp.capture_time,
        nl.name as location_name,
        fp.event_type
    FROM file_provenance fp
    JOIN attachment a ON fp.attachment_id = a.id
    JOIN attachment_blob ab ON a.blob_id = ab.id
    JOIN prov_location pl ON fp.location_id = pl.id
    LEFT JOIN named_location nl ON pl.named_location_id = nl.id
    WHERE ST_DWithin(pl.point, ST_SetSRID(ST_MakePoint(p_lon, p_lat), 4326)::geography, p_radius_m)
    AND (p_from IS NULL OR fp.capture_time && tstzrange(p_from, COALESCE(p_to, 'infinity'::timestamptz)))
    ORDER BY distance_m
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION find_memories_near IS 'Find file attachments captured within radius of lat/lon (optionally filtered by time)';

-- Find memories in a time range
CREATE OR REPLACE FUNCTION find_memories_in_timerange(
    p_from TIMESTAMPTZ,
    p_to TIMESTAMPTZ,
    p_limit INTEGER DEFAULT 100
)
RETURNS TABLE(
    provenance_id UUID,
    attachment_id UUID,
    note_id UUID,
    capture_time tstzrange,
    event_type TEXT,
    location_name TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        fp.id as provenance_id,
        fp.attachment_id,
        a.note_id,
        fp.capture_time,
        fp.event_type,
        nl.name as location_name
    FROM file_provenance fp
    JOIN attachment a ON fp.attachment_id = a.id
    LEFT JOIN prov_location pl ON fp.location_id = pl.id
    LEFT JOIN named_location nl ON pl.named_location_id = nl.id
    WHERE fp.capture_time && tstzrange(p_from, p_to)
    ORDER BY lower(fp.capture_time)
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION find_memories_in_timerange IS 'Find file attachments captured within a time range';

-- Reverse geocode: find named location containing a point
CREATE OR REPLACE FUNCTION find_named_location_at(
    p_lat DOUBLE PRECISION,
    p_lon DOUBLE PRECISION
)
RETURNS TABLE(
    id UUID,
    name TEXT,
    location_type TEXT,
    distance_m DOUBLE PRECISION
) AS $$
DECLARE
    query_point geography;
BEGIN
    query_point := ST_SetSRID(ST_MakePoint(p_lon, p_lat), 4326)::geography;

    RETURN QUERY
    -- First check boundaries (polygons)
    SELECT nl.id, nl.name, nl.location_type, 0::double precision as distance_m
    FROM named_location nl
    WHERE nl.boundary IS NOT NULL
    AND ST_Contains(nl.boundary::geometry, query_point::geometry)

    UNION ALL

    -- Then check point + radius
    SELECT nl.id, nl.name, nl.location_type,
           ST_Distance(nl.point, query_point) as distance_m
    FROM named_location nl
    WHERE nl.point IS NOT NULL
    AND nl.radius_m IS NOT NULL
    AND ST_DWithin(nl.point, query_point, nl.radius_m)

    ORDER BY distance_m
    LIMIT 5;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION find_named_location_at IS 'Reverse geocode: find named locations containing a lat/lon point';

-- ============================================================================
-- PART 7: STATISTICS VIEW
-- ============================================================================

CREATE OR REPLACE VIEW file_provenance_stats AS
SELECT
    (SELECT COUNT(*) FROM file_provenance) as total_records,
    (SELECT COUNT(*) FROM file_provenance WHERE capture_time IS NOT NULL) as with_time,
    (SELECT COUNT(*) FROM file_provenance WHERE location_id IS NOT NULL) as with_location,
    (SELECT COUNT(*) FROM file_provenance WHERE device_id IS NOT NULL) as with_device,
    (SELECT COUNT(*) FROM file_provenance WHERE capture_time IS NOT NULL AND location_id IS NOT NULL) as with_both,
    (SELECT COUNT(*) FROM file_provenance WHERE user_corrected = TRUE) as user_corrected,
    (SELECT COUNT(DISTINCT device_id) FROM file_provenance) as unique_devices,
    (SELECT COUNT(*) FROM named_location) as named_locations,
    (SELECT COUNT(*) FROM prov_location WHERE confidence = 'high') as high_confidence_locations;

COMMENT ON VIEW file_provenance_stats IS 'File provenance system statistics (coverage, accuracy, user corrections)';

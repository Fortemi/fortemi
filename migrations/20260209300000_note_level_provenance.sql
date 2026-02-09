-- ============================================================================
-- Note-Level Provenance (Issue #262)
-- Extends file_provenance to unified provenance table supporting both
-- attachment-level and note-level spatial-temporal context.
-- ============================================================================
--
-- CHANGES:
--   1. Rename file_provenance → provenance
--   2. Make attachment_id nullable
--   3. Add note_id column with FK to note(id)
--   4. XOR constraint: exactly one of attachment_id or note_id must be set
--   5. Partial unique index on note_id (one provenance per note)
--   6. Compatibility view: file_provenance
--   7. Update statistics view
--   8. Update query functions to include note-level provenance
--   9. Apply changes across all archive schemas
-- ============================================================================

-- ============================================================================
-- PART 1: RENAME TABLE AND ALTER COLUMNS (public schema)
-- ============================================================================

ALTER TABLE file_provenance RENAME TO provenance;

-- Make attachment_id nullable (was NOT NULL)
ALTER TABLE provenance ALTER COLUMN attachment_id DROP NOT NULL;

-- Add note_id column
ALTER TABLE provenance ADD COLUMN note_id UUID REFERENCES note(id) ON DELETE CASCADE;

-- XOR constraint: exactly one of attachment_id or note_id must be set
ALTER TABLE provenance ADD CONSTRAINT provenance_target_xor
    CHECK ((attachment_id IS NOT NULL) != (note_id IS NOT NULL));

-- Expand CHECK constraints to support note-level provenance enum values
ALTER TABLE provenance DROP CONSTRAINT IF EXISTS check_time_source;
ALTER TABLE provenance ADD CONSTRAINT check_time_source
    CHECK (time_source IN ('exif', 'file_mtime', 'user_manual', 'ai_estimated', 'gps', 'network', 'manual', 'file_metadata'));

ALTER TABLE provenance DROP CONSTRAINT IF EXISTS check_time_confidence;
ALTER TABLE provenance ADD CONSTRAINT check_time_confidence
    CHECK (time_confidence IN ('high', 'medium', 'low', 'unknown', 'exact', 'approximate', 'estimated'));

ALTER TABLE provenance DROP CONSTRAINT IF EXISTS check_event_type;
ALTER TABLE provenance ADD CONSTRAINT check_event_type
    CHECK (event_type IN ('photo', 'video', 'audio', 'scan', 'screenshot', 'recording', 'unknown', 'created', 'modified', 'accessed', 'shared'));

-- ============================================================================
-- PART 2: INDEXES
-- ============================================================================

-- Rename existing indexes
ALTER INDEX IF EXISTS idx_file_provenance_attachment RENAME TO idx_provenance_attachment;
ALTER INDEX IF EXISTS idx_file_provenance_time RENAME TO idx_provenance_time;
ALTER INDEX IF EXISTS idx_file_provenance_location RENAME TO idx_provenance_location;
ALTER INDEX IF EXISTS idx_file_provenance_device RENAME TO idx_provenance_device;
ALTER INDEX IF EXISTS idx_file_provenance_activity RENAME TO idx_provenance_activity;
ALTER INDEX IF EXISTS idx_file_provenance_metadata RENAME TO idx_provenance_metadata;

-- Partial unique index: one provenance record per note
CREATE UNIQUE INDEX idx_provenance_note_id ON provenance(note_id) WHERE note_id IS NOT NULL;

-- Index on note_id for general lookups
CREATE INDEX idx_provenance_note_lookup ON provenance(note_id) WHERE note_id IS NOT NULL;

-- ============================================================================
-- PART 3: COMPATIBILITY VIEW
-- ============================================================================

-- Drop the old stats view first (depends on old table name)
DROP VIEW IF EXISTS file_provenance_stats;

-- Compatibility view for code that still references file_provenance
CREATE VIEW file_provenance AS
    SELECT * FROM provenance WHERE attachment_id IS NOT NULL;

-- ============================================================================
-- PART 4: UPDATE STATISTICS VIEW
-- ============================================================================

CREATE OR REPLACE VIEW provenance_stats AS
SELECT
    (SELECT COUNT(*) FROM provenance) as total_records,
    (SELECT COUNT(*) FROM provenance WHERE attachment_id IS NOT NULL) as file_records,
    (SELECT COUNT(*) FROM provenance WHERE note_id IS NOT NULL) as note_records,
    (SELECT COUNT(*) FROM provenance WHERE capture_time IS NOT NULL) as with_time,
    (SELECT COUNT(*) FROM provenance WHERE location_id IS NOT NULL) as with_location,
    (SELECT COUNT(*) FROM provenance WHERE device_id IS NOT NULL) as with_device,
    (SELECT COUNT(*) FROM provenance WHERE capture_time IS NOT NULL AND location_id IS NOT NULL) as with_both,
    (SELECT COUNT(*) FROM provenance WHERE user_corrected = TRUE) as user_corrected,
    (SELECT COUNT(DISTINCT device_id) FROM provenance) as unique_devices,
    (SELECT COUNT(*) FROM named_location) as named_locations,
    (SELECT COUNT(*) FROM prov_location WHERE confidence = 'high') as high_confidence_locations;

COMMENT ON VIEW provenance_stats IS 'Provenance system statistics covering both file and note provenance';

-- Keep backward-compatible alias
CREATE OR REPLACE VIEW file_provenance_stats AS SELECT * FROM provenance_stats;

-- ============================================================================
-- PART 5: UPDATE QUERY FUNCTIONS
-- ============================================================================

-- Update find_memories_near to include note-level provenance
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
        p.id as provenance_id,
        p.attachment_id,
        COALESCE(p.note_id, a.note_id) as note_id,
        a.filename,
        ab.content_type,
        ST_Distance(pl.point, ST_SetSRID(ST_MakePoint(p_lon, p_lat), 4326)::geography) as distance_m,
        p.capture_time,
        nl.name as location_name,
        p.event_type
    FROM provenance p
    LEFT JOIN attachment a ON p.attachment_id = a.id
    LEFT JOIN attachment_blob ab ON a.blob_id = ab.id
    JOIN prov_location pl ON p.location_id = pl.id
    LEFT JOIN named_location nl ON pl.named_location_id = nl.id
    WHERE ST_DWithin(pl.point, ST_SetSRID(ST_MakePoint(p_lon, p_lat), 4326)::geography, p_radius_m)
    AND (p_from IS NULL OR p.capture_time && tstzrange(p_from, COALESCE(p_to, 'infinity'::timestamptz)))
    ORDER BY distance_m
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

-- Update find_memories_in_timerange to include note-level provenance
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
        p.id as provenance_id,
        p.attachment_id,
        COALESCE(p.note_id, a.note_id) as note_id,
        p.capture_time,
        p.event_type,
        nl.name as location_name
    FROM provenance p
    LEFT JOIN attachment a ON p.attachment_id = a.id
    LEFT JOIN prov_location pl ON p.location_id = pl.id
    LEFT JOIN named_location nl ON pl.named_location_id = nl.id
    WHERE p.capture_time && tstzrange(p_from, p_to)
    ORDER BY lower(p.capture_time)
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql STABLE;

-- ============================================================================
-- PART 6: APPLY TO ARCHIVE SCHEMAS
-- ============================================================================

DO $archive_loop$
DECLARE
    r RECORD;
BEGIN
    FOR r IN SELECT schema_name FROM archive_registry WHERE schema_name != 'public'
    LOOP
        -- Rename table
        EXECUTE format('ALTER TABLE IF EXISTS %I.file_provenance RENAME TO provenance', r.schema_name);

        -- Make attachment_id nullable
        EXECUTE format('ALTER TABLE IF EXISTS %I.provenance ALTER COLUMN attachment_id DROP NOT NULL', r.schema_name);

        -- Add note_id column
        EXECUTE format('ALTER TABLE IF EXISTS %I.provenance ADD COLUMN IF NOT EXISTS note_id UUID REFERENCES %I.note(id) ON DELETE CASCADE', r.schema_name, r.schema_name);

        -- XOR constraint
        BEGIN
            EXECUTE format('ALTER TABLE %I.provenance ADD CONSTRAINT provenance_target_xor CHECK ((attachment_id IS NOT NULL) != (note_id IS NOT NULL))', r.schema_name);
        EXCEPTION WHEN duplicate_object THEN
            NULL; -- constraint already exists
        END;

        -- Expand CHECK constraints for note-level enum values
        EXECUTE format('ALTER TABLE %I.provenance DROP CONSTRAINT IF EXISTS check_time_source', r.schema_name);
        EXECUTE format($c$ALTER TABLE %I.provenance ADD CONSTRAINT check_time_source CHECK (time_source IN ('exif', 'file_mtime', 'user_manual', 'ai_estimated', 'gps', 'network', 'manual', 'file_metadata'))$c$, r.schema_name);
        EXECUTE format('ALTER TABLE %I.provenance DROP CONSTRAINT IF EXISTS check_time_confidence', r.schema_name);
        EXECUTE format($c$ALTER TABLE %I.provenance ADD CONSTRAINT check_time_confidence CHECK (time_confidence IN ('high', 'medium', 'low', 'unknown', 'exact', 'approximate', 'estimated'))$c$, r.schema_name);
        EXECUTE format('ALTER TABLE %I.provenance DROP CONSTRAINT IF EXISTS check_event_type', r.schema_name);
        EXECUTE format($c$ALTER TABLE %I.provenance ADD CONSTRAINT check_event_type CHECK (event_type IN ('photo', 'video', 'audio', 'scan', 'screenshot', 'recording', 'unknown', 'created', 'modified', 'accessed', 'shared'))$c$, r.schema_name);

        -- Indexes
        EXECUTE format('ALTER INDEX IF EXISTS %I.idx_file_provenance_attachment RENAME TO idx_provenance_attachment', r.schema_name);
        EXECUTE format('ALTER INDEX IF EXISTS %I.idx_file_provenance_time RENAME TO idx_provenance_time', r.schema_name);
        EXECUTE format('ALTER INDEX IF EXISTS %I.idx_file_provenance_location RENAME TO idx_provenance_location', r.schema_name);
        EXECUTE format('ALTER INDEX IF EXISTS %I.idx_file_provenance_device RENAME TO idx_provenance_device', r.schema_name);
        EXECUTE format('ALTER INDEX IF EXISTS %I.idx_file_provenance_activity RENAME TO idx_provenance_activity', r.schema_name);
        EXECUTE format('ALTER INDEX IF EXISTS %I.idx_file_provenance_metadata RENAME TO idx_provenance_metadata', r.schema_name);

        -- Partial unique index on note_id
        EXECUTE format('CREATE UNIQUE INDEX IF NOT EXISTS idx_provenance_note_id ON %I.provenance(note_id) WHERE note_id IS NOT NULL', r.schema_name);
        EXECUTE format('CREATE INDEX IF NOT EXISTS idx_provenance_note_lookup ON %I.provenance(note_id) WHERE note_id IS NOT NULL', r.schema_name);
    END LOOP;
END;
$archive_loop$;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE provenance IS 'Unified W3C PROV provenance for both file attachments and notes - links entities to temporal, spatial, device, and activity context';
COMMENT ON COLUMN provenance.attachment_id IS 'File attachment target (NULL for note-level provenance)';
COMMENT ON COLUMN provenance.note_id IS 'Note target (NULL for file-level provenance). XOR with attachment_id.';

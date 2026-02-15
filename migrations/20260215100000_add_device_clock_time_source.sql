-- ============================================================================
-- Add 'device_clock' to provenance time_source check constraint (Issue #405)
-- ============================================================================

-- Update public schema
ALTER TABLE provenance DROP CONSTRAINT IF EXISTS check_time_source;
ALTER TABLE provenance ADD CONSTRAINT check_time_source
    CHECK (time_source IN ('exif', 'file_mtime', 'user_manual', 'ai_estimated', 'gps', 'network', 'manual', 'file_metadata', 'device_clock'));

-- Update archive schemas
DO $archive_loop$
DECLARE
    r RECORD;
BEGIN
    FOR r IN SELECT schema_name FROM archive_registry WHERE schema_name != 'public'
    LOOP
        EXECUTE format('ALTER TABLE %I.provenance DROP CONSTRAINT IF EXISTS check_time_source', r.schema_name);
        EXECUTE format($c$ALTER TABLE %I.provenance ADD CONSTRAINT check_time_source CHECK (time_source IN ('exif', 'file_mtime', 'user_manual', 'ai_estimated', 'gps', 'network', 'manual', 'file_metadata', 'device_clock'))$c$, r.schema_name);
    END LOOP;
END;
$archive_loop$;

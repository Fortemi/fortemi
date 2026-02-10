-- Relax provenance target constraint from XOR to "at least one"
-- File provenance from EXIF extraction needs both attachment_id AND note_id
-- so the record is discoverable via both the note and attachment endpoints.

-- Public schema
ALTER TABLE provenance DROP CONSTRAINT IF EXISTS provenance_target_xor;
ALTER TABLE provenance ADD CONSTRAINT provenance_target_check
    CHECK (attachment_id IS NOT NULL OR note_id IS NOT NULL);

-- Apply to all archive schemas
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT schema_name FROM information_schema.schemata
        WHERE schema_name LIKE 'archive_%'
    LOOP
        EXECUTE format('ALTER TABLE %I.provenance DROP CONSTRAINT IF EXISTS provenance_target_xor', r.schema_name);
        EXECUTE format('ALTER TABLE %I.provenance ADD CONSTRAINT provenance_target_check CHECK (attachment_id IS NOT NULL OR note_id IS NOT NULL)', r.schema_name);
    END LOOP;
END $$;

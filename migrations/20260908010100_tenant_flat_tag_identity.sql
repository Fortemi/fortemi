-- #1144: identical tag names in different tenants are distinct identities.
-- Preserve the validated tenant-qualified relationship installed by ADR-090;
-- remove only the obsolete single-column relationship and global primary key.
-- Include registered archive copies. New archives clone the corrected catalog.
DO $tenant_tags$
DECLARE
    target_schema TEXT;
    old_fk RECORD;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.tag', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint c
            WHERE c.conrelid = to_regclass(format('%I.note_tag', target_schema))
              AND c.confrelid = to_regclass(format('%I.tag', target_schema))
              AND c.contype = 'f' AND c.convalidated
              AND array_length(c.conkey, 1) = 2
        ) THEN
            RAISE EXCEPTION 'tenant-qualified tag relationship is required before migration';
        END IF;
        FOR old_fk IN
            SELECT c.conname, c.conrelid::regclass AS child
            FROM pg_constraint c
            WHERE c.confrelid = to_regclass(format('%I.tag', target_schema))
              AND c.contype = 'f' AND array_length(c.conkey, 1) = 1
        LOOP
            EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', old_fk.child, old_fk.conname);
        END LOOP;
        EXECUTE format('ALTER TABLE %I.tag DROP CONSTRAINT tag_pkey', target_schema);
        EXECUTE format('ALTER TABLE %I.tag ADD CONSTRAINT tag_pkey PRIMARY KEY (tenant_id, name)', target_schema);
    END LOOP;
END
$tenant_tags$;

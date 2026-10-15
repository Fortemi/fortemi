-- #1217: optional query instruction; existing sets keep their current contract.
DO $query_template$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.embedding_set', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I.embedding_set ADD COLUMN IF NOT EXISTS query_template TEXT',
                target_schema
            );
        END IF;
    END LOOP;
END
$query_template$;

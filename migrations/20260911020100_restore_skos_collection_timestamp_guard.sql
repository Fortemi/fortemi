-- In-place shard restore supplies the source timestamp. Ordinary native edits
-- retain the original timestamp trigger and function identity.
DO $restore_skos_timestamp$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.skos_collection', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_skos_collection_updated
                BEFORE UPDATE ON %I.skos_collection FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.update_skos_collection_timestamp()', target_schema);
        END IF;
    END LOOP;
END
$restore_skos_timestamp$;

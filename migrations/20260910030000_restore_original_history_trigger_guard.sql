-- Validated restore supplies authoritative history and version numbers itself.
-- Preserve native versioning outside that transaction-local restore context.
DO $restore_history_guard$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.note_original', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER snapshot_original_before_update
                BEFORE UPDATE OF content ON %I.note_original FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.snapshot_original_on_update()', target_schema);
        END IF;
    END LOOP;
END
$restore_history_guard$;

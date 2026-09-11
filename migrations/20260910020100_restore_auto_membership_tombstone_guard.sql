-- Preserve native tombstone filtering alongside the restore-context guard.
DO $restore_membership_guard$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.note', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_auto_add_note_to_embedding_sets
                AFTER INSERT OR UPDATE ON %I.note FOR EACH ROW
                WHEN (NEW.deleted_at IS NULL
                    AND current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.auto_add_note_to_embedding_sets()', target_schema);
        END IF;
    END LOOP;
END
$restore_membership_guard$;

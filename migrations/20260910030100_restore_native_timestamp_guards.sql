-- In-place restore preserves source timestamps and edit flags. Native writes
-- outside the restore transaction retain their original trigger predicates.
DO $restore_timestamp_guards$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.note_original', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER update_original_edited
                BEFORE UPDATE OF content ON %I.note_original FOR EACH ROW
                WHEN (OLD.content IS DISTINCT FROM NEW.content
                    AND current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.update_original_edited_timestamp()', target_schema);
        END IF;
        IF to_regclass(format('%I.note_revision', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER track_revision_user_edit
                BEFORE UPDATE OF content ON %I.note_revision FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.track_revision_edit()', target_schema);
        END IF;
        IF to_regclass(format('%I.named_location', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER named_location_updated
                BEFORE UPDATE ON %I.named_location FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.update_named_location_timestamp()', target_schema);
        END IF;
    END LOOP;
END
$restore_timestamp_guards$;

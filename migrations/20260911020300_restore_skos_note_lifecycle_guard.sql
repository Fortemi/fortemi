-- Note-owned restore does not grant authority to author the referenced concept's
-- lifecycle or snapshot. Ordinary native tagging retains literary-warrant rules.
DO $restore_skos_note_guard$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.note_skos_concept', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_skos_note_count
                AFTER INSERT OR DELETE ON %I.note_skos_concept FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.skos_update_note_count()', target_schema);
        END IF;
    END LOOP;
END
$restore_skos_note_guard$;

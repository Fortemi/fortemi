-- Restore applies declared identities and snapshots, not live authoring effects.
-- Keep the original functions active for ordinary native relationship writes.
DO $restore_skos_relation_guards$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.skos_semantic_relation_edge', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_skos_update_hierarchy
                AFTER INSERT OR UPDATE OR DELETE ON %I.skos_semantic_relation_edge
                FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.skos_update_hierarchy_metadata()', target_schema);
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_skos_reciprocal
                AFTER INSERT ON %I.skos_semantic_relation_edge FOR EACH ROW
                WHEN (NOT NEW.is_inferred AND
                    current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.skos_create_reciprocal_relation()', target_schema);
        END IF;
    END LOOP;
END
$restore_skos_relation_guards$;

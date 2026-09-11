-- Ordinary writes retain immediate guards. Restore may traverse temporarily
-- invalid graphs, but an unconditional deferred constraint enforces final state.
CREATE OR REPLACE FUNCTION public.skos_validate_relation_final_state()
RETURNS TRIGGER AS $$
DECLARE
    current_edge RECORD;
    relation_count BIGINT;
    invalid_hierarchy BOOLEAN;
BEGIN
    -- Deferred events may describe an earlier update or an already deleted row.
    -- Resolve the final row in the triggering schema, not the caller search path.
    EXECUTE format('SELECT subject_id, object_id, relation_type::text AS relation_type
        FROM %I.skos_semantic_relation_edge WHERE id=$1', TG_TABLE_SCHEMA)
        INTO current_edge USING NEW.id;
    IF current_edge.subject_id IS NULL THEN
        RETURN NULL;
    END IF;

    IF current_edge.relation_type = 'broader' THEN
        EXECUTE format('SELECT count(*) FROM %I.skos_semantic_relation_edge
            WHERE subject_id=$1 AND relation_type=''broader''', TG_TABLE_SCHEMA)
            INTO relation_count USING current_edge.subject_id;
        IF relation_count > 3 THEN
            RAISE EXCEPTION 'Polyhierarchy limit exceeded: concept already has 3 broader concepts';
        END IF;
        EXECUTE format('WITH RECURSIVE edges(subject_id,object_id) AS (
                SELECT subject_id,object_id FROM %I.skos_semantic_relation_edge
                WHERE relation_type=''broader''
            ), affected(node) AS (
                SELECT $1::uuid
                UNION SELECT e.subject_id FROM edges e JOIN affected a ON e.object_id=a.node
            ), walk(node,depth) AS (
                SELECT node,0 FROM affected
                UNION SELECT e.object_id,w.depth+1 FROM edges e
                    JOIN walk w ON e.subject_id=w.node WHERE w.depth<6
            ) SELECT EXISTS(SELECT 1 FROM walk WHERE depth=6)', TG_TABLE_SCHEMA)
            INTO invalid_hierarchy USING current_edge.subject_id;
        IF invalid_hierarchy THEN
            RAISE EXCEPTION 'SKOS hierarchy exceeds maximum depth 5 or contains a cycle';
        END IF;
    ELSIF current_edge.relation_type = 'narrower' THEN
        EXECUTE format('SELECT count(*) FROM %I.skos_semantic_relation_edge e
            JOIN %I.skos_concept c ON c.id=e.object_id
            WHERE e.subject_id=$1 AND e.relation_type=''narrower'' AND c.status=''approved''',
            TG_TABLE_SCHEMA, TG_TABLE_SCHEMA)
            INTO relation_count USING current_edge.subject_id;
        IF relation_count > 200 THEN
            RAISE EXCEPTION 'Breadth limit exceeded: concept already has 200 promoted narrower concepts';
        END IF;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DO $atomic_skos_batches$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.skos_semantic_relation_edge', target_schema)) IS NOT NULL THEN
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_skos_validate_broader
                BEFORE INSERT OR UPDATE ON %I.skos_semantic_relation_edge FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.skos_validate_broader_relation()', target_schema);
            EXECUTE format('CREATE OR REPLACE TRIGGER trg_skos_validate_narrower
                BEFORE INSERT OR UPDATE ON %I.skos_semantic_relation_edge FOR EACH ROW
                WHEN (current_setting(''app.shard_import'', true) IS DISTINCT FROM ''on'')
                EXECUTE FUNCTION public.skos_validate_narrower_relation()', target_schema);
            EXECUTE format('DROP TRIGGER IF EXISTS trg_skos_validate_final_state
                ON %I.skos_semantic_relation_edge', target_schema);
            EXECUTE format('CREATE CONSTRAINT TRIGGER trg_skos_validate_final_state
                AFTER INSERT OR UPDATE ON %I.skos_semantic_relation_edge
                DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
                EXECUTE FUNCTION public.skos_validate_relation_final_state()', target_schema);
        END IF;
    END LOOP;
END
$atomic_skos_batches$;

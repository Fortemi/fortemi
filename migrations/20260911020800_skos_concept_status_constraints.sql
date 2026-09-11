-- Status changes alter narrower capacity without changing any relation row.
-- Validate the final transaction state so paired promotions/demotions are atomic.
CREATE OR REPLACE FUNCTION public.skos_validate_concept_status_final_state()
RETURNS TRIGGER LANGUAGE plpgsql AS $concept_status$
DECLARE
    current_status TEXT;
    invalid_breadth BOOLEAN;
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF OLD.status IS NOT DISTINCT FROM NEW.status THEN
            RETURN NULL;
        END IF;
    END IF;
    -- Queued events may refer to superseded updates or deleted rows.
    EXECUTE format('SELECT status::text FROM %I.skos_concept WHERE id=$1', TG_TABLE_SCHEMA)
        INTO current_status USING NEW.id;
    IF current_status IS DISTINCT FROM 'approved' THEN
        RETURN NULL;
    END IF;
    EXECUTE format('SELECT EXISTS (
        SELECT 1 FROM %I.skos_semantic_relation_edge e
        JOIN %I.skos_concept c ON c.id=e.object_id
        WHERE e.relation_type=''narrower'' AND c.status=''approved''
          AND e.subject_id IN (
              SELECT subject_id FROM %I.skos_semantic_relation_edge
              WHERE object_id=$1 AND relation_type=''narrower'')
        GROUP BY e.subject_id HAVING count(*)>200)',
        TG_TABLE_SCHEMA, TG_TABLE_SCHEMA, TG_TABLE_SCHEMA)
        INTO invalid_breadth USING NEW.id;
    IF invalid_breadth THEN
        RAISE EXCEPTION 'Breadth limit exceeded: concept already has 200 promoted narrower concepts';
    END IF;
    RETURN NULL;
END
$concept_status$;

DO $concept_status_guards$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.skos_concept', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;
        -- Acquire before row locks, including updates whose status is set by a
        -- native trigger. Reuse relation coordination rather than a second lock.
        EXECUTE format('CREATE OR REPLACE TRIGGER aaa_skos_serialize_concept_writes
            BEFORE INSERT OR UPDATE OR DELETE ON %I.skos_concept
            FOR EACH STATEMENT EXECUTE FUNCTION public.skos_serialize_relation_writes()', target_schema);
        EXECUTE format('DROP TRIGGER IF EXISTS trg_skos_validate_concept_status_final_state
            ON %I.skos_concept', target_schema);
        EXECUTE format('CREATE CONSTRAINT TRIGGER trg_skos_validate_concept_status_final_state
            AFTER INSERT OR UPDATE ON %I.skos_concept
            DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
            EXECUTE FUNCTION public.skos_validate_concept_status_final_state()', target_schema);
    END LOOP;
END
$concept_status_guards$;

-- Validate the proposed broader graph, including unselected descendants.
-- A cycle always admits a walk of length six; bounded UNION traversal rejects
-- both cycles and paths beyond depth five without trusting cached snapshots.
CREATE OR REPLACE FUNCTION public.skos_validate_broader_relation()
RETURNS TRIGGER AS $$
DECLARE
    broader_count INTEGER;
    invalid_hierarchy BOOLEAN;
BEGIN
    IF NEW.relation_type != 'broader' THEN
        RETURN NEW;
    END IF;

    SELECT COUNT(*) INTO broader_count
    FROM skos_semantic_relation_edge
    WHERE subject_id = NEW.subject_id AND relation_type = 'broader'
      AND id IS DISTINCT FROM NEW.id;

    IF broader_count >= 3 THEN
        RAISE EXCEPTION 'Polyhierarchy limit exceeded: concept already has 3 broader concepts';
    END IF;

    WITH RECURSIVE edges(subject_id, object_id) AS (
        SELECT subject_id, object_id FROM skos_semantic_relation_edge
        WHERE relation_type = 'broader' AND id IS DISTINCT FROM NEW.id
        UNION SELECT NEW.subject_id, NEW.object_id
    ), affected(node) AS (
        SELECT NEW.subject_id
        UNION SELECT e.subject_id FROM edges e JOIN affected a ON e.object_id = a.node
    ), walk(node, depth) AS (
        SELECT node, 0 FROM affected
        UNION SELECT e.object_id, w.depth + 1 FROM edges e
            JOIN walk w ON e.subject_id = w.node WHERE w.depth < 6
    )
    SELECT EXISTS (SELECT 1 FROM walk WHERE depth = 6) INTO invalid_hierarchy;

    IF invalid_hierarchy THEN
        RAISE EXCEPTION 'SKOS hierarchy exceeds maximum depth 5 or contains a cycle';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

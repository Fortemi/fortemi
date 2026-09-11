-- Validate the resulting relation count, not the retained row plus its replay.
-- Keep shared function identities so existing archive triggers use the correction.
CREATE OR REPLACE FUNCTION public.skos_validate_broader_relation()
RETURNS TRIGGER AS $$
DECLARE
    subject_depth INTEGER;
    broader_count INTEGER;
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

    SELECT COALESCE(depth, 0) + 1 INTO subject_depth
    FROM skos_concept
    WHERE id = NEW.object_id;

    IF subject_depth > 5 THEN
        RAISE EXCEPTION 'Depth limit exceeded: adding this relation would exceed maximum depth of 5';
    END IF;

    IF skos_has_circular_hierarchy(NEW.subject_id) THEN
        RAISE EXCEPTION 'Circular hierarchy detected';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION public.skos_validate_narrower_relation()
RETURNS TRIGGER AS $$
DECLARE
    promoted_count INTEGER;
BEGIN
    IF NEW.relation_type != 'narrower' THEN
        RETURN NEW;
    END IF;

    SELECT COUNT(*) INTO promoted_count
    FROM skos_semantic_relation_edge e
    JOIN skos_concept c ON c.id = e.object_id
    WHERE e.subject_id = NEW.subject_id
      AND e.relation_type = 'narrower'
      AND c.status = 'approved'
      AND e.id IS DISTINCT FROM NEW.id;

    IF EXISTS (SELECT 1 FROM skos_concept WHERE id = NEW.object_id AND status = 'approved') THEN
        promoted_count := promoted_count + 1;
    END IF;

    IF promoted_count > 200 THEN
        RAISE EXCEPTION 'Breadth limit exceeded: concept already has 200 promoted narrower concepts';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Only count promoted (approved) children toward the SKOS breadth limit.
--
-- Candidate concepts are auto-created by the extraction pipeline and most
-- never get promoted. Counting them toward the breadth limit causes parent
-- concepts like tool/ and technology/ to hit the cap prematurely. Since only
-- approved concepts participate in SKOS search, the breadth guard should
-- only count those.

CREATE OR REPLACE FUNCTION skos_validate_narrower_relation()
RETURNS TRIGGER AS $$
DECLARE
    promoted_count INTEGER;
BEGIN
    IF NEW.relation_type != 'narrower' THEN
        RETURN NEW;
    END IF;

    -- Count only approved (promoted) children toward the breadth limit
    SELECT COUNT(*) INTO promoted_count
    FROM skos_semantic_relation_edge e
    JOIN skos_concept c ON c.id = e.object_id
    WHERE e.subject_id = NEW.subject_id
      AND e.relation_type = 'narrower'
      AND c.status = 'approved';

    IF promoted_count >= 200 THEN
        RAISE EXCEPTION 'Breadth limit exceeded: concept already has 200 promoted narrower concepts';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

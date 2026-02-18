-- Raise SKOS breadth limit from 50 to 200 children per concept.
--
-- With EXTRACTION_TARGET_CONCEPTS lowered to 5, AI extraction still
-- produces hierarchical concepts under common parents (tool/, technology/,
-- concept/, methodology/). Top-level categories hit the 50-child limit
-- within the first batch of notes. 200 accommodates real-world knowledge
-- bases while still preventing unbounded growth.

CREATE OR REPLACE FUNCTION skos_validate_narrower_relation()
RETURNS TRIGGER AS $$
DECLARE
    narrower_count INTEGER;
BEGIN
    IF NEW.relation_type != 'narrower' THEN
        RETURN NEW;
    END IF;

    -- Check breadth limit (max 200 children)
    SELECT COUNT(*) INTO narrower_count
    FROM skos_semantic_relation_edge
    WHERE subject_id = NEW.subject_id AND relation_type = 'narrower';

    IF narrower_count >= 200 THEN
        RAISE EXCEPTION 'Breadth limit exceeded: concept already has 200 narrower concepts';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Update antipattern detection threshold (warn at >160, was >40)
CREATE OR REPLACE FUNCTION skos_detect_antipatterns(concept_uuid UUID)
RETURNS tag_antipattern[] AS $$
DECLARE
    c RECORD;
    patterns tag_antipattern[] := '{}';
BEGIN
    SELECT * INTO c FROM skos_concept WHERE id = concept_uuid;

    IF c IS NULL THEN
        RETURN patterns;
    END IF;

    -- Orphan: no broader or narrower relations
    IF c.broader_count = 0 AND c.narrower_count = 0 AND c.related_count = 0 THEN
        patterns := array_append(patterns, 'orphan'::tag_antipattern);
    END IF;

    -- Under-used: approved but rarely tagged
    IF c.status = 'approved' AND c.note_count < 3 THEN
        patterns := array_append(patterns, 'under_used'::tag_antipattern);
    END IF;

    -- Too broad: excessive children (approaching limit of 200)
    IF c.narrower_count > 160 THEN
        patterns := array_append(patterns, 'too_broad'::tag_antipattern);
    END IF;

    -- Too deep: depth > 4 (approaching limit)
    IF c.depth > 4 THEN
        patterns := array_append(patterns, 'too_deep'::tag_antipattern);
    END IF;

    -- Polyhierarchy excess: approaching limit
    IF c.broader_count > 2 THEN
        patterns := array_append(patterns, 'polyhierarchy_excess'::tag_antipattern);
    END IF;

    RETURN patterns;
END;
$$ LANGUAGE plpgsql;

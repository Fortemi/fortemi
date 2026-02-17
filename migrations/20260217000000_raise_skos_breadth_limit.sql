-- Raise SKOS breadth limit from 10 to 50 children per concept.
--
-- The original limit of 10 was designed for human-curated taxonomies.
-- With AI extraction (GLiNER + LLM) producing hierarchical concepts like
-- concept/X, methodology/Y, tool/Z, top-level categories quickly hit the
-- limit and reject new children. 50 accommodates automated extraction
-- while still preventing runaway taxonomy explosion.

CREATE OR REPLACE FUNCTION skos_validate_narrower_relation()
RETURNS TRIGGER AS $$
DECLARE
    narrower_count INTEGER;
BEGIN
    IF NEW.relation_type != 'narrower' THEN
        RETURN NEW;
    END IF;

    -- Check breadth limit (max 50 children)
    SELECT COUNT(*) INTO narrower_count
    FROM skos_semantic_relation_edge
    WHERE subject_id = NEW.subject_id AND relation_type = 'narrower';

    IF narrower_count >= 50 THEN
        RAISE EXCEPTION 'Breadth limit exceeded: concept already has 50 narrower concepts';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Also update the antipattern detection threshold (was >8, now >40)
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

    -- Too broad: excessive children (approaching limit of 50)
    IF c.narrower_count > 40 THEN
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

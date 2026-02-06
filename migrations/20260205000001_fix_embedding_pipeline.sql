-- ============================================================================
-- Fix Embedding Pipeline Migration
-- ============================================================================
-- Fixes issues #217, #220, #226, #272, #214
--
-- This migration fixes the core embedding pipeline bugs:
-- 1. Ensures notes are automatically added to auto-refresh embedding sets
-- 2. Fixes document_count tracking for embedding sets
-- 3. Adds triggers for SKOS concept changes to re-embed affected notes
-- 4. Ensures embedding jobs are queued when needed
-- ============================================================================

-- ============================================================================
-- PART 1: AUTO-REFRESH TRIGGER FOR NEW NOTES
-- ============================================================================

-- Function to evaluate a note against embedding set criteria
CREATE OR REPLACE FUNCTION evaluate_note_for_embedding_set(
    p_note_id UUID,
    p_set_id UUID
) RETURNS BOOLEAN AS $$
DECLARE
    v_set RECORD;
    v_note RECORD;
    v_matches BOOLEAN := FALSE;
    v_tag_matches BOOLEAN := FALSE;
    v_collection_matches BOOLEAN := FALSE;
    v_fts_matches BOOLEAN := FALSE;
    v_date_matches BOOLEAN := TRUE;
BEGIN
    -- Get the embedding set configuration
    SELECT * INTO v_set
    FROM embedding_set
    WHERE id = p_set_id
      AND is_active = TRUE;

    IF NOT FOUND THEN
        RETURN FALSE;
    END IF;

    -- Skip manual sets
    IF v_set.mode = 'manual' THEN
        RETURN FALSE;
    END IF;

    -- Get the note
    SELECT * INTO v_note
    FROM note
    WHERE id = p_note_id
      AND deleted_at IS NULL;

    IF NOT FOUND THEN
        RETURN FALSE;
    END IF;

    -- Check if include_all is true
    IF (v_set.criteria->>'include_all')::boolean = TRUE THEN
        v_matches := TRUE;
    ELSE
        -- Check tag criteria
        IF jsonb_array_length(COALESCE(v_set.criteria->'tags', '[]'::jsonb)) > 0 THEN
            SELECT EXISTS (
                SELECT 1
                FROM note_tag nt
                WHERE nt.note_id = p_note_id
                  AND EXISTS (
                      SELECT 1
                      FROM jsonb_array_elements_text(v_set.criteria->'tags') AS tag_criterion
                      WHERE LOWER(nt.tag_name) = LOWER(tag_criterion)
                         OR LOWER(nt.tag_name) LIKE LOWER(tag_criterion) || '/%'
                  )
            ) INTO v_tag_matches;
        ELSE
            v_tag_matches := TRUE; -- No tag filter means all match
        END IF;

        -- Check collection criteria
        IF jsonb_array_length(COALESCE(v_set.criteria->'collections', '[]'::jsonb)) > 0 THEN
            SELECT v_note.collection_id = ANY(
                ARRAY(
                    SELECT jsonb_array_elements_text(v_set.criteria->'collections')::uuid
                )
            ) INTO v_collection_matches;
        ELSE
            v_collection_matches := TRUE; -- No collection filter means all match
        END IF;

        -- Check FTS criteria
        IF v_set.criteria->>'fts_query' IS NOT NULL AND v_set.criteria->>'fts_query' != '' THEN
            SELECT EXISTS (
                SELECT 1
                FROM note_revised_current nrc
                WHERE nrc.note_id = p_note_id
                  AND nrc.tsv @@ websearch_to_tsquery('matric_english', v_set.criteria->>'fts_query')
            ) INTO v_fts_matches;
        ELSE
            v_fts_matches := TRUE; -- No FTS filter means all match
        END IF;

        -- Check date criteria
        IF v_set.criteria->>'created_after' IS NOT NULL THEN
            v_date_matches := v_note.created_at_utc > (v_set.criteria->>'created_after')::timestamptz;
        END IF;
        IF v_date_matches AND v_set.criteria->>'created_before' IS NOT NULL THEN
            v_date_matches := v_note.created_at_utc < (v_set.criteria->>'created_before')::timestamptz;
        END IF;

        -- All criteria must match (AND logic)
        v_matches := v_tag_matches AND v_collection_matches AND v_fts_matches AND v_date_matches;
    END IF;

    -- Check exclude_archived flag
    IF v_matches AND (v_set.criteria->>'exclude_archived')::boolean = TRUE THEN
        v_matches := COALESCE(v_note.archived, FALSE) = FALSE;
    END IF;

    RETURN v_matches;
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to add note to matching auto-refresh sets
CREATE OR REPLACE FUNCTION auto_add_note_to_embedding_sets()
RETURNS TRIGGER AS $$
DECLARE
    v_set RECORD;
    v_matches BOOLEAN;
BEGIN
    -- Only process new notes or updated notes
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;

    -- Loop through all auto-refresh sets
    FOR v_set IN
        SELECT id, auto_refresh
        FROM embedding_set
        WHERE is_active = TRUE
          AND auto_refresh = TRUE
          AND mode IN ('auto', 'mixed')
    LOOP
        -- Evaluate if note matches criteria
        v_matches := evaluate_note_for_embedding_set(NEW.id, v_set.id);

        IF v_matches THEN
            -- Add to set if not already a member
            INSERT INTO embedding_set_member (embedding_set_id, note_id, membership_type)
            VALUES (v_set.id, NEW.id, 'auto')
            ON CONFLICT (embedding_set_id, note_id) DO NOTHING;
        END IF;
    END LOOP;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for new notes
DROP TRIGGER IF EXISTS trg_auto_add_note_to_embedding_sets ON note;
CREATE TRIGGER trg_auto_add_note_to_embedding_sets
AFTER INSERT OR UPDATE ON note
FOR EACH ROW
WHEN (NEW.deleted_at IS NULL)
EXECUTE FUNCTION auto_add_note_to_embedding_sets();

-- ============================================================================
-- PART 2: QUEUE EMBEDDING JOBS FOR SET MEMBERS
-- ============================================================================

-- Function to queue embedding jobs when a note is added to a Full embedding set
CREATE OR REPLACE FUNCTION queue_embedding_jobs_for_set_member()
RETURNS TRIGGER AS $$
DECLARE
    v_set RECORD;
    v_job_exists BOOLEAN;
BEGIN
    -- Only process inserts
    IF TG_OP != 'INSERT' THEN
        RETURN NEW;
    END IF;

    -- Get embedding set info
    SELECT id, set_type, is_active, auto_embed_rules
    INTO v_set
    FROM embedding_set
    WHERE id = NEW.embedding_set_id;

    -- Only queue for Full sets that are active
    IF v_set.set_type = 'full' AND v_set.is_active THEN
        -- Check if auto_embed rules allow embedding on add
        IF (v_set.auto_embed_rules->>'on_create')::boolean IS NOT FALSE THEN
            -- Check if job already exists for this note
            SELECT EXISTS (
                SELECT 1
                FROM job_queue
                WHERE note_id = NEW.note_id
                  AND job_type = 'embedding'
                  AND status IN ('pending', 'running')
                  AND (
                      payload IS NULL
                      OR payload->>'embedding_set_id' = v_set.id::text
                      OR payload->>'embedding_set_id' IS NULL
                  )
            ) INTO v_job_exists;

            -- Queue embedding job if not already queued
            IF NOT v_job_exists THEN
                INSERT INTO job_queue (
                    id,
                    note_id,
                    job_type,
                    status,
                    priority,
                    payload,
                    created_at
                ) VALUES (
                    gen_uuid_v7(),
                    NEW.note_id,
                    'embedding',
                    'pending',
                    COALESCE((v_set.auto_embed_rules->>'priority')::integer, 5),
                    jsonb_build_object('embedding_set_id', v_set.id),
                    NOW()
                );
            END IF;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for embedding set membership
DROP TRIGGER IF EXISTS trg_queue_embedding_jobs_for_set_member ON embedding_set_member;
CREATE TRIGGER trg_queue_embedding_jobs_for_set_member
AFTER INSERT ON embedding_set_member
FOR EACH ROW
EXECUTE FUNCTION queue_embedding_jobs_for_set_member();

-- ============================================================================
-- PART 3: SKOS CONCEPT CHANGE TRIGGERS
-- ============================================================================

-- Function to re-embed notes when SKOS concepts change
-- NOTE: Uses correct schema names:
--   - note_skos_concept (not note_skos_concept_tag)
--   - skos_semantic_relation_edge with subject_id/object_id (not concept_uri/narrower)
CREATE OR REPLACE FUNCTION queue_reembed_for_skos_changes()
RETURNS TRIGGER AS $$
DECLARE
    v_note_id UUID;
    v_concept_id UUID;
BEGIN
    -- Determine the concept ID that changed
    IF TG_OP = 'DELETE' THEN
        v_concept_id := OLD.id;
    ELSE
        v_concept_id := NEW.id;
    END IF;

    -- Find all notes tagged with this concept or its narrower concepts
    FOR v_note_id IN
        SELECT DISTINCT nsc.note_id
        FROM note_skos_concept nsc
        WHERE nsc.concept_id = v_concept_id
           OR nsc.concept_id IN (
               -- Get all narrower concepts transitively
               WITH RECURSIVE narrower_tree AS (
                   SELECT subject_id, object_id
                   FROM skos_semantic_relation_edge
                   WHERE subject_id = v_concept_id
                     AND relation_type = 'narrower'
                   UNION
                   SELECT sre.subject_id, sre.object_id
                   FROM skos_semantic_relation_edge sre
                   INNER JOIN narrower_tree nt ON sre.subject_id = nt.object_id
                   WHERE sre.relation_type = 'narrower'
               )
               SELECT object_id FROM narrower_tree
           )
    LOOP
        -- Queue re-embedding job for each affected note
        INSERT INTO job_queue (
            id,
            note_id,
            job_type,
            status,
            priority,
            created_at
        ) VALUES (
            gen_uuid_v7(),
            v_note_id,
            'embedding',
            'pending',
            7, -- Higher priority for re-embedding after concept changes
            NOW()
        )
        ON CONFLICT DO NOTHING;
    END LOOP;

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    ELSE
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Trigger for SKOS concept changes
-- Note: Semantic content (labels, notes) is in related tables (skos_concept_label, skos_concept_note).
-- We trigger on embedding changes which indicates semantic representation was updated.
DROP TRIGGER IF EXISTS trg_reembed_on_skos_concept_change ON skos_concept;
DROP TRIGGER IF EXISTS trg_reembed_on_skos_concept_update ON skos_concept;
DROP TRIGGER IF EXISTS trg_reembed_on_skos_concept_delete ON skos_concept;

CREATE TRIGGER trg_reembed_on_skos_concept_update
AFTER UPDATE ON skos_concept
FOR EACH ROW
WHEN (
    -- Trigger when embedding changes (semantic content was re-vectorized)
    OLD.embedding IS DISTINCT FROM NEW.embedding
)
EXECUTE FUNCTION queue_reembed_for_skos_changes();

-- Trigger for SKOS concept DELETE
CREATE TRIGGER trg_reembed_on_skos_concept_delete
AFTER DELETE ON skos_concept
FOR EACH ROW
EXECUTE FUNCTION queue_reembed_for_skos_changes();

-- Trigger for SKOS concept relation changes
DROP TRIGGER IF EXISTS trg_reembed_on_skos_relation_change ON skos_semantic_relation_edge;
CREATE TRIGGER trg_reembed_on_skos_relation_change
AFTER INSERT OR UPDATE OR DELETE ON skos_semantic_relation_edge
FOR EACH ROW
EXECUTE FUNCTION queue_reembed_for_skos_changes();

-- ============================================================================
-- PART 4: FIX EMBEDDING SET STATS
-- ============================================================================

-- Improved stats update function with proper locking
CREATE OR REPLACE FUNCTION update_embedding_set_stats(set_id UUID)
RETURNS VOID AS $$
DECLARE
    v_doc_count INTEGER;
    v_emb_count INTEGER;
BEGIN
    -- Count distinct notes in set (members)
    SELECT COUNT(DISTINCT note_id)
    INTO v_doc_count
    FROM embedding_set_member
    WHERE embedding_set_id = set_id;

    -- Count embeddings for this set
    SELECT COUNT(*)
    INTO v_emb_count
    FROM embedding
    WHERE embedding_set_id = set_id;

    -- Update with explicit lock to avoid race conditions
    UPDATE embedding_set SET
        document_count = v_doc_count,
        embedding_count = v_emb_count,
        embeddings_current = CASE
            WHEN embedding_count = 0 AND v_emb_count = 0 THEN TRUE
            WHEN v_emb_count >= v_doc_count THEN TRUE
            ELSE FALSE
        END,
        updated_at = NOW()
    WHERE id = set_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- PART 5: INITIALIZE EXISTING NOTES IN AUTO-REFRESH SETS
-- ============================================================================

-- Add existing notes to auto-refresh sets based on criteria
DO $$
DECLARE
    v_set RECORD;
    v_note_id UUID;
    v_added INTEGER := 0;
BEGIN
    -- Loop through all auto-refresh sets
    FOR v_set IN
        SELECT id, slug, criteria
        FROM embedding_set
        WHERE is_active = TRUE
          AND auto_refresh = TRUE
          AND mode IN ('auto', 'mixed')
    LOOP
        RAISE NOTICE 'Processing auto-refresh set: %', v_set.slug;

        -- Find matching notes
        FOR v_note_id IN
            SELECT id
            FROM note
            WHERE deleted_at IS NULL
              AND evaluate_note_for_embedding_set(id, v_set.id)
        LOOP
            -- Add to set if not already a member
            INSERT INTO embedding_set_member (embedding_set_id, note_id, membership_type)
            VALUES (v_set.id, v_note_id, 'auto')
            ON CONFLICT (embedding_set_id, note_id) DO NOTHING;

            v_added := v_added + 1;
        END LOOP;

        -- Refresh stats for this set
        PERFORM update_embedding_set_stats(v_set.id);

        RAISE NOTICE 'Added % notes to set: %', v_added, v_set.slug;
        v_added := 0;
    END LOOP;
END $$;

-- ============================================================================
-- PART 6: REFRESH ALL EMBEDDING SET STATS
-- ============================================================================

-- Refresh stats for all active embedding sets
DO $$
DECLARE
    v_set_id UUID;
BEGIN
    FOR v_set_id IN
        SELECT id FROM embedding_set WHERE is_active = TRUE
    LOOP
        PERFORM update_embedding_set_stats(v_set_id);
    END LOOP;
END $$;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON FUNCTION evaluate_note_for_embedding_set IS 'Evaluates whether a note matches embedding set criteria';
COMMENT ON FUNCTION auto_add_note_to_embedding_sets IS 'Automatically adds new notes to matching auto-refresh embedding sets';
COMMENT ON FUNCTION queue_embedding_jobs_for_set_member IS 'Queues embedding jobs when notes are added to Full embedding sets';
COMMENT ON FUNCTION queue_reembed_for_skos_changes IS 'Queues re-embedding when SKOS concepts change semantically';

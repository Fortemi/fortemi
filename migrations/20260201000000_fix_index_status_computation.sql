-- Migration: Fix index_status computation logic
-- Issue: #361 - index_status shows "ready" when embedding_count is 0
-- This migration adds automatic index_status computation based on document/embedding counts

-- ============================================================================
-- FUNCTION: Compute index_status based on counts
-- ============================================================================

-- Function to compute the correct index_status based on counts
CREATE OR REPLACE FUNCTION compute_index_status(
    doc_count INTEGER,
    emb_count INTEGER,
    current_status embedding_index_status
)
RETURNS embedding_index_status AS $$
BEGIN
    -- If both counts are 0, status is "empty"
    IF doc_count = 0 AND emb_count = 0 THEN
        RETURN 'empty'::embedding_index_status;
    END IF;

    -- If document_count > 0 but embedding_count = 0, status is "pending"
    IF doc_count > 0 AND emb_count = 0 THEN
        RETURN 'pending'::embedding_index_status;
    END IF;

    -- If embedding_count < document_count, status is "stale"
    IF emb_count < doc_count THEN
        RETURN 'stale'::embedding_index_status;
    END IF;

    -- If currently building, keep that status
    IF current_status = 'building' THEN
        RETURN 'building'::embedding_index_status;
    END IF;

    -- If currently disabled, keep that status
    IF current_status = 'disabled' THEN
        RETURN 'disabled'::embedding_index_status;
    END IF;

    -- Otherwise, if embedding_count >= document_count and both > 0, status is "ready"
    RETURN 'ready'::embedding_index_status;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- NOTE: The 'empty' enum value is added in migration 20260131500000_add_empty_index_status.sql
-- This ensures the value is committed before any functions try to use it.
-- (PostgreSQL requires new enum values to be committed before use)

-- ============================================================================
-- UPDATE FUNCTION: Update stats with automatic status computation
-- ============================================================================

-- Update the stats function to automatically set the correct index_status
CREATE OR REPLACE FUNCTION update_embedding_set_stats(set_id UUID)
RETURNS VOID AS $$
DECLARE
    doc_count INTEGER;
    emb_count INTEGER;
    current_status embedding_index_status;
BEGIN
    -- Get current status
    SELECT index_status INTO current_status
    FROM embedding_set
    WHERE id = set_id;

    -- Calculate counts
    SELECT COUNT(DISTINCT esm.note_id) INTO doc_count
    FROM embedding_set_member esm
    JOIN note n ON n.id = esm.note_id
    WHERE esm.embedding_set_id = set_id
      AND n.deleted_at IS NULL;

    SELECT COUNT(*) INTO emb_count
    FROM embedding e
    JOIN note n ON n.id = e.note_id
    WHERE e.embedding_set_id = set_id
      AND n.deleted_at IS NULL;

    -- Update stats with computed status
    UPDATE embedding_set SET
        document_count = doc_count,
        embedding_count = emb_count,
        index_status = compute_index_status(doc_count, emb_count, current_status),
        updated_at = NOW()
    WHERE id = set_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FIX EXISTING DATA
-- ============================================================================

-- Refresh stats for all sets to fix any existing incorrect statuses
DO $$
DECLARE
    set_record RECORD;
BEGIN
    FOR set_record IN SELECT id FROM embedding_set
    LOOP
        PERFORM update_embedding_set_stats(set_record.id);
    END LOOP;
END $$;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON FUNCTION compute_index_status IS 'Computes the correct index_status based on document_count, embedding_count, and current status';
COMMENT ON FUNCTION update_embedding_set_stats IS 'Updates document and embedding counts for an embedding set, excluding soft-deleted notes, and automatically sets index_status';

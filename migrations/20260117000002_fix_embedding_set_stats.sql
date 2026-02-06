-- ============================================================================
-- Fix Embedding Set Stats to Exclude Deleted Notes + Add PurgeNote Job Type
-- ============================================================================
-- The original stats function counted all members including soft-deleted notes.
-- This fix ensures document_count only includes active (non-deleted) notes.
-- Also adds the purge_note job type for permanent note deletion.
-- ============================================================================

-- Add purge_note job type for permanent deletion
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'purge_note'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'purge_note';
    END IF;
END$$;

-- Update the stats function to exclude soft-deleted notes
CREATE OR REPLACE FUNCTION update_embedding_set_stats(set_id UUID)
RETURNS VOID AS $$
BEGIN
    UPDATE embedding_set SET
        document_count = (
            SELECT COUNT(DISTINCT esm.note_id)
            FROM embedding_set_member esm
            JOIN note n ON n.id = esm.note_id
            WHERE esm.embedding_set_id = set_id
              AND n.deleted_at IS NULL
        ),
        embedding_count = (
            SELECT COUNT(*)
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            WHERE e.embedding_set_id = set_id
              AND n.deleted_at IS NULL
        ),
        updated_at = NOW()
    WHERE id = set_id;
END;
$$ LANGUAGE plpgsql;

-- Also update the view to show accurate counts
DROP VIEW IF EXISTS embedding_set_summary;
CREATE VIEW embedding_set_summary AS
SELECT
    es.id,
    es.name,
    es.slug,
    es.description,
    es.purpose,
    es.usage_hints,
    es.keywords,
    es.mode::text as mode,
    -- Use subquery to get accurate document count excluding deleted notes
    (SELECT COUNT(DISTINCT esm.note_id)
     FROM embedding_set_member esm
     JOIN note n ON n.id = esm.note_id
     WHERE esm.embedding_set_id = es.id AND n.deleted_at IS NULL
    ) as document_count,
    -- Use subquery to get accurate embedding count excluding deleted notes
    (SELECT COUNT(*)
     FROM embedding e
     JOIN note n ON n.id = e.note_id
     WHERE e.embedding_set_id = es.id AND n.deleted_at IS NULL
    ) as embedding_count,
    es.index_status::text as index_status,
    es.is_system,
    es.is_active,
    es.index_size_bytes,
    es.last_indexed_at,
    es.agent_metadata,
    es.criteria,
    ec.model,
    ec.dimension,
    es.created_at,
    es.updated_at
FROM embedding_set es
LEFT JOIN embedding_config ec ON es.embedding_config_id = ec.id
WHERE es.is_active = TRUE
ORDER BY es.is_system DESC, document_count DESC;

-- Refresh stats for all sets to fix any existing incorrect counts
DO $$
DECLARE
    set_record RECORD;
BEGIN
    FOR set_record IN SELECT id FROM embedding_set
    LOOP
        PERFORM update_embedding_set_stats(set_record.id);
    END LOOP;
END $$;

COMMENT ON FUNCTION update_embedding_set_stats IS 'Updates document and embedding counts for an embedding set, excluding soft-deleted notes';

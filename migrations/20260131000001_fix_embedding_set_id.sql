-- Migration: Fix embedding_set_id for existing embeddings
-- Issue: #353 - embeddings were being stored without embedding_set_id
-- This migration backfills the default embedding set ID for all orphaned embeddings

-- Update all embeddings that don't have an embedding_set_id set
-- by assigning them to the default embedding set
UPDATE embedding
SET embedding_set_id = (
    SELECT id FROM embedding_set
    WHERE is_system = TRUE AND slug = 'default'
    LIMIT 1
)
WHERE embedding_set_id IS NULL;

-- Report how many embeddings were fixed
DO $$
DECLARE
    updated_count INT;
    default_set_id UUID;
BEGIN
    -- Get the default set ID
    SELECT id INTO default_set_id
    FROM embedding_set
    WHERE is_system = TRUE AND slug = 'default'
    LIMIT 1;

    -- Check if there are any remaining NULL embeddings
    SELECT COUNT(*) INTO updated_count
    FROM embedding
    WHERE embedding_set_id IS NULL;

    IF updated_count > 0 THEN
        RAISE WARNING 'Found % embeddings still without embedding_set_id. Default set ID: %', updated_count, default_set_id;
    ELSE
        RAISE NOTICE 'All embeddings have been assigned to an embedding set';
    END IF;
END $$;

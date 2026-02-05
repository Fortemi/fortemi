-- Migration: Add 'empty' status to embedding_index_status enum
-- This must be a separate migration because PostgreSQL requires enum values
-- to be committed before they can be used in subsequent statements.
-- See: https://www.postgresql.org/docs/current/sql-altertype.html
--
-- NOTE: ALTER TYPE ... ADD VALUE cannot run inside a transaction block,
-- so this must be in its own migration file.

-- Add "empty" status if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'empty'
                   AND enumtypid = 'embedding_index_status'::regtype) THEN
        ALTER TYPE embedding_index_status ADD VALUE 'empty';
    END IF;
END$$;

COMMENT ON TYPE embedding_index_status IS 'Status of embedding index: pending, building, ready, stale, disabled, empty';

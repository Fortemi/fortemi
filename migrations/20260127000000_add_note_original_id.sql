-- ============================================================================
-- ADD ID COLUMN TO note_original
-- The UUIDv7 code inserts (id, note_id, content, hash) but the table
-- only had (note_id, content, hash, ...). This adds the missing id column.
-- ============================================================================

-- Add id column with default for backfilling existing rows
ALTER TABLE note_original ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid();

-- Backfill any existing rows that got NULL (shouldn't happen with DEFAULT, but safe)
UPDATE note_original SET id = gen_random_uuid() WHERE id IS NULL;

-- Migration: FTS Unicode Normalization (Accent/Diacritic Folding)
-- Issue: #328
-- Description: Enable unaccent extension and create custom text search configuration
--              to support searching for accented characters using unaccented queries
--              (e.g., searching "cafe" matches "café")

-- =============================================================================
-- Step 1: Enable unaccent extension
-- =============================================================================
-- The unaccent extension provides a dictionary that removes accents/diacritics
-- from characters. This is idempotent (IF NOT EXISTS).
CREATE EXTENSION IF NOT EXISTS unaccent;

-- =============================================================================
-- Step 2: Create custom text search configuration
-- =============================================================================
-- Create a new text search configuration based on English but with unaccent
-- preprocessing. The DROP IF EXISTS makes this migration idempotent.
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_english CASCADE;

CREATE TEXT SEARCH CONFIGURATION matric_english (COPY = english);

-- Apply unaccent to word tokens before stemming
-- This ensures "café" → "cafe" → "caf" (stem)
-- Order matters: unaccent first, then stem
ALTER TEXT SEARCH CONFIGURATION matric_english
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, english_stem;

-- =============================================================================
-- Step 3: Update generated column on note_revised_current
-- =============================================================================
-- PostgreSQL requires dropping and recreating generated columns to change them
-- We must:
-- 1. Drop the GIN index that depends on the column
-- 2. Drop the generated column
-- 3. Recreate with new configuration
-- 4. Rebuild the index

-- Drop dependent index
DROP INDEX IF EXISTS idx_note_revised_tsv;

-- Drop the old generated column
ALTER TABLE note_revised_current
  DROP COLUMN IF EXISTS tsv;

-- Recreate with matric_english configuration
ALTER TABLE note_revised_current
  ADD COLUMN tsv tsvector
  GENERATED ALWAYS AS (to_tsvector('matric_english', content)) STORED;

-- Rebuild the GIN index
CREATE INDEX idx_note_revised_tsv ON note_revised_current
  USING gin (tsv);

-- =============================================================================
-- Step 4: Update note_original FTS index
-- =============================================================================
-- The note_original table uses a functional index, so we just recreate it

DROP INDEX IF EXISTS idx_note_original_fts;

CREATE INDEX idx_note_original_fts ON note_original
  USING gin (to_tsvector('matric_english', content));

-- =============================================================================
-- Verification Query (for testing)
-- =============================================================================
-- After this migration, the following should return TRUE:
--
--   SELECT to_tsvector('matric_english', 'café') @@ plainto_tsquery('matric_english', 'cafe');
--
-- Test cases:
--   café/cafe, naïve/naive, résumé/resume, Zürich/Zurich
-- =============================================================================

-- Migration: Phase 1 Multilingual FTS Support
-- Issues: #364 (websearch_to_tsquery), #365 (matric_simple config)
--
-- This migration adds:
-- 1. matric_simple text search configuration (no stemming, CJK-friendly)
-- 2. GIN index on simple tsvector for fallback searches
--
-- Note: websearch_to_tsquery is used in application code, not schema
-- This migration creates the database infrastructure needed.

-- ============================================================================
-- Phase 1A: Create matric_simple text search configuration
-- ============================================================================

-- Create simple configuration with unaccent for Unicode normalization
-- This config tokenizes on whitespace/punctuation without stemming,
-- making it suitable for CJK and mixed-script content.
CREATE TEXT SEARCH CONFIGURATION matric_simple (COPY = simple);

-- Add unaccent mapping for Unicode normalization (e.g., cafe matches cafe)
ALTER TEXT SEARCH CONFIGURATION matric_simple
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, simple;

-- ============================================================================
-- Phase 1B: Create index for matric_simple configuration
-- ============================================================================

-- Create GIN index on simple tsvector for notes content
-- This enables fast FTS queries using the simple configuration
-- CONCURRENTLY allows zero-downtime migration
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_revised_tsv_simple
  ON note_revised_current USING gin (to_tsvector('matric_simple', content));

-- Create GIN index on note titles using simple config
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_title_tsv_simple
  ON note USING gin (to_tsvector('matric_simple', COALESCE(title, '')));

-- Create index on SKOS concept labels for simple search
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_skos_label_tsv_simple
  ON skos_concept_label USING gin (to_tsvector('matric_simple', value));

-- ============================================================================
-- Verification Queries
-- ============================================================================
-- After migration, verify with:
--
-- 1. Verify matric_simple configuration exists:
--    SELECT cfgname FROM pg_ts_config WHERE cfgname = 'matric_simple';
--
-- 2. Verify indexes exist:
--    SELECT indexname FROM pg_indexes
--    WHERE indexname LIKE '%tsv_simple%';
--
-- 3. Test websearch_to_tsquery (application-level change):
--    SELECT websearch_to_tsquery('matric_english', 'cat OR dog');
--    -- Should return: 'cat' | 'dog'
--
-- 4. Test matric_simple with CJK:
--    SELECT to_tsvector('matric_simple', '你好 世界');
--    -- Should return: '世界':2 '你好':1
--
-- ============================================================================
-- Rollback
-- ============================================================================
-- To rollback this migration:
--
-- DROP INDEX IF EXISTS idx_note_revised_tsv_simple;
-- DROP INDEX IF EXISTS idx_note_title_tsv_simple;
-- DROP INDEX IF EXISTS idx_skos_label_tsv_simple;
-- DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_simple;

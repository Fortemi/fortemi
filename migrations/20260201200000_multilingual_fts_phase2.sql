-- Migration: Phase 2 Multilingual FTS Support - Trigram Indexes
-- Issue: #366 (pg_trgm trigram indexes)
--
-- This migration adds:
-- 1. pg_trgm extension for trigram-based similarity search
-- 2. GIN trigram indexes on content and title columns
-- 3. Enables emoji search and fuzzy matching
--
-- pg_trgm provides:
-- - Substring matching (LIKE with index support)
-- - Fuzzy matching via similarity()
-- - Universal UTF-8 support (works with all scripts including emoji)

-- ============================================================================
-- Phase 2A: Enable pg_trgm extension
-- ============================================================================

-- pg_trgm is included with PostgreSQL, no external installation needed
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ============================================================================
-- Phase 2B: Create trigram indexes
-- ============================================================================

-- Trigram index on note content for emoji/symbol/substring search
-- Uses CONCURRENTLY for zero-downtime migration
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_revised_trgm
  ON note_revised_current USING gin (content gin_trgm_ops);

-- Trigram index on note titles
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_title_trgm
  ON note USING gin (title gin_trgm_ops);

-- Trigram index on SKOS concept labels for fuzzy concept search
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_skos_label_trgm
  ON skos_concept_label USING gin (value gin_trgm_ops);

-- ============================================================================
-- Phase 2C: Create similarity search function
-- ============================================================================

-- Trigram similarity threshold (0.3 is a reasonable default)
-- Lower values = more matches but less precision
-- Higher values = fewer matches but more precision
-- Note: This can be adjusted at runtime with SET pg_trgm.similarity_threshold

-- No function needed - we use the similarity() and % operators directly in SQL

-- ============================================================================
-- Verification Queries
-- ============================================================================
-- After migration, verify with:
--
-- 1. Verify pg_trgm extension exists:
--    SELECT extname FROM pg_extension WHERE extname = 'pg_trgm';
--
-- 2. Verify trigram indexes exist:
--    SELECT indexname FROM pg_indexes
--    WHERE indexname LIKE '%trgm%';
--
-- 3. Test emoji search:
--    SELECT * FROM note_revised_current
--    WHERE content LIKE '%🎉%';
--
-- 4. Test similarity search:
--    SELECT content, similarity(content, 'programming') as sim
--    FROM note_revised_current
--    WHERE content % 'programming'
--    ORDER BY sim DESC
--    LIMIT 10;
--
-- 5. Test substring search (LIKE with index):
--    SELECT * FROM note
--    WHERE title ILIKE '%test%';
--
-- ============================================================================
-- Rollback
-- ============================================================================
-- To rollback this migration:
--
-- DROP INDEX IF EXISTS idx_note_revised_trgm;
-- DROP INDEX IF EXISTS idx_note_title_trgm;
-- DROP INDEX IF EXISTS idx_skos_label_trgm;
-- DROP EXTENSION IF EXISTS pg_trgm;

-- Migration: Phase 3 Multilingual FTS Support - pg_bigm & Language Configs
-- Issues: #368 (pg_bigm CJK support), #369 (additional language configs)
--
-- This migration adds:
-- 1. pg_bigm extension for optimized CJK search (optional, graceful fallback)
-- 2. Additional language-specific text search configurations
-- 3. GIN bigram indexes for CJK-optimized search
--
-- Note: pg_bigm requires external installation - this migration handles
-- the case where it's not available gracefully.

-- ============================================================================
-- Phase 3A: Create additional language text search configurations
-- ============================================================================

-- German configuration with unaccent
CREATE TEXT SEARCH CONFIGURATION matric_german (COPY = german);
ALTER TEXT SEARCH CONFIGURATION matric_german
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, german_stem;

-- French configuration with unaccent
CREATE TEXT SEARCH CONFIGURATION matric_french (COPY = french);
ALTER TEXT SEARCH CONFIGURATION matric_french
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, french_stem;

-- Spanish configuration with unaccent
CREATE TEXT SEARCH CONFIGURATION matric_spanish (COPY = spanish);
ALTER TEXT SEARCH CONFIGURATION matric_spanish
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, spanish_stem;

-- Russian configuration with unaccent
CREATE TEXT SEARCH CONFIGURATION matric_russian (COPY = russian);
ALTER TEXT SEARCH CONFIGURATION matric_russian
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, russian_stem;

-- Portuguese configuration with unaccent
CREATE TEXT SEARCH CONFIGURATION matric_portuguese (COPY = portuguese);
ALTER TEXT SEARCH CONFIGURATION matric_portuguese
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, portuguese_stem;

-- ============================================================================
-- Phase 3B: Try to enable pg_bigm (optional)
-- ============================================================================

-- pg_bigm provides 2-gram indexing optimized for CJK
-- It requires external installation - if not available, we skip gracefully
-- The application will fall back to pg_trgm for CJK search

DO $$
BEGIN
    -- Try to create pg_bigm extension
    BEGIN
        CREATE EXTENSION IF NOT EXISTS pg_bigm;
        RAISE NOTICE 'pg_bigm extension enabled - optimal CJK search available';
    EXCEPTION WHEN OTHERS THEN
        RAISE NOTICE 'pg_bigm extension not available - falling back to pg_trgm for CJK search';
    END;
END $$;

-- ============================================================================
-- Phase 3C: Create bigram indexes (only if pg_bigm is available)
-- ============================================================================

-- Create bigram indexes conditionally
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_bigm') THEN
        -- Bigram index on note content (optimized for CJK)
        IF NOT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE indexname = 'idx_note_revised_bigm'
        ) THEN
            EXECUTE 'CREATE INDEX idx_note_revised_bigm
                ON note_revised_current USING gin (content gin_bigm_ops)';
            RAISE NOTICE 'Created bigram index: idx_note_revised_bigm';
        END IF;

        -- Bigram index on note titles
        IF NOT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE indexname = 'idx_note_title_bigm'
        ) THEN
            EXECUTE 'CREATE INDEX idx_note_title_bigm
                ON note USING gin (title gin_bigm_ops)';
            RAISE NOTICE 'Created bigram index: idx_note_title_bigm';
        END IF;

        -- Bigram index on SKOS concept labels
        IF NOT EXISTS (
            SELECT 1 FROM pg_indexes
            WHERE indexname = 'idx_skos_label_bigm'
        ) THEN
            EXECUTE 'CREATE INDEX idx_skos_label_bigm
                ON skos_concept_label USING gin (value gin_bigm_ops)';
            RAISE NOTICE 'Created bigram index: idx_skos_label_bigm';
        END IF;
    ELSE
        RAISE NOTICE 'Skipping bigram indexes - pg_bigm not available';
    END IF;
END $$;

-- ============================================================================
-- Verification Queries
-- ============================================================================
-- After migration, verify with:
--
-- 1. List all matric text search configurations:
--    SELECT cfgname FROM pg_ts_config WHERE cfgname LIKE 'matric_%';
--
-- 2. Check if pg_bigm is available:
--    SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_bigm');
--
-- 3. List bigram indexes (if pg_bigm available):
--    SELECT indexname FROM pg_indexes WHERE indexname LIKE '%bigm%';
--
-- 4. Test German stemming:
--    SELECT to_tsvector('matric_german', 'Häuser sind teuer');
--    -- Should show 'haus':1 'sind':2 'teuer':3
--
-- 5. Test Russian stemming:
--    SELECT to_tsvector('matric_russian', 'Книги интересные');
--
-- 6. Test bigram search (if available):
--    SELECT content, bigm_similarity(content, '人工') as sim
--    FROM note_revised_current
--    WHERE content LIKE likequery('人工')
--    ORDER BY sim DESC;
--
-- ============================================================================
-- Rollback
-- ============================================================================
-- To rollback this migration:
--
-- -- Drop bigram indexes (if they exist)
-- DROP INDEX IF EXISTS idx_note_revised_bigm;
-- DROP INDEX IF EXISTS idx_note_title_bigm;
-- DROP INDEX IF EXISTS idx_skos_label_bigm;
-- DROP EXTENSION IF EXISTS pg_bigm;
--
-- -- Drop language configurations
-- DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_german;
-- DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_french;
-- DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_spanish;
-- DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_russian;
-- DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_portuguese;

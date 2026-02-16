-- Migration: Qualify FTS config names with public schema prefix
-- Issue: #412
-- Description: Fix "invalid byte sequence for encoding UTF8" errors in non-default
--              archives by schema-qualifying all text search config references.
--
-- Root cause: Generated columns and functional indexes use unqualified config names
-- like 'matric_english'. When archive schemas clone tables via LIKE ... INCLUDING ALL,
-- the generated column definition is copied verbatim. In non-default schemas, the
-- unqualified config name can fail to resolve correctly during FTS tokenization,
-- causing multi-byte UTF-8 sequences to be truncated mid-character.
--
-- Fix: Qualify all FTS config references with 'public.' prefix so they resolve
-- unambiguously regardless of the current search_path or schema context.

-- =============================================================================
-- Step 1: Fix note_revised_current generated column (public schema)
-- =============================================================================

-- Drop dependent index first
DROP INDEX IF EXISTS idx_note_revised_tsv;

-- Drop the old generated column with unqualified config
ALTER TABLE note_revised_current
  DROP COLUMN IF EXISTS tsv;

-- Recreate with schema-qualified config name
ALTER TABLE note_revised_current
  ADD COLUMN tsv tsvector
  GENERATED ALWAYS AS (to_tsvector('public.matric_english', content)) STORED;

-- Rebuild the GIN index
CREATE INDEX idx_note_revised_tsv ON note_revised_current
  USING gin (tsv);

-- =============================================================================
-- Step 2: Fix functional indexes on note_original (public schema)
-- =============================================================================

DROP INDEX IF EXISTS idx_note_original_fts;

CREATE INDEX idx_note_original_fts ON note_original
  USING gin (to_tsvector('public.matric_english', content));

-- =============================================================================
-- Step 3: Fix matric_simple functional indexes (public schema)
-- =============================================================================

DROP INDEX IF EXISTS idx_note_revised_tsv_simple;
CREATE INDEX idx_note_revised_tsv_simple
  ON note_revised_current USING gin (to_tsvector('public.matric_simple', content));

DROP INDEX IF EXISTS idx_note_title_tsv_simple;
CREATE INDEX idx_note_title_tsv_simple
  ON note USING gin (to_tsvector('public.matric_simple', COALESCE(title, '')));

DROP INDEX IF EXISTS idx_skos_label_tsv_simple;
CREATE INDEX idx_skos_label_tsv_simple
  ON skos_concept_label USING gin (to_tsvector('public.matric_simple', value));

-- =============================================================================
-- Step 4: Fix existing archive schemas
-- =============================================================================
-- For each existing archive, recreate the generated column with qualified config.
-- This uses a DO block to iterate over all archive schemas.

DO $$
DECLARE
    archive_rec RECORD;
    schema_name TEXT;
BEGIN
    FOR archive_rec IN
        SELECT name FROM archive_registry
    LOOP
        schema_name := 'archive_' || replace(archive_rec.name, '-', '_');

        -- Check if the table exists in this schema
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = schema_name
              AND table_name = 'note_revised_current'
        ) THEN
            -- Drop dependent index
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_revised_tsv', schema_name);

            -- Drop old generated column
            EXECUTE format('ALTER TABLE %I.note_revised_current DROP COLUMN IF EXISTS tsv', schema_name);

            -- Recreate with qualified config
            EXECUTE format(
                'ALTER TABLE %I.note_revised_current ADD COLUMN tsv tsvector GENERATED ALWAYS AS (to_tsvector(''public.matric_english'', content)) STORED',
                schema_name
            );

            -- Rebuild index
            EXECUTE format(
                'CREATE INDEX idx_note_revised_tsv ON %I.note_revised_current USING gin (tsv)',
                schema_name
            );

            -- Fix matric_simple indexes
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_revised_tsv_simple', schema_name);
            EXECUTE format(
                'CREATE INDEX idx_note_revised_tsv_simple ON %I.note_revised_current USING gin (to_tsvector(''public.matric_simple'', content))',
                schema_name
            );

            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_title_tsv_simple', schema_name);
            IF EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = schema_name AND table_name = 'note'
            ) THEN
                EXECUTE format(
                    'CREATE INDEX idx_note_title_tsv_simple ON %I.note USING gin (to_tsvector(''public.matric_simple'', COALESCE(title, '''')))',
                    schema_name
                );
            END IF;

            -- Fix note_original index
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_original_fts', schema_name);
            IF EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = schema_name AND table_name = 'note_original'
            ) THEN
                EXECUTE format(
                    'CREATE INDEX idx_note_original_fts ON %I.note_original USING gin (to_tsvector(''public.matric_english'', content))',
                    schema_name
                );
            END IF;

            -- Fix skos label index
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_skos_label_tsv_simple', schema_name);
            IF EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = schema_name AND table_name = 'skos_concept_label'
            ) THEN
                EXECUTE format(
                    'CREATE INDEX idx_skos_label_tsv_simple ON %I.skos_concept_label USING gin (to_tsvector(''public.matric_simple'', value))',
                    schema_name
                );
            END IF;
        END IF;
    END LOOP;
END $$;

-- Migration: Fix FTS generated columns in archive schemas after LIKE ... INCLUDING ALL
-- Issue: #412
--
-- Root cause: PostgreSQL's LIKE ... INCLUDING ALL uses pg_get_expr() to deparse
-- generated column expressions. pg_get_expr() strips the schema qualifier for
-- objects in the default search_path (e.g., 'public.matric_english' becomes
-- 'matric_english'::regconfig). In non-default schemas, this causes FTS tokenization
-- failures on multi-byte UTF-8 characters.
--
-- This migration fixes ALL existing archive schemas by:
-- 1. Dropping and recreating tsv generated columns with 'public.matric_english'
-- 2. Dropping and recreating FTS functional indexes with qualified config names
-- 3. Dropping any lingering archive-local text search configs

DO $$
DECLARE
    archive_rec RECORD;
    schema_name TEXT;
    ts_config_rec RECORD;
BEGIN
    FOR archive_rec IN
        SELECT ar.name, ar.schema_name
        FROM archive_registry ar
        WHERE ar.is_default = FALSE
    LOOP
        schema_name := archive_rec.schema_name;

        -- Skip if schema doesn't exist (orphaned registry entry)
        IF NOT EXISTS (
            SELECT 1 FROM pg_namespace WHERE nspname = schema_name
        ) THEN
            RAISE NOTICE 'Skipping non-existent schema: %', schema_name;
            CONTINUE;
        END IF;

        -- 1. Fix note_revised_current.tsv generated column
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = schema_name AND table_name = 'note_revised_current'
        ) THEN
            -- Drop tsv column (cascades to dependent indexes like idx_note_revised_tsv)
            EXECUTE format(
                'ALTER TABLE %I.note_revised_current DROP COLUMN IF EXISTS tsv',
                schema_name
            );

            -- Recreate with schema-qualified config
            EXECUTE format(
                'ALTER TABLE %I.note_revised_current ADD COLUMN tsv tsvector GENERATED ALWAYS AS (to_tsvector(''public.matric_english'', content)) STORED',
                schema_name
            );

            -- Rebuild GIN index on stored column
            EXECUTE format(
                'CREATE INDEX idx_note_revised_tsv ON %I.note_revised_current USING gin (tsv)',
                schema_name
            );

            -- Fix matric_simple functional index
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_revised_tsv_simple', schema_name);
            EXECUTE format(
                'CREATE INDEX idx_note_revised_tsv_simple ON %I.note_revised_current USING gin (to_tsvector(''public.matric_simple'', content))',
                schema_name
            );

            RAISE NOTICE 'Fixed FTS columns/indexes on %.note_revised_current', schema_name;
        END IF;

        -- 2. Fix note_original functional index
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = schema_name AND table_name = 'note_original'
        ) THEN
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_original_fts', schema_name);
            EXECUTE format(
                'CREATE INDEX idx_note_original_fts ON %I.note_original USING gin (to_tsvector(''public.matric_english'', content))',
                schema_name
            );
        END IF;

        -- 3. Fix note title functional index
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = schema_name AND table_name = 'note'
        ) THEN
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_note_title_tsv_simple', schema_name);
            EXECUTE format(
                'CREATE INDEX idx_note_title_tsv_simple ON %I.note USING gin (to_tsvector(''public.matric_simple'', COALESCE(title, '''')))',
                schema_name
            );
        END IF;

        -- 4. Fix skos_concept_label functional index
        IF EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = schema_name AND table_name = 'skos_concept_label'
        ) THEN
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_skos_label_tsv_simple', schema_name);
            EXECUTE format(
                'CREATE INDEX idx_skos_label_tsv_simple ON %I.skos_concept_label USING gin (to_tsvector(''public.matric_simple'', value))',
                schema_name
            );
        END IF;

        -- 5. Drop any lingering archive-local text search configs
        FOR ts_config_rec IN
            SELECT c.cfgname::text AS config_name
            FROM pg_ts_config c
            JOIN pg_namespace n ON n.oid = c.cfgnamespace
            WHERE n.nspname = schema_name
        LOOP
            EXECUTE format(
                'DROP TEXT SEARCH CONFIGURATION IF EXISTS %I.%I CASCADE',
                schema_name, ts_config_rec.config_name
            );
            RAISE NOTICE 'Dropped stale TS config: %.%', schema_name, ts_config_rec.config_name;
        END LOOP;

        RAISE NOTICE 'Completed FTS fix for schema: %', schema_name;
    END LOOP;
END $$;

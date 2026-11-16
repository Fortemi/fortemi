-- Index-backed lexical candidates for existing archive schemas.
-- Mirrors the public-schema indexes added by 20261115010000..030000 (those run
-- CONCURRENTLY; DO blocks cannot, so archive indexes use plain CREATE INDEX).
-- New archives receive the indexes through LIKE ... INCLUDING ALL plus the
-- config-qualification fix in matric-db archives.rs.
DO $$
DECLARE
    target_schema TEXT;
BEGIN
    IF to_regclass('public.archive_registry') IS NULL THEN
        RETURN;
    END IF;
    FOR target_schema IN
        SELECT schema_name FROM public.archive_registry WHERE schema_name <> 'public'
    LOOP
        IF target_schema !~ '^archive_[a-z0-9_]+$' THEN
            RAISE EXCEPTION 'refusing unsafe archive schema name';
        END IF;
        IF to_regclass(format('%I.note', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'CREATE INDEX IF NOT EXISTS idx_note_title_tsv_english ON %I.note USING gin (to_tsvector(''public.matric_english'', COALESCE(title, '''')))',
                target_schema);
        END IF;
        IF to_regclass(format('%I.attachment', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'CREATE INDEX IF NOT EXISTS idx_attachment_extracted_fts_english ON %I.attachment USING gin (to_tsvector(''public.matric_english'', COALESCE(extracted_text, '''')))',
                target_schema);
            EXECUTE format(
                'CREATE INDEX IF NOT EXISTS idx_attachment_extracted_fts_simple ON %I.attachment USING gin (to_tsvector(''public.matric_simple'', COALESCE(extracted_text, '''')))',
                target_schema);
        END IF;
    END LOOP;
END $$;

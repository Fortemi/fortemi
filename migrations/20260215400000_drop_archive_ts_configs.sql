-- Drop archive-local text search configs that cause FTS UTF-8 errors (Issue #412).
--
-- Problem: Archive auto-migration created per-schema copies of text search configs
-- (e.g., archive_X.matric_english). When FTS queries use public.matric_english with
-- search_path = [archive_X, public], the archive-local config causes dictionary
-- resolution issues that truncate multi-byte UTF-8 sequences (e.g., → U+2192).
--
-- Fix: All FTS queries now use public.-qualified config names. Archive-local copies
-- are unnecessary and actively harmful. Drop them from all existing archive schemas.

DO $$
DECLARE
    schema_rec RECORD;
    config_rec RECORD;
BEGIN
    FOR schema_rec IN
        SELECT schema_name FROM archive_registry WHERE schema_name != 'public'
    LOOP
        FOR config_rec IN
            SELECT c.cfgname::text AS cfgname
            FROM pg_ts_config c
            JOIN pg_namespace n ON n.oid = c.cfgnamespace
            WHERE n.nspname = schema_rec.schema_name
        LOOP
            EXECUTE format(
                'DROP TEXT SEARCH CONFIGURATION IF EXISTS %I.%I CASCADE',
                schema_rec.schema_name, config_rec.cfgname
            );
            RAISE NOTICE 'Dropped TS config %.%', schema_rec.schema_name, config_rec.cfgname;
        END LOOP;
    END LOOP;
END;
$$;

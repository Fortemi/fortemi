-- #1217: provider output size is separate from the capped storage/index size.
DO $provider_dimension$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public' UNION SELECT schema_name FROM public.archive_registry
    LOOP
        IF to_regclass(format('%I.embedding_config', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I.embedding_config ADD COLUMN IF NOT EXISTS provider_dimension INTEGER',
                target_schema
            );
            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conrelid = to_regclass(format('%I.embedding_config', target_schema))
                  AND conname = 'embedding_config_provider_dimension_check'
            ) THEN
                EXECUTE format(
                    'ALTER TABLE %I.embedding_config ADD CONSTRAINT embedding_config_provider_dimension_check
                     CHECK (provider_dimension IS NULL OR
                            (provider_dimension BETWEEN dimension AND 16384))',
                    target_schema
                );
            END IF;
        END IF;
    END LOOP;
END
$provider_dimension$;

-- Fortemi #1183 R1: entity profile vectors.
--
-- Profiles share the existing embedding storage and per-shape HNSW indexes.
-- R1 allows body_chunk and profile only; a later R2 migration will widen the
-- vector_kind CHECK for page_chunk and rollup once those contracts exist.

DO $entity_profile_vectors$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public'
        UNION
        SELECT schema_name FROM public.archive_registry
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.embedding
                 ADD COLUMN IF NOT EXISTS vector_kind TEXT NOT NULL DEFAULT ''body_chunk'',
                 ADD COLUMN IF NOT EXISTS template_version TEXT,
                 ADD COLUMN IF NOT EXISTS profile_hash TEXT,
                 ADD COLUMN IF NOT EXISTS profile_text TEXT',
            target_schema
        );

        IF NOT EXISTS (
            SELECT 1
            FROM pg_constraint c
            JOIN pg_class r ON r.oid = c.conrelid
            JOIN pg_namespace n ON n.oid = r.relnamespace
            WHERE n.nspname = target_schema
              AND r.relname = 'embedding'
              AND c.conname = 'embedding_vector_kind_check'
        ) THEN
            EXECUTE format(
                'ALTER TABLE %I.embedding
                     ADD CONSTRAINT embedding_vector_kind_check
                     CHECK (vector_kind IN (''body_chunk'', ''profile'')) NOT VALID',
                target_schema
            );
        END IF;

        IF NOT EXISTS (
            SELECT 1
            FROM pg_constraint c
            JOIN pg_class r ON r.oid = c.conrelid
            JOIN pg_namespace n ON n.oid = r.relnamespace
            WHERE n.nspname = target_schema
              AND r.relname = 'embedding'
              AND c.conname = 'embedding_profile_fields_check'
        ) THEN
            EXECUTE format(
                'ALTER TABLE %I.embedding
                     ADD CONSTRAINT embedding_profile_fields_check
                     CHECK (
                         vector_kind <> ''profile''
                         OR (
                             note_id IS NOT NULL
                             AND embedding_set_id IS NOT NULL
                             AND template_version IS NOT NULL
                             AND profile_hash IS NOT NULL
                             AND profile_text IS NOT NULL
                         )
                     ) NOT VALID',
                target_schema
            );
        END IF;

        EXECUTE format('ALTER TABLE %I.embedding VALIDATE CONSTRAINT embedding_vector_kind_check', target_schema);
        EXECUTE format('ALTER TABLE %I.embedding VALIDATE CONSTRAINT embedding_profile_fields_check', target_schema);

        EXECUTE format(
            'ALTER TABLE %I.embedding DROP CONSTRAINT IF EXISTS embedding_note_set_chunk_unique',
            target_schema
        );

        EXECUTE format(
            'CREATE UNIQUE INDEX IF NOT EXISTS embedding_body_chunk_coordinate_unique
                 ON %I.embedding (tenant_id, note_id, embedding_set_id, chunk_index)
                 WHERE vector_kind = ''body_chunk''',
            target_schema
        );

        EXECUTE format(
            'CREATE UNIQUE INDEX IF NOT EXISTS embedding_profile_note_set_unique
                 ON %I.embedding (tenant_id, note_id, embedding_set_id)
                 WHERE vector_kind = ''profile''',
            target_schema
        );

        EXECUTE format(
            'COMMENT ON COLUMN %I.embedding.vector_kind IS %L',
            target_schema,
            'Embedding row role. R1 supports body_chunk and profile; R2 will widen this CHECK for page_chunk and rollup.'
        );
        EXECUTE format(
            'COMMENT ON COLUMN %I.embedding.template_version IS %L',
            target_schema,
            'Profile template version for vector_kind=profile rows; NULL for body chunks.'
        );
        EXECUTE format(
            'COMMENT ON COLUMN %I.embedding.profile_hash IS %L',
            target_schema,
            'Stable digest of the profile_text/template inputs for idempotent profile vector upserts.'
        );
        EXECUTE format(
            'COMMENT ON COLUMN %I.embedding.profile_text IS %L',
            target_schema,
            'Canonical text embedded for vector_kind=profile rows.'
        );
    END LOOP;
END
$entity_profile_vectors$;

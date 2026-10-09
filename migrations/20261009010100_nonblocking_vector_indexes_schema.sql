-- Nonblocking vector index ownership, schema part (#1175/#1181).
-- See 20261009010000 for the job type and the full design note.

ALTER TABLE public.embedding_set
    ADD COLUMN IF NOT EXISTS defer_index_build BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN public.embedding_set.defer_index_build IS
    'When true, set/config changes do not enqueue HNSW index builds until the explicit build-index action clears the flag.';

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
        IF to_regclass(format('%I.embedding_set', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I.embedding_set ADD COLUMN IF NOT EXISTS defer_index_build BOOLEAN NOT NULL DEFAULT FALSE',
                target_schema
            );
        END IF;
    END LOOP;
END $$;

DO $$
DECLARE
    index_row RECORD;
BEGIN
    FOR index_row IN
        SELECT n.nspname, c.relname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'i'
          AND (
            c.relname ~ '^idx_embedding_hnsw_[0-9a-f]{32}$'
            OR c.relname ~ '^idx_attach_emb_hnsw_[0-9a-f]{32}$'
          )
    LOOP
        EXECUTE format('DROP INDEX IF EXISTS %I.%I', index_row.nspname, index_row.relname);
    END LOOP;
END $$;

CREATE OR REPLACE FUNCTION public.queue_embedding_hnsw_index_build_for_schema(
    target_schema TEXT,
    target_config_id UUID,
    force_build BOOLEAN DEFAULT FALSE,
    reason TEXT DEFAULT 'legacy_helper'
)
RETURNS VOID
LANGUAGE plpgsql
AS $queue_embedding_hnsw_index_build_for_schema$
DECLARE
    shape RECORD;
    index_name TEXT;
    payload JSONB;
BEGIN
    IF to_regclass(format('%I.embedding_set', target_schema)) IS NULL THEN
        RETURN;
    END IF;

    EXECUTE format(
        'SELECT ec.dimension, ec.vector_type,
                COALESCE(ec.hnsw_m, 16) AS hnsw_m,
                COALESCE(ec.hnsw_ef_construction, 64) AS hnsw_ef_construction
           FROM public.embedding_config ec
          WHERE ec.id = $1
            AND EXISTS (
                SELECT 1 FROM %I.embedding_set es
                 WHERE es.embedding_config_id = ec.id
                   AND COALESCE(es.defer_index_build, FALSE) IS FALSE
            )',
        target_schema
    )
    INTO shape
    USING target_config_id;

    IF shape.dimension IS NULL THEN
        RETURN;
    END IF;

    index_name := format('idx_embedding_hnsw_%s_%s', shape.vector_type, shape.dimension);

    IF force_build IS FALSE AND EXISTS (
        SELECT 1
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_index i ON i.indexrelid = c.oid
        WHERE n.nspname = target_schema AND c.relname = index_name AND i.indisvalid
    ) THEN
        EXECUTE format(
            'UPDATE %I.embedding_set
                SET index_status = ''ready''::embedding_index_status,
                    last_indexed_at = COALESCE(last_indexed_at, NOW()),
                    updated_at = NOW()
              WHERE embedding_config_id = $1
                AND COALESCE(defer_index_build, FALSE) IS FALSE',
            target_schema
        )
        USING target_config_id;
        RETURN;
    END IF;

    payload := jsonb_build_object(
        'schema', target_schema,
        'dimension', shape.dimension,
        'vector_type', shape.vector_type,
        'hnsw_m', shape.hnsw_m,
        'hnsw_ef_construction', shape.hnsw_ef_construction,
        'force', force_build,
        'reason', reason
    );

    IF force_build IS FALSE AND EXISTS (
        SELECT 1 FROM public.job_queue
        WHERE job_type = 'build_set_index'
          AND status IN ('pending','running')
          AND payload->>'schema' = target_schema
          AND (payload->>'dimension')::int = shape.dimension
          AND payload->>'vector_type' = shape.vector_type
    ) THEN
        RETURN;
    END IF;

    EXECUTE format(
        'UPDATE %I.embedding_set
            SET index_status = ''pending''::embedding_index_status,
                updated_at = NOW()
          WHERE embedding_config_id = $1
            AND COALESCE(defer_index_build, FALSE) IS FALSE',
        target_schema
    )
    USING target_config_id;

    INSERT INTO public.job_queue(id, job_type, status, priority, payload, created_at)
    VALUES (pg_catalog.uuidv7(), 'build_set_index', 'pending', 3, payload, NOW());
END
$queue_embedding_hnsw_index_build_for_schema$;

CREATE OR REPLACE FUNCTION public.recreate_embedding_hnsw_index_for_schema(
    target_schema TEXT,
    target_config_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
AS $recreate_embedding_hnsw_index_for_schema$
BEGIN
    PERFORM public.queue_embedding_hnsw_index_build_for_schema(
        target_schema,
        target_config_id,
        FALSE,
        'legacy_recreate_helper'
    );
END
$recreate_embedding_hnsw_index_for_schema$;

CREATE OR REPLACE FUNCTION public.recreate_embedding_hnsw_index(target_config_id UUID)
RETURNS VOID
LANGUAGE plpgsql
AS $recreate_embedding_hnsw_index$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public'
        UNION
        SELECT schema_name
        FROM public.archive_registry
        WHERE schema_name <> 'public'
    LOOP
        PERFORM public.recreate_embedding_hnsw_index_for_schema(target_schema, target_config_id);
    END LOOP;
END
$recreate_embedding_hnsw_index$;

-- Index builds run in the background (CREATE INDEX CONCURRENTLY cannot run
-- inside the transaction sqlx uses for this file). Queue one build_set_index
-- job per vector shape in use, so existing deployments get their HNSW indexes
-- without blocking writes during the upgrade.
INSERT INTO public.job_queue (id, tenant_id, job_type, status, priority, payload, created_at)
SELECT pg_catalog.uuidv7(), shape.tenant_id, 'build_set_index'::job_type, 'pending'::job_status, 3,
       jsonb_build_object(
           'schema', 'public',
           'dimension', shape.dimension,
           'vector_type', shape.vector_type,
           'hnsw_m', shape.hnsw_m,
           'hnsw_ef_construction', shape.hnsw_ef_construction,
           'force', false,
           'reason', 'migration 20261009010100'),
       NOW()
FROM (
    SELECT DISTINCT ON (ec.tenant_id, ec.dimension, ec.vector_type)
           ec.tenant_id, ec.dimension, ec.vector_type,
           COALESCE(ec.hnsw_m, 16) AS hnsw_m,
           COALESCE(ec.hnsw_ef_construction, 64) AS hnsw_ef_construction
    FROM public.embedding_config ec
    WHERE EXISTS (
        SELECT 1 FROM public.embedding_set es
        WHERE es.embedding_config_id = ec.id AND NOT es.defer_index_build
    ) OR ec.is_default
    ORDER BY ec.tenant_id, ec.dimension, ec.vector_type, ec.is_default DESC, ec.created_at
) shape;

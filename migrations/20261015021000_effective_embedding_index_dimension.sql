-- MRL indexes use the stored (effective) dimension, never the provider raw dimension (#1217).
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
    build_payload JSONB;
BEGIN
    IF to_regclass(format('%I.embedding_set', target_schema)) IS NULL THEN
        RETURN;
    END IF;

    FOR shape IN EXECUTE format(
        'SELECT DISTINCT COALESCE(es.truncate_dim, ec.dimension) AS dimension,
                ec.vector_type,
                COALESCE(ec.hnsw_m, 16) AS hnsw_m,
                COALESCE(ec.hnsw_ef_construction, 64) AS hnsw_ef_construction
           FROM public.embedding_config ec
           JOIN %I.embedding_set es ON es.embedding_config_id = ec.id
          WHERE ec.id = $1
            AND COALESCE(es.defer_index_build, FALSE) IS FALSE',
        target_schema
    ) USING target_config_id
    LOOP
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
                    AND COALESCE(truncate_dim, $2) = $3
                    AND COALESCE(defer_index_build, FALSE) IS FALSE',
                target_schema
            )
            USING target_config_id,
                  (SELECT dimension FROM public.embedding_config WHERE id = target_config_id),
                  shape.dimension;
            CONTINUE;
        END IF;

        build_payload := jsonb_build_object(
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
            CONTINUE;
        END IF;

        EXECUTE format(
            'UPDATE %I.embedding_set
                SET index_status = ''pending''::embedding_index_status,
                    updated_at = NOW()
              WHERE embedding_config_id = $1
                    AND COALESCE(truncate_dim, $2) = $3
                AND COALESCE(defer_index_build, FALSE) IS FALSE',
            target_schema
        )
        USING target_config_id,
                  (SELECT dimension FROM public.embedding_config WHERE id = target_config_id),
                  shape.dimension;

        INSERT INTO public.job_queue(id, job_type, status, priority, payload, created_at)
        VALUES (pg_catalog.uuidv7(), 'build_set_index', 'pending', 3, build_payload, NOW());
    END LOOP;
END
$queue_embedding_hnsw_index_build_for_schema$;


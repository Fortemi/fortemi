-- Fortemi #1175 part A: configurable embedding dimensions and HNSW types.
--
-- The historical schema constrained shared note embeddings, SKOS embeddings,
-- and attachment text embeddings to vector(768). embedding_config.dimension
-- already carried richer model dimensions, so 1024-dimensional seeded configs
-- could be declared but not stored.
--
-- The live embedding table has only the legacy ivfflat vector index from the
-- initial schema; the HNSW embedding index exists only in the unused legacy
-- crates/matric-db/migrations path. This migration moves the storage columns
-- to untyped vector so existing rows are preserved losslessly, and introduces
-- per-config expression HNSW indexes for vector(n) and halfvec(n).
--
-- PostgreSQL partial-index predicates cannot contain subqueries. The helper
-- below materializes the current set ids for a config into the predicate and
-- recreates the index when configs or sets change.

ALTER TABLE public.embedding_config
    ADD COLUMN IF NOT EXISTS vector_type TEXT NOT NULL DEFAULT 'vector';

DO $embedding_config_dimension_checks$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.embedding_config'::regclass
          AND conname = 'embedding_config_vector_type_check'
    ) THEN
        ALTER TABLE public.embedding_config
            ADD CONSTRAINT embedding_config_vector_type_check
            CHECK (vector_type IN ('vector', 'halfvec'));
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.embedding_config'::regclass
          AND conname = 'embedding_config_dimension_type_check'
    ) THEN
        ALTER TABLE public.embedding_config
            ADD CONSTRAINT embedding_config_dimension_type_check
            CHECK (
                (vector_type = 'vector' AND dimension BETWEEN 1 AND 2000)
                OR (vector_type = 'halfvec' AND dimension BETWEEN 1 AND 4000)
            );
    END IF;
END
$embedding_config_dimension_checks$;

CREATE OR REPLACE FUNCTION public.recreate_embedding_hnsw_index_for_schema(
    target_schema TEXT,
    target_config_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
AS $recreate_embedding_hnsw_index_for_schema$
DECLARE
    config_row RECORD;
    set_ids UUID[];
    index_name TEXT;
    attachment_index_name TEXT;
    vector_cast TEXT;
    operator_class TEXT;
    v_hnsw_m INTEGER;
    v_hnsw_ef_construction INTEGER;
BEGIN
    SELECT ec.id, ec.dimension, ec.vector_type, ec.hnsw_m, ec.hnsw_ef_construction
    INTO config_row
    FROM public.embedding_config ec
    WHERE ec.id = target_config_id;

    IF config_row.id IS NULL THEN
        RETURN;
    END IF;

    IF config_row.vector_type = 'vector' AND config_row.dimension NOT BETWEEN 1 AND 2000 THEN
        RAISE EXCEPTION 'vector embedding dimensions must be between 1 and 2000';
    ELSIF config_row.vector_type = 'halfvec' AND config_row.dimension NOT BETWEEN 1 AND 4000 THEN
        RAISE EXCEPTION 'halfvec embedding dimensions must be between 1 and 4000';
    END IF;

    vector_cast := format('%s(%s)', config_row.vector_type, config_row.dimension);
    operator_class := CASE
        WHEN config_row.vector_type = 'halfvec' THEN 'halfvec_cosine_ops'
        ELSE 'vector_cosine_ops'
    END;
    v_hnsw_m := COALESCE(config_row.hnsw_m, 16);
    v_hnsw_ef_construction := COALESCE(config_row.hnsw_ef_construction, 64);
    index_name := 'idx_embedding_hnsw_' || replace(target_config_id::text, '-', '');
    attachment_index_name := 'idx_attach_emb_hnsw_' || replace(target_config_id::text, '-', '');

    IF to_regclass(format('%I.embedding', target_schema)) IS NOT NULL
       AND to_regclass(format('%I.embedding_set', target_schema)) IS NOT NULL THEN
        EXECUTE format(
            'SELECT COALESCE(array_agg(id ORDER BY id), ARRAY[]::uuid[])
             FROM %I.embedding_set WHERE embedding_config_id = $1',
            target_schema
        )
        INTO set_ids
        USING target_config_id;

        EXECUTE format('DROP INDEX IF EXISTS %I.%I', target_schema, index_name);
        EXECUTE format(
            'CREATE INDEX %I ON %I.embedding USING hnsw ((vector::%s) %s)
             WITH (m = %s, ef_construction = %s)
             WHERE vector IS NOT NULL AND embedding_set_id = ANY (%L::uuid[])',
            index_name,
            target_schema,
            vector_cast,
            operator_class,
            v_hnsw_m,
            v_hnsw_ef_construction,
            set_ids
        );
    END IF;

    IF to_regclass(format('%I.attachment_embedding', target_schema)) IS NOT NULL
       AND to_regclass(format('%I.embedding_set', target_schema)) IS NOT NULL THEN
        EXECUTE format(
            'SELECT COALESCE(array_agg(id ORDER BY id), ARRAY[]::uuid[])
             FROM %I.embedding_set WHERE embedding_config_id = $1',
            target_schema
        )
        INTO set_ids
        USING target_config_id;

        EXECUTE format('DROP INDEX IF EXISTS %I.%I', target_schema, attachment_index_name);
        EXECUTE format(
            'CREATE INDEX %I ON %I.attachment_embedding USING hnsw ((vector::%s) %s)
             WITH (m = %s, ef_construction = %s)
             WHERE vector IS NOT NULL AND embedding_set_id = ANY (%L::uuid[])',
            attachment_index_name,
            target_schema,
            vector_cast,
            operator_class,
            v_hnsw_m,
            v_hnsw_ef_construction,
            set_ids
        );
    END IF;
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

DO $embedding_vector_storage$
DECLARE
    target_schema TEXT;
    dependent RECORD;
BEGIN
    -- Views that read these columns (skos_concept_with_label and
    -- skos_governance_dashboard read skos_concept.embedding) block ALTER
    -- COLUMN TYPE. Save their definitions, drop them, and recreate them
    -- unchanged after the conversion.
    CREATE TEMP TABLE embedding_dimension_dependent_views (
        view_oid OID PRIMARY KEY,
        view_schema TEXT NOT NULL,
        view_name TEXT NOT NULL,
        definition TEXT NOT NULL,
        view_comment TEXT
    ) ON COMMIT DROP;
    -- Triggers whose WHEN clause reads these columns block the conversion
    -- the same way (trg_reembed_on_skos_concept_update).
    CREATE TEMP TABLE embedding_dimension_dependent_triggers (
        trigger_oid OID PRIMARY KEY,
        table_schema TEXT NOT NULL,
        table_name TEXT NOT NULL,
        trigger_name TEXT NOT NULL,
        definition TEXT NOT NULL,
        is_enabled "char" NOT NULL
    ) ON COMMIT DROP;

    FOR target_schema IN
        SELECT 'public'
        UNION
        SELECT schema_name
        FROM public.archive_registry
        WHERE schema_name <> 'public'
    LOOP
        INSERT INTO embedding_dimension_dependent_views
        SELECT DISTINCT v.oid, vn.nspname, v.relname,
               pg_get_viewdef(v.oid, true), obj_description(v.oid, 'pg_class')
        FROM pg_depend d
        JOIN pg_rewrite r ON r.oid = d.objid
        JOIN pg_class v ON v.oid = r.ev_class AND v.relkind = 'v'
        JOIN pg_namespace vn ON vn.oid = v.relnamespace
        JOIN pg_class t ON t.oid = d.refobjid
        JOIN pg_namespace tn ON tn.oid = t.relnamespace AND tn.nspname = target_schema
        JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = d.refobjsubid
        WHERE v.oid <> t.oid
          AND (t.relname, a.attname) IN (
              ('embedding', 'vector'),
              ('attachment_embedding', 'vector'),
              ('skos_concept', 'embedding'),
              ('skos_concept_scheme', 'embedding'))
        ON CONFLICT (view_oid) DO NOTHING;

        FOR dependent IN
            SELECT view_schema, view_name FROM embedding_dimension_dependent_views
            WHERE view_schema = target_schema ORDER BY view_oid DESC
        LOOP
            EXECUTE format('DROP VIEW %I.%I', dependent.view_schema, dependent.view_name);
        END LOOP;

        INSERT INTO embedding_dimension_dependent_triggers
        SELECT DISTINCT tg.oid, tn.nspname, t.relname, tg.tgname,
               pg_get_triggerdef(tg.oid, true), tg.tgenabled
        FROM pg_depend d
        JOIN pg_trigger tg ON tg.oid = d.objid AND d.classid = 'pg_trigger'::regclass
        JOIN pg_class t ON t.oid = d.refobjid
        JOIN pg_namespace tn ON tn.oid = t.relnamespace AND tn.nspname = target_schema
        JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = d.refobjsubid
        WHERE NOT tg.tgisinternal
          AND (t.relname, a.attname) IN (
              ('embedding', 'vector'),
              ('attachment_embedding', 'vector'),
              ('skos_concept', 'embedding'),
              ('skos_concept_scheme', 'embedding'))
        ON CONFLICT (trigger_oid) DO NOTHING;

        FOR dependent IN
            SELECT table_schema, table_name, trigger_name
            FROM embedding_dimension_dependent_triggers WHERE table_schema = target_schema
        LOOP
            EXECUTE format('DROP TRIGGER %I ON %I.%I',
                dependent.trigger_name, dependent.table_schema, dependent.table_name);
        END LOOP;
        IF to_regclass(format('%I.embedding', target_schema)) IS NOT NULL THEN
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_embedding_vector', target_schema);
            EXECUTE format(
                'ALTER TABLE %I.embedding ALTER COLUMN vector TYPE vector USING vector::vector',
                target_schema
            );
        END IF;

        IF to_regclass(format('%I.attachment_embedding', target_schema)) IS NOT NULL THEN
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_attachment_embedding_hnsw', target_schema);
            EXECUTE format(
                'ALTER TABLE %I.attachment_embedding ALTER COLUMN vector TYPE vector USING vector::vector',
                target_schema
            );
            EXECUTE format(
                'CREATE INDEX IF NOT EXISTS idx_attachment_embedding_hnsw_768
                 ON %I.attachment_embedding USING hnsw ((vector::vector(768)) vector_cosine_ops)
                 WITH (m = 16, ef_construction = 64)
                 WHERE vector IS NOT NULL',
                target_schema
            );
        END IF;

        IF to_regclass(format('%I.skos_concept_scheme', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I.skos_concept_scheme
                 ALTER COLUMN embedding TYPE vector USING embedding::vector',
                target_schema
            );
        END IF;

        IF to_regclass(format('%I.skos_concept', target_schema)) IS NOT NULL THEN
            EXECUTE format('DROP INDEX IF EXISTS %I.idx_skos_concept_embedding', target_schema);
            EXECUTE format(
                'ALTER TABLE %I.skos_concept
                 ALTER COLUMN embedding TYPE vector USING embedding::vector',
                target_schema
            );
            EXECUTE format(
                'CREATE INDEX IF NOT EXISTS idx_skos_concept_embedding_768
                 ON %I.skos_concept USING ivfflat ((embedding::vector(768)) vector_cosine_ops)
                 WITH (lists = 100)
                 WHERE embedding IS NOT NULL',
                target_schema
            );
        END IF;

        FOR dependent IN
            SELECT view_schema, view_name, definition, view_comment
            FROM embedding_dimension_dependent_views
            WHERE view_schema = target_schema ORDER BY view_oid
        LOOP
            EXECUTE format('CREATE VIEW %I.%I AS %s',
                dependent.view_schema, dependent.view_name, dependent.definition);
            IF dependent.view_comment IS NOT NULL THEN
                EXECUTE format('COMMENT ON VIEW %I.%I IS %L',
                    dependent.view_schema, dependent.view_name, dependent.view_comment);
            END IF;
        END LOOP;

        FOR dependent IN
            SELECT table_schema, table_name, trigger_name, definition, is_enabled
            FROM embedding_dimension_dependent_triggers
            WHERE table_schema = target_schema ORDER BY trigger_oid
        LOOP
            EXECUTE dependent.definition;
            IF dependent.is_enabled = 'D' THEN
                EXECUTE format('ALTER TABLE %I.%I DISABLE TRIGGER %I',
                    dependent.table_schema, dependent.table_name, dependent.trigger_name);
            END IF;
        END LOOP;
    END LOOP;
END
$embedding_vector_storage$;

SELECT public.recreate_embedding_hnsw_index(id)
FROM public.embedding_config
WHERE is_default = TRUE AND dimension = 768 AND vector_type = 'vector';

COMMENT ON COLUMN public.embedding_config.vector_type IS
    'Storage/index vector family for this embedding config: vector or halfvec.';

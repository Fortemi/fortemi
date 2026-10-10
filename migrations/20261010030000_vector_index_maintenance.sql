-- no-transaction
-- Fortemi #1181 R1: vector index maintenance knobs.
--
-- Rationale: entity-profile sets hold ~1M 1024-dim vectors with weekly
-- re-embed/import deltas. Re-embedding churns dead tuples into `embedding`,
-- and stale planner statistics make PostgreSQL misjudge HNSW versus sequential
-- scans. Tightening per-table autovacuum keeps bloat (which slows HNSW scans
-- and wastes heap pages) reclaimed promptly and keeps row estimates fresh
-- after bulk loads, without the exclusive locks of VACUUM FULL.
-- `autovacuum_vacuum_scale_factor = 0.02` triggers vacuum after ~2% of the
-- table changes instead of the 20% default; `autovacuum_analyze_scale_factor
-- = 0.01` refreshes statistics after ~1% changes instead of the 10% default.
-- Per-set `ef_search` stores the query-time HNSW recall knob next to the set
-- it tunes; NULL means the tuning default applies.

ALTER TABLE public.embedding SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01
);

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
        IF to_regclass(format('%I.embedding', target_schema)) IS NOT NULL THEN
            EXECUTE format(
                'ALTER TABLE %I.embedding SET (autovacuum_vacuum_scale_factor = 0.02, autovacuum_analyze_scale_factor = 0.01)',
                target_schema
            );
        END IF;
    END LOOP;
END $$;

ALTER TABLE public.embedding_set
    ADD COLUMN IF NOT EXISTS ef_search INTEGER;

DO $embedding_set_ef_search_checks$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.embedding_set'::regclass
          AND conname = 'embedding_set_ef_search_range'
    ) THEN
        ALTER TABLE public.embedding_set
            ADD CONSTRAINT embedding_set_ef_search_range
            CHECK (ef_search IS NULL OR (ef_search BETWEEN 10 AND 1000));
    END IF;
END
$embedding_set_ef_search_checks$;

COMMENT ON COLUMN public.embedding_set.ef_search IS
    'Per-set HNSW query default (10-1000); NULL falls back to the tuning default';

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
                'ALTER TABLE %I.embedding_set ADD COLUMN IF NOT EXISTS ef_search INTEGER',
                target_schema
            );
            IF NOT EXISTS (
                SELECT 1
                FROM pg_constraint
                WHERE conrelid = format('%I.embedding_set', target_schema)::regclass
                  AND conname = 'embedding_set_ef_search_range'
            ) THEN
                EXECUTE format(
                    'ALTER TABLE %I.embedding_set ADD CONSTRAINT embedding_set_ef_search_range CHECK (ef_search IS NULL OR (ef_search BETWEEN 10 AND 1000))',
                    target_schema
                );
            END IF;
        END IF;
    END LOOP;
END $$;

-- Maintenance job type for post-batch ANALYZE scheduling (#1181 R1).
-- ALTER TYPE ... ADD VALUE cannot run inside a transaction block, hence the
-- -- no-transaction header on this migration.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'analyze_embedding';

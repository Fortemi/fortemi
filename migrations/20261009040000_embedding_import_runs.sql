-- Fortemi #1177 R1 part E: externally generated embedding profile import runs.

CREATE TABLE embedding_import_run (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    set_id UUID NOT NULL,
    run_id TEXT NOT NULL CHECK (length(run_id) BETWEEN 1 AND 200),
    previous_run_id TEXT CHECK (previous_run_id IS NULL OR length(previous_run_id) BETWEEN 1 AND 200),
    status TEXT NOT NULL CHECK (status IN ('applying', 'applied', 'failed')),
    manifest JSONB NOT NULL,
    report JSONB NOT NULL DEFAULT '{}'::jsonb,
    profiles_count INTEGER NOT NULL DEFAULT 0 CHECK (profiles_count >= 0),
    deletions_count INTEGER NOT NULL DEFAULT 0 CHECK (deletions_count >= 0),
    inserted_count INTEGER NOT NULL DEFAULT 0 CHECK (inserted_count >= 0),
    updated_count INTEGER NOT NULL DEFAULT 0 CHECK (updated_count >= 0),
    unchanged_count INTEGER NOT NULL DEFAULT 0 CHECK (unchanged_count >= 0),
    deleted_count INTEGER NOT NULL DEFAULT 0 CHECK (deleted_count >= 0),
    rejected_count INTEGER NOT NULL DEFAULT 0 CHECK (rejected_count >= 0),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    initiated_by_user_id UUID NULL,
    initiated_by_kind TEXT NULL,
    CONSTRAINT embedding_import_run_identity UNIQUE (tenant_id, set_id, run_id),
    CONSTRAINT embedding_import_run_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT embedding_import_run_set_fk FOREIGN KEY (tenant_id, set_id)
        REFERENCES embedding_set(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE
);

CREATE INDEX embedding_import_run_set_started
    ON embedding_import_run (tenant_id, set_id, started_at DESC, run_id);

COMMENT ON TABLE embedding_import_run IS
    'Journal for externally generated profile embedding run-folder imports.';
COMMENT ON COLUMN embedding_import_run.initiated_by_user_id IS
    'Reserved for Fortemi #1191 principal attribution.';
COMMENT ON COLUMN embedding_import_run.initiated_by_kind IS
    'Reserved for Fortemi #1191 principal attribution class.';

DO $embedding_import_run_archives$
DECLARE
    archive_row RECORD;
BEGIN
    FOR archive_row IN
        SELECT schema_name
          FROM archive_registry
         WHERE schema_name <> 'public'
         ORDER BY schema_name
    LOOP
        IF archive_row.schema_name !~ '^archive_[a-z0-9_]+$' THEN
            RAISE EXCEPTION 'refusing unsafe archive schema name';
        END IF;

        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I.embedding_import_run (LIKE public.embedding_import_run INCLUDING ALL)',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.embedding_import_run ADD CONSTRAINT embedding_import_run_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.embedding_import_run ADD CONSTRAINT embedding_import_run_set_fk FOREIGN KEY (tenant_id, set_id) REFERENCES %I.embedding_set(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE',
            archive_row.schema_name,
            archive_row.schema_name
        );
    END LOOP;
END
$embedding_import_run_archives$;

-- Fortemi issue #1090: provider-neutral, source-addressed note upsert.
--
-- These are live persistence tables, not Knowledge Shard components. They are
-- memory-local and therefore exist in public plus every archive schema. Raw
-- external keys stay in source_identity; import journals store only digests and
-- redacted receipts.

CREATE TABLE source_import_run (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    source_namespace TEXT NOT NULL CHECK (length(source_namespace) BETWEEN 1 AND 200),
    import_run_id TEXT NOT NULL CHECK (length(import_run_id) BETWEEN 1 AND 200),
    source_id TEXT CHECK (source_id IS NULL OR length(source_id) BETWEEN 1 AND 500),
    source_schema_version TEXT NOT NULL CHECK (length(source_schema_version) BETWEEN 1 AND 100),
    workspace_id TEXT CHECK (workspace_id IS NULL OR length(workspace_id) BETWEEN 1 AND 500),
    checkpoint JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT source_import_run_identity UNIQUE (tenant_id, source_namespace, import_run_id),
    CONSTRAINT source_import_run_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE TABLE source_identity (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    source_namespace TEXT NOT NULL CHECK (length(source_namespace) BETWEEN 1 AND 200),
    external_id TEXT NOT NULL CHECK (length(external_id) BETWEEN 1 AND 1000),
    note_id UUID NOT NULL,
    source_id TEXT CHECK (source_id IS NULL OR length(source_id) BETWEEN 1 AND 500),
    source_schema_version TEXT NOT NULL CHECK (length(source_schema_version) BETWEEN 1 AND 100),
    content_digest TEXT NOT NULL CHECK (content_digest ~ '^sha256:[0-9a-f]{64}$'),
    import_run_id TEXT NOT NULL CHECK (length(import_run_id) BETWEEN 1 AND 200),
    caller_stable_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT source_identity_external_key UNIQUE (tenant_id, source_namespace, external_id),
    CONSTRAINT source_identity_tenant_note_fk FOREIGN KEY (tenant_id, note_id)
        REFERENCES note(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE,
    CONSTRAINT source_identity_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE UNIQUE INDEX source_identity_caller_stable_id
    ON source_identity (tenant_id, caller_stable_id)
    WHERE caller_stable_id IS NOT NULL;

CREATE TABLE source_import_batch (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    source_namespace TEXT NOT NULL CHECK (length(source_namespace) BETWEEN 1 AND 200),
    import_run_id TEXT NOT NULL CHECK (length(import_run_id) BETWEEN 1 AND 200),
    batch_id TEXT NOT NULL CHECK (length(batch_id) BETWEEN 1 AND 200),
    request_digest TEXT NOT NULL CHECK (request_digest ~ '^sha256:[0-9a-f]{64}$'),
    receipt JSONB NOT NULL,
    checkpoint JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT source_import_batch_identity
        UNIQUE (tenant_id, source_namespace, import_run_id, batch_id),
    CONSTRAINT source_import_batch_run_fk
        FOREIGN KEY (tenant_id, source_namespace, import_run_id)
        REFERENCES source_import_run(tenant_id, source_namespace, import_run_id)
        ON UPDATE RESTRICT ON DELETE CASCADE,
    CONSTRAINT source_import_batch_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

COMMENT ON TABLE source_identity IS
    'Live source-address identity authority. Not part of any Knowledge Shard profile.';
COMMENT ON TABLE source_import_batch IS
    'Idempotency journal containing only request digests and redacted response receipts.';

DO $source_upsert_archives$
DECLARE
    archive_row RECORD;
    table_name TEXT;
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

        FOREACH table_name IN ARRAY ARRAY[
            'source_import_run', 'source_identity', 'source_import_batch'
        ] LOOP
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS %I.%I (LIKE public.%I INCLUDING ALL)',
                archive_row.schema_name,
                table_name,
                table_name
            );
        END LOOP;

        EXECUTE format(
            'ALTER TABLE %I.source_import_run ADD CONSTRAINT source_import_run_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.source_identity ADD CONSTRAINT source_identity_tenant_note_fk FOREIGN KEY (tenant_id, note_id) REFERENCES %I.note(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE',
            archive_row.schema_name,
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.source_identity ADD CONSTRAINT source_identity_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.source_import_batch ADD CONSTRAINT source_import_batch_run_fk FOREIGN KEY (tenant_id, source_namespace, import_run_id) REFERENCES %I.source_import_run(tenant_id, source_namespace, import_run_id) ON UPDATE RESTRICT ON DELETE CASCADE',
            archive_row.schema_name,
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.source_import_batch ADD CONSTRAINT source_import_batch_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );

        FOREACH table_name IN ARRAY ARRAY[
            'source_import_run', 'source_identity', 'source_import_batch'
        ] LOOP
            EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', archive_row.schema_name, table_name);
            EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', archive_row.schema_name, table_name);
            EXECUTE format(
                'CREATE POLICY tenant_isolation ON %I.%I USING (tenant_id = current_setting(''app.current_tenant'')::uuid) WITH CHECK (tenant_id = current_setting(''app.current_tenant'')::uuid)',
                archive_row.schema_name,
                table_name
            );
        END LOOP;
    END LOOP;
END
$source_upsert_archives$;

ALTER TABLE public.source_import_run ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.source_import_run FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.source_import_run
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.source_identity ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.source_identity FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.source_identity
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.source_import_batch ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.source_import_batch FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.source_import_batch
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

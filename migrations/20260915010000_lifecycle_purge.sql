-- Fortemi #1092: previewable graph purge with resumable cleanup and
-- content-free terminal receipts.
--
-- Selector and storage-path material is operational state, never receipt
-- content. It is protected by tenant RLS and cleared or cascade-deleted when
-- no longer needed. Re-erasure targets intentionally survive completion so a
-- restored Knowledge Shard can be purged again before becoming searchable.

CREATE TABLE lifecycle_purge_preview (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    selector_fingerprint TEXT NOT NULL CHECK (selector_fingerprint ~ '^sha256:[0-9a-f]{64}$'),
    selector JSONB NOT NULL CHECK (jsonb_typeof(selector) = 'object'),
    selected_note_ids UUID[] NOT NULL,
    counts JSONB NOT NULL CHECK (jsonb_typeof(counts) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_by UUID,
    CONSTRAINT lifecycle_purge_preview_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT lifecycle_purge_preview_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX lifecycle_purge_preview_expiry
    ON lifecycle_purge_preview (tenant_id, expires_at)
    WHERE consumed_by IS NULL;

CREATE TABLE lifecycle_purge_operation (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    preview_id UUID NOT NULL UNIQUE,
    selector_fingerprint TEXT NOT NULL CHECK (selector_fingerprint ~ '^sha256:[0-9a-f]{64}$'),
    state TEXT NOT NULL CHECK (state IN ('cleanup_pending', 'completed')),
    counts JSONB NOT NULL CHECK (jsonb_typeof(counts) = 'object'),
    search_cleanup_complete BOOLEAN NOT NULL DEFAULT FALSE,
    attempt_count INTEGER NOT NULL DEFAULT 1 CHECK (attempt_count > 0),
    reerasure_count INTEGER NOT NULL DEFAULT 0 CHECK (reerasure_count >= 0),
    last_failure_class TEXT CHECK (
        last_failure_class IS NULL OR last_failure_class IN (
            'filesystem', 'search_cache', 'database', 'invalid_storage_path'
        )
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT lifecycle_purge_operation_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT lifecycle_purge_operation_preview_fk FOREIGN KEY (tenant_id, preview_id)
        REFERENCES lifecycle_purge_preview(tenant_id, id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT lifecycle_purge_operation_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX lifecycle_purge_operation_pending
    ON lifecycle_purge_operation (tenant_id, updated_at, id)
    WHERE state = 'cleanup_pending';

CREATE TABLE lifecycle_purge_blob_cleanup (
    operation_id UUID NOT NULL,
    blob_id UUID NOT NULL,
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    storage_path TEXT,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'completed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_failure_class TEXT CHECK (
        last_failure_class IS NULL OR last_failure_class IN (
            'filesystem', 'invalid_storage_path'
        )
    ),
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (operation_id, blob_id),
    CONSTRAINT lifecycle_purge_blob_operation_fk FOREIGN KEY (tenant_id, operation_id)
        REFERENCES lifecycle_purge_operation(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE,
    CONSTRAINT lifecycle_purge_blob_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX lifecycle_purge_blob_pending
    ON lifecycle_purge_blob_cleanup (tenant_id, operation_id)
    WHERE state = 'pending';

CREATE TABLE lifecycle_purge_erasure_target (
    operation_id UUID NOT NULL,
    note_id UUID NOT NULL,
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    PRIMARY KEY (operation_id, note_id),
    CONSTRAINT lifecycle_purge_target_operation_fk FOREIGN KEY (tenant_id, operation_id)
        REFERENCES lifecycle_purge_operation(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE,
    CONSTRAINT lifecycle_purge_target_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE TABLE deletion_receipt (
    operation_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid,
    outcome TEXT NOT NULL CHECK (outcome = 'completed'),
    counts JSONB NOT NULL CHECK (jsonb_typeof(counts) = 'object'),
    completed_at TIMESTAMPTZ NOT NULL,
    policy JSONB NOT NULL CHECK (jsonb_typeof(policy) = 'object'),
    CONSTRAINT deletion_receipt_operation_fk FOREIGN KEY (tenant_id, operation_id)
        REFERENCES lifecycle_purge_operation(tenant_id, id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT deletion_receipt_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

DO $lifecycle_purge_archives$
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
            'lifecycle_purge_preview',
            'lifecycle_purge_operation',
            'lifecycle_purge_blob_cleanup',
            'lifecycle_purge_erasure_target',
            'deletion_receipt'
        ] LOOP
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS %I.%I (LIKE public.%I INCLUDING ALL)',
                archive_row.schema_name,
                table_name,
                table_name
            );
        END LOOP;

        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_preview ADD CONSTRAINT lifecycle_purge_preview_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_operation ADD CONSTRAINT lifecycle_purge_operation_preview_fk FOREIGN KEY (tenant_id, preview_id) REFERENCES %I.lifecycle_purge_preview(tenant_id, id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name,
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_operation ADD CONSTRAINT lifecycle_purge_operation_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_blob_cleanup ADD CONSTRAINT lifecycle_purge_blob_operation_fk FOREIGN KEY (tenant_id, operation_id) REFERENCES %I.lifecycle_purge_operation(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE',
            archive_row.schema_name,
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_blob_cleanup ADD CONSTRAINT lifecycle_purge_blob_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_erasure_target ADD CONSTRAINT lifecycle_purge_target_operation_fk FOREIGN KEY (tenant_id, operation_id) REFERENCES %I.lifecycle_purge_operation(tenant_id, id) ON UPDATE RESTRICT ON DELETE CASCADE',
            archive_row.schema_name,
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.lifecycle_purge_erasure_target ADD CONSTRAINT lifecycle_purge_target_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.deletion_receipt ADD CONSTRAINT deletion_receipt_operation_fk FOREIGN KEY (tenant_id, operation_id) REFERENCES %I.lifecycle_purge_operation(tenant_id, id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name,
            archive_row.schema_name
        );
        EXECUTE format(
            'ALTER TABLE %I.deletion_receipt ADD CONSTRAINT deletion_receipt_tenant_fk FOREIGN KEY (tenant_id) REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT',
            archive_row.schema_name
        );

        FOREACH table_name IN ARRAY ARRAY[
            'lifecycle_purge_preview',
            'lifecycle_purge_operation',
            'lifecycle_purge_blob_cleanup',
            'lifecycle_purge_erasure_target',
            'deletion_receipt'
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
$lifecycle_purge_archives$;

ALTER TABLE public.lifecycle_purge_preview ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lifecycle_purge_preview FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.lifecycle_purge_preview
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.lifecycle_purge_operation ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lifecycle_purge_operation FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.lifecycle_purge_operation
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.lifecycle_purge_blob_cleanup ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lifecycle_purge_blob_cleanup FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.lifecycle_purge_blob_cleanup
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.lifecycle_purge_erasure_target ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lifecycle_purge_erasure_target FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.lifecycle_purge_erasure_target
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.deletion_receipt ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.deletion_receipt FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON public.deletion_receipt
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

COMMENT ON TABLE deletion_receipt IS
    'Content-free terminal deletion receipt authority for Fortemi #1092.';
COMMENT ON TABLE lifecycle_purge_erasure_target IS
    'Private target IDs retained only to re-erase records resurrected by a restore.';

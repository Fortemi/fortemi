-- #730/#734: resumable tenant-scoped user-secret rewrap lifecycle.

ALTER TABLE public.user_secrets
    ADD COLUMN rewrapped_at TIMESTAMPTZ;

CREATE TABLE public.user_secret_rewrap_job (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL DEFAULT (current_setting('app.current_tenant'))::uuid
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'retryable', 'completed', 'failed')),
    batch_size INTEGER NOT NULL CHECK (batch_size BETWEEN 1 AND 1000),
    cursor_created_at TIMESTAMPTZ,
    cursor_id UUID,
    scanned_count BIGINT NOT NULL DEFAULT 0 CHECK (scanned_count >= 0),
    rewrapped_count BIGINT NOT NULL DEFAULT 0 CHECK (rewrapped_count >= 0),
    skipped_count BIGINT NOT NULL DEFAULT 0 CHECK (skipped_count >= 0),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_id UUID,
    lease_expires_at TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ,
    last_failure_class TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    completed_at TIMESTAMPTZ,
    CHECK ((cursor_created_at IS NULL) = (cursor_id IS NULL)),
    CHECK ((lease_id IS NULL) = (lease_expires_at IS NULL)),
    CHECK (last_failure_class IS NULL OR last_failure_class ~ '^[a-z0-9_]{1,64}$'),
    CHECK ((status = 'completed') = (completed_at IS NOT NULL))
);

CREATE INDEX user_secret_rewrap_job_claim_idx
    ON public.user_secret_rewrap_job (
        tenant_id, status, next_attempt_at, lease_expires_at, created_at, id
    );

ALTER TABLE public.user_secret_rewrap_job ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.user_secret_rewrap_job FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.user_secret_rewrap_job
    FOR ALL
    USING (tenant_id = (current_setting('app.current_tenant'))::uuid)
    WITH CHECK (tenant_id = (current_setting('app.current_tenant'))::uuid);

COMMENT ON TABLE public.user_secret_rewrap_job IS
    'Tenant-scoped leased checkpoints and metadata-only receipts for user-secret DEK rewrap.';
COMMENT ON COLUMN public.user_secret_rewrap_job.last_failure_class IS
    'Stable provider-neutral reason class only; raw provider errors and key identifiers are forbidden.';
COMMENT ON COLUMN public.user_secrets.rewrapped_at IS
    'Last successful atomic wrapped-key replacement; payload ciphertext is unchanged.';

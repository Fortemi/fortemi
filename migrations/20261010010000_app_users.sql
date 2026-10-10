-- Fortemi #1191: OIDC-backed application principals.

CREATE TABLE public.app_user (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    iss TEXT NOT NULL CHECK (length(trim(iss)) > 0 AND iss !~ '[[:cntrl:]]'),
    sub TEXT NOT NULL CHECK (octet_length(sub) BETWEEN 1 AND 255 AND sub !~ '[[:cntrl:]]'),
    email TEXT CHECK (email IS NULL OR email !~ '[[:cntrl:]]'),
    email_verified BOOLEAN NOT NULL DEFAULT false,
    display_name TEXT CHECK (display_name IS NULL OR display_name !~ '[[:cntrl:]]'),
    groups TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    current_scopes TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    kind TEXT NOT NULL CHECK (kind IN ('user', 'service')),
    azp TEXT CHECK (azp IS NULL OR azp !~ '[[:cntrl:]]'),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_oidc_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT app_user_tenant_id_uq UNIQUE (tenant_id, id),
    CONSTRAINT app_user_tenant_issuer_subject UNIQUE (tenant_id, iss, sub),
    CONSTRAINT app_user_groups_no_control CHECK (
        array_to_string(groups, '') !~ '[[:cntrl:]]'
    ),
    CONSTRAINT app_user_scopes_no_control CHECK (
        array_to_string(current_scopes, '') !~ '[[:cntrl:]]'
    )
);

CREATE INDEX app_user_tenant_status_idx ON public.app_user (tenant_id, status, last_seen_at DESC);
CREATE INDEX app_user_tenant_email_idx ON public.app_user (tenant_id, email) WHERE email IS NOT NULL;

ALTER TABLE public.app_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.app_user FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.app_user
    FOR ALL
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

ALTER TABLE public.note
    ADD COLUMN IF NOT EXISTS created_by_user_id UUID,
    ADD COLUMN IF NOT EXISTS updated_by_user_id UUID;

ALTER TABLE public.note
    ADD CONSTRAINT note_created_by_user_fk
    FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES public.app_user(tenant_id, id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;

ALTER TABLE public.note
    ADD CONSTRAINT note_updated_by_user_fk
    FOREIGN KEY (tenant_id, updated_by_user_id)
    REFERENCES public.app_user(tenant_id, id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;

CREATE INDEX note_created_by_user_idx
    ON public.note (tenant_id, created_by_user_id)
    WHERE created_by_user_id IS NOT NULL;

ALTER TABLE public.job_queue
    ADD COLUMN IF NOT EXISTS initiated_by_user_id UUID;

ALTER TABLE public.job_queue
    ADD CONSTRAINT job_queue_initiated_by_user_fk
    FOREIGN KEY (tenant_id, initiated_by_user_id)
    REFERENCES public.app_user(tenant_id, id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;

CREATE INDEX job_queue_initiated_by_user_idx
    ON public.job_queue (tenant_id, initiated_by_user_id)
    WHERE initiated_by_user_id IS NOT NULL;

ALTER TABLE public.embedding_import_run
    ADD CONSTRAINT embedding_import_run_initiated_by_user_fk
    FOREIGN KEY (tenant_id, initiated_by_user_id)
    REFERENCES public.app_user(tenant_id, id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;

DO $audit_runtime_grants$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'fortemi_runtime') THEN
        REVOKE UPDATE, DELETE, TRUNCATE ON TABLE public.audit_event FROM fortemi_runtime;
    END IF;
END
$audit_runtime_grants$;

COMMENT ON TABLE public.app_user IS
    'Tenant-scoped OIDC application principals keyed by exact issuer and subject.';
COMMENT ON COLUMN public.app_user.email IS
    'Display-only OIDC email. Never used for lookup, linking, authorization or uniqueness.';
COMMENT ON COLUMN public.note.created_by_user_id IS
    'Application user that created this note when available from OIDC/PAT context.';
COMMENT ON COLUMN public.job_queue.initiated_by_user_id IS
    'Application user captured at enqueue time; workers use this for async provenance.';

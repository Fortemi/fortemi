-- ADR-093 / #730: tenant- and user-scoped encrypted provider credentials.

CREATE TABLE public.user_secrets (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL DEFAULT (current_setting('app.current_tenant'))::uuid
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    user_id TEXT NOT NULL
        CHECK (octet_length(user_id) BETWEEN 1 AND 256)
        CHECK (user_id ~ '^[A-Za-z0-9_./:@-]+$'),
    provider TEXT NOT NULL
        CHECK (octet_length(provider) BETWEEN 1 AND 64)
        CHECK (provider ~ '^[a-z0-9_.-]+$'),
    name TEXT NOT NULL
        CHECK (length(trim(name)) BETWEEN 1 AND 100)
        CHECK (name !~ '[[:cntrl:]]'),
    encrypted_blob JSONB NOT NULL
        CHECK (jsonb_typeof(encrypted_blob) = 'object')
        CHECK (pg_column_size(encrypted_blob) <= 65536),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    CHECK (last_used_at IS NULL OR last_used_at >= created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

CREATE INDEX user_secrets_tenant_user_provider_state_idx
    ON public.user_secrets (tenant_id, user_id, provider, revoked_at, id);

ALTER TABLE public.user_secrets ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.user_secrets FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.user_secrets
    FOR ALL
    USING (tenant_id = (current_setting('app.current_tenant'))::uuid)
    WITH CHECK (tenant_id = (current_setting('app.current_tenant'))::uuid);

COMMENT ON TABLE public.user_secrets IS
    'Tenant/user-scoped provider credentials encrypted through the provider-neutral KeyProvider envelope contract.';
COMMENT ON COLUMN public.user_secrets.encrypted_blob IS
    'Provider-neutral EncryptedBlob JSON. It is never returned by user-facing metadata endpoints.';
COMMENT ON COLUMN public.user_secrets.revoked_at IS
    'Fortemi lifecycle revocation. This does not claim destruction of provider or KMS key material.';

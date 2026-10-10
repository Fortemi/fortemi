-- Fortemi #1192: user-bound personal access tokens.

CREATE TABLE public.personal_access_token (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    tenant_id UUID NOT NULL DEFAULT current_setting('app.current_tenant')::uuid
        REFERENCES public.tenant_registry(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    user_id UUID NOT NULL,
    token_hash TEXT NOT NULL UNIQUE CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    token_prefix TEXT NOT NULL CHECK (token_prefix ~ '^mm_pat_'),
    token_last4 TEXT NOT NULL CHECK (length(token_last4) = 4),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120 AND name !~ '[[:cntrl:]]'),
    scopes TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'revoked')),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    last_used_at TIMESTAMPTZ,
    last_used_ip TEXT CHECK (last_used_ip IS NULL OR length(last_used_ip) <= 45),
    use_count BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT personal_access_token_user_fk
        FOREIGN KEY (tenant_id, user_id)
        REFERENCES public.app_user(tenant_id, id)
        ON UPDATE RESTRICT ON DELETE CASCADE,
    CONSTRAINT personal_access_token_scopes_no_control CHECK (
        array_to_string(scopes, '') !~ '[[:cntrl:]]'
    )
);

CREATE INDEX personal_access_token_tenant_user_idx
    ON public.personal_access_token (tenant_id, user_id, created_at DESC);
CREATE INDEX personal_access_token_status_expiry_idx
    ON public.personal_access_token (status, expires_at)
    WHERE status IN ('active', 'suspended');

ALTER TABLE public.personal_access_token ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.personal_access_token FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON public.personal_access_token
    FOR ALL
    USING (tenant_id = current_setting('app.current_tenant')::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid);

COMMENT ON TABLE public.personal_access_token IS
    'User-bound mm_pat_ bearer credentials. Token material is shown once; only an HMAC-SHA-256 value is stored.';
COMMENT ON COLUMN public.personal_access_token.token_hash IS
    'HMAC-SHA-256 over the full token using the deployment PAT pepper derived with HKDF.';

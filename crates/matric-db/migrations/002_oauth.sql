-- OAuth2 with Dynamic Client Registration (RFC 7591)
-- Supports authorization code flow, client credentials, and token refresh

-- OAuth client registration
CREATE TABLE IF NOT EXISTS oauth_client (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id TEXT UNIQUE NOT NULL,
    client_secret_hash TEXT NOT NULL,
    client_name TEXT NOT NULL,
    client_uri TEXT,
    logo_uri TEXT,

    -- Redirect URIs (array for multiple)
    redirect_uris TEXT[] NOT NULL DEFAULT '{}',

    -- Grant types: authorization_code, client_credentials, refresh_token
    grant_types TEXT[] NOT NULL DEFAULT '{authorization_code,refresh_token}',

    -- Response types: code, token
    response_types TEXT[] NOT NULL DEFAULT '{code}',

    -- Scopes this client can request
    scope TEXT NOT NULL DEFAULT 'read',

    -- Token endpoint auth method: client_secret_basic, client_secret_post, none
    token_endpoint_auth_method TEXT NOT NULL DEFAULT 'client_secret_basic',

    -- Software statement (JWT) for signed metadata
    software_statement TEXT,
    software_id TEXT,
    software_version TEXT,

    -- Contacts for this client
    contacts TEXT[],

    -- Policy and TOS URIs
    policy_uri TEXT,
    tos_uri TEXT,

    -- JWKS for client assertion auth
    jwks_uri TEXT,
    jwks JSONB,

    -- Client status
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_confidential BOOLEAN NOT NULL DEFAULT TRUE,

    -- Registration metadata
    registration_access_token TEXT UNIQUE,
    registration_client_uri TEXT,

    -- Timestamps
    client_id_issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    client_secret_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- OAuth authorization codes (short-lived, single-use)
CREATE TABLE IF NOT EXISTS oauth_authorization_code (
    code TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_client(client_id) ON DELETE CASCADE,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL,
    state TEXT,

    -- PKCE support (RFC 7636)
    code_challenge TEXT,
    code_challenge_method TEXT, -- plain or S256

    -- User context (for future user auth)
    user_id TEXT,

    -- Single use tracking
    used BOOLEAN NOT NULL DEFAULT FALSE,

    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- OAuth access and refresh tokens
CREATE TABLE IF NOT EXISTS oauth_token (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Token values (hashed for security)
    access_token_hash TEXT UNIQUE NOT NULL,
    refresh_token_hash TEXT UNIQUE,

    -- Token metadata
    token_type TEXT NOT NULL DEFAULT 'Bearer',
    scope TEXT NOT NULL,

    -- Client and user association
    client_id TEXT NOT NULL REFERENCES oauth_client(client_id) ON DELETE CASCADE,
    user_id TEXT,

    -- Expiration
    access_token_expires_at TIMESTAMPTZ NOT NULL,
    refresh_token_expires_at TIMESTAMPTZ,

    -- Revocation
    revoked BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

-- Predefined scopes
CREATE TABLE IF NOT EXISTS oauth_scope (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default scopes
INSERT INTO oauth_scope (name, description, is_default) VALUES
    ('read', 'Read access to notes, tags, and search', TRUE),
    ('write', 'Create and update notes and tags', FALSE),
    ('delete', 'Delete notes', FALSE),
    ('admin', 'Administrative access including job management', FALSE),
    ('mcp', 'MCP server access (includes read and write)', FALSE)
ON CONFLICT (name) DO NOTHING;

-- API keys (simpler alternative to OAuth for trusted integrations)
CREATE TABLE IF NOT EXISTS api_key (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash TEXT UNIQUE NOT NULL,
    key_prefix TEXT NOT NULL, -- First 8 chars for identification
    name TEXT NOT NULL,
    description TEXT,
    scope TEXT NOT NULL DEFAULT 'read',

    -- Rate limiting
    rate_limit_per_minute INTEGER DEFAULT 60,
    rate_limit_per_hour INTEGER DEFAULT 1000,

    -- Usage tracking
    last_used_at TIMESTAMPTZ,
    use_count BIGINT NOT NULL DEFAULT 0,

    -- Status
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    expires_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Rate limiting tracking
CREATE TABLE IF NOT EXISTS rate_limit (
    key TEXT PRIMARY KEY, -- client_id or api_key_id + window
    count INTEGER NOT NULL DEFAULT 0,
    window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    window_end TIMESTAMPTZ NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_oauth_client_client_id ON oauth_client(client_id);
CREATE INDEX IF NOT EXISTS idx_oauth_client_active ON oauth_client(is_active) WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_oauth_code_expires ON oauth_authorization_code(expires_at);
CREATE INDEX IF NOT EXISTS idx_oauth_code_client ON oauth_authorization_code(client_id);

CREATE INDEX IF NOT EXISTS idx_oauth_token_client ON oauth_token(client_id);
CREATE INDEX IF NOT EXISTS idx_oauth_token_expires ON oauth_token(access_token_expires_at);
CREATE INDEX IF NOT EXISTS idx_oauth_token_refresh_expires ON oauth_token(refresh_token_expires_at);
CREATE INDEX IF NOT EXISTS idx_oauth_token_active ON oauth_token(revoked, access_token_expires_at)
    WHERE revoked = FALSE;

CREATE INDEX IF NOT EXISTS idx_api_key_prefix ON api_key(key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_key_active ON api_key(is_active) WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_rate_limit_window ON rate_limit(window_end);

-- Cleanup function for expired tokens and codes
CREATE OR REPLACE FUNCTION cleanup_expired_oauth() RETURNS void AS $$
BEGIN
    -- Delete expired authorization codes
    DELETE FROM oauth_authorization_code WHERE expires_at < NOW();

    -- Delete expired tokens (keep revoked for audit for 30 days)
    DELETE FROM oauth_token
    WHERE access_token_expires_at < NOW() - INTERVAL '30 days'
      AND (refresh_token_expires_at IS NULL OR refresh_token_expires_at < NOW() - INTERVAL '30 days');

    -- Clean up old rate limit entries
    DELETE FROM rate_limit WHERE window_end < NOW();
END;
$$ LANGUAGE plpgsql;

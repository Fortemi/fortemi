-- OAuth2 and API Key tables for MCP authentication
-- Implements RFC 6749 (OAuth 2.0), RFC 7591 (Dynamic Client Registration), RFC 7662 (Introspection)

-- OAuth2 Clients (Dynamic Client Registration)
CREATE TABLE oauth_client (
    id UUID PRIMARY KEY,
    client_id TEXT UNIQUE NOT NULL,
    client_secret_hash TEXT NOT NULL,
    client_name TEXT NOT NULL,
    client_uri TEXT,
    logo_uri TEXT,
    redirect_uris TEXT[] NOT NULL DEFAULT '{}',
    grant_types TEXT[] NOT NULL DEFAULT ARRAY['authorization_code', 'refresh_token'],
    response_types TEXT[] NOT NULL DEFAULT ARRAY['code'],
    scope TEXT NOT NULL DEFAULT 'read',
    token_endpoint_auth_method TEXT NOT NULL DEFAULT 'client_secret_basic',
    software_id TEXT,
    software_version TEXT,
    software_statement TEXT,
    contacts TEXT[] DEFAULT '{}',
    policy_uri TEXT,
    tos_uri TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_confidential BOOLEAN NOT NULL DEFAULT true,
    registration_access_token TEXT,
    client_id_issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    client_secret_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_oauth_client_client_id ON oauth_client(client_id);
CREATE INDEX idx_oauth_client_active ON oauth_client(is_active) WHERE is_active = true;

-- Authorization Codes (single-use, short-lived)
CREATE TABLE oauth_authorization_code (
    code TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_client(client_id) ON DELETE CASCADE,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL,
    state TEXT,
    code_challenge TEXT,
    code_challenge_method TEXT,
    user_id TEXT,
    used BOOLEAN NOT NULL DEFAULT false,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_oauth_authz_code_client ON oauth_authorization_code(client_id);
CREATE INDEX idx_oauth_authz_code_expires ON oauth_authorization_code(expires_at);

-- OAuth Tokens (access and refresh)
CREATE TABLE oauth_token (
    id UUID PRIMARY KEY,
    access_token_hash TEXT UNIQUE NOT NULL,
    refresh_token_hash TEXT UNIQUE,
    token_type TEXT NOT NULL DEFAULT 'Bearer',
    scope TEXT NOT NULL,
    client_id TEXT NOT NULL REFERENCES oauth_client(client_id) ON DELETE CASCADE,
    user_id TEXT,
    access_token_expires_at TIMESTAMPTZ NOT NULL,
    refresh_token_expires_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT false,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_oauth_token_access ON oauth_token(access_token_hash);
CREATE INDEX idx_oauth_token_refresh ON oauth_token(refresh_token_hash) WHERE refresh_token_hash IS NOT NULL;
CREATE INDEX idx_oauth_token_client ON oauth_token(client_id);
CREATE INDEX idx_oauth_token_active ON oauth_token(revoked, access_token_expires_at)
    WHERE revoked = false;

-- API Keys (simpler authentication for scripts/tools)
CREATE TABLE api_key (
    id UUID PRIMARY KEY,
    key_hash TEXT UNIQUE NOT NULL,
    key_prefix TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    scope TEXT NOT NULL DEFAULT 'read',
    rate_limit_per_minute INTEGER DEFAULT 60,
    rate_limit_per_hour INTEGER DEFAULT 1000,
    last_used_at TIMESTAMPTZ,
    use_count BIGINT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_key_hash ON api_key(key_hash);
CREATE INDEX idx_api_key_active ON api_key(is_active, expires_at)
    WHERE is_active = true;
CREATE INDEX idx_api_key_prefix ON api_key(key_prefix);

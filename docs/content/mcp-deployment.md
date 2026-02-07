# MCP Deployment Guide

Advanced guide for deploying and managing the Fortemi MCP server with automatic credential management.

## Overview

The Fortemi MCP server provides AI agents (Claude Code, Claude Desktop, etc.) with direct access to your knowledge base through the Model Context Protocol. In bundle deployment, the MCP server runs alongside the API with automatic OAuth credential management.

## How MCP Authentication Works

### Architecture

```
┌───────────────┐                    ┌──────────────┐                  ┌─────────────┐
│               │  Bearer Token      │              │  Introspect      │             │
│  MCP Client   ├───────────────────>│  MCP Server  ├─────────────────>│  Fortemi    │
│  (Claude)     │                    │  (port 3001) │  (Client Auth)   │  API        │
│               │<───────────────────┤              │<─────────────────┤  (port 3000)│
└───────────────┘  Response          └──────────────┘  active/inactive └─────────────┘
```

### Authentication Flow

1. **Client authentication**: Claude Code obtains an OAuth2 access token via authorization code flow
2. **MCP request**: Client sends tool call to MCP server with `Authorization: Bearer <token>` header
3. **Token introspection**: MCP server validates the bearer token by calling the API's introspection endpoint
4. **Introspection authentication**: MCP uses its own OAuth client credentials to authenticate the introspection request
5. **Response**: API returns `{"active": true}` if token is valid, MCP processes the request

**Key insight:** The MCP server itself needs OAuth credentials to validate client tokens. This is handled automatically by the bundle entrypoint.

## Credential Lifecycle

### Auto-Registration (Default)

The bundle entrypoint automatically manages MCP OAuth credentials on startup:

**Startup sequence:**

1. PostgreSQL starts and waits for readiness
2. API starts and waits for health check to pass
3. Entrypoint checks for persisted credentials at `$PGDATA/.fortemi-mcp-credentials`
4. If credentials exist:
   - Loads credentials from file
   - Validates against API's introspection endpoint
   - If valid, proceeds to step 7
   - If invalid, proceeds to step 5
5. If credentials missing or invalid:
   - Registers new OAuth client via `POST /oauth/register`
   - Request body: `{"client_name":"MCP Server (auto-registered)","grant_types":["client_credentials"],"scope":"mcp read write"}`
6. Persists new credentials to `$PGDATA/.fortemi-mcp-credentials`
7. Starts MCP server with valid credentials

**Credential persistence:**

- Credentials are stored on the pgdata volume at `/var/lib/postgresql/data/.fortemi-mcp-credentials`
- File format: Shell-sourceable environment variables
- File permissions: `0600` (owner read/write only)
- Lifecycle: Tied to pgdata volume

**Priority order:**

When MCP credentials are provided from multiple sources, the entrypoint uses this priority:

1. Persisted file on pgdata volume (highest priority - always matches DB state)
2. Environment variables from `.env` (used if persisted file doesn't exist)
3. Auto-registration (fallback if no credentials provided)

**Why persisted file takes precedence:** After a clean deploy (`docker compose down -v`), the database is wiped and all OAuth clients are deleted. Environment variables from `.env` may contain stale credentials from the previous deployment, so the entrypoint validates them and re-registers if needed.

## Deployment Scenarios

| Scenario | Credential Source | MCP Status | Action Required |
|----------|------------------|------------|-----------------|
| First deploy | Auto-registered | Working | None - credentials auto-generated |
| Routine restart | Loaded from volume | Working | None - credentials persist |
| Image update | Loaded from volume | Working | None - `pull` + `up` preserves volume |
| Clean deploy (`down -v`) | Auto-registered | Working | None - credentials regenerated automatically |
| Manual override | From `.env` | Working | Set `MCP_CLIENT_ID` and `MCP_CLIENT_SECRET` |
| Stale env vars + volume wipe | Auto-registered | Working | Old env vars ignored, fresh credentials generated |

**Key takeaway:** In normal operations, you never need to manually manage MCP credentials. The entrypoint handles all credential lifecycle events automatically.

## Security Considerations

### Credential Storage

**Plaintext on pgdata volume:**

- MCP credentials are stored in plaintext at `$PGDATA/.fortemi-mcp-credentials`
- Security posture: Same as PostgreSQL data itself
- If an attacker has access to the pgdata volume, they already have access to all knowledge base data
- Credentials grant `mcp read write` scope, equivalent to full database access

**Recommendation:** Secure the pgdata volume using Docker volume encryption or disk-level encryption.

### Token Scopes

**MCP client scope:**

```json
{
  "scope": "mcp read write"
}
```

- `mcp`: Grants access to MCP-specific endpoints (tool introspection)
- `read`: Grants read access to all knowledge base resources
- `write`: Grants write access (note creation, updates, deletions)

**Scope enforcement:** When `REQUIRE_AUTH=true`, all API endpoints enforce scope-based access control. The MCP client's scope determines what operations are allowed.

### Token Introspection

**Introspection flow:**

1. Every MCP request includes a bearer token from the client
2. MCP server calls `POST /oauth/introspect` with the token
3. API validates token and returns active status + scopes
4. MCP server checks scopes against required permissions
5. If authorized, MCP server proxies the request to the API

**Security properties:**

- Token validation happens on every request (no caching)
- Revoked tokens are rejected immediately
- Expired tokens fail introspection
- Invalid tokens return `{"active": false}`

### Network Security

**Internal communication:**

- MCP introspection calls use `FORTEMI_URL=http://localhost:3000`
- Traffic never leaves the container (localhost-only)
- No TLS required for internal calls

**External access:**

- MCP endpoint exposed via nginx reverse proxy
- **Always use TLS for external MCP access** (configure nginx with SSL certificates)
- Client tokens are transmitted in `Authorization` headers (TLS prevents eavesdropping)

**Example nginx configuration:**

```nginx
# API endpoint
location / {
    proxy_pass http://localhost:3000/;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}

# MCP endpoint
location = /mcp {
    proxy_pass http://localhost:3001/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
}

location /mcp/ {
    proxy_pass http://localhost:3001/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
}
```

### Rate Limiting

**API-level rate limiting:**

- Controlled by `RATE_LIMIT_ENABLED` environment variable (default: `false`)
- When enabled: 100 requests/minute per IP, burst of 200
- Applies to all API endpoints including introspection
- MCP introspection calls are internal (localhost) and not rate limited

**Recommendation:** Enable rate limiting in production to prevent abuse of public endpoints. MCP introspection is unaffected because it uses internal networking.

## Manual Credential Management

For users who want explicit control over MCP credentials, follow this workflow:

### Step 1: Register OAuth Client

```bash
# Register OAuth client for MCP server
curl -X POST http://localhost:3000/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read write"}'
```

**Response:**

```json
{
  "client_id": "mm_abc123def456",
  "client_secret": "secret_xyz789",
  "client_name": "MCP Server",
  "grant_types": ["client_credentials"],
  "scope": "mcp read write"
}
```

### Step 2: Configure Environment Variables

Add credentials to `.env`:

```bash
# .env
ISSUER_URL=http://localhost:3000
MCP_CLIENT_ID=mm_abc123def456
MCP_CLIENT_SECRET=secret_xyz789
```

**Why `ISSUER_URL` is required:** The API uses `ISSUER_URL` to generate OAuth2 discovery metadata (`.well-known/oauth-authorization-server`). Claude Code uses this metadata to discover the authorization and token endpoints.

### Step 3: Restart Container

```bash
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

**What happens on restart:**

- Entrypoint checks for persisted credentials on pgdata volume
- If found, validates them (may be stale after volume wipe)
- If validation fails or no file exists, loads from `.env`
- Validates `.env` credentials against API
- If valid, persists to volume and starts MCP server
- If invalid, auto-registers new credentials

**Priority:** Even with `.env` credentials, the persisted file takes precedence if it exists and validates successfully.

## Credential Rotation

To rotate MCP OAuth credentials (recommended for security best practices):

### Step 1: Delete Persisted Credentials

```bash
docker exec fortemi-matric-1 rm /var/lib/postgresql/data/.fortemi-mcp-credentials
```

### Step 2: Restart Container

```bash
docker compose -f docker-compose.bundle.yml restart
```

**What happens:**

- Entrypoint finds no persisted credentials
- Checks `.env` for credentials
- If `.env` has valid credentials, persists them to volume
- If `.env` has no credentials or invalid credentials, auto-registers new client
- New credentials are persisted to volume

### Step 3: Update Connected Clients

**Claude Code:**

- Existing access tokens remain valid until expiry (typically 1 hour)
- When tokens expire, Claude Code automatically refreshes using refresh token
- If refresh fails (client secret changed), Claude Code re-runs OAuth dance
- User must re-authenticate with browser

**Alternative:** Revoke old client credentials from the database to force immediate re-authentication:

```bash
docker exec -it fortemi-matric-1 psql -U matric -d matric \
  -c "DELETE FROM oauth_clients WHERE client_name = 'MCP Server (auto-registered)';"
```

## Monitoring

### Startup Logs

Check credential status during container startup:

```bash
docker compose -f docker-compose.bundle.yml logs matric | grep -E "MCP|credential"
```

**Expected messages:**

**Routine restart (credentials persist):**

```
>>> Loading MCP credentials from persistent storage...
>>> Validating MCP credentials (client_id: mm_abc123def456)...
  MCP credentials valid
>>> Starting MCP Server...
  MCP server started (PID: 123)
```

**Clean deploy (credentials regenerated):**

```
>>> No MCP credentials configured
>>> Auto-registering MCP OAuth client...
  Registered MCP client: mm_abc123def456
  Credentials persisted to /var/lib/postgresql/data/.fortemi-mcp-credentials

  ================================================================
  NOTE: To persist across volume wipes, update your .env file:
    MCP_CLIENT_ID=mm_abc123def456
    MCP_CLIENT_SECRET=secret_xyz789
  ================================================================

>>> Starting MCP Server...
  MCP server started (PID: 123)
```

**Stale credentials (auto-reregistration):**

```
>>> Loading MCP credentials from persistent storage...
>>> Validating MCP credentials (client_id: mm_old123)...
  MCP credentials invalid (HTTP 401)
>>> Auto-registering MCP OAuth client...
  Registered MCP client: mm_new456
  Credentials persisted to /var/lib/postgresql/data/.fortemi-mcp-credentials
```

### Warning Messages

**Failed auto-registration:**

```
>>> Auto-registering MCP OAuth client...
  WARNING: MCP client auto-registration failed
  Response: {"error":"invalid_request"}
  MCP server will start but token introspection will fail
  Fix: manually register via POST /oauth/register
```

**Cause:** API not ready or database migration failed.

**Fix:** Check API logs for errors, ensure migrations completed successfully.

### Health Checks

**MCP health endpoint:**

```bash
curl http://localhost:3001/health
```

**Expected response:**

```json
{
  "status": "healthy",
  "transport": "http",
  "version": "2026.2.0"
}
```

**OAuth discovery metadata:**

```bash
curl http://localhost:3001/.well-known/oauth-protected-resource
```

**Expected response:**

```json
{
  "resource": "http://localhost:3000/mcp",
  "authorization_servers": ["http://localhost:3000"],
  "scopes_supported": ["read", "write", "mcp"],
  "bearer_methods_supported": ["header"],
  "resource_documentation": "http://localhost:3000/docs"
}
```

**Note:** In production, replace `localhost` URLs with your external domain and ensure `ISSUER_URL` matches. If the `resource` field shows an unexpected URL, `ISSUER_URL` is not configured correctly.

## Environment Variable Reference

| Variable | Default | Required | Description |
|----------|---------|----------|-------------|
| `ISSUER_URL` | `https://localhost:3000` | **Yes** | External URL for OAuth2 issuer. Used for discovery metadata and token validation. |
| `MCP_CLIENT_ID` | (auto) | No | OAuth client ID for MCP server. Auto-managed if not provided. |
| `MCP_CLIENT_SECRET` | (auto) | No | OAuth client secret for MCP server. Auto-managed if not provided. |
| `MCP_TRANSPORT` | `http` (bundle) | No | Transport mode: `http` for bundle deployment, `stdio` for local development. |
| `MCP_PORT` | `3001` | No | MCP server listening port inside container. |
| `MCP_BASE_URL` | `${ISSUER_URL}/mcp` | No | External MCP URL. Claude Code uses this for OAuth discovery. |
| `FORTEMI_URL` | `http://localhost:3000` | No | Internal API URL for MCP→API calls. Avoids nginx hairpin routing. |

### ISSUER_URL Configuration

**Format:** Full URL including scheme and domain (e.g., `http://localhost:3000` for local, `https://your-domain.com` for production).

**What it's used for:**

- OAuth2 authorization server metadata (`.well-known/oauth-authorization-server`)
- OAuth2 protected resource metadata (`.well-known/oauth-protected-resource`)
- Token issuer validation (tokens must be issued by this URL)

**Common mistake:** Setting `ISSUER_URL=http://localhost:3000` in production.

**Correct:**

```bash
# Production
ISSUER_URL=https://fortemi.example.com

# Local development
ISSUER_URL=http://localhost:3000
```

### FORTEMI_URL vs ISSUER_URL

**FORTEMI_URL:** Internal API URL for MCP→API communication (always `http://localhost:3000` in bundle).

**ISSUER_URL:** External API URL for OAuth2 discovery and client authentication (matches your domain).

**Why separate variables?**

- MCP server runs in the same container as the API (localhost networking)
- MCP server uses `FORTEMI_URL` for introspection calls (internal, no TLS)
- Claude Code uses `ISSUER_URL` for OAuth discovery (external, TLS required)
- Avoids nginx hairpin routing (container calling itself via external nginx proxy)

## Troubleshooting

For common deployment issues and diagnostic commands, see [MCP Troubleshooting Guide](./mcp-troubleshooting.md).

**Quick diagnostic:**

```bash
# Check container status
docker compose -f docker-compose.bundle.yml ps

# Check startup logs
docker compose -f docker-compose.bundle.yml logs matric | tail -50

# Check MCP credentials in container
docker exec fortemi-matric-1 printenv | grep -E 'ISSUER_URL|MCP_CLIENT'

# Test MCP health
curl http://localhost:3001/health

# Test OAuth discovery
curl http://localhost:3001/.well-known/oauth-protected-resource
```

**Common issues:**

1. **"Protected resource URL mismatch"** - `ISSUER_URL` not set correctly
2. **"unauthorized" with valid token** - MCP credentials not configured
3. **MCP not responding** - MCP server crashed, check logs
4. **Token validation fails** - Stale credentials, delete persisted file and restart

## Related Documentation

- [MCP Server Overview](./mcp.md) - Tool reference and usage guide
- [MCP Troubleshooting](./mcp-troubleshooting.md) - Common issues and fixes
- [MCP Permissions](./mcp-permissions.md) - OAuth scope and permission model
- [API Authentication](../CLAUDE.md#authentication) - OAuth2 configuration

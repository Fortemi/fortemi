# MCP Troubleshooting Guide

Quick reference for diagnosing and fixing MCP connection issues.

## Diagnostic Commands

Run these commands to quickly identify the issue:

```bash
# 1. Check container is running
docker compose -f docker-compose.bundle.yml ps

# 2. Check startup logs for MCP credential status
docker compose -f docker-compose.bundle.yml logs matric | grep -E "MCP|credential"
# Expected: "MCP credentials valid" or "Registered MCP client: mm_xxxxx"

# 3. Check OAuth protected resource metadata
curl http://localhost:3001/.well-known/oauth-protected-resource
# Should return: { "resource": "http://localhost:3000/mcp", ... }

# 4. Check OAuth authorization server metadata
curl http://localhost:3000/.well-known/oauth-authorization-server
# Should return: { "issuer": "http://localhost:3000", ... }

# 5. Check MCP server is reachable (should return 401, not connection refused)
curl -s -o /dev/null -w "%{http_code}" http://localhost:3001/
# Expected: 401 (auth required)
```

## Common Issues

### 1. "Protected resource URL mismatch"

**Symptom:** Claude Code shows error about protected resource URL not matching

**Diagnosis:**
```bash
curl http://localhost:3001/.well-known/oauth-protected-resource
# Look at "resource" field - it should match your ISSUER_URL
```

**Cause:** `ISSUER_URL` not set in `.env`

**Fix:**
```bash
echo "ISSUER_URL=http://localhost:3000" >> .env
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### 2. "unauthorized" after deploy

**Symptom:** MCP requests fail with "unauthorized"

**Diagnosis:**
```bash
# Check startup logs for credential status
docker compose -f docker-compose.bundle.yml logs matric | grep -E "credential|auto-regist"
```

**Cause:** MCP credentials are invalid or auto-registration failed

**Fix:** In most cases, a restart resolves this since credentials are auto-registered on startup:
```bash
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

If auto-registration itself is failing, check that the API is starting correctly:
```bash
docker compose -f docker-compose.bundle.yml logs matric | grep -E "ERROR|API is healthy"
```

For manual credential management, see the [MCP Deployment Guide](./mcp-deployment.md).

### 3. "Authentication successful but reconnection failed"

**Symptom:** OAuth dance completes but MCP doesn't connect

**Diagnosis:**
```bash
# Check Claude Code's cached credentials
cat ~/.claude/.credentials.json | jq '.mcpOAuth | keys'

# Look for duplicate entries for same server with different hashes
# e.g., "Fortémi|abc123" and "Fortémi|def456"
```

**Cause:** Stale OAuth credentials cached from previous configuration

**Fix:**
```bash
# Remove stale credentials (keep the one with valid accessToken)
cat ~/.claude/.credentials.json | jq 'del(.mcpOAuth["Fortémi|STALE_HASH"])' > /tmp/creds.json
mv /tmp/creds.json ~/.claude/.credentials.json

# Restart Claude Code
```

### 4. MCP server not responding

**Symptom:** Curl to MCP endpoints hangs or times out

**Diagnosis:**
```bash
# Check container logs
docker compose -f docker-compose.bundle.yml logs --tail=50

# Check if MCP process is running
docker exec fortemi-matric-1 ps aux | grep node

# Check internal health
docker exec fortemi-matric-1 curl -s http://localhost:3001/health
```

**Cause:** MCP server crashed or didn't start

**Fix:**
```bash
# Restart container
docker compose -f docker-compose.bundle.yml restart

# Or full restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### 5. Nginx proxy issues

**Symptom:** MCP endpoints return 502 or 404

**Diagnosis:**
```bash
# Test direct container access (bypassing nginx)
curl http://localhost:3001/health

# If that works, nginx is the issue
```

**Cause:** Nginx not configured correctly for MCP routes

**Fix:** Ensure nginx config includes:
```nginx
location = /mcp {
    proxy_pass http://localhost:3001/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_buffering off;
    proxy_read_timeout 86400s;
}

location /mcp/ {
    proxy_pass http://localhost:3001/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_buffering off;
    proxy_read_timeout 86400s;
}
```

### 6. MCP credentials keep regenerating on every restart

**Symptom:** Startup logs always show "Auto-registering MCP OAuth client" instead of "Loading MCP credentials from persistent storage"

**Cause:** The `.env` file has stale `MCP_CLIENT_ID`/`MCP_CLIENT_SECRET` values that override the persisted credentials. The stale values fail validation, triggering re-registration each time.

**Fix:** Remove or comment out `MCP_CLIENT_ID` and `MCP_CLIENT_SECRET` from `.env` to let auto-management handle it:
```bash
# Edit .env - comment out or remove these lines:
# MCP_CLIENT_ID=mm_xxxxx
# MCP_CLIENT_SECRET=xxxxx

# Restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

The entrypoint prioritizes persisted credentials (from the pgdata volume) over env vars, but only if the persisted file exists. On clean deploys, env vars are tried first, and if they're stale, a new client is registered.

## Token Validation

Test if a token is valid:

```bash
# Get token from Claude Code credentials
TOKEN=$(cat ~/.claude/.credentials.json | jq -r '.mcpOAuth["Fortémi|HASH"].accessToken')

# Test against MCP server (should return SSE initialize response)
curl -X POST http://localhost:3001/ \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'

# Should return SSE event with initialize result
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ISSUER_URL` | Yes | External URL (e.g., `http://localhost:3000`) |
| `MCP_CLIENT_ID` | No | OAuth client ID (auto-managed by default) |
| `MCP_CLIENT_SECRET` | No | OAuth client secret (auto-managed by default) |
| `MCP_BASE_URL` | No | Defaults to `${ISSUER_URL}/mcp` |

For the full environment variable reference and credential lifecycle details, see the [MCP Deployment Guide](./mcp-deployment.md).

## Claude Code Credential Location

OAuth credentials are stored in: `~/.claude/.credentials.json`

Structure:
```json
{
  "mcpOAuth": {
    "server-name|config-hash": {
      "serverName": "Fortémi",
      "serverUrl": "http://localhost:3001",
      "clientId": "mm_xxxxx",
      "clientSecret": "xxxxx",
      "accessToken": "mm_at_xxxxx",
      "refreshToken": "mm_rt_xxxxx",
      "expiresAt": 1234567890
    }
  }
}
```

## First-Time Setup Checklist

1. [ ] Set `ISSUER_URL` in `.env` (e.g., `http://localhost:3000`)
2. [ ] Start container: `docker compose -f docker-compose.bundle.yml up -d`
3. [ ] Wait for ready: look for `"=== Matric Memory Bundle Ready ==="` in logs
4. [ ] Verify API: `curl http://localhost:3000/health`
5. [ ] Verify OAuth metadata: `curl http://localhost:3001/.well-known/oauth-protected-resource`
6. [ ] Connect from Claude Code: add `"url": "http://localhost:3001"` to `.mcp.json`

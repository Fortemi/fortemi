# MCP Troubleshooting Guide

Quick reference for diagnosing and fixing MCP connection issues.

## Diagnostic Commands

Run these commands to quickly identify the issue:

```bash
# 1. Check container is running
docker compose -f docker-compose.bundle.yml ps

# 2. Check MCP health
curl https://your-domain.com/mcp/health

# 3. Check OAuth protected resource metadata
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
# Should return: { "resource": "https://your-domain.com/mcp", ... }

# 4. Check OAuth authorization server metadata
curl https://your-domain.com/mcp/.well-known/oauth-authorization-server
# Should return: { "issuer": "https://your-domain.com", ... }

# 5. Check environment variables in container
docker exec Fortémi-matric-1 printenv | grep -E 'ISSUER_URL|MCP_'
```

## Common Issues

### 1. "Protected resource URL mismatch"

**Symptom:** Claude Code shows error about protected resource URL not matching

**Diagnosis:**
```bash
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
# Look at "resource" field - if it says "localhost", that's the problem
```

**Cause:** `ISSUER_URL` not set in `.env`

**Fix:**
```bash
echo "ISSUER_URL=https://your-domain.com" >> .env
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### 2. "unauthorized" with valid token

**Symptom:** Authentication succeeds but MCP requests fail with "unauthorized"

**Diagnosis:**
```bash
# Check if MCP has client credentials
docker exec Fortémi-matric-1 printenv | grep MCP_CLIENT
# If empty, that's the problem
```

**Cause:** `MCP_CLIENT_ID` and `MCP_CLIENT_SECRET` not configured - MCP server cannot introspect tokens

**Fix:**
```bash
# Register OAuth client for MCP
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read"}'

# Add credentials to .env
echo "MCP_CLIENT_ID=mm_xxxxx" >> .env
echo "MCP_CLIENT_SECRET=xxxxx" >> .env

# Restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

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
docker exec Fortémi-matric-1 ps aux | grep node

# Check internal health
docker exec Fortémi-matric-1 curl -s http://localhost:3001/health
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
# Test direct container access
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
}

location /mcp/ {
    proxy_pass http://localhost:3001/;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
}
```

## Token Validation

Test if a token is valid:

```bash
# Get token from Claude Code credentials
TOKEN=$(cat ~/.claude/.credentials.json | jq -r '.mcpOAuth["Fortémi|HASH"].accessToken')

# Introspect token
CLIENT_ID=$(cat ~/.claude/.credentials.json | jq -r '.mcpOAuth["Fortémi|HASH"].clientId')
CLIENT_SECRET=$(cat ~/.claude/.credentials.json | jq -r '.mcpOAuth["Fortémi|HASH"].clientSecret')

curl -X POST https://your-domain.com/oauth/introspect \
  -u "$CLIENT_ID:$CLIENT_SECRET" \
  -d "token=$TOKEN"

# Should return { "active": true, ... }
```

## MCP Initialize Test

Test full MCP connection:

```bash
TOKEN="your-access-token"

curl -X POST https://your-domain.com/mcp/ \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'

# Should return SSE event with initialize result
```

## Required Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ISSUER_URL` | Yes | External domain URL (e.g., `http://localhost:3000`) |
| `MCP_CLIENT_ID` | Yes | OAuth client ID for token introspection |
| `MCP_CLIENT_SECRET` | Yes | OAuth client secret |
| `MCP_BASE_URL` | No | Defaults to `${ISSUER_URL}/mcp` |

## Claude Code Credential Location

OAuth credentials are stored in: `~/.claude/.credentials.json`

Structure:
```json
{
  "mcpOAuth": {
    "server-name|config-hash": {
      "serverName": "Fortémi",
      "serverUrl": "https://domain.com/mcp",
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

1. [ ] Start container: `docker compose -f docker-compose.bundle.yml up -d`
2. [ ] Wait for healthy: `docker compose logs -f`
3. [ ] Register MCP OAuth client: `POST /oauth/register`
4. [ ] Create `.env` with `ISSUER_URL`, `MCP_CLIENT_ID`, `MCP_CLIENT_SECRET`
5. [ ] Restart: `docker compose down && docker compose up -d`
6. [ ] Verify protected resource: `curl .../mcp/.well-known/oauth-protected-resource`
7. [ ] Verify MCP credentials in container: `printenv | grep MCP_CLIENT`
8. [ ] Test Claude Code: `/mcp`

# MCP Integration Learnings

**Date**: 2026-01-05
**Status**: Resolved

## Summary

Successfully integrated matric-memory MCP server with Claude Code after resolving two critical issues blocking the StreamableHTTP transport.

## Issues Encountered

### Issue 1: Express.json() Body Parsing Conflict

**Symptom**: MCP server returned `{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error: Invalid JSON"},"id":null}` for valid JSON requests.

**Root Cause**: The `express.json()` middleware was pre-parsing the request body before `StreamableHTTPServerTransport` could read it. The transport expects to read the raw body stream itself.

**Solution**: Apply `express.json()` only to routes that need pre-parsed bodies (like `/messages` for SSE transport), not to the root path used by StreamableHTTP.

```javascript
// WRONG - applies to all routes including StreamableHTTP
app.use(express.json());

// CORRECT - only apply to routes that need it
app.use('/messages', express.json());
```

**Reference**: mcp-hound uses raw `node:http` createServer and manually reads body chunks, avoiding this issue entirely.

### Issue 2: nginx 301 Redirect Converting POST to GET

**Symptom**: Claude Code's POST to `/mcp` was being redirected to `/mcp/` with HTTP 301, which converts POST to GET per HTTP spec. The GET then failed with 400.

**nginx logs showed**:
```
POST /mcp HTTP/1.1" 301 178   <- redirect
GET /mcp/ HTTP/1.1" 400 99    <- POST became GET
```

**Root Cause**: nginx `location /mcp/` only matches paths with trailing slash. Requests to `/mcp` fell through to the default location, which triggered a redirect.

**Solution**: Add exact match location for `/mcp` without trailing slash:

```nginx
# Exact match for /mcp (no trailing slash)
location = /mcp {
    proxy_pass http://127.0.0.1:3001/;
    # ... same config as /mcp/ location
}

# Prefix match for /mcp/ and subpaths
location /mcp/ {
    proxy_pass http://127.0.0.1:3001/;
    # ...
}
```

## Key Learnings

1. **StreamableHTTPServerTransport reads raw body** - Don't use body-parsing middleware on routes handled by this transport.

2. **nginx trailing slash behavior** - Always test both with and without trailing slash when proxying to MCP servers.

3. **301 vs 307/308 redirects** - HTTP 301 redirects convert POST to GET. Use 307 (Temporary) or 308 (Permanent) to preserve the HTTP method if redirect is necessary.

4. **MCP OAuth flow** - The protected resource metadata must be at `/.well-known/oauth-protected-resource` relative to the MCP server URL. Claude Code fetches this to discover the authorization server.

5. **MCP-Session-Id header** - Must be exposed in CORS and passed through nginx proxy for StreamableHTTP transport to work.

## Configuration Reference

### Required Environment Variables (MCP Server)
```bash
MCP_TRANSPORT=http
MCP_PORT=3001
MCP_BASE_URL=https://memory.integrolabs.net/mcp
MCP_BASE_PATH=/mcp
MCP_CLIENT_ID=<oauth_client_id>
MCP_CLIENT_SECRET=<oauth_client_secret>
MATRIC_MEMORY_URL=http://127.0.0.1:3000
```

### Required nginx Headers
```nginx
proxy_set_header MCP-Session-Id $http_mcp_session_id;
proxy_pass_header MCP-Session-Id;
```

### Required CORS Headers
```javascript
cors({
  allowedHeaders: ['Content-Type', 'Authorization', 'MCP-Session-Id'],
  exposedHeaders: ['MCP-Session-Id'],
})
```

## Verification Commands

```bash
# Test MCP endpoint with token
TOKEN=$(curl -s -X POST https://memory.integrolabs.net/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -u "$CLIENT_ID:$CLIENT_SECRET" \
  -d "grant_type=client_credentials&scope=mcp read" | jq -r '.access_token')

curl -X POST https://memory.integrolabs.net/mcp \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'
```

## Files Modified

1. `mcp-server/index.js` - Moved express.json() to /messages route only
2. `/etc/nginx/sites-available/memory` - Added exact match location for /mcp
3. `/etc/systemd/system/matric-mcp.service` - Added MCP_BASE_URL environment variable

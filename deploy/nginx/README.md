# Nginx Reverse Proxy Configuration

This directory contains nginx configuration templates for deploying Fortemi with HotM (Hall of the Mountain) SPA frontend.

## Architecture Overview

```
                    ┌─────────────────────────────────────────┐
                    │              nginx (443)                │
                    │                                         │
    Client ────────►│  /.well-known/oauth-*  ──► API (3000)  │
                    │  /oauth/*              ──► API (3000)  │
                    │  /api/*                ──► API (3000)  │
                    │  /mcp                  ──► MCP (3001)  │
                    │  /*                    ──► SPA (4180)  │
                    └─────────────────────────────────────────┘
```

## Critical Concerns: SPA at Root with API Backend

When deploying a Single Page Application at the site root (`/`) with API backends, **route ordering is critical**. The SPA catch-all will intercept ALL unmatched requests, returning HTML instead of proxying to the backend.

### Problem: OAuth 405 Errors

**Symptom**: MCP clients receive HTML error pages instead of JSON:

```
Error: HTTP 405: Invalid OAuth error response: SyntaxError: JSON Parse error:
Unrecognized token '<'. Raw body: <html><head><title>405 Not Allowed</title>...
```

**Cause**: OAuth endpoints (`/.well-known/oauth-authorization-server`, `/oauth/*`) fall through to the SPA catch-all, which returns 405 HTML because SPAs don't handle POST/PUT requests to arbitrary paths.

**Solution**: Explicitly route OAuth endpoints BEFORE the SPA catch-all:

```nginx
# OAuth Discovery (RFC 8414) - MUST come before SPA
location = /.well-known/oauth-authorization-server {
    proxy_pass http://127.0.0.1:3000/.well-known/oauth-authorization-server;
    # ... headers
}

# OAuth endpoints - MUST come before SPA
location /oauth/ {
    proxy_pass http://127.0.0.1:3000/oauth/;
    # ... headers
}

# SPA catch-all - MUST be LAST
location / {
    proxy_pass http://127.0.0.1:4180;
}
```

### Route Priority Checklist

When adding new API endpoints, ensure they are routed BEFORE the SPA catch-all:

| Path Pattern | Backend | Priority |
|--------------|---------|----------|
| `/.well-known/*` | API | High (exact match) |
| `/oauth/*` | API | High |
| `/api/*` | API | High |
| `/mcp` | MCP Server | High |
| `/health` | API | Medium |
| `/*` | SPA | Lowest (catch-all) |

### Common Mistakes

1. **Adding new API endpoints without nginx routes**
   - API serves `/foo/bar` endpoint
   - nginx has no `/foo/` location
   - Requests to `/foo/bar` hit SPA, return 404 HTML

2. **Relying on SPA routing for API paths**
   - SPA router handles `/api/*` client-side for display
   - But actual API calls need nginx to proxy to backend

3. **Forgetting OAuth well-known endpoints**
   - OAuth discovery at `/.well-known/oauth-authorization-server`
   - MCP authorization at `/.well-known/mcp-authorization-servers` (future)

## Endpoint Reference

### Fortemi API (port 3000)

| Endpoint | Description |
|----------|-------------|
| `/health` | Health check |
| `/api/v1/notes` | Notes CRUD |
| `/api/v1/search` | Hybrid search |
| `/api/v1/tags` | Tag management |
| `/api/v1/collections` | Collections |
| `/oauth/authorize` | OAuth authorization |
| `/oauth/token` | OAuth token exchange |
| `/oauth/register` | Dynamic client registration |
| `/oauth/introspect` | Token introspection |
| `/oauth/revoke` | Token revocation |
| `/.well-known/oauth-authorization-server` | OAuth metadata |

### MCP Server (port 3001)

| Endpoint | Description |
|----------|-------------|
| `/` | MCP StreamableHTTP transport |

### HotM SPA (port 4180)

| Path | Description |
|------|-------------|
| `/*` | All other paths (React router handles client-side) |

## Installation

```bash
# Copy config to nginx sites
sudo cp deploy/nginx/your-domain.com.conf /etc/nginx/sites-available/memory

# Enable site
sudo ln -sf /etc/nginx/sites-available/memory /etc/nginx/sites-enabled/memory

# Test configuration
sudo nginx -t

# Reload nginx
sudo systemctl reload nginx
```

## Testing

After deployment, verify all endpoints route correctly:

```bash
# OAuth discovery (should return JSON, not HTML)
curl -s https://your-domain.com/.well-known/oauth-authorization-server | head -1
# Expected: {"issuer":"https://your-domain.com",...}

# OAuth token (should return 401 JSON, not 405 HTML)
curl -s -o /dev/null -w "%{http_code}" -X POST https://your-domain.com/oauth/token
# Expected: 401 (not 405)

# API health
curl -s https://your-domain.com/api/v1/health
# Expected: {"status":"healthy",...}

# MCP (should return auth error JSON)
curl -s https://your-domain.com/mcp -X POST -H "Content-Type: application/json" -d '{}'
# Expected: {"error":"unauthorized",...}

# SPA (should return HTML)
curl -s https://your-domain.com/ | head -1
# Expected: <!DOCTYPE html>...
```

## Streaming and Large File Operations

The API location block includes settings critical for video/audio streaming and large file uploads/downloads:

### Range Requests (Video/Audio Streaming)

HTML5 `<video>` and `<audio>` elements rely on HTTP Range requests for seeking. The config forwards the `Range` and `If-Range` headers from the client to the upstream API, and `proxy_force_ranges on` ensures nginx preserves the upstream's `Accept-Ranges` and `Content-Range` response headers.

Without these, video players will be unable to seek and audio players won't support progressive playback.

### Upload Size (`client_max_body_size`)

Set to `500M` to accommodate large video, 3D model, and audio uploads. Adjust upward if users upload files larger than 500MB. When uploads exceed this limit, nginx returns 413 Request Entity Too Large before the request reaches the API.

### Request Buffering (`proxy_request_buffering off`)

By default nginx buffers the entire request body in memory/temp files before forwarding to upstream. With `off`, the upload streams directly to the API as it arrives, reducing memory usage and upload latency for large files.

### Response Buffering (`proxy_buffering off`)

Disabled for the API location to support SSE (Server-Sent Events) and large file downloads. Responses stream directly from the API to the client without nginx buffering them in temp files.

### Timeouts

- `proxy_read_timeout 600s` — 10 minutes for large extraction jobs that take time
- `proxy_send_timeout 600s` — 10 minutes for slow clients downloading large files

### Gzip (`gzip_proxied off`)

Disabled for proxied responses in the API block because:
1. Gzip compression interferes with Range requests (compressed responses can't be byte-ranged)
2. Binary media files (video, audio, 3D models) don't compress well
3. The API already serves pre-compressed text responses where beneficial

## Troubleshooting

### HTML Responses Instead of JSON

If API endpoints return HTML (especially error pages), check:

1. **Route exists in nginx**: Does the endpoint have a `location` block?
2. **Route order**: Is the route BEFORE the SPA catch-all?
3. **Backend running**: Is the API server listening on the expected port?

```bash
# Check if backend is running
curl -s http://127.0.0.1:3000/health

# Check nginx error log
sudo tail -f /var/log/nginx/memory_error.log
```

### 502 Bad Gateway

Backend is not running or not listening on expected port:

```bash
# Check Fortemi API
curl http://127.0.0.1:3000/health

# Check MCP server
curl http://127.0.0.1:3001/

# Check HotM SPA
curl http://127.0.0.1:4180/
```

### SSL Certificate Issues

Ensure wildcard cert covers the domain:

```bash
openssl s_client -connect your-domain.com:443 -servername your-domain.com </dev/null 2>/dev/null | openssl x509 -noout -subject -dates
```

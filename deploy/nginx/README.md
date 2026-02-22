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

# Verify temp directory permissions (must match nginx worker user)
NGINX_USER=$(grep '^user ' /etc/nginx/nginx.conf | awk '{print $2}' | tr -d ';')
sudo chown -R "$NGINX_USER:$NGINX_USER" /var/lib/nginx/

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

Set to `1024M` (1 GB) to accommodate large video, 3D model, and audio uploads. This must be >= the API's `MATRIC_MAX_UPLOAD_SIZE_BYTES` setting. When uploads exceed this limit, nginx returns 413 Request Entity Too Large before the request reaches the API.

### Upload Endpoint (Streaming)

File uploads use a dedicated `location` block with `proxy_request_buffering off`, which streams the request body directly to the API without buffering to a temp file. This is necessary because Chrome's HTTP/2 implementation drops connections mid-transfer when nginx tries to buffer large uploads (200+ MB) to temp files.

The upload location matches `~ ^/api/v1/notes/[^/]+/attachments/upload$` and must appear **before** the general `/api/` location block (nginx evaluates regex locations in order of appearance, and they take priority over prefix matches).

### Request Buffering (`proxy_request_buffering on`)

The general `/api/` location uses `proxy_request_buffering on` (nginx default). This buffers the request body to `/var/lib/nginx/body/` before forwarding to upstream, which works well for JSON API requests. See [Temp Directory Permissions](#temp-directory-permissions) for ownership requirements.

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

## Temp Directory Permissions

nginx uses several temp directories under `/var/lib/nginx/` for buffering:

| Directory | Purpose |
|-----------|---------|
| `body/` | Request body buffering (file uploads) |
| `proxy/` | Response buffering from upstream (SPA JS bundles, API responses) |
| `fastcgi/` | FastCGI response buffering |
| `scgi/` | SCGI response buffering |
| `uwsgi/` | uWSGI response buffering |

**These directories must be owned by the nginx worker user.** Check which user nginx runs as:

```bash
# Check nginx.conf worker user
grep '^user ' /etc/nginx/nginx.conf
# e.g.: user www-data;

# Verify workers match
ps -eo pid,user,comm | grep 'nginx: worker' | head -3
```

If the worker user doesn't own the temp directories, nginx silently fails to buffer requests/responses. Uploads return `400` with empty bodies, and large proxied responses (like SPA JS bundles) get truncated causing white screens.

**Fix permissions:**

```bash
# Replace www-data with your nginx worker user if different
sudo chown -R www-data:www-data /var/lib/nginx/
sudo find /var/lib/nginx -type d -exec chmod 700 {} \;
sudo systemctl reload nginx
```

**Common causes of permission mismatch:**
- Package upgrades changing the default user
- Manual `nginx.conf` edits changing `user` without updating temp dirs
- Running `chown` to match a containerized nginx instead of the host nginx

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

### White Screen (SPA Loads but JS Bundle Fails)

**Symptom**: `https://your-domain.com/` returns a blank white page. The HTML loads but the JavaScript bundle doesn't.

**Cause**: nginx can't buffer the proxied response from the SPA server. The JS bundle (~3.6 MB) exceeds the in-memory proxy buffer, so nginx tries to write to `/var/lib/nginx/proxy/` but lacks permission.

**Diagnosis**:

```bash
# Check nginx error log for permission errors
sudo tail -20 /var/log/nginx/memory_error.log | grep 'Permission denied'
# e.g.: open() "/var/lib/nginx/proxy/4/92/0000002924" failed (13: Permission denied)

# Check ownership
ls -la /var/lib/nginx/
# All dirs should be owned by the nginx worker user (e.g., www-data)
```

**Fix**: See [Temp Directory Permissions](#temp-directory-permissions).

### Large File Upload Fails (`400 0` or "Failed to fetch")

**Symptom**: Uploading files from the browser fails. nginx access log shows `400 0` (400 status, 0-byte response). No error in API logs — the request never reaches the API intact. `curl` uploads through the same nginx server succeed.

**Possible causes (check in order)**:

1. **Temp directory permissions** — If `proxy_request_buffering on` is used, nginx writes the upload body to `/var/lib/nginx/body/`. Permission denied causes silent truncation. Check: `sudo tail -50 /var/log/nginx/memory_error.log | grep 'Permission denied'`. Fix: see [Temp Directory Permissions](#temp-directory-permissions).

2. **Upload size limit mismatch** — `client_max_body_size` in nginx must be >= `MATRIC_MAX_UPLOAD_SIZE_BYTES` in the API. The API defaults to 50 MB; set `MATRIC_MAX_UPLOAD_SIZE_BYTES` in `.env` for larger files.

3. **Chrome HTTP/2 large upload bug** — Chrome drops HTTP/2 connections mid-transfer for files >200 MB. Diagnosis: enable detailed logging (`rt`, `rl`, `cl`, `us` fields) and compare `rl` (received length) vs `cl` (content-length). If `rl < cl` and `us=-` (never forwarded), the browser disconnected mid-upload. Workaround: the upload endpoint uses a dedicated location block with `proxy_request_buffering off` to stream directly to the API. For very large files, the frontend should use chunked/resumable uploads or ensure HTTP client libraries are up to date.

### SSL Certificate Issues

Ensure wildcard cert covers the domain:

```bash
openssl s_client -connect your-domain.com:443 -servername your-domain.com </dev/null 2>/dev/null | openssl x509 -noout -subject -dates
```

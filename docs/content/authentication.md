# Authentication Guide

Fortémi supports two authentication mechanisms: **API Keys** (simple, token-based) and **OAuth2** (full authorization flow with PKCE). Choose the method that best fits your use case.

## Table of Contents

- [Quick Start](#quick-start)
- [API Key Authentication](#api-key-authentication)
- [OAuth2 Authentication](#oauth2-authentication)
- [Scopes and Permissions](#scopes-and-permissions)
- [Rate Limiting](#rate-limiting)
- [Error Handling](#error-handling)
- [Security Best Practices](#security-best-practices)

---

## Quick Start

### For Simple Scripts and CLI Tools

Use API keys for straightforward authentication:

```bash
# Create an API key
curl -X POST http://localhost:3000/api/v1/api-keys \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Script",
    "description": "Script for daily note imports",
    "scope": "read write",
    "expires_in_days": 90
  }'

# Response (save the api_key value - shown only once!)
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "mm_key_abcdefghijklmnopqrstuvwxyz123456",
  "key_prefix": "mm_key_abcde",
  "name": "My Script",
  "scope": "read write",
  "expires_at": "2024-04-15T10:00:00Z",
  "created_at": "2024-01-15T10:00:00Z"
}
```

### For Applications with User Context

Use OAuth2 for applications that need user-scoped access:

```bash
# Register your application
curl -X POST http://localhost:3000/oauth/register \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "My App",
    "redirect_uris": ["http://localhost:3000/callback"],
    "grant_types": ["authorization_code", "refresh_token"],
    "scope": "read write"
  }'
```

---

## API Key Authentication

API keys are ideal for:
- Server-to-server integrations
- CLI tools and scripts
- Personal automation
- MCP server (stdio mode)

### Creating an API Key

**Endpoint:** `POST /api/v1/api-keys`

**Request:**
```json
{
  "name": "Production Integration",
  "description": "API access for production app",
  "scope": "read write",
  "expires_in_days": 365
}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "mm_key_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7",
  "key_prefix": "mm_key_xYz9Q",
  "name": "Production Integration",
  "scope": "read write",
  "expires_at": "2025-01-15T10:00:00Z",
  "created_at": "2024-01-15T10:00:00Z"
}
```

**Important:** The `api_key` field is only returned once. Store it securely immediately.

### Using an API Key

Include the key in the `Authorization` header with the `Bearer` scheme:

```bash
curl http://localhost:3000/api/v1/notes \
  -H "Authorization: Bearer mm_key_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7"
```

**Python Example:**
```python
import requests

API_BASE = "http://localhost:3000"
API_KEY = "mm_key_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7"

headers = {
    "Authorization": f"Bearer {API_KEY}",
    "Content-Type": "application/json"
}

# Create a note
response = requests.post(
    f"{API_BASE}/api/v1/notes",
    headers=headers,
    json={
        "content": "My new note",
        "tags": ["important"]
    }
)
print(response.json())
```

**JavaScript Example:**
```javascript
const API_BASE = "http://localhost:3000";
const API_KEY = "mm_key_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7";

async function createNote(content, tags = []) {
  const response = await fetch(`${API_BASE}/api/v1/notes`, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${API_KEY}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({ content, tags })
  });
  return response.json();
}

const result = await createNote("My new note", ["important"]);
console.log(result);
```

### Managing API Keys

**List All Keys (shows prefix only):**
```bash
GET /api/v1/api-keys
```

**Response:**
```json
{
  "api_keys": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "key_prefix": "mm_key_xYz9Q",
      "name": "Production Integration",
      "description": "API access for production app",
      "scope": "read write",
      "rate_limit_per_minute": 60,
      "rate_limit_per_hour": 1000,
      "last_used_at": "2024-01-15T14:30:00Z",
      "use_count": 1247,
      "is_active": true,
      "expires_at": "2025-01-15T10:00:00Z",
      "created_at": "2024-01-15T10:00:00Z"
    }
  ]
}
```

**Revoke a Key:**
```bash
DELETE /api/v1/api-keys/{id}
```

### API Key Format

- **Format:** `mm_key_{32_random_chars}`
- **Prefix:** First 12 characters (e.g., `mm_key_xYz9Q`) shown in listings
- **Storage:** SHA256 hash stored in database
- **Expiration:** Optional, defaults to no expiration

---

## OAuth2 Authentication

OAuth2 is ideal for:
- Web applications with user authentication
- Mobile applications
- Third-party integrations requiring user consent
- MCP server (HTTP mode)

Fortémi implements **OAuth 2.0** with:
- **Dynamic Client Registration** (RFC 7591)
- **Authorization Code Flow** with PKCE (RFC 7636)
- **Client Credentials Grant**
- **Refresh Tokens** (30-day expiration)
- **Token Introspection** (RFC 7662)
- **Token Revocation** (RFC 7009)

### Discovery Endpoint

OAuth2 server metadata is available at:

```bash
GET /.well-known/oauth-authorization-server
```

**Response:**
```json
{
  "issuer": "http://localhost:3000",
  "authorization_endpoint": "http://localhost:3000/oauth/authorize",
  "token_endpoint": "http://localhost:3000/oauth/token",
  "registration_endpoint": "http://localhost:3000/oauth/register",
  "introspection_endpoint": "http://localhost:3000/oauth/introspect",
  "revocation_endpoint": "http://localhost:3000/oauth/revoke",
  "response_types_supported": ["code"],
  "grant_types_supported": [
    "authorization_code",
    "client_credentials",
    "refresh_token"
  ],
  "token_endpoint_auth_methods_supported": [
    "client_secret_basic",
    "client_secret_post"
  ],
  "scopes_supported": ["read", "write", "delete", "admin", "mcp"],
  "code_challenge_methods_supported": ["S256", "plain"]
}
```

### 1. Register Your Application

**Endpoint:** `POST /oauth/register`

**Request:**
```json
{
  "client_name": "My Application",
  "client_uri": "https://myapp.example.com",
  "redirect_uris": ["https://myapp.example.com/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "scope": "read write",
  "contacts": ["admin@myapp.example.com"]
}
```

**Response:**
```json
{
  "client_id": "mm_AbCdEfGh12345678901234",
  "client_secret": "sEcReT_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN",
  "client_id_issued_at": 1705320000,
  "client_secret_expires_at": 0,
  "client_name": "My Application",
  "redirect_uris": ["https://myapp.example.com/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "scope": "read write",
  "token_endpoint_auth_method": "client_secret_basic",
  "registration_access_token": "rEgToKeN_123...",
  "registration_client_uri": "http://localhost:3000/oauth/register/mm_AbCdEfGh12345678901234"
}
```

**Important:** Save `client_id` and `client_secret` securely. The secret is only shown once.

### 2. Authorization Code Flow (with PKCE)

#### Step 1: Generate PKCE Parameters

```python
import secrets
import hashlib
import base64

# Generate code verifier (random 43-128 chars)
code_verifier = base64.urlsafe_b64encode(secrets.token_bytes(32)).decode('utf-8').rstrip('=')

# Generate code challenge (SHA256 hash)
code_challenge = base64.urlsafe_b64encode(
    hashlib.sha256(code_verifier.encode()).digest()
).decode('utf-8').rstrip('=')

print(f"Verifier: {code_verifier}")
print(f"Challenge: {code_challenge}")
```

#### Step 2: Redirect User to Authorization Endpoint

```
GET /oauth/authorize?
  response_type=code&
  client_id=mm_AbCdEfGh12345678901234&
  redirect_uri=https://myapp.example.com/callback&
  scope=read write&
  state=random_state_value&
  code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM&
  code_challenge_method=S256
```

The user will see a consent page and can approve or deny access.

#### Step 3: Handle Redirect with Authorization Code

After approval, the user is redirected to:
```
https://myapp.example.com/callback?code=AUTH_CODE_HERE&state=random_state_value
```

#### Step 4: Exchange Code for Tokens

**Endpoint:** `POST /oauth/token`

**Request (using client_secret_basic):**
```bash
curl -X POST http://localhost:3000/oauth/token \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=authorization_code" \
  -d "code=AUTH_CODE_HERE" \
  -d "redirect_uri=https://myapp.example.com/callback" \
  -d "code_verifier=VERIFIER_FROM_STEP1"
```

**Response:**
```json
{
  "access_token": "mm_at_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN9qR",
  "token_type": "Bearer",
  "expires_in": 3600,
  "refresh_token": "mm_rt_AbCdEfGh12345678901234567890123456789012",
  "scope": "read write"
}
```

**Python Example:**
```python
import requests
import base64

client_id = "mm_AbCdEfGh12345678901234"
client_secret = "sEcReT_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN"
auth_code = "AUTH_CODE_FROM_REDIRECT"
redirect_uri = "https://myapp.example.com/callback"
code_verifier = "VERIFIER_FROM_STEP1"

# Basic auth header
credentials = f"{client_id}:{client_secret}"
auth_header = base64.b64encode(credentials.encode()).decode()

response = requests.post(
    "http://localhost:3000/oauth/token",
    headers={
        "Authorization": f"Basic {auth_header}",
        "Content-Type": "application/x-www-form-urlencoded"
    },
    data={
        "grant_type": "authorization_code",
        "code": auth_code,
        "redirect_uri": redirect_uri,
        "code_verifier": code_verifier
    }
)

tokens = response.json()
access_token = tokens["access_token"]
refresh_token = tokens["refresh_token"]
```

### 3. Using Access Tokens

Include the access token in the `Authorization` header:

```bash
curl http://localhost:3000/api/v1/notes \
  -H "Authorization: Bearer mm_at_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN9qR"
```

### 4. Refreshing Tokens

Access tokens expire after 1 hour by default (configurable via `OAUTH_TOKEN_LIFETIME_SECS`). Use refresh tokens to obtain new access tokens without user interaction.

**Endpoint:** `POST /oauth/token`

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/token \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=refresh_token" \
  -d "refresh_token=mm_rt_AbCdEfGh12345678901234567890123456789012"
```

**Response:**
```json
{
  "access_token": "mm_at_NewAccessToken123456789",
  "token_type": "Bearer",
  "expires_in": 3600,
  "refresh_token": "mm_rt_NewRefreshToken987654321",
  "scope": "read write"
}
```

**Note:** Refresh tokens are single-use. Each refresh returns a new access token AND a new refresh token.

### 5. Client Credentials Grant

For machine-to-machine authentication without user context:

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/token \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "scope=read write"
```

**Response:**
```json
{
  "access_token": "mm_at_ClientAccessToken123456789",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "read write"
}
```

### 6. Token Introspection

Check if a token is active and retrieve its metadata (requires client authentication).

**Endpoint:** `POST /oauth/introspect`

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/introspect \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=mm_at_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN9qR"
```

**Response (active token):**
```json
{
  "active": true,
  "scope": "read write",
  "client_id": "mm_AbCdEfGh12345678901234",
  "token_type": "Bearer",
  "exp": 1705323600,
  "iat": 1705320000,
  "iss": "http://localhost:3000"
}
```

**Response (inactive token):**
```json
{
  "active": false
}
```

### 7. Token Revocation

Revoke access or refresh tokens when they're no longer needed.

**Endpoint:** `POST /oauth/revoke`

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/revoke \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=mm_at_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN9qR" \
  -d "token_type_hint=access_token"
```

**Response:** `200 OK` (always returns success per RFC 7009, even if token doesn't exist)

---

## Scopes and Permissions

Fortémi uses OAuth2 scopes to control access levels.

| Scope    | Description                                      | Permissions                                    |
|----------|--------------------------------------------------|------------------------------------------------|
| `read`   | Read-only access                                 | List/get notes, search, view tags/collections |
| `write`  | Create and update resources                      | `read` + create/update notes, tags            |
| `delete` | Delete resources                                 | `read` `write` + delete notes, purge          |
| `admin`  | Full administrative access                       | All permissions + API key management          |
| `mcp`    | MCP server access (includes read + write)        | `read` `write` + MCP-specific operations      |

### Scope Hierarchy

- `admin` includes all other scopes
- `mcp` includes `read` and `write`
- `delete` typically requires `write`
- Scopes can be combined with spaces: `"read write delete"`

### Checking Scopes in Code

```python
# The API validates scopes automatically.
# If your token lacks the required scope, you'll receive a 403 Forbidden response.

# Example: Creating a note requires 'write' scope
response = requests.post(
    "http://localhost:3000/api/v1/notes",
    headers={"Authorization": f"Bearer {token}"},
    json={"content": "New note"}
)

if response.status_code == 403:
    print("Insufficient permissions. 'write' scope required.")
```

---

## Rate Limiting

API keys include automatic rate limiting to prevent abuse.

### Default Limits

- **Per minute:** 60 requests
- **Per hour:** 1000 requests

### Rate Limit Headers

Response headers indicate current limit status:

```
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1705320060
```

### Handling Rate Limits

**HTTP 429 Response:**
```json
{
  "error": "Rate limit exceeded",
  "retry_after": 30
}
```

**Best Practices:**
- Implement exponential backoff when rate limited
- Cache frequently accessed data
- Batch operations when possible (e.g., `bulk_create_notes`)
- Monitor `X-RateLimit-Remaining` header

**Python Example:**
```python
import time
import requests

def api_call_with_retry(url, headers, max_retries=3):
    for attempt in range(max_retries):
        response = requests.get(url, headers=headers)

        if response.status_code == 429:
            retry_after = int(response.headers.get("Retry-After", 60))
            print(f"Rate limited. Waiting {retry_after}s...")
            time.sleep(retry_after)
            continue

        return response

    raise Exception("Max retries exceeded")
```

---

## Error Handling

### Authentication Errors

#### 401 Unauthorized

**Cause:** Missing, invalid, or expired token

```json
{
  "error": "Authentication required"
}
```

**Resolution:**
- Verify token is included in `Authorization: Bearer {token}` header
- Check token hasn't expired (access tokens expire after 1 hour)
- Refresh token if expired (OAuth2) or generate new API key

#### 403 Forbidden

**Cause:** Valid token but insufficient permissions

```json
{
  "error": "Missing required scope: write"
}
```

**Resolution:**
- Check token has required scope
- Request new token with broader scope
- For API keys, create new key with appropriate scope

### OAuth2 Errors

OAuth2 errors follow RFC 6749 format:

```json
{
  "error": "invalid_grant",
  "error_description": "Authorization code has expired"
}
```

| Error Code                  | Description                           | Common Cause                          |
|-----------------------------|---------------------------------------|---------------------------------------|
| `invalid_request`           | Missing or malformed parameter        | Missing required field                |
| `invalid_client`            | Client authentication failed          | Wrong client_id or client_secret      |
| `invalid_grant`             | Authorization code/refresh token bad  | Expired or already used code          |
| `unauthorized_client`       | Client not authorized for grant type  | Requesting unsupported grant type     |
| `unsupported_grant_type`    | Grant type not supported              | Typo in grant_type parameter          |
| `invalid_scope`             | Requested scope invalid               | Non-existent or unauthorized scope    |

**Error Handling Example:**
```python
def exchange_code_for_token(auth_code):
    try:
        response = requests.post(
            "http://localhost:3000/oauth/token",
            headers={"Authorization": f"Basic {auth_header}"},
            data={
                "grant_type": "authorization_code",
                "code": auth_code,
                "redirect_uri": redirect_uri,
                "code_verifier": code_verifier
            }
        )

        if response.status_code == 400:
            error = response.json()
            if error["error"] == "invalid_grant":
                print("Code expired or already used. Restart authorization flow.")
            elif error["error"] == "invalid_client":
                print("Client credentials invalid. Check client_id/secret.")

        response.raise_for_status()
        return response.json()

    except requests.exceptions.HTTPError as e:
        print(f"Token exchange failed: {e}")
        return None
```

---

## Security Best Practices

### Token Storage

**DO:**
- Store API keys and client secrets in environment variables or secure vaults
- Use encrypted storage for tokens on client devices
- Implement token rotation for long-lived applications
- Clear tokens on logout

**DON'T:**
- Commit tokens to version control
- Store tokens in localStorage for sensitive apps (use httpOnly cookies instead)
- Share tokens between users
- Log tokens in application logs

### PKCE for Public Clients

Always use PKCE (Proof Key for Code Exchange) for:
- Single-page applications (SPAs)
- Mobile applications
- Desktop applications
- Any client that cannot securely store secrets

### Secure Communication

- **Always use HTTPS** in production
- Validate SSL/TLS certificates
- Use certificate pinning for mobile apps

### Token Lifecycle

- **Access tokens:** 1 hour default expiration (use refresh tokens)
- **MCP access tokens:** 4 hour default expiration (longer to support interactive AI sessions)
- **Refresh tokens:** 30 days expiration (require re-authentication after)
- **API keys:** Optional expiration (recommend 90-365 days for rotation)
- **Authorization codes:** 10 minutes expiration (single-use)

#### Configurable Token Lifetimes

Token lifetimes can be tuned via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `OAUTH_TOKEN_LIFETIME_SECS` | `3600` (1 hour) | Standard access token lifetime |
| `OAUTH_MCP_TOKEN_LIFETIME_SECS` | `14400` (4 hours) | MCP access token lifetime |

**Tradeoffs:**
- **Shorter tokens** improve security posture but require more frequent re-authentication
- **Longer MCP tokens** reduce mid-session disconnects for interactive AI workflows
- Recommend not exceeding 24 hours for standard tokens or 48 hours for MCP tokens

### Scope Minimization

Request only the scopes you need:

```python
# Good: Minimal scope
scope = "read"

# Bad: Over-privileged
scope = "read write delete admin"
```

### Revocation

Revoke tokens immediately when:
- User logs out
- Security incident detected
- Token potentially compromised
- User revokes application access

### Environment-Specific Configuration

**Development:**
```bash
# .env.development
MATRIC_MEMORY_URL=http://localhost:3000
MATRIC_MEMORY_API_KEY=mm_key_dev_test_only_12345
```

**Production:**
```bash
# .env.production (use secrets manager)
MATRIC_MEMORY_URL=http://localhost:3000
MATRIC_MEMORY_API_KEY=${VAULT_API_KEY}  # Loaded from vault
```

---

## MCP Server Authentication

The MCP server supports both authentication modes:

### Stdio Mode (API Keys)

```bash
# Set environment variable
export MATRIC_MEMORY_API_KEY="mm_key_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7"

# Run MCP server
cd mcp-server
node index.js
```

### HTTP Mode (OAuth2)

```bash
# Set transport mode
export MCP_TRANSPORT=http
export MCP_PORT=3001

# Run MCP server
cd mcp-server
node index.js
```

The MCP server will:
1. Use token introspection to validate OAuth2 access tokens
2. Store tokens per-session using AsyncLocalStorage
3. Automatically include tokens in API requests

---

## Complete Examples

### Python OAuth2 Client

```python
import requests
import secrets
import hashlib
import base64
from urllib.parse import urlencode

class MatricMemoryClient:
    def __init__(self, client_id, client_secret, redirect_uri):
        self.client_id = client_id
        self.client_secret = client_secret
        self.redirect_uri = redirect_uri
        self.base_url = "http://localhost:3000"
        self.access_token = None
        self.refresh_token = None

    def get_authorization_url(self):
        """Generate authorization URL with PKCE"""
        # Generate PKCE parameters
        self.code_verifier = base64.urlsafe_b64encode(
            secrets.token_bytes(32)
        ).decode('utf-8').rstrip('=')

        code_challenge = base64.urlsafe_b64encode(
            hashlib.sha256(self.code_verifier.encode()).digest()
        ).decode('utf-8').rstrip('=')

        self.state = secrets.token_urlsafe(32)

        params = {
            "response_type": "code",
            "client_id": self.client_id,
            "redirect_uri": self.redirect_uri,
            "scope": "read write",
            "state": self.state,
            "code_challenge": code_challenge,
            "code_challenge_method": "S256"
        }

        return f"{self.base_url}/oauth/authorize?{urlencode(params)}"

    def exchange_code(self, code):
        """Exchange authorization code for tokens"""
        credentials = f"{self.client_id}:{self.client_secret}"
        auth_header = base64.b64encode(credentials.encode()).decode()

        response = requests.post(
            f"{self.base_url}/oauth/token",
            headers={
                "Authorization": f"Basic {auth_header}",
                "Content-Type": "application/x-www-form-urlencoded"
            },
            data={
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": self.redirect_uri,
                "code_verifier": self.code_verifier
            }
        )
        response.raise_for_status()

        tokens = response.json()
        self.access_token = tokens["access_token"]
        self.refresh_token = tokens["refresh_token"]
        return tokens

    def refresh(self):
        """Refresh access token"""
        credentials = f"{self.client_id}:{self.client_secret}"
        auth_header = base64.b64encode(credentials.encode()).decode()

        response = requests.post(
            f"{self.base_url}/oauth/token",
            headers={
                "Authorization": f"Basic {auth_header}",
                "Content-Type": "application/x-www-form-urlencoded"
            },
            data={
                "grant_type": "refresh_token",
                "refresh_token": self.refresh_token
            }
        )
        response.raise_for_status()

        tokens = response.json()
        self.access_token = tokens["access_token"]
        self.refresh_token = tokens["refresh_token"]
        return tokens

    def request(self, method, path, **kwargs):
        """Make authenticated API request"""
        headers = kwargs.get("headers", {})
        headers["Authorization"] = f"Bearer {self.access_token}"
        kwargs["headers"] = headers

        response = requests.request(method, f"{self.base_url}{path}", **kwargs)

        # Auto-refresh on 401
        if response.status_code == 401 and self.refresh_token:
            self.refresh()
            headers["Authorization"] = f"Bearer {self.access_token}"
            response = requests.request(method, f"{self.base_url}{path}", **kwargs)

        response.raise_for_status()
        return response.json() if response.content else None

    def create_note(self, content, tags=None):
        """Create a new note"""
        return self.request(
            "POST",
            "/api/v1/notes",
            json={"content": content, "tags": tags or []}
        )

    def search_notes(self, query, limit=20):
        """Search notes"""
        return self.request(
            "GET",
            f"/api/v1/search?q={query}&limit={limit}"
        )

# Usage
if __name__ == "__main__":
    client = MatricMemoryClient(
        client_id="mm_AbCdEfGh12345678901234",
        client_secret="sEcReT_xYz9Q4rTp2Lm8vBn3HjK6WcE1DfG5sA7UiO0pN",
        redirect_uri="http://localhost:3000/callback"
    )

    # Step 1: Get authorization URL
    auth_url = client.get_authorization_url()
    print(f"Visit: {auth_url}")

    # Step 2: After redirect, exchange code
    code = input("Enter code from redirect: ")
    client.exchange_code(code)

    # Step 3: Use API
    result = client.create_note("Hello from OAuth2!", ["test"])
    print(f"Created note: {result}")
```

### Simple API Key Script

```python
#!/usr/bin/env python3
import os
import requests

API_BASE = "http://localhost:3000"
API_KEY = os.environ.get("MATRIC_MEMORY_API_KEY")

if not API_KEY:
    print("Error: MATRIC_MEMORY_API_KEY environment variable not set")
    exit(1)

headers = {
    "Authorization": f"Bearer {API_KEY}",
    "Content-Type": "application/json"
}

def create_note(content, tags=None):
    response = requests.post(
        f"{API_BASE}/api/v1/notes",
        headers=headers,
        json={"content": content, "tags": tags or []}
    )
    response.raise_for_status()
    return response.json()

def search_notes(query):
    response = requests.get(
        f"{API_BASE}/api/v1/search",
        headers=headers,
        params={"q": query}
    )
    response.raise_for_status()
    return response.json()

if __name__ == "__main__":
    # Create a note
    note = create_note("Daily standup notes", ["work", "meetings"])
    print(f"Created: {note}")

    # Search notes
    results = search_notes("standup")
    print(f"Found {results['total']} results")
```

---

## Additional Resources

- **API Reference:** `/docs` (Swagger UI)
- **OpenAPI Spec:** `/openapi.yaml`
- **OAuth2 RFC 6749:** https://datatracker.ietf.org/doc/html/rfc6749
- **PKCE RFC 7636:** https://datatracker.ietf.org/doc/html/rfc7636
- **Token Introspection RFC 7662:** https://datatracker.ietf.org/doc/html/rfc7662

For questions or issues, please contact support or open an issue on the project repository.

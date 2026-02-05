# UAT Phase 17: OAuth & Authentication

**Duration**: ~12 minutes
**Tools Tested**: API endpoints (curl-based)
**Dependencies**: Phase 0 (preflight)

---

## Overview

This phase tests OAuth 2.0 flows, API key management, and authentication mechanisms. Matric Memory implements OAuth 2.0 with client credentials grant for machine-to-machine authentication (MCP server) and API key authentication for programmatic access.

---

## Important Notes

- OAuth discovery endpoint follows OpenID Connect Discovery spec
- Client registration is dynamic (no pre-registration required)
- Tokens use JWT format with configurable expiry
- API keys are long-lived bearer tokens
- Scope enforcement: `mcp`, `read`, `write`
- Base URL: `http://localhost:3000`

---

## Test Setup

For these tests, you'll need:
- A running matric-memory API server on localhost:3000
- `curl` command-line tool
- `jq` for JSON parsing (optional but recommended)

```bash
BASE_URL="http://localhost:3000"
```

---

## Test Cases

### OAuth Discovery

#### AUTH-001: Get OpenID Configuration

**Command**:
```bash
curl -s "$BASE_URL/.well-known/openid-configuration" | jq
```

**Expected Response**:
```json
{
  "issuer": "http://localhost:3000",
  "token_endpoint": "http://localhost:3000/oauth/token",
  "introspection_endpoint": "http://localhost:3000/oauth/introspect",
  "revocation_endpoint": "http://localhost:3000/oauth/revoke",
  "registration_endpoint": "http://localhost:3000/oauth/register",
  "grant_types_supported": ["client_credentials"],
  "token_endpoint_auth_methods_supported": ["client_secret_post"],
  "scopes_supported": ["mcp", "read", "write"]
}
```

**Pass Criteria**:
- Returns 200 OK
- Contains required OAuth endpoints
- Includes supported grant types and scopes

**Store**: Endpoint URLs for subsequent tests

---

### Client Registration

#### AUTH-002: Register OAuth Client

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/register" \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "UAT Test Client",
    "grant_types": ["client_credentials"],
    "scope": "mcp read write"
  }' | jq
```

**Expected Response**:
```json
{
  "client_id": "mm_xxxxxxxxxxxxxxxx",
  "client_secret": "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "client_name": "UAT Test Client",
  "grant_types": ["client_credentials"],
  "scope": "mcp read write",
  "created_at": "<timestamp>"
}
```

**Pass Criteria**:
- Returns 201 Created
- `client_id` starts with `mm_`
- `client_secret` is returned (shown only once)

**Store**: `CLIENT_ID`, `CLIENT_SECRET`

---

#### AUTH-003: Register Client - Minimal Request

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/register" \
  -H "Content-Type: application/json" \
  -d '{"client_name": "Minimal Client"}' | jq
```

**Pass Criteria**:
- Returns 201 Created
- Default grant_types: `["client_credentials"]`
- Default scope includes basic permissions

---

#### AUTH-004: Register Client - Invalid Request

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/register" \
  -H "Content-Type: application/json" \
  -d '{"grant_types": ["invalid_grant"]}' | jq
```

**Expected**: 400 Bad Request

**Pass Criteria**: Rejects unsupported grant types

---

### Token Issuance

#### AUTH-005: Request Access Token

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" \
  -d "scope=read write" | jq
```

**Expected Response**:
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "read write"
}
```

**Pass Criteria**:
- Returns 200 OK
- `access_token` is JWT format (3 base64url parts separated by dots)
- `token_type` is "Bearer"
- `expires_in` is numeric

**Store**: `ACCESS_TOKEN`

---

#### AUTH-006: Request Token - Wrong Credentials

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=wrong_secret" | jq
```

**Expected**: 401 Unauthorized

**Pass Criteria**: Rejects invalid credentials

---

#### AUTH-007: Request Token - Missing Parameters

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" | jq
```

**Expected**: 400 Bad Request

**Pass Criteria**: Requires client_id and client_secret

---

### Token Introspection

#### AUTH-008: Introspect Active Token

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" | jq
```

**Expected Response**:
```json
{
  "active": true,
  "scope": "read write",
  "client_id": "mm_xxxxxxxxxxxxxxxx",
  "exp": 1234567890,
  "iat": 1234564290
}
```

**Pass Criteria**:
- Returns 200 OK
- `active` is true
- Contains scope and expiry info

---

#### AUTH-009: Introspect Invalid Token

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=invalid.token.here" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" | jq
```

**Expected Response**:
```json
{
  "active": false
}
```

**Pass Criteria**: Returns `active: false` for invalid tokens

---

### Token Revocation

#### AUTH-010: Revoke Token

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/revoke" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET"
```

**Expected**: 200 OK (empty response)

**Pass Criteria**: Token successfully revoked

---

#### AUTH-011: Verify Token Revoked

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" | jq
```

**Expected**: `{ "active": false }`

**Pass Criteria**: Revoked token is now inactive

---

### API Key Management

#### AUTH-012: Create API Key

**Setup**: First get a fresh access token for API requests

```bash
# Get new token since previous was revoked
ACCESS_TOKEN=$(curl -s -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" | jq -r .access_token)
```

**Command**:
```bash
curl -X POST "$BASE_URL/api/v1/api-keys" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "UAT Test API Key",
    "scopes": ["read", "write"],
    "expires_at": "2027-12-31T23:59:59Z"
  }' | jq
```

**Expected Response**:
```json
{
  "id": "<uuid>",
  "key": "mmk_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "name": "UAT Test API Key",
  "scopes": ["read", "write"],
  "created_at": "<timestamp>",
  "expires_at": "2027-12-31T23:59:59Z"
}
```

**Pass Criteria**:
- Returns 201 Created
- `key` starts with `mmk_`
- Key is shown only once

**Store**: `API_KEY`, `API_KEY_ID`

---

#### AUTH-013: List API Keys

**Command**:
```bash
curl -X GET "$BASE_URL/api/v1/api-keys" \
  -H "Authorization: Bearer $ACCESS_TOKEN" | jq
```

**Expected Response**:
```json
{
  "api_keys": [
    {
      "id": "<uuid>",
      "name": "UAT Test API Key",
      "scopes": ["read", "write"],
      "created_at": "<timestamp>",
      "last_used": null,
      "expires_at": "2027-12-31T23:59:59Z"
    }
  ]
}
```

**Pass Criteria**:
- Returns 200 OK
- Lists created API key (without revealing actual key)

---

#### AUTH-014: Authenticate with API Key

**Command**:
```bash
curl -X GET "$BASE_URL/api/v1/notes?limit=1" \
  -H "Authorization: Bearer $API_KEY" | jq
```

**Expected**: 200 OK with notes array

**Pass Criteria**: API key successfully authenticates request

---

#### AUTH-015: Test API Key Scope Enforcement

**Command** (attempt write operation with read-only key):
```bash
# First, create a read-only API key
READONLY_KEY=$(curl -s -X POST "$BASE_URL/api/v1/api-keys" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Read Only", "scopes": ["read"]}' | jq -r .key)

# Try to create a note (write operation)
curl -X POST "$BASE_URL/api/v1/notes" \
  -H "Authorization: Bearer $READONLY_KEY" \
  -H "Content-Type: application/json" \
  -d '{"content": "Test", "tags": ["test"]}' | jq
```

**Expected**: 403 Forbidden

**Pass Criteria**: Scope restrictions are enforced

---

#### AUTH-016: Revoke API Key

**Command**:
```bash
curl -X DELETE "$BASE_URL/api/v1/api-keys/$API_KEY_ID" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**Expected**: 204 No Content

**Pass Criteria**: API key successfully revoked

---

#### AUTH-017: Verify API Key Revoked

**Command**:
```bash
curl -X GET "$BASE_URL/api/v1/notes?limit=1" \
  -H "Authorization: Bearer $API_KEY" | jq
```

**Expected**: 401 Unauthorized

**Pass Criteria**: Revoked API key no longer authenticates

---

### Edge Cases

#### AUTH-018: Expired Token Handling

**Command** (simulate with past expiry):
```bash
# This test requires waiting for token expiry or using a test token
# For UAT, verify that the API rejects expired tokens
# Implementation depends on token TTL configuration

# Check current token validity
curl -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" | jq '.exp'
```

**Pass Criteria**: Expiry time (`exp`) is validated

---

#### AUTH-019: Missing Authorization Header

**Command**:
```bash
curl -X GET "$BASE_URL/api/v1/notes" | jq
```

**Expected**: 401 Unauthorized

**Pass Criteria**: Protected endpoints require authentication

---

#### AUTH-020: Invalid Bearer Token Format

**Command**:
```bash
curl -X GET "$BASE_URL/api/v1/notes" \
  -H "Authorization: Bearer not-a-valid-token" | jq
```

**Expected**: 401 Unauthorized

**Pass Criteria**: Invalid tokens are rejected

---

### MCP Server Authentication Flow

#### AUTH-021: MCP Server Registration Flow

**Simulated MCP server registration**:

```bash
# 1. MCP server registers client
MCP_CLIENT=$(curl -s -X POST "$BASE_URL/oauth/register" \
  -H "Content-Type: application/json" \
  -d '{"client_name": "MCP Server", "grant_types": ["client_credentials"], "scope": "mcp read"}' | jq)

MCP_CLIENT_ID=$(echo $MCP_CLIENT | jq -r .client_id)
MCP_CLIENT_SECRET=$(echo $MCP_CLIENT | jq -r .client_secret)

# 2. MCP server gets token
MCP_TOKEN=$(curl -s -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=$MCP_CLIENT_ID" \
  -d "client_secret=$MCP_CLIENT_SECRET" \
  -d "scope=mcp read" | jq -r .access_token)

# 3. MCP server uses token
curl -X GET "$BASE_URL/api/v1/notes?limit=1" \
  -H "Authorization: Bearer $MCP_TOKEN" | jq
```

**Pass Criteria**:
- Client registration succeeds
- Token issuance succeeds
- MCP scope grants read access

---

#### AUTH-022: MCP Token Introspection

**Command**:
```bash
curl -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$MCP_TOKEN" \
  -d "client_id=$MCP_CLIENT_ID" \
  -d "client_secret=$MCP_CLIENT_SECRET" | jq
```

**Expected**: `active: true`, `scope` includes "mcp"

**Pass Criteria**: MCP token is valid and has correct scope

---

## Cleanup

```bash
# Revoke all test tokens
# Delete test API keys via DELETE /api/v1/api-keys/:id
# OAuth clients persist in database (cleanup via admin tools if needed)

# Clean environment variables
unset CLIENT_ID CLIENT_SECRET ACCESS_TOKEN API_KEY API_KEY_ID
unset MCP_CLIENT_ID MCP_CLIENT_SECRET MCP_TOKEN READONLY_KEY
```

---

## Success Criteria

| Test ID | Name | Status |
|---------|------|--------|
| AUTH-001 | OpenID Configuration | |
| AUTH-002 | Register OAuth Client | |
| AUTH-003 | Register Minimal Client | |
| AUTH-004 | Invalid Registration | |
| AUTH-005 | Request Access Token | |
| AUTH-006 | Wrong Credentials | |
| AUTH-007 | Missing Parameters | |
| AUTH-008 | Introspect Active Token | |
| AUTH-009 | Introspect Invalid Token | |
| AUTH-010 | Revoke Token | |
| AUTH-011 | Verify Revoked | |
| AUTH-012 | Create API Key | |
| AUTH-013 | List API Keys | |
| AUTH-014 | Authenticate with Key | |
| AUTH-015 | Scope Enforcement | |
| AUTH-016 | Revoke API Key | |
| AUTH-017 | Verify Key Revoked | |
| AUTH-018 | Expired Token | |
| AUTH-019 | Missing Auth Header | |
| AUTH-020 | Invalid Token Format | |
| AUTH-021 | MCP Registration Flow | |
| AUTH-022 | MCP Token Introspection | |

**Pass Rate Required**: 95% (21/22)

---

## API Endpoints Tested

| Endpoint | Method | Tests |
|----------|--------|-------|
| `/.well-known/openid-configuration` | GET | AUTH-001 |
| `/oauth/register` | POST | AUTH-002, AUTH-003, AUTH-004 |
| `/oauth/token` | POST | AUTH-005, AUTH-006, AUTH-007, AUTH-021 |
| `/oauth/introspect` | POST | AUTH-008, AUTH-009, AUTH-011, AUTH-022 |
| `/oauth/revoke` | POST | AUTH-010 |
| `/api/v1/api-keys` | POST | AUTH-012, AUTH-015 |
| `/api/v1/api-keys` | GET | AUTH-013 |
| `/api/v1/api-keys/:id` | DELETE | AUTH-016 |
| `/api/v1/notes` | GET | AUTH-014, AUTH-017, AUTH-019, AUTH-020, AUTH-021 |

**Coverage**: 9 endpoints, 22 tests

---

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

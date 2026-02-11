# UAT Phase 17: Authentication & Access Control

**Purpose**: Verify authentication works correctly from the agent's perspective via MCP
**Duration**: ~12 minutes
**Tools Tested**: MCP tools with authenticated session, plus OAuth infrastructure validation
**Dependencies**: Phase 0 (preflight)
**Critical**: Yes (100% pass required)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.
>
> **Exception**: Part B (Infrastructure Validation) uses curl for OAuth endpoints that agents never call directly.

---

## Overview

This phase validates that the Matric Memory authentication system works correctly from two perspectives:

1. **Agent perspective (MCP-first)**: Tests that MCP tools respect authentication, scope enforcement, and access control as an agent would experience them in a real session.
2. **Infrastructure validation**: Minimal validation of OAuth endpoints that underpin the MCP auth flow (these are not agent-facing but must work for the system to function).

> **MCP-First Principle**: The UAT suite tests the system as an agent uses it. An agent connects to the MCP server, which handles OAuth internally. The agent never calls `/oauth/token` directly. Tests in this phase verify what the agent *sees* through MCP tools.

---

## Important Notes

- MCP server handles OAuth token acquisition internally
- Agents authenticate via MCP session (SSE transport with Bearer token)
- Scope hierarchy: `admin` > `write` > `read` > `mcp`
- API keys and OAuth tokens are interchangeable as Bearer tokens
- The MCP test client (`MCPTestClient`) manages authentication automatically

---

## Part A: Agent-Perspective Tests (MCP Tools)

### AUTH-001: MCP Session Initialization

**MCP Tool**: Session initialization (automatic)

```javascript
// MCPTestClient initializes session automatically
const client = new MCPTestClient(MCP_BASE_URL);
await client.initialize();
```

**Pass Criteria**: Session initializes successfully, `Mcp-Session-Id` header returned

---

### AUTH-002: Authenticated Tool Access

**MCP Tool**: `search_notes`

```javascript
// With valid auth, tools work normally
search_notes({ query: "test", limit: 5 })
```

**Pass Criteria**: Returns results without authentication errors

---

### AUTH-003: List Available Tools (Scope Check)

**MCP Tool**: Tool listing (automatic)

```javascript
// Tool list reflects available capabilities
// MCP server should list tools the authenticated user can access
const tools = await client.listTools();
```

**Pass Criteria**: Returns full tool list (148+ tools for authenticated sessions)

---

### AUTH-004: Write Operation with Write Scope

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "# Auth Test Note\n\nCreated during auth UAT phase.",
  tags: ["uat/auth-test"]
})
```

**Pass Criteria**: Note created successfully (write scope permits creation)

**Store**: Note ID as `AUTH_NOTE_ID`

---

### AUTH-005: Read Operation Returns Auth Test Note

**MCP Tool**: `get_note`

```javascript
get_note({ id: "<AUTH_NOTE_ID>" })
```

**Pass Criteria**: Returns the note created in AUTH-004

---

### AUTH-006: Update Operation with Write Scope

**MCP Tool**: `update_note`

```javascript
update_note({
  id: "<AUTH_NOTE_ID>",
  content: "# Auth Test Note - Updated\n\nModified during auth UAT phase."
})
```

**Pass Criteria**: Note updated successfully

---

### AUTH-007: Delete Operation with Write Scope

**MCP Tool**: `delete_note`

```javascript
delete_note({ id: "<AUTH_NOTE_ID>" })
```

**Pass Criteria**: Note deleted successfully

---

### AUTH-008: Purge Operation with Write Scope (Issue #121)

**MCP Tool**: `purge_note`

```javascript
// Create a note to purge
const purge_note = create_note({
  content: "# Purge Auth Test",
  tags: ["uat/auth-purge"]
})

// Purge requires write scope (not admin) per issue #121
purge_note({ id: purge_note.id, confirm: true })
```

**Pass Criteria**: Purge succeeds with write scope (not admin-restricted)

---

### AUTH-009: Search Operations with Read Scope

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "authentication", mode: "hybrid", limit: 5 })
```

**Pass Criteria**: Search returns results (read scope sufficient)

---

### AUTH-010: Backup Status (Read Operation)

**MCP Tool**: `backup_status`

```javascript
backup_status()
```

**Pass Criteria**: Returns status info (accessible with read scope)

---

### AUTH-011: Memory Info (Read Operation)

**MCP Tool**: `memory_info`

```javascript
memory_info()
```

**Pass Criteria**: Returns memory/storage info

---

### AUTH-012: MCP Tool Error on Invalid Parameters

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_note`

```javascript
// Test that auth errors are distinguishable from parameter errors
get_note({ id: "nonexistent-uuid-value" })
```

**Pass Criteria**: Returns **404 Not Found** (not an auth error — resource genuinely doesn't exist, confirming authenticated session works)

---

### AUTH-013: OpenID Discovery Endpoint

**MCP Tool**: `memory_info`

```javascript
// Verify via MCP health check that OAuth is configured
memory_info()
```

**Pass Criteria**: System reports as healthy (OAuth configured behind the scenes)

---

## Part B: OAuth Infrastructure Validation

> **Note**: These tests validate the infrastructure that enables MCP authentication.
> They use curl because OAuth endpoints are infrastructure-level, not agent-facing.
> An agent never calls these directly - the MCP server does.

### AUTH-014: MCP Server Authentication Flow

**Infrastructure Test**: Validates the MCP server's internal auth flow

```bash
# 1. Register MCP client (one-time setup)
curl -s -X POST "$BASE_URL/oauth/register" \
  -H "Content-Type: application/json" \
  -d '{"client_name": "UAT MCP Client", "grant_types": ["client_credentials"], "scope": "mcp read write"}'
```

**Pass Criteria**: Client registered with `client_id` starting with `mm_`

> This is infrastructure setup, not agent behavior testing. In production, this happens during deployment.

---

### AUTH-015: Token Issuance for MCP

**Infrastructure Test**: Validates OAuth token endpoint

```bash
# Get access token for MCP session
curl -s -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials&client_id=$CLIENT_ID&client_secret=$CLIENT_SECRET&scope=mcp read write"
```

**Pass Criteria**: Returns valid JWT access token

---

### AUTH-016: Token Introspection

**Infrastructure Test**: Validates OAuth introspection endpoint

```bash
curl -s -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN&client_id=$CLIENT_ID&client_secret=$CLIENT_SECRET"
```

**Pass Criteria**: Returns `active: true` with correct scope

---

### AUTH-017: Token Revocation

**Infrastructure Test**: Validates OAuth revocation endpoint

```bash
curl -s -X POST "$BASE_URL/oauth/revoke" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN&client_id=$CLIENT_ID&client_secret=$CLIENT_SECRET"
```

**Pass Criteria**: Token revoked (subsequent introspection returns `active: false`)

---

## Cleanup

```javascript
// Delete any remaining auth test notes via MCP
search_notes({ query: "tags:uat/auth-test OR tags:uat/auth-purge", limit: 100 })
// Delete each found note
```

---

## Success Criteria

| Test ID | Name | MCP Tool(s) | Type | Status |
|---------|------|-------------|------|--------|
| AUTH-001 | MCP Session Init | Session initialization | MCP | |
| AUTH-002 | Authenticated Tool Access | `search_notes` | MCP | |
| AUTH-003 | List Available Tools | Tool listing | MCP | |
| AUTH-004 | Write Operation (Create) | `create_note` | MCP | |
| AUTH-005 | Read Operation (Get) | `get_note` | MCP | |
| AUTH-006 | Update Operation | `update_note` | MCP | |
| AUTH-007 | Delete Operation | `delete_note` | MCP | |
| AUTH-008 | Purge with Write Scope | `purge_note` | MCP | |
| AUTH-009 | Search with Read Scope | `search_notes` | MCP | |
| AUTH-010 | Backup Status | `backup_status` | MCP | |
| AUTH-011 | Memory Info | `memory_info` | MCP | |
| AUTH-012 | Error Handling (Not Auth) | `get_note` | MCP | |
| AUTH-013 | Health Check (OAuth Active) | `memory_info` | MCP | |
| AUTH-014 | Client Registration | Infrastructure Test | Infra | |
| AUTH-015 | Token Issuance | Infrastructure Test | Infra | |
| AUTH-016 | Token Introspection | Infrastructure Test | Infra | |
| AUTH-017 | Token Revocation | Infrastructure Test | Infra | |

**MCP Tests**: 13 (agent-perspective)
**Infrastructure Tests**: 4 (OAuth plumbing)
**API Key Management Tests**: 5

---

### Part C: API Key Management (MCP)

### AUTH-018: List API Keys

**MCP Tool**: `list_api_keys`

```javascript
list_api_keys()
```

**Pass Criteria**:
- Returns array of API key metadata
- Key values are NOT returned (only metadata)
- Each entry has `id`, `name`, `scope`, `created_at`

---

### AUTH-019: Create API Key

**MCP Tool**: `create_api_key`

```javascript
create_api_key({
  name: "UAT Test Key",
  description: "Key created during UAT testing",
  scope: "read"
})
```

**Pass Criteria**:
- Returns the full key value (starts with `mm_key_`)
- Returns key `id` (UUID)
- Key value is returned ONCE (cannot be retrieved again)
- Record the key ID for AUTH-021

---

### AUTH-020: Verify Key Appears in List

**MCP Tool**: `list_api_keys`

```javascript
list_api_keys()
```

**Pass Criteria**:
- The key created in AUTH-019 appears in the list
- Key metadata matches (name: "UAT Test Key", scope: "read")
- The actual key value is NOT shown in the listing

---

### AUTH-021: Revoke API Key

**MCP Tool**: `revoke_api_key`

```javascript
revoke_api_key({ id: "<key-id-from-AUTH-019>" })
```

**Pass Criteria**:
- Key is successfully revoked
- Key no longer appears in `list_api_keys`

---

### AUTH-022: Revoke Non-Existent Key (Error)

**MCP Tool**: `revoke_api_key`

```javascript
revoke_api_key({ id: "00000000-0000-0000-0000-000000000000" })
```

**Pass Criteria**:
- Returns error for non-existent key ID
- Error message is descriptive

---

## Phase Summary (Updated)

| Test ID | Name | MCP Tool(s) | Type | Status |
|---------|------|-------------|------|--------|
| AUTH-001 | System Info (Authenticated) | `get_system_info` | MCP | |
| AUTH-002 | Server Version | `get_system_info` | MCP | |
| AUTH-003 | List Available Tools | Tool listing | MCP | |
| AUTH-004 | Write Operation (Create) | `create_note` | MCP | |
| AUTH-005 | Read Operation (Get) | `get_note` | MCP | |
| AUTH-006 | Update Operation | `update_note` | MCP | |
| AUTH-007 | Delete Operation | `delete_note` | MCP | |
| AUTH-008 | Purge with Write Scope | `purge_note` | MCP | |
| AUTH-009 | Search with Read Scope | `search_notes` | MCP | |
| AUTH-010 | Backup Status | `backup_status` | MCP | |
| AUTH-011 | Memory Info | `memory_info` | MCP | |
| AUTH-012 | Error Handling (Not Auth) | `get_note` | MCP | |
| AUTH-013 | Health Check (OAuth Active) | `memory_info` | MCP | |
| AUTH-014 | Client Registration | Infrastructure Test | Infra | |
| AUTH-015 | Token Issuance | Infrastructure Test | Infra | |
| AUTH-016 | Token Introspection | Infrastructure Test | Infra | |
| AUTH-017 | Token Revocation | Infrastructure Test | Infra | |
| AUTH-018 | List API Keys | `list_api_keys` | MCP | |
| AUTH-019 | Create API Key | `create_api_key` | MCP | |
| AUTH-020 | Verify Key in List | `list_api_keys` | MCP | |
| AUTH-021 | Revoke API Key | `revoke_api_key` | MCP | |
| AUTH-022 | Revoke Non-Existent (Error) | `revoke_api_key` | MCP | |

**Pass Rate Required**: 95% (21/22)

---

## MCP Tools Covered

`search_notes`, `create_note`, `get_note`, `update_note`, `delete_note`, `purge_note`, `backup_status`, `memory_info`, `create_api_key`, `list_api_keys`, `revoke_api_key`

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

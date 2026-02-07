# UAT Phase 17: Authentication & Access Control

**Purpose**: Verify authentication works correctly from the agent's perspective via MCP
**Duration**: ~12 minutes
**Tools Tested**: MCP tools with authenticated session, plus OAuth infrastructure validation
**Dependencies**: Phase 0 (preflight)
**Critical**: Yes (100% pass required)

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

```javascript
// MCPTestClient initializes session automatically
const client = new MCPTestClient(MCP_BASE_URL);
await client.initialize();
```

**Pass Criteria**: Session initializes successfully, `Mcp-Session-Id` header returned

---

### AUTH-002: Authenticated Tool Access

```javascript
// With valid auth, tools work normally
search_notes({ query: "test", limit: 5 })
```

**Pass Criteria**: Returns results without authentication errors

---

### AUTH-003: List Available Tools (Scope Check)

```javascript
// Tool list reflects available capabilities
// MCP server should list tools the authenticated user can access
const tools = await client.listTools();
```

**Pass Criteria**: Returns full tool list (148+ tools for authenticated sessions)

---

### AUTH-004: Write Operation with Write Scope

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

```javascript
get_note({ id: "<AUTH_NOTE_ID>" })
```

**Pass Criteria**: Returns the note created in AUTH-004

---

### AUTH-006: Update Operation with Write Scope

```javascript
update_note({
  id: "<AUTH_NOTE_ID>",
  content: "# Auth Test Note - Updated\n\nModified during auth UAT phase."
})
```

**Pass Criteria**: Note updated successfully

---

### AUTH-007: Delete Operation with Write Scope

```javascript
delete_note({ id: "<AUTH_NOTE_ID>" })
```

**Pass Criteria**: Note deleted successfully

---

### AUTH-008: Purge Operation with Write Scope (Issue #121)

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

```javascript
search_notes({ query: "authentication", mode: "hybrid", limit: 5 })
```

**Pass Criteria**: Search returns results (read scope sufficient)

---

### AUTH-010: Backup Status (Read Operation)

```javascript
backup_status()
```

**Pass Criteria**: Returns status info (accessible with read scope)

---

### AUTH-011: Memory Info (Read Operation)

```javascript
memory_info()
```

**Pass Criteria**: Returns memory/storage info

---

### AUTH-012: MCP Tool Error on Invalid Parameters

```javascript
// Test that auth errors are distinguishable from parameter errors
get_note({ id: "nonexistent-uuid-value" })
```

**Pass Criteria**: Returns "not found" error (not auth error) - confirms authenticated session works

---

## Part B: OAuth Infrastructure Validation

> **Note**: These tests validate the infrastructure that enables MCP authentication.
> They use curl because OAuth endpoints are infrastructure-level, not agent-facing.
> An agent never calls these directly - the MCP server does.

### AUTH-013: OpenID Discovery Endpoint

```javascript
// Verify via MCP health check that OAuth is configured
memory_info()
```

**Pass Criteria**: System reports as healthy (OAuth configured behind the scenes)

---

### AUTH-014: MCP Server Authentication Flow

> **Infrastructure Test**: Validates the MCP server's internal auth flow

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

```bash
# Get access token for MCP session
curl -s -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials&client_id=$CLIENT_ID&client_secret=$CLIENT_SECRET&scope=mcp read write"
```

**Pass Criteria**: Returns valid JWT access token

---

### AUTH-016: Token Introspection

```bash
curl -s -X POST "$BASE_URL/oauth/introspect" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=$ACCESS_TOKEN&client_id=$CLIENT_ID&client_secret=$CLIENT_SECRET"
```

**Pass Criteria**: Returns `active: true` with correct scope

---

### AUTH-017: Token Revocation

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

| Test ID | Name | Type | Status |
|---------|------|------|--------|
| AUTH-001 | MCP Session Init | MCP | |
| AUTH-002 | Authenticated Tool Access | MCP | |
| AUTH-003 | List Available Tools | MCP | |
| AUTH-004 | Write Operation (Create) | MCP | |
| AUTH-005 | Read Operation (Get) | MCP | |
| AUTH-006 | Update Operation | MCP | |
| AUTH-007 | Delete Operation | MCP | |
| AUTH-008 | Purge with Write Scope | MCP | |
| AUTH-009 | Search with Read Scope | MCP | |
| AUTH-010 | Backup Status | MCP | |
| AUTH-011 | Memory Info | MCP | |
| AUTH-012 | Error Handling (Not Auth) | MCP | |
| AUTH-013 | Health Check (OAuth Active) | MCP | |
| AUTH-014 | Client Registration | Infra | |
| AUTH-015 | Token Issuance | Infra | |
| AUTH-016 | Token Introspection | Infra | |
| AUTH-017 | Token Revocation | Infra | |

**MCP Tests**: 13 (agent-perspective)
**Infrastructure Tests**: 4 (OAuth plumbing)
**Pass Rate Required**: 95% (16/17)

---

## MCP Tools Covered

`search_notes`, `create_note`, `get_note`, `update_note`, `delete_note`, `purge_note`, `backup_status`, `memory_info`

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

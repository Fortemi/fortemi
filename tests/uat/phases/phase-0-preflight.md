# UAT Phase 0: Preflight & System

**Purpose**: Verify system health, capability discovery, and test environment readiness before executing functional tests.

**Duration**: ~2 minutes

**Prerequisites**:
- Fortemi instance running and accessible
- MCP server connected
- Test data directories available

**Tools Tested**: `health_check`, `get_system_info`, `get_documentation`

---

> **MCP-First Requirement**
>
> All tests in this phase use MCP tool calls or the standard MCP `tools/list` request. No direct HTTP/curl commands.
> System verification establishes baseline capabilities for subsequent test phases.

---

## Test Cases

### PF-001: Health Check
**MCP Tool**: `health_check`

```javascript
await mcp.call_tool("health_check", {});
```

**Expected Response**:
```json
{
  "status": "healthy",
  "version": "2026.7.x",
  "components": { ... }
}
```

**Pass Criteria**:
- [ ] Response contains `status` field with value "healthy"
- [ ] Response contains `version` field matching CalVer format
- [ ] Response contains component health details when reported by the server

---

### PF-002: System Information
**MCP Tool**: `get_system_info`

```javascript
await mcp.call_tool("get_system_info", {});
```

**Expected Response**:
```json
{
  "versions": { "release": "2026.7.x", "mcp_server": "1.0.0" },
  "infrastructure": {
    "search": { "full_text": "...", "semantic": "...", "hybrid": "..." },
    "embedding": { "provider": "...", "model": "...", "dimension": 768 }
  },
  "stats": { "total_notes": 0, "pending_jobs": 0 },
  "components": { ... }
}
```

**Pass Criteria**:
- [ ] Response contains `versions`, `infrastructure`, and `stats` objects
- [ ] `versions.release` is a CalVer value
- [ ] Infrastructure includes search and embedding configuration

**Store**: `system_capabilities` (for reference in later phases)

---

### PF-003: Core Tool Count Verification
**MCP Request**: `tools/list`

```javascript
const response = await mcp.request({ method: "tools/list", params: {} });
const toolNames = response.tools.map(tool => tool.name);
const toolCount = toolNames.length;
```

**Expected Response**:
```json
{ "tools": [{ "name": "health_check", "inputSchema": { ... } }, ...] }
```

**Pass Criteria**:
- [ ] System reports exactly 43 core MCP tools available
- [ ] Tool list includes all consolidated tools (capture_knowledge, search, etc.)
- [ ] No full-mode-only granular tools present (`create_note`, `search_notes_fts`, etc.)

---

### PF-004: Documentation Available
**MCP Tool**: `get_documentation`

```javascript
await mcp.call_tool("get_documentation", {});
```

**Expected Response**:
```json
{ "topic": "overview", "content": "..." }
```

**Pass Criteria**:
- [ ] Response identifies the `overview` topic
- [ ] Content describes the consolidated core tools and their actions
- [ ] Content includes usage guidance or examples

---

### PF-005: Tool Schema Availability
**MCP Request**: `tools/list`

```javascript
const response = await mcp.request({ method: "tools/list", params: {} });
const invalid = response.tools.filter(tool =>
  !tool.description || tool.inputSchema?.type !== "object"
);
```

**Expected Response**:
```
invalid.length === 0
```

**Pass Criteria**:
- [ ] Every advertised tool has a non-empty description
- [ ] Every advertised tool has an object input schema
- [ ] `invalid` is empty

---

### PF-006: Provision Test Memory Archives
**MCP Tool**: `manage_archives`, `select_memory`, `get_active_memory`

> **Why Preflight?** Multiple UAT phases (Phase 8: Multi-Memory, Phase 12: Feature Chains) require secondary memory archives. Provisioning them once in preflight avoids duplicated setup and ensures all downstream phases can rely on them.

```javascript
// Provision "test-archive" for Phase 8 (MEM-004 through MEM-008)
const archive1 = await mcp.call_tool("manage_archives", {
  action: "create",
  name: "test-archive",
  description: "UAT test memory for multi-memory validation (Phase 8)"
});
// Success or "already exists" error — both are OK
console.log(`test-archive: ${archive1.name ? "created" : "already exists"}`);

// Provision "uat-test-memory" for Phase 12 (CHAIN-012 through CHAIN-014)
const archive2 = await mcp.call_tool("manage_archives", {
  action: "create",
  name: "uat-test-memory",
  description: "UAT test memory for feature chain validation (Phase 12)"
});
console.log(`uat-test-memory: ${archive2.name ? "created" : "already exists"}`);

// Verify both archives are selectable via MCP
await mcp.call_tool("select_memory", { name: "test-archive" });
const mem1 = await mcp.call_tool("get_active_memory", {});
console.log(`Active memory after select: ${mem1.name}`);

await mcp.call_tool("select_memory", { name: "uat-test-memory" });
const mem2 = await mcp.call_tool("get_active_memory", {});
console.log(`Active memory after select: ${mem2.name}`);

// Switch back to default memory
await mcp.call_tool("select_memory", { name: "public" });
```

**Pass Criteria**:
- [ ] `test-archive` created or already exists (no error)
- [ ] `uat-test-memory` created or already exists (no error)
- [ ] Both archives selectable via `select_memory`
- [ ] Active memory restored to `public` after verification

**Store**: `test_archive_provisioned = true` (phases 8 and 12 can assume archive exists)

---

## Phase Summary

| Test ID | Tool | Focus | Result |
|---------|------|-------|--------|
| PF-001  | health_check | System health | ⬜ |
| PF-002  | get_system_info | Capability discovery | ⬜ |
| PF-003  | tools/list | Tool count verification | ⬜ |
| PF-004  | get_documentation | Documentation availability | ⬜ |
| PF-005  | tools/list | Tool schema readiness | ⬜ |
| PF-006  | manage_archives + MCP | Test archive provisioning | ⬜ |

**Phase Result**: ⬜ PASS / ⬜ FAIL

**Notes**:
- PF-006 provisions secondary memory archives required by Phases 8 and 12. If this step fails, multi-memory tests will be unable to verify isolation and federated search.

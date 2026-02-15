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
> All tests in this phase use MCP tool calls exclusively. No direct HTTP/curl commands.
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
  "version": "2026.2.x",
  "database": "connected",
  "capabilities": { ... }
}
```

**Pass Criteria**:
- [ ] Response contains `status` field with value "healthy"
- [ ] Response contains `version` field matching CalVer format
- [ ] Response contains `database` field indicating connection status

---

### PF-002: System Information
**MCP Tool**: `get_system_info`

```javascript
await mcp.call_tool("get_system_info", {});
```

**Expected Response**:
```json
{
  "version": "2026.2.x",
  "capabilities": {
    "search": { "semantic": true, "fts": true, "hybrid": true },
    "embedding": { ... },
    "extraction_strategies": [...],
    "media": { "vision": true/false, "audio": true/false }
  },
  "limits": {
    "max_note_size": 10485760,
    "max_batch_size": 100
  }
}
```

**Pass Criteria**:
- [ ] Response contains `capabilities` object
- [ ] Response contains `limits` object
- [ ] Response contains `version` field
- [ ] Capabilities include search and embedding configuration

**Store**: `system_capabilities` (for reference in later phases)

---

### PF-003: Core Tool Count Verification
**MCP Tool**: `get_system_info`

```javascript
const info = await mcp.call_tool("get_system_info", {});
const toolCount = info.available_tools?.length || 0;
```

**Expected Response**:
```json
{
  "available_tools": ["health_check", "get_system_info", "get_documentation", ...]
}
```

**Pass Criteria**:
- [ ] System reports exactly 23 core MCP tools available
- [ ] Tool list includes all consolidated tools (capture_knowledge, search, etc.)
- [ ] No legacy granular tools present (create_note, search_notes_fts, etc.)

---

### PF-004: Documentation Available
**MCP Tool**: `get_documentation`

```javascript
await mcp.call_tool("get_documentation", {});
```

**Expected Response**:
```json
{
  "tools": {
    "capture_knowledge": {
      "description": "...",
      "actions": ["create", "bulk_create", "from_template", "upload"],
      "parameters": { ... }
    },
    "search": { ... },
    ...
  },
  "usage_guide": "..."
}
```

**Pass Criteria**:
- [ ] Response contains documentation for all 23 core tools
- [ ] Each consolidated tool lists its available actions
- [ ] Response includes usage guidance or examples
- [ ] Documentation structure is navigable

---

### PF-005: Test Data Availability
**MCP Tool**: None (filesystem verification)

```javascript
// Verify test data directories exist
const testDirs = [
  "/home/roctinam/dev/fortemi/tests/uat/test-data",
  "/home/roctinam/dev/fortemi/tests/uat/fixtures"
];

for (const dir of testDirs) {
  const exists = await fs.access(dir).then(() => true).catch(() => false);
  console.log(`${dir}: ${exists ? "✓" : "✗"}`);
}
```

**Expected Response**:
```
/home/roctinam/dev/fortemi/tests/uat/test-data: ✓
/home/roctinam/dev/fortemi/tests/uat/fixtures: ✓
```

**Pass Criteria**:
- [ ] Test data directory exists and is accessible
- [ ] Fixtures directory exists and contains sample files
- [ ] No permission errors when accessing directories

---

### PF-006: Provision Test Memory Archive
**MCP Tool**: None (HTTP API — archive creation is not an MCP core tool)

> **Why Preflight?** Multiple UAT phases (Phase 8: Multi-Memory, Phase 12: Feature Chains) require a secondary memory archive. Provisioning it once in preflight avoids duplicated setup and ensures all downstream phases can rely on it.

```javascript
// Provision "test-archive" for Phase 8 (MEM-004 through MEM-008)
const archiveResponse1 = await fetch("http://localhost:3000/api/v1/archives", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    name: "test-archive",
    description: "UAT test memory for multi-memory validation (Phase 8)"
  })
});
// 201 = created, 409 = already exists — both are OK
console.log(`test-archive: ${archiveResponse1.status === 201 ? "created" : "already exists"}`);

// Provision "uat-test-memory" for Phase 12 (CHAIN-012 through CHAIN-014)
const archiveResponse2 = await fetch("http://localhost:3000/api/v1/archives", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    name: "uat-test-memory",
    description: "UAT test memory for feature chain validation (Phase 12)"
  })
});
console.log(`uat-test-memory: ${archiveResponse2.status === 201 ? "created" : "already exists"}`);

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
- [ ] `test-archive` provisioned (201) or already exists (409)
- [ ] `uat-test-memory` provisioned (201) or already exists (409)
- [ ] Both archives selectable via `select_memory`
- [ ] Active memory restored to `public` after verification

**Store**: `test_archive_provisioned = true` (phases 8 and 12 can assume archive exists)

---

## Phase Summary

| Test ID | Tool | Focus | Result |
|---------|------|-------|--------|
| PF-001  | health_check | System health | ⬜ |
| PF-002  | get_system_info | Capability discovery | ⬜ |
| PF-003  | get_system_info | Tool count verification | ⬜ |
| PF-004  | get_documentation | Documentation availability | ⬜ |
| PF-005  | (filesystem) | Test data readiness | ⬜ |
| PF-006  | (HTTP API + MCP) | Test archive provisioning | ⬜ |

**Phase Result**: ⬜ PASS / ⬜ FAIL

**Notes**:
- PF-006 provisions secondary memory archives required by Phases 8 and 12. If this step fails, multi-memory tests will be unable to verify isolation and federated search.

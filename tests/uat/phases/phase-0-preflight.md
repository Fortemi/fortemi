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
- [ ] System reports exactly 27 core MCP tools available
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
- [ ] Response contains documentation for all 27 core tools
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
| PF-003  | get_system_info | Tool count verification | ⬜ |
| PF-004  | get_documentation | Documentation availability | ⬜ |
| PF-005  | (filesystem) | Test data readiness | ⬜ |
| PF-006  | manage_archives + MCP | Test archive provisioning | ⬜ |

**Phase Result**: ⬜ PASS / ⬜ FAIL

**Notes**:
- PF-006 provisions secondary memory archives required by Phases 8 and 12. If this step fails, multi-memory tests will be unable to verify isolation and federated search.

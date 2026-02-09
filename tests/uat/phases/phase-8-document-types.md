# UAT Phase 8: Document Types

**Purpose**: Verify document type detection, management, and chunking
**Duration**: ~5 minutes
**Prerequisites**: None (uses system data)
**Tools Tested**: `list_document_types`, `get_document_type`, `detect_document_type`, `create_document_type`, `update_document_type`, `delete_document_type`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Document Type Listing

### DOC-001: List All Document Types

**MCP Tool**: `list_document_types`

```javascript
list_document_types()
```

**Pass Criteria**: Returns 131+ document types across 20 categories

---

### DOC-002: Filter by Category

**MCP Tool**: `list_document_types`

```javascript
list_document_types({ category: "code" })
```

**Pass Criteria**: Returns only code document types (rust, python, typescript, etc.)

---

### DOC-003: Filter by System Flag

**MCP Tool**: `list_document_types`

> **Note**: The `is_system` filter is not currently supported by the API or MCP tool.
> The `list_document_types` tool only supports filtering by `category`.
> System types can be identified by checking the `is_system` field in individual type responses.

```javascript
// Workaround: list all and inspect is_system field on each type
list_document_types({ detail: true })
```

**Pass Criteria**: Returns types with `is_system` field present on each type object

---

## Document Type Details

### DOC-004: Get Document Type

**MCP Tool**: `get_document_type`

```javascript
get_document_type({ name: "rust" })
```

**Expected Response**:
```json
{
  "name": "rust",
  "display_name": "Rust",
  "category": "code",
  "file_extensions": [".rs"],
  "chunking_strategy": "syntactic",
  "is_system": true
}
```

**Pass Criteria**: Returns full type details

---

### DOC-005: Get Agentic Document Type

**MCP Tool**: `get_document_type`

```javascript
get_document_type({ name: "agent-prompt" })
```

**Pass Criteria**: Returns type with `category: "agentic"`

---

## Document Type Detection

### DOC-006: Detect by Extension

**MCP Tool**: `detect_document_type`

```javascript
detect_document_type({
  filename: "main.rs"
})
```

**Pass Criteria**: Returns `{ detected_type: "rust", confidence: 0.9 }`

---

### DOC-007: Detect by Filename Pattern

**MCP Tool**: `detect_document_type`

```javascript
detect_document_type({
  filename: "docker-compose.yml"
})
```

**Pass Criteria**: Returns `{ detected_type: "docker-compose", confidence: 1.0 }`

---

### DOC-008: Detect by Content Magic

**MCP Tool**: `detect_document_type`

```javascript
detect_document_type({
  content: "openapi: 3.1.0\ninfo:\n  title: Test API"
})
```

**Pass Criteria**: Returns `{ detected_type: "openapi", confidence: 0.7+ }`

---

### DOC-009: Detect Combined

**MCP Tool**: `detect_document_type`

```javascript
detect_document_type({
  filename: "api.yaml",
  content: "openapi: 3.1.0\ninfo:"
})
```

**Pass Criteria**: Higher confidence due to combined signals

---

## Custom Document Types

### DOC-010: Create Custom Type

**MCP Tool**: `create_document_type`

```javascript
create_document_type({
  name: "uat-custom-type",
  display_name: "UAT Custom Type",
  category: "custom",
  description: "Custom type for UAT testing",
  file_extensions: [".uat"],
  filename_patterns: ["uat-*.txt"],
  magic_patterns: ["^UAT:"],
  chunking_strategy: "semantic",
  chunk_size_default: 1000
})
```

**Pass Criteria**: Returns created type with `is_system: false`

---

### DOC-011: Update Custom Type

**MCP Tool**: `update_document_type`

```javascript
update_document_type({
  name: "uat-custom-type",
  display_name: "UAT Custom Type (Updated)",
  chunk_size_default: 1500
})
```

**Pass Criteria**: Type updated successfully

---

### DOC-012: Cannot Update System Type

**Isolation**: Required — negative test expects error response

**MCP Tool**: `update_document_type`

```javascript
update_document_type({
  name: "rust",
  display_name: "Modified Rust"
})
```

**Pass Criteria**: Returns error (system types are immutable)

---

### DOC-013: Delete Custom Type

**MCP Tool**: `delete_document_type`

```javascript
delete_document_type({ name: "uat-custom-type" })
```

**Pass Criteria**: Type deleted successfully

---

### DOC-014: Cannot Delete System Type

**Isolation**: Required — negative test expects error response

**MCP Tool**: `delete_document_type`

```javascript
delete_document_type({ name: "rust" })
```

**Pass Criteria**: Returns error (system types cannot be deleted)

---

## Agentic Document Types

### DOC-015: List Agentic Types

**MCP Tool**: `list_document_types`

```javascript
list_document_types({ category: "agentic" })
```

**Pass Criteria**: Returns 8 agentic document types:
- agent-prompt
- agent-skill
- agent-workflow
- mcp-tool
- rag-context
- ai-conversation
- fine-tune-data
- evaluation-set

---

### DOC-016: Verify Agentic Config

**MCP Tool**: `get_document_type`

```javascript
get_document_type({ name: "agent-prompt" })
```

**Pass Criteria**: Type includes `agentic_config` with generation hints

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| DOC-001 | List All Types | `list_document_types` | |
| DOC-002 | Filter by Category | `list_document_types` | |
| DOC-003 | Filter by System Flag | `list_document_types` | |
| DOC-004 | Get Document Type | `get_document_type` | |
| DOC-005 | Get Agentic Type | `get_document_type` | |
| DOC-006 | Detect by Extension | `detect_document_type` | |
| DOC-007 | Detect by Filename | `detect_document_type` | |
| DOC-008 | Detect by Content | `detect_document_type` | |
| DOC-009 | Detect Combined | `detect_document_type` | |
| DOC-010 | Create Custom Type | `create_document_type` | |
| DOC-011 | Update Custom Type | `update_document_type` | |
| DOC-012 | Cannot Update System | `update_document_type` | |
| DOC-013 | Delete Custom Type | `delete_document_type` | |
| DOC-014 | Cannot Delete System | `delete_document_type` | |
| DOC-015 | List Agentic Types | `list_document_types` | |
| DOC-016 | Verify Agentic Config | `get_document_type` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

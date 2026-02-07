# UAT Phase 8: Document Types

**Purpose**: Verify document type detection, management, and chunking
**Duration**: ~5 minutes
**Prerequisites**: None (uses system data)

---

## Document Type Listing

### DOC-001: List All Document Types

```javascript
list_document_types()
```

**Pass Criteria**: Returns 131+ document types across 20 categories

---

### DOC-002: Filter by Category

```javascript
list_document_types({ category: "code" })
```

**Pass Criteria**: Returns only code document types (rust, python, typescript, etc.)

---

### DOC-003: Filter by System Flag

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

```javascript
get_document_type({ name: "agent-prompt" })
```

**Pass Criteria**: Returns type with `category: "agentic"`

---

## Document Type Detection

### DOC-006: Detect by Extension

```javascript
detect_document_type({
  filename: "main.rs"
})
```

**Pass Criteria**: Returns `{ detected_type: "rust", confidence: 0.9 }`

---

### DOC-007: Detect by Filename Pattern

```javascript
detect_document_type({
  filename: "docker-compose.yml"
})
```

**Pass Criteria**: Returns `{ detected_type: "docker-compose", confidence: 1.0 }`

---

### DOC-008: Detect by Content Magic

```javascript
detect_document_type({
  content: "openapi: 3.1.0\ninfo:\n  title: Test API"
})
```

**Pass Criteria**: Returns `{ detected_type: "openapi", confidence: 0.7+ }`

---

### DOC-009: Detect Combined

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

```javascript
update_document_type({
  name: "rust",
  display_name: "Modified Rust"
})
```

**Pass Criteria**: Returns error (system types are immutable)

---

### DOC-013: Delete Custom Type

```javascript
delete_document_type({ name: "uat-custom-type" })
```

**Pass Criteria**: Type deleted successfully

---

### DOC-014: Cannot Delete System Type

```javascript
delete_document_type({ name: "rust" })
```

**Pass Criteria**: Returns error (system types cannot be deleted)

---

## Agentic Document Types

### DOC-015: List Agentic Types

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

```javascript
get_document_type({ name: "agent-prompt" })
```

**Pass Criteria**: Type includes `agentic_config` with generation hints

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| DOC-001 | List All Types | |
| DOC-002 | Filter by Category | |
| DOC-003 | Filter by System Flag | |
| DOC-004 | Get Document Type | |
| DOC-005 | Get Agentic Type | |
| DOC-006 | Detect by Extension | |
| DOC-007 | Detect by Filename | |
| DOC-008 | Detect by Content | |
| DOC-009 | Detect Combined | |
| DOC-010 | Create Custom Type | |
| DOC-011 | Update Custom Type | |
| DOC-012 | Cannot Update System | |
| DOC-013 | Delete Custom Type | |
| DOC-014 | Cannot Delete System | |
| DOC-015 | List Agentic Types | |
| DOC-016 | Verify Agentic Config | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

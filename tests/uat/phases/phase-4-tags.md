# UAT Phase 4: Tag System

**Purpose**: Verify tag management and SKOS concept hierarchy
**Duration**: ~5 minutes
**Prerequisites**: Phase 1 seed data exists
**Tools Tested**: `list_tags`, `list_notes`, `create_note`, `set_note_tags`, `list_concept_schemes`, `create_concept_scheme`, `create_concept`, `tag_note_concept`, `get_governance_stats`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

---

## Basic Tag Operations

### TAG-001: List Tags

**MCP Tool**: `list_tags`

```javascript
list_tags()
```

**Pass Criteria**: Returns array with `name` and `note_count` for each tag

---

### TAG-002: Verify Hierarchical Tags

**MCP Tool**: `list_tags`

```javascript
list_tags()
```

**Pass Criteria**: Contains `uat/hierarchy/level1/level2/level3` from Phase 1

---

### TAG-003: Case Insensitivity

**MCP Tool**: `create_note`, `list_notes`

```javascript
// Create with uppercase
create_note({
  content: "Case test",
  tags: ["UAT/CASE-TEST"],
  revision_mode: "none"
})

// Query with lowercase
list_notes({ tags: ["uat/case-test"] })
```

**Pass Criteria**: Note found with lowercase query

---

### TAG-004: Tag Prefix Matching

**MCP Tool**: `list_notes`

```javascript
// Should find all notes with tags starting with "uat/ml"
list_notes({ tags: ["uat/ml"], limit: 100 })
```

**Pass Criteria**: Returns SEED-ML-001, SEED-ML-002, SEED-ML-003

---

### TAG-005: Set Note Tags

**MCP Tool**: `set_note_tags`

```javascript
set_note_tags({
  id: "<note_id>",
  tags: ["uat/replaced", "uat/new-tags"]
})
```

**Pass Criteria**: Note now has only the new tags (previous removed)

---

## SKOS Concepts (Optional)

### SKOS-001: List Concept Schemes

**MCP Tool**: `list_concept_schemes`

```javascript
list_concept_schemes()
```

**Pass Criteria**: Returns array of schemes (may be empty)

---

### SKOS-002: Create Concept Scheme

**MCP Tool**: `create_concept_scheme`

```javascript
create_concept_scheme({
  title: "UAT Test Scheme",
  description: "Testing SKOS concepts"
})
```

**Pass Criteria**: Returns scheme with ID

---

### SKOS-003: Create Concept

**MCP Tool**: `create_concept`

```javascript
create_concept({
  scheme_id: "<scheme_id>",
  pref_label: "Machine Learning",
  alt_labels: ["ML"],
  definition: "AI systems that learn from data"
})
```

**Pass Criteria**: Returns concept with ID

---

### SKOS-004: Create Hierarchy

**MCP Tool**: `create_concept`

```javascript
// Create parent
const parent = create_concept({
  scheme_id: "<scheme_id>",
  pref_label: "Artificial Intelligence"
})

// Create child with broader relationship
create_concept({
  scheme_id: "<scheme_id>",
  pref_label: "Deep Learning",
  broader_ids: [parent.id]
})
```

**Pass Criteria**: Child concept has parent in `broader` relationship

---

### SKOS-005: Tag Note with Concept

**MCP Tool**: `tag_note_concept`

```javascript
tag_note_concept({
  note_id: "<ml_note_id>",
  concept_id: "<ml_concept_id>"
})
```

**Pass Criteria**: Note is tagged with concept

---

### SKOS-006: Get Governance Stats

**MCP Tool**: `get_governance_stats`

```javascript
get_governance_stats()
```

**Pass Criteria**: Returns stats including concept counts

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| TAG-001 | List Tags | `list_tags` | |
| TAG-002 | Verify Hierarchical Tags | `list_tags` | |
| TAG-003 | Case Insensitivity | `create_note`, `list_notes` | |
| TAG-004 | Tag Prefix Matching | `list_notes` | |
| TAG-005 | Set Note Tags | `set_note_tags` | |
| SKOS-001 | List Concept Schemes | `list_concept_schemes` | |
| SKOS-002 | Create Concept Scheme | `create_concept_scheme` | |
| SKOS-003 | Create Concept | `create_concept` | |
| SKOS-004 | Create Hierarchy | `create_concept` | |
| SKOS-005 | Tag Note with Concept | `tag_note_concept` | |
| SKOS-006 | Get Governance Stats | `get_governance_stats` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

# UAT Phase 13: SKOS Taxonomy

**Duration**: ~12 minutes
**Tools Tested**: `list_concept_schemes`, `create_concept_scheme`, `get_concept_scheme`, `delete_concept_scheme`, `create_concept`, `get_concept`, `get_concept_full`, `update_concept`, `delete_concept`, `search_concepts`, `autocomplete_concepts`, `get_broader`, `add_broader`, `get_narrower`, `add_narrower`, `get_related`, `add_related`, `remove_broader`, `remove_narrower`, `remove_related`, `tag_note_concept`, `get_note_concepts`, `untag_note_concept`, `get_top_concepts`, `get_governance_stats`, `list_skos_collections`, `create_skos_collection`, `get_skos_collection`, `update_skos_collection`, `delete_skos_collection`, `add_skos_collection_member`, `remove_skos_collection_member`, `export_skos_turtle`
**Dependencies**: Phase 0 (preflight), Phase 1 (seed data)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Overview

SKOS (Simple Knowledge Organization System) provides W3C-standard semantic tagging with hierarchical relationships. This phase tests concept schemes, concepts, and semantic relations.

---

## SKOS Concepts

- **Concept Scheme**: A vocabulary/taxonomy container
- **Concept**: A term/tag with semantic meaning
- **Broader**: Parent relationship (is-a, part-of)
- **Narrower**: Child relationship (inverse of broader)
- **Related**: Associative relationship (see-also)

---

## Test Cases

### Concept Schemes

### SKOS-001: List Concept Schemes

**MCP Tool**: `list_concept_schemes`

```javascript
list_concept_schemes()
```

**Expected**: Array of schemes (may be empty)

**Pass Criteria**: Returns valid response

---

### SKOS-002: Create Concept Scheme

**MCP Tool**: `create_concept_scheme`

```javascript
create_concept_scheme({
  notation: "UAT-TECH",
  title: "UAT Technology Taxonomy",
  description: "Technology concepts for UAT testing",
  uri: "https://example.com/skos/uat-tech"
})
```

**Expected**: `{ id: "<uuid>" }`

**Store**: `tech_scheme_id`

---

### SKOS-003: Create Second Scheme

**MCP Tool**: `create_concept_scheme`

```javascript
create_concept_scheme({
  notation: "UAT-DOMAIN",
  title: "UAT Domain Taxonomy",
  description: "Domain concepts for UAT testing"
})
```

**Store**: `domain_scheme_id`

---

### SKOS-004: Get Concept Scheme

**MCP Tool**: `get_concept_scheme`

```javascript
get_concept_scheme({ id: tech_scheme_id })
```

**Expected**: Full scheme with title, notation, description

**Pass Criteria**: All fields present

---

### Concepts

### SKOS-005: Create Root Concept

**MCP Tool**: `create_concept`

```javascript
create_concept({
  scheme_id: tech_scheme_id,
  pref_label: "Programming",
  notation: "PROG",
  definition: "The practice of writing computer programs",
  scope_note: "Includes all programming paradigms"
})
```

**Expected**: `{ id: "<uuid>" }`

**Store**: `programming_concept_id`

---

### SKOS-006: Create Child Concept

**MCP Tool**: `create_concept`

```javascript
create_concept({
  scheme_id: tech_scheme_id,
  pref_label: "Rust",
  notation: "RUST",
  definition: "Systems programming language focused on safety",
  broader_ids: [programming_concept_id]
})
```

**Store**: `rust_concept_id`

---

### SKOS-007: Create Sibling Concept

**MCP Tool**: `create_concept`

```javascript
create_concept({
  scheme_id: tech_scheme_id,
  pref_label: "Python",
  notation: "PY",
  definition: "High-level programming language",
  broader_ids: [programming_concept_id]
})
```

**Store**: `python_concept_id`

---

### SKOS-008: Create Concept with Alt Labels

**MCP Tool**: `create_concept`

```javascript
create_concept({
  scheme_id: tech_scheme_id,
  pref_label: "Machine Learning",
  notation: "ML",
  alt_labels: ["ML", "Statistical Learning", "Predictive Modeling"],
  definition: "AI subset using data to improve performance"
})
```

**Store**: `ml_concept_id`

---

### SKOS-009: Get Concept

**MCP Tool**: `get_concept`

```javascript
get_concept({ id: rust_concept_id })
```

**Expected**: Concept with pref_label "Rust"

---

### SKOS-010: Get Concept Full

**MCP Tool**: `get_concept_full`

```javascript
get_concept_full({ id: programming_concept_id })
```

**Expected**:
- Concept details
- `narrower` array with Rust, Python
- `broader` array (empty for root)
- Labels and notes

**Pass Criteria**: Hierarchy populated

---

### SKOS-011: Search Concepts

**MCP Tool**: `search_concepts`

```javascript
search_concepts({
  q: "programming",
  scheme_id: tech_scheme_id
})
```

**Expected**: Results include Programming concept

---

### SKOS-012: Autocomplete Concepts

**MCP Tool**: `autocomplete_concepts`

```javascript
autocomplete_concepts({
  q: "Ru",
  limit: 5
})
```

**Expected**: Rust appears in suggestions

---

### Relations

### SKOS-013: Get Broader

**MCP Tool**: `get_broader`

```javascript
get_broader({ id: rust_concept_id })
```

**Expected**: Returns [Programming]

---

### SKOS-014: Get Narrower

**MCP Tool**: `get_narrower`

```javascript
get_narrower({ id: programming_concept_id })
```

**Expected**: Returns [Rust, Python] (may include others)

---

### SKOS-015: Add Related

**MCP Tool**: `add_related`

```javascript
add_related({
  id: ml_concept_id,
  target_id: python_concept_id
})
```

**Expected**: Associative relationship created

---

### SKOS-016: Get Related

**MCP Tool**: `get_related`

```javascript
get_related({ id: ml_concept_id })
```

**Expected**: Returns [Python]

---

### SKOS-017: Verify Symmetric Related

**MCP Tool**: `get_related`

```javascript
get_related({ id: python_concept_id })
```

**Expected**: Returns [Machine Learning]

**Pass Criteria**: Related is bidirectional

---

### SKOS-018: Add Broader

**MCP Tool**: `add_broader`

```javascript
// Create Deep Learning under ML
create_concept({
  scheme_id: tech_scheme_id,
  pref_label: "Deep Learning",
  notation: "DL"
})

add_broader({
  id: deep_learning_id,
  target_id: ml_concept_id
})
```

**Expected**: Deep Learning → broader → ML

**Store**: `deep_learning_id`

---

### SKOS-019: Add Narrower

**MCP Tool**: `add_narrower`

```javascript
// Add Neural Networks under Deep Learning
create_concept({
  scheme_id: tech_scheme_id,
  pref_label: "Neural Networks",
  notation: "NN"
})

add_narrower({
  id: deep_learning_id,
  target_id: neural_networks_id
})
```

**Expected**: Deep Learning → narrower → Neural Networks

**Store**: `neural_networks_id`

---

### Note Tagging

### SKOS-020: Tag Note with Concept

**MCP Tool**: `tag_note_concept`

```javascript
// Use a note from seed data or create one
const note = create_note({
  content: "# Rust Memory Safety\n\nOwnership system explanation.",
  tags: ["uat/skos-test"],
  revision_mode: "none"
})

tag_note_concept({
  note_id: note.id,
  concept_id: rust_concept_id,
  is_primary: true
})
```

**Expected**: Note tagged with Rust concept

**Store**: `tagged_note_id`

---

### SKOS-021: Get Note Concepts

**MCP Tool**: `get_note_concepts`

```javascript
get_note_concepts({ note_id: tagged_note_id })
```

**Expected**: Returns [Rust] with is_primary: true

---

### SKOS-022: Untag Note Concept

**MCP Tool**: `untag_note_concept`

```javascript
untag_note_concept({
  note_id: tagged_note_id,
  concept_id: rust_concept_id
})
```

**Expected**: Concept removed from note

**Verify**: `get_note_concepts` returns empty

---

### Governance

### SKOS-023: Get Top Concepts

**MCP Tool**: `get_top_concepts`

```javascript
get_top_concepts({ scheme_id: tech_scheme_id })
```

**Expected**: Returns root concepts (Programming, ML)

---

### SKOS-024: Get Governance Stats

**MCP Tool**: `get_governance_stats`

```javascript
get_governance_stats({ scheme_id: tech_scheme_id })
```

**Expected**:
```json
{
  "total_concepts": 6,
  "candidates": 0,
  "approved": 6,
  "deprecated": 0,
  "orphans": 0,
  "under_used": <n>,
  "avg_note_count": <n>,
  "max_depth": 3
}
```

---

### SKOS-025: Update Concept Status

**MCP Tool**: `update_concept`

```javascript
update_concept({
  id: neural_networks_id,
  status: "deprecated",
  deprecation_reason: "Replaced by specific architecture types"
})
```

**Expected**: Status changed to deprecated

---

### Deletion

### SKOS-026: Delete Concept

**MCP Tool**: `delete_concept`

```javascript
delete_concept({ id: neural_networks_id })
```

**Expected**: Concept removed

**Verify**: `get_concept` returns 404

---

### SKOS-027: Delete Scheme

**MCP Tool**: `delete_concept_scheme`

```javascript
// Delete the test scheme (may need force flag)
delete_concept_scheme({
  id: domain_scheme_id,
  force: true
})
```

**Expected**: Scheme and concepts deleted

---

## SKOS Collections (#450)

### SKOS-028: List SKOS Collections

**MCP Tool**: `list_skos_collections`

```javascript
list_skos_collections({ scheme_id: tech_scheme_id })
```

**Expected**: Empty array initially

---

### SKOS-029: Create SKOS Collection

**MCP Tool**: `create_skos_collection`

```javascript
create_skos_collection({
  scheme_id: tech_scheme_id,
  pref_label: "Learning Path",
  notation: "LPATH",
  definition: "Ordered progression of concepts",
  ordered: true
})
```

**Expected**: `{ id: "<uuid>" }`

**Store**: `collection_id`

---

### SKOS-030: Get SKOS Collection

**MCP Tool**: `get_skos_collection`

```javascript
get_skos_collection({ id: collection_id })
```

**Expected**: Collection with empty members array

---

### SKOS-031: Add Collection Member

**MCP Tool**: `add_skos_collection_member`

```javascript
add_skos_collection_member({
  id: collection_id,
  concept_id: programming_concept_id,
  position: 0
})

add_skos_collection_member({
  id: collection_id,
  concept_id: rust_concept_id,
  position: 1
})
```

**Expected**: Members added in order

---

### SKOS-032: Verify Collection Members

**MCP Tool**: `get_skos_collection`

```javascript
get_skos_collection({ id: collection_id })
```

**Expected**: Members in order: [Programming, Rust]

---

### SKOS-033: Update SKOS Collection

**MCP Tool**: `update_skos_collection`

```javascript
update_skos_collection({
  id: collection_id,
  pref_label: "Updated Learning Path",
  definition: "Updated description"
})
```

**Expected**: Collection updated successfully

---

### SKOS-034: Remove Collection Member

**MCP Tool**: `remove_skos_collection_member`

```javascript
remove_skos_collection_member({
  id: collection_id,
  concept_id: rust_concept_id
})
```

**Expected**: Member removed

**Verify**: Collection has 1 member

---

### SKOS-035: Delete SKOS Collection

**MCP Tool**: `delete_skos_collection`

```javascript
delete_skos_collection({ id: collection_id })
```

**Expected**: Collection deleted

**Verify**: `list_skos_collections` no longer includes it

---

## Relation Removal (#451)

### SKOS-036: Remove Broader

**MCP Tool**: `remove_broader`

```javascript
// First verify rust has broader=programming
get_broader({ id: rust_concept_id })

// Remove the relationship
remove_broader({
  id: rust_concept_id,
  target_id: programming_concept_id
})
```

**Expected**: Broader relationship removed

**Verify**: `get_broader` returns empty array

---

### SKOS-037: Remove Narrower

**MCP Tool**: `remove_narrower`

```javascript
// Re-add the relationship first
add_broader({ id: rust_concept_id, target_id: programming_concept_id })

// Remove via narrower
remove_narrower({
  id: programming_concept_id,
  target_id: rust_concept_id
})
```

**Expected**: Narrower relationship removed (same as broader inverse)

---

### SKOS-038: Remove Related

**MCP Tool**: `remove_related`

```javascript
// Verify ML and Python are related
get_related({ id: ml_concept_id })

// Remove the relationship
remove_related({
  id: ml_concept_id,
  target_id: python_concept_id
})
```

**Expected**: Related relationship removed from both directions

**Verify**: Both `get_related` calls return empty

---

## SKOS Export (#460)

### SKOS-039: Export SKOS Turtle

**MCP Tool**: `export_skos_turtle`

```javascript
export_skos_turtle({ scheme_id: tech_scheme_id })
```

**Expected**: Valid W3C Turtle format containing:
- `@prefix skos:` declaration
- Concept scheme as `skos:ConceptScheme`
- Concepts with `skos:prefLabel`
- Relationships: `skos:broader`, `skos:narrower`, `skos:related`

---

### SKOS-040: Export All Schemes

**MCP Tool**: `export_skos_turtle`

```javascript
export_skos_turtle()  // No scheme_id = all schemes
```

**Expected**: Turtle with all concept schemes

---

### SKOS-041: Update Concept Scheme

**MCP Tool**: `update_concept_scheme`, `get_concept_scheme`

```javascript
update_concept_scheme({
  id: "<tech_scheme_id>",
  title: "UAT Renamed Tech Taxonomy",
  description: "Updated description for UAT testing"
})

get_concept_scheme({ id: "<tech_scheme_id>" })
```

**Pass Criteria**:
- Title updated to "UAT Renamed Tech Taxonomy"
- Description updated to new value
- Scheme ID remains the same
- `get_concept_scheme` confirms changes persisted

---

## Cleanup

```javascript
// Delete test concepts (bottom-up)
delete_concept({ id: deep_learning_id })
delete_concept({ id: ml_concept_id })
delete_concept({ id: rust_concept_id })
delete_concept({ id: python_concept_id })
delete_concept({ id: programming_concept_id })

// Delete test scheme
delete_concept_scheme({ id: tech_scheme_id, force: true })

// Delete test note
delete_note({ id: tagged_note_id })
```

---

## Success Criteria

| Test | MCP Tool(s) | Status | Notes |
|------|-------------|--------|-------|
| SKOS-001 | `list_concept_schemes` | | List schemes |
| SKOS-002 | `create_concept_scheme` | | Create scheme |
| SKOS-003 | `create_concept_scheme` | | Create second scheme |
| SKOS-004 | `get_concept_scheme` | | Get scheme |
| SKOS-005 | `create_concept` | | Create root concept |
| SKOS-006 | `create_concept` | | Create child concept |
| SKOS-007 | `create_concept` | | Create sibling concept |
| SKOS-008 | `create_concept` | | Concept with alt labels |
| SKOS-009 | `get_concept` | | Get concept |
| SKOS-010 | `get_concept_full` | | Get concept full |
| SKOS-011 | `search_concepts` | | Search concepts |
| SKOS-012 | `autocomplete_concepts` | | Autocomplete |
| SKOS-013 | `get_broader` | | Get broader |
| SKOS-014 | `get_narrower` | | Get narrower |
| SKOS-015 | `add_related` | | Add related |
| SKOS-016 | `get_related` | | Get related |
| SKOS-017 | `get_related` | | Verify symmetric |
| SKOS-018 | `add_broader` | | Add broader |
| SKOS-019 | `add_narrower` | | Add narrower |
| SKOS-020 | `tag_note_concept` | | Tag note |
| SKOS-021 | `get_note_concepts` | | Get note concepts |
| SKOS-022 | `untag_note_concept` | | Untag note |
| SKOS-023 | `get_top_concepts` | | Get top concepts |
| SKOS-024 | `get_governance_stats` | | Governance stats |
| SKOS-025 | `update_concept` | | Update concept status |
| SKOS-026 | `delete_concept` | | Delete concept |
| SKOS-027 | `delete_concept_scheme` | | Delete scheme |
| SKOS-028 | `list_skos_collections` | | List collections |
| SKOS-029 | `create_skos_collection` | | Create collection |
| SKOS-030 | `get_skos_collection` | | Get collection |
| SKOS-031 | `add_skos_collection_member` | | Add collection members |
| SKOS-032 | `get_skos_collection` | | Verify member order |
| SKOS-033 | `update_skos_collection` | | Update collection |
| SKOS-034 | `remove_skos_collection_member` | | Remove collection member |
| SKOS-035 | `delete_skos_collection` | | Delete collection |
| SKOS-036 | `remove_broader` | | Remove broader |
| SKOS-037 | `remove_narrower` | | Remove narrower |
| SKOS-038 | `remove_related` | | Remove related |
| SKOS-039 | `export_skos_turtle` | | Export turtle (scheme) |
| SKOS-040 | `export_skos_turtle` | | Export turtle (all) |
| SKOS-041 | `update_concept_scheme`, `get_concept_scheme` | | Update concept scheme |

**Pass Rate Required**: 95% (39/41)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `list_concept_schemes` | SKOS-001 |
| `create_concept_scheme` | SKOS-002, SKOS-003 |
| `get_concept_scheme` | SKOS-004 |
| `update_concept_scheme` | SKOS-041 |
| `delete_concept_scheme` | SKOS-027 |
| `create_concept` | SKOS-005, SKOS-006, SKOS-007, SKOS-008, SKOS-018, SKOS-019 |
| `get_concept` | SKOS-009 |
| `get_concept_full` | SKOS-010 |
| `update_concept` | SKOS-025 |
| `delete_concept` | SKOS-026 |
| `search_concepts` | SKOS-011 |
| `autocomplete_concepts` | SKOS-012 |
| `get_broader` | SKOS-013 |
| `add_broader` | SKOS-018 |
| `get_narrower` | SKOS-014 |
| `add_narrower` | SKOS-019 |
| `get_related` | SKOS-016, SKOS-017 |
| `add_related` | SKOS-015 |
| `tag_note_concept` | SKOS-020 |
| `get_note_concepts` | SKOS-021 |
| `untag_note_concept` | SKOS-022 |
| `get_top_concepts` | SKOS-023 |
| `get_governance_stats` | SKOS-024 |
| `list_skos_collections` | SKOS-028 |
| `create_skos_collection` | SKOS-029 |
| `get_skos_collection` | SKOS-030, SKOS-032 |
| `update_skos_collection` | SKOS-033 |
| `delete_skos_collection` | SKOS-035 |
| `add_skos_collection_member` | SKOS-031 |
| `remove_skos_collection_member` | SKOS-034 |
| `remove_broader` | SKOS-036 |
| `remove_narrower` | SKOS-037 |
| `remove_related` | SKOS-038 |
| `export_skos_turtle` | SKOS-039, SKOS-040 |

**Coverage**: 34/34 SKOS tools (100%)

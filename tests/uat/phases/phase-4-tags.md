# UAT Phase 4: Tag System

**Purpose**: Verify tag management and SKOS concept hierarchy
**Duration**: ~5 minutes
**Prerequisites**: Phase 1 seed data exists

---

## Basic Tag Operations

### TAG-001: List Tags

```javascript
list_tags()
```

**Pass Criteria**: Returns array with `name` and `note_count` for each tag

---

### TAG-002: Verify Hierarchical Tags

```javascript
list_tags()
```

**Pass Criteria**: Contains `uat/hierarchy/level1/level2/level3` from Phase 1

---

### TAG-003: Case Insensitivity

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

```javascript
// Should find all notes with tags starting with "uat/ml"
list_notes({ tags: ["uat/ml"], limit: 100 })
```

**Pass Criteria**: Returns SEED-ML-001, SEED-ML-002, SEED-ML-003

---

### TAG-005: Set Note Tags

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

```javascript
list_concept_schemes()
```

**Pass Criteria**: Returns array of schemes (may be empty)

---

### SKOS-002: Create Concept Scheme

```javascript
create_concept_scheme({
  title: "UAT Test Scheme",
  description: "Testing SKOS concepts"
})
```

**Pass Criteria**: Returns scheme with ID

---

### SKOS-003: Create Concept

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

```javascript
tag_note_concept({
  note_id: "<ml_note_id>",
  concept_id: "<ml_concept_id>"
})
```

**Pass Criteria**: Note is tagged with concept

---

### SKOS-006: Get Governance Stats

```javascript
get_governance_stats()
```

**Pass Criteria**: Returns stats including concept counts

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| TAG-001 | List Tags | |
| TAG-002 | Verify Hierarchical Tags | |
| TAG-003 | Case Insensitivity | |
| TAG-004 | Tag Prefix Matching | |
| TAG-005 | Set Note Tags | |
| SKOS-001 | List Concept Schemes | |
| SKOS-002 | Create Concept Scheme | |
| SKOS-003 | Create Concept | |
| SKOS-004 | Create Hierarchy | |
| SKOS-005 | Tag Note with Concept | |
| SKOS-006 | Get Governance Stats | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

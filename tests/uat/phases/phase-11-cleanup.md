# UAT Phase 11: Cleanup

**Purpose**: Remove all test data created during UAT
**Duration**: ~2 minutes
**Prerequisites**: All other phases completed

---

## Cleanup Procedure

### CLEAN-001: Get All UAT Notes

```javascript
const uatNotes = list_notes({ tags: ["uat"], limit: 1000 })
console.log(`Found ${uatNotes.total} UAT notes to clean up`)
```

**Pass Criteria**: Returns count of UAT notes

---

### CLEAN-002: Delete UAT Notes

```javascript
for (const note of uatNotes.notes) {
  delete_note({ id: note.id })
}
```

**Pass Criteria**: All notes soft-deleted

---

### CLEAN-003: Purge UAT Notes

```javascript
// Get soft-deleted notes
const deletedNotes = list_notes({ filter: "deleted", tags: ["uat"], limit: 1000 })

// Permanently delete
for (const note of deletedNotes.notes) {
  purge_note({ id: note.id })
}
```

**Pass Criteria**: All UAT notes permanently removed

---

### CLEAN-004: Delete UAT Collections

```javascript
// Get UAT collections
const collections = list_collections()
const uatCollections = collections.filter(c => c.name.startsWith("UAT-"))

// Delete each
for (const coll of uatCollections) {
  delete_collection({ id: coll.id })
}
```

**Pass Criteria**: All UAT collections removed

---

### CLEAN-005: Delete UAT Embedding Sets

```javascript
// Delete test embedding set if created
delete_embedding_set({ slug: "uat-test-set" })
```

**Pass Criteria**: Test embedding set removed (or skip if not created)

---

### CLEAN-006: Delete UAT Concepts

```javascript
// Get UAT concept scheme if created
const schemes = list_concept_schemes()
const uatScheme = schemes.find(s => s.title.includes("UAT"))

if (uatScheme) {
  // Delete concepts first
  const concepts = search_concepts({ scheme_id: uatScheme.id })
  for (const concept of concepts) {
    delete_concept({ id: concept.id })
  }

  // Then delete scheme
  delete_concept_scheme({ id: uatScheme.id })
}
```

**Pass Criteria**: Test concepts and scheme removed

---

### CLEAN-007: Verify Cleanup

```javascript
const remaining = list_notes({ tags: ["uat"], limit: 100 })
```

**Pass Criteria**: `remaining.total === 0`

---

### CLEAN-008: Final State Check

```javascript
memory_info()
```

**Pass Criteria**: System healthy, no orphaned data

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| CLEAN-001 | Get UAT Notes | |
| CLEAN-002 | Delete UAT Notes | |
| CLEAN-003 | Purge UAT Notes | |
| CLEAN-004 | Delete Collections | |
| CLEAN-005 | Delete Embedding Sets | |
| CLEAN-006 | Delete Concepts | |
| CLEAN-007 | Verify Cleanup | |
| CLEAN-008 | Final State Check | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

---

## Final UAT Summary

After completing all phases, compile final report:

```markdown
# Matric-Memory UAT Final Report

## Execution Summary
- **Date**: YYYY-MM-DD
- **Duration**: XX minutes
- **Executor**: [Agent/Human]

## Results by Phase

| Phase | Description | Passed | Failed | Pass Rate |
|-------|-------------|--------|--------|-----------|
| 0 | Pre-flight | X/3 | X | XX% |
| 1 | Seed Data | X/11 | X | XX% |
| 2 | CRUD | X/17 | X | XX% |
| 3 | Search | X/18 | X | XX% |
| 4 | Tags | X/11 | X | XX% |
| 5 | Collections | X/10 | X | XX% |
| 6 | Links | X/8 | X | XX% |
| 7 | Embeddings | X/11 | X | XX% |
| 8 | Document Types | X/16 | X | XX% |
| 9 | Edge Cases | X/15 | X | XX% |
| 10 | Backup | X/14 | X | XX% |
| 11 | Cleanup | X/8 | X | XX% |
| **TOTAL** | | **X/142** | | **XX%** |

## Critical Phase Status
- Phase 0-3 (Critical): [ ] ALL PASS / [ ] FAIL
- Phase 4-11 (Standard): [ ] ≥90% PASS / [ ] FAIL

## Overall Result: [ ] APPROVED / [ ] NOT APPROVED

## Failed Tests (if any)
[List each failed test with details]

## Observations
[Any notable findings]

## Recommendations
[Suggested improvements or issues to address]
```

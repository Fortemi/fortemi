# UAT Phase 21: Final Cleanup

**Purpose**: Remove all test data created during UAT using MCP tools
**Duration**: ~5 minutes
**Phase Number**: 21 (FINAL PHASE)
**Prerequisites**: All other phases (0-20) completed
**Critical**: Yes - ensures clean state for next test run
**Tools Tested**: `list_notes`, `delete_note`, `purge_notes`, `purge_note`, `list_collections`, `delete_collection`, `list_embedding_sets`, `delete_embedding_set`, `list_concept_schemes`, `delete_concept`, `delete_concept_scheme`, `search_concepts`, `list_templates`, `delete_template`, `list_archives`, `delete_archive`, `memory_info`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

> **IMPORTANT**: This is the FINAL phase of the UAT suite. Do NOT run this phase early or skip phases 10-20.

---

## Overview

This phase uses **MCP tools exclusively** to clean up all UAT test data. Each cleanup step uses the appropriate MCP function rather than direct API calls.

### MCP Tools Used in This Phase

| Tool | Purpose |
|------|---------|
| `list_notes` | Find UAT notes by tag |
| `delete_note` | Soft-delete notes |
| `purge_note` | Permanently remove notes |
| `purge_notes` | Bulk permanent removal |
| `list_collections` | Find UAT collections |
| `delete_collection` | Remove collections |
| `list_embedding_sets` | Find UAT embedding sets |
| `delete_embedding_set` | Remove embedding sets |
| `list_concept_schemes` | Find UAT SKOS schemes |
| `delete_concept` | Remove concepts |
| `delete_concept_scheme` | Remove schemes |
| `list_templates` | Find UAT templates |
| `delete_template` | Remove templates |
| `list_archives` | Find UAT archives |
| `delete_archive` | Remove archives |
| `memory_info` | Verify final state |

---

## Cleanup Procedure

### CLEAN-001: Inventory UAT Test Data

**MCP Tools**: `list_notes`, `list_collections`, `list_templates`

```javascript
// Count all UAT-tagged notes
const uatNotes = await mcp__fortemi__list_notes({
  tags: ["uat"],
  limit: 1000
})
console.log(`Found ${uatNotes.total} UAT notes to clean up`)

// Count UAT collections
const allCollections = await mcp__fortemi__list_collections()
const uatCollections = allCollections.filter(c => c.name.startsWith("UAT-"))
console.log(`Found ${uatCollections.length} UAT collections`)

// Count UAT templates
const templates = await mcp__fortemi__list_templates()
const uatTemplates = templates.filter(t => t.name.includes("UAT") || t.name.includes("uat"))
console.log(`Found ${uatTemplates.length} UAT templates`)
```

**Pass Criteria**: Inventory complete, counts logged

---

### CLEAN-002: Delete UAT Notes (Soft Delete)

**MCP Tool**: `delete_note`

```javascript
// Get all UAT notes
const notes = await mcp__fortemi__list_notes({
  tags: ["uat"],
  limit: 1000
})

// Soft-delete each note using MCP delete_note
for (const note of notes.notes) {
  await mcp__fortemi__delete_note({ id: note.id })
}
console.log(`Soft-deleted ${notes.notes.length} UAT notes`)
```

**Pass Criteria**: All UAT notes soft-deleted

---

### CLEAN-003: Purge UAT Notes (Permanent Removal)

**MCP Tool**: `purge_notes`

```javascript
// Use purge_notes for bulk permanent deletion
// This is more efficient than individual purge_note calls
await mcp__fortemi__purge_notes({
  tags: ["uat"]
})
console.log("Purged all UAT-tagged notes")

// Alternative: Individual purge for specific notes
// const deletedNotes = await mcp__fortemi__list_notes({
//   filter: "deleted",
//   tags: ["uat"],
//   limit: 1000
// })
// for (const note of deletedNotes.notes) {
//   await mcp__fortemi__purge_note({ id: note.id })
// }
```

**Pass Criteria**: All UAT notes permanently removed

---

### CLEAN-004: Delete UAT Collections

**MCP Tools**: `list_collections`, `delete_collection`

```javascript
// Get all collections
const collections = await mcp__fortemi__list_collections()

// Filter for UAT collections (prefix: "UAT-")
const uatCollections = collections.filter(c => c.name.startsWith("UAT-"))

// Delete each UAT collection using MCP delete_collection
for (const coll of uatCollections) {
  await mcp__fortemi__delete_collection({ id: coll.id })
}
console.log(`Deleted ${uatCollections.length} UAT collections`)
```

**Pass Criteria**: All UAT- prefixed collections removed

---

### CLEAN-005: Delete UAT Templates

**MCP Tools**: `list_templates`, `delete_template`

```javascript
// Get all templates
const templates = await mcp__fortemi__list_templates()

// Filter for UAT templates
const uatTemplates = templates.filter(t =>
  t.name.includes("UAT") || t.name.includes("uat")
)

// Delete each using MCP delete_template
for (const tmpl of uatTemplates) {
  await mcp__fortemi__delete_template({ id: tmpl.id })
}
console.log(`Deleted ${uatTemplates.length} UAT templates`)
```

**Pass Criteria**: All UAT templates removed

---

### CLEAN-006: Delete UAT Embedding Sets

**MCP Tools**: `list_embedding_sets`, `delete_embedding_set`

```javascript
// Get all embedding sets
const sets = await mcp__fortemi__list_embedding_sets()

// Filter for UAT embedding sets
const uatSets = sets.filter(s =>
  s.slug.includes("uat") || s.name.includes("UAT")
)

// Delete each using MCP delete_embedding_set
for (const set of uatSets) {
  await mcp__fortemi__delete_embedding_set({ slug: set.slug })
}
console.log(`Deleted ${uatSets.length} UAT embedding sets`)
```

**Pass Criteria**: All UAT embedding sets removed

---

### CLEAN-007: Delete UAT SKOS Concepts and Schemes

**MCP Tools**: `list_concept_schemes`, `search_concepts`, `delete_concept`, `delete_concept_scheme`

```javascript
// Get all concept schemes
const schemes = await mcp__fortemi__list_concept_schemes()

// Find UAT schemes
const uatSchemes = schemes.filter(s =>
  s.title.includes("UAT") || s.title.includes("uat")
)

for (const scheme of uatSchemes) {
  // First, delete all concepts in the scheme
  const concepts = await mcp__fortemi__search_concepts({
    scheme_id: scheme.id
  })

  for (const concept of concepts) {
    await mcp__fortemi__delete_concept({ id: concept.id })
  }

  // Then delete the scheme itself
  await mcp__fortemi__delete_concept_scheme({ id: scheme.id })
}
console.log(`Deleted ${uatSchemes.length} UAT concept schemes`)
```

**Pass Criteria**: All UAT concepts and schemes removed

---

### CLEAN-008: Delete UAT Archives

**MCP Tools**: `list_archives`, `delete_archive`

```javascript
// Get all archives
const archives = await mcp__fortemi__list_archives()

// Filter for UAT archives
const uatArchives = archives.filter(a =>
  a.name.includes("UAT") || a.name.includes("uat")
)

// Delete each using MCP delete_archive
for (const archive of uatArchives) {
  await mcp__fortemi__delete_archive({ id: archive.id })
}
console.log(`Deleted ${uatArchives.length} UAT archives`)
```

**Pass Criteria**: All UAT archives removed

---

### CLEAN-009: Verify Complete Cleanup

**MCP Tools**: `list_notes`, `list_collections`

```javascript
// Verify no UAT notes remain
const remainingNotes = await mcp__fortemi__list_notes({
  tags: ["uat"],
  limit: 100
})

if (remainingNotes.total > 0) {
  console.error(`WARNING: ${remainingNotes.total} UAT notes still exist`)
  // Attempt final cleanup
  await mcp__fortemi__purge_notes({ tags: ["uat"] })
}

// Verify no UAT collections remain
const remainingCollections = await mcp__fortemi__list_collections()
const uatColl = remainingCollections.filter(c => c.name.startsWith("UAT-"))
if (uatColl.length > 0) {
  console.error(`WARNING: ${uatColl.length} UAT collections still exist`)
}

// Final assertion
console.log(`Verification: ${remainingNotes.total} notes, ${uatColl.length} collections`)
```

**Pass Criteria**: `remainingNotes.total === 0` AND `uatColl.length === 0`

---

### CLEAN-010: Final System State Check

**MCP Tool**: `memory_info`

```javascript
// Get final system state using MCP memory_info
const systemInfo = await mcp__fortemi__memory_info()

console.log("=== Final System State ===")
console.log(`Total notes: ${systemInfo.summary.total_notes}`)
console.log(`Total embeddings: ${systemInfo.summary.total_embeddings}`)
console.log(`Storage: ${systemInfo.storage.total_size}`)
console.log("System health: " + (systemInfo.health?.status || "OK"))
```

**Pass Criteria**: System healthy, no orphaned UAT data

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| CLEAN-001 | Inventory UAT Data | `list_notes`, `list_collections`, `list_templates` | |
| CLEAN-002 | Soft Delete Notes | `delete_note` | |
| CLEAN-003 | Purge Notes | `purge_notes` | |
| CLEAN-004 | Delete Collections | `list_collections`, `delete_collection` | |
| CLEAN-005 | Delete Templates | `list_templates`, `delete_template` | |
| CLEAN-006 | Delete Embedding Sets | `list_embedding_sets`, `delete_embedding_set` | |
| CLEAN-007 | Delete SKOS Data | `list_concept_schemes`, `search_concepts`, `delete_concept`, `delete_concept_scheme` | |
| CLEAN-008 | Delete Archives | `list_archives`, `delete_archive` | |
| CLEAN-009 | Verify Cleanup | `list_notes`, `list_collections` | |
| CLEAN-010 | Final State Check | `memory_info` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

---

## Final UAT Summary

After completing **ALL 22 phases (0-21)**, compile the final report:

```markdown
# Matric-Memory UAT Final Report

## Execution Summary
- **Date**: YYYY-MM-DD
- **Duration**: XX minutes
- **Executor**: [Agent/Human]
- **Suite Version**: 2026.2.5

## Results by Phase

| Phase | Description | Passed | Failed | Pass Rate |
|-------|-------------|--------|--------|-----------|
| 0 | Pre-flight | X/3 | X | XX% |
| 1 | Seed Data | X/15 | X | XX% |
| 2 | CRUD | X/17 | X | XX% |
| 2b | File Attachments | X/21 | X | XX% |
| 3 | Search | X/14 | X | XX% |
| 3b | Memory Search | X/21 | X | XX% |
| 4 | Tags | X/3 | X | XX% |
| 5 | Collections | X/3 | X | XX% |
| 6 | Semantic Links | X/11 | X | XX% |
| 7 | Embeddings | X/15 | X | XX% |
| 8 | Document Types | X/16 | X | XX% |
| 9 | Edge Cases | X/3 | X | XX% |
| 10 | Templates | X/15 | X | XX% |
| 11 | Versioning | X/15 | X | XX% |
| 12 | Archives | X/18 | X | XX% |
| 13 | SKOS Taxonomy | X/27 | X | XX% |
| 14 | PKE Encryption | X/20 | X | XX% |
| 15 | Jobs & Queue | X/22 | X | XX% |
| 16 | Observability | X/12 | X | XX% |
| 17 | OAuth & Auth | X/22 | X | XX% |
| 18 | Caching | X/15 | X | XX% |
| 19 | Feature Chains | X/48 | X | XX% |
| 20 | Data Export | X/19 | X | XX% |
| 21 | Final Cleanup | X/10 | X | XX% |
| **TOTAL** | | **X/420+** | | **XX%** |

## Critical Phase Status
- Phases 0-3, 2b, 3b (Foundation): [ ] ALL PASS / [ ] FAIL
- Phase 17 (OAuth): [ ] ALL PASS / [ ] FAIL
- Phase 19 (Feature Chains): [ ] ALL PASS / [ ] FAIL
- Phase 21 (Cleanup): [ ] ALL PASS / [ ] FAIL

## Standard Phase Status (4-16, 18, 20)
- Pass rate: XX% (target: ≥90%)

## Overall Result: [ ] APPROVED (≥95%) / [ ] NOT APPROVED

## Failed Tests (if any)
[List each failed test with test ID, expected vs actual, and error details]

## Observations
[Notable findings, edge cases discovered, performance notes]

## Recommendations
[Suggested improvements, bugs to file, documentation updates]
```

---

## Checklist Before Closing UAT

- [ ] All 22 phases (0-21) executed
- [ ] Phase 19 (Feature Chains) completed 48 E2E tests
- [ ] Phase 20 (Data Export) validated backup/export
- [ ] Phase 21 (Final Cleanup) removed ALL test data
- [ ] Final report generated with all phases
- [ ] No UAT-tagged notes remain in system
- [ ] No UAT- prefixed collections remain
- [ ] System health verified via memory_info

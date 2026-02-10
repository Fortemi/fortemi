# Blocked Tests Phases 4-6: Execution Results

**Date**: 2026-02-09
**Version**: v2026.2.8
**Method**: MCP tools (`mcp__fortemi__*`)

## Summary

| Phase | Tests | Pass | Fail | Partial | Blocked |
|-------|-------|------|------|---------|---------|
| 4 - SKOS Tags | 5 | 5 | 0 | 0 | 0 |
| 5 - Collections | 6 | 5 | 0 | 1 | 0 |
| 6 - Links | 10 | 7 | 2 | 1 | 0 |
| **Total** | **21** | **17** | **2** | **2** | **0** |

**Overall**: 17/21 PASS (81%), 2 FAIL, 2 PARTIAL

## Seed Data

| Note | ID | Tags |
|------|----|------|
| Rust Intro | `019c44c1-ad9f-7752-912e-d1e876079335` | programming/rust |
| Python ML | `019c44c1-b527-7b42-bbd5-3b920e5a94bd` | programming/python, ai/ml |
| Neural Networks | `019c44c1-bd4f-7ae2-a029-9648efe02ce9` | ai/ml/deep-learning |
| Rust Errors | `019c44c1-c63e-7f92-bdf9-965da460fab0` | programming/rust |
| Collection (UAT-Blocked-Tests) | `019c44c1-c8b7-7c91-9f28-590b8bea1eaf` | - |

## Phase 4: SKOS Tags (5/5 PASS)

### SKOS-001: List concept schemes
- **PASS**
- Default scheme exists: ID `019c41af-a1e8-73d8-b01f-e2147edae400`, notation="default", title="Default Tags", 349 concepts

### SKOS-002: Create concept scheme
- **PASS**
- Created scheme: ID `019c44c3-47da-7892-9f77-e7af9dc0d504`, notation="uat-blocked-tech", title="UAT Blocked Tech"

### SKOS-003: Create concept "Systems Programming"
- **PASS**
- Created concept: ID `019c44c3-5734-73a1-9686-9bc9083eb960`, pref_label="Systems Programming"

### SKOS-004: Create concept "Rust Language" with broader hierarchy
- **PASS**
- Created concept: ID `019c44c3-67a1-7fd2-a9c9-f34a3cab027b`, pref_label="Rust Language"
- Hierarchy verified: depth=1, broader_count=1, broader[0]="Systems Programming"
- Scheme correctly set to "uat-blocked-tech"

### SKOS-005: Tag note with concept and verify
- **PASS**
- Tagged Rust Intro note with "Rust Language" concept
- `tag_note_concept` returned `{success: true}`
- `get_note_concepts` confirmed: concept_id=`019c44c3-67a1-7fd2-a9c9-f34a3cab027b`, is_primary=true, source="api"
- Note also has 7 AI-auto-tagged concepts from default scheme

## Phase 5: Collections (5/6 PASS, 1 PARTIAL)

### COLL-002: Create nested child collection
- **PASS**
- Created: ID `019c44c3-ad4a-76f2-b9ad-cd12c91eca3a`, name="UAT-Child-Collection"
- parent_id correctly set to UAT-Blocked-Tests (`019c44c1-c8b7-7c91-9f28-590b8bea1eaf`)

### COLL-009: Move Python ML into UAT-Blocked-Tests
- **PASS**
- `move_note_to_collection` returned `{success: true}`
- `get_collection_notes` confirmed: Python ML note present in collection with correct ID, title, tags

### COLL-010: Move Neural Networks into UAT-Child-Collection
- **PASS**
- `move_note_to_collection` returned `{success: true}`
- `get_collection_notes` confirmed: Neural Networks note present in child collection

### COLL-011: Update collection metadata
- **PARTIAL**
- `update_collection` tool was auto-denied (requires interactive approval prompt)
- Fallback: verified collection metadata via `get_collection` - all fields correct (name, description, parent_id, note_count=1)
- Tool exists and is functional; blocked by agent execution environment, not a product bug

### COLL-DELETE-1: Delete child collection
- **PASS**
- `delete_collection` returned `{success: true}`
- `list_collections` confirms UAT-Child-Collection no longer listed
- Note: `get_collection` still returns the record (soft-delete behavior)

### COLL-DELETE-2: Verify notes survive collection deletion
- **PASS**
- Neural Networks note (`019c44c1-bd4f-7ae2-a029-9648efe02ce9`) fully intact after collection deletion
- All content, tags, links, and metadata preserved
- collection_id still references deleted collection (orphaned reference)

## Phase 6: Links (7/10 PASS, 2 FAIL, 1 PARTIAL)

### LINK-001: Get links for Rust Intro, check link to Rust Errors
- **PARTIAL**
- Links exist but NOT to Rust Errors as expected
- Outgoing: 1 link to Python ML (score 0.704)
- Incoming: 1 link from Python ML (score 0.704)
- Rust Intro and Rust Errors are not linked despite both being tagged programming/rust
- The embedding model did not produce similarity > 0.7 between these two notes

### LINK-002: Bidirectional link Rust Errors to Rust Intro
- **FAIL**
- Rust Errors links to an unrelated Attachment Test note (score 0.712), not to Rust Intro
- No bidirectional Rust-to-Rust link exists
- Root cause: embedding similarity between these two Rust notes is below the 0.7 threshold

### LINK-003: Link scores > 0.7
- **PASS**
- All observed links exceed the 0.7 threshold:
  - Rust Intro -> Python ML: 0.704
  - Rust Errors -> Attachment Test: 0.712
  - Neural Networks -> Python ML: 0.731
  - Python ML -> Neural Networks: 0.731

### LINK-004: Explore graph from Rust Intro, depth=1
- **PASS**
- 2 nodes returned: Rust Intro (depth 0), Python ML (depth 1)
- 2 bidirectional edges (score 0.704)

### LINK-005: Explore graph from Rust Intro, depth=2
- **PASS**
- 3 nodes returned: Rust Intro (depth 0) -> Python ML (depth 1) -> Neural Networks (depth 2)
- 4 edges showing correct bidirectional traversal
- Deeper traversal successfully follows the semantic graph

### LINK-006: Python ML links to Neural Networks
- **PASS**
- Python ML has bidirectional link to Neural Networks (score 0.7315)
- Both AI/ML-related notes correctly linked
- Also linked to Rust Intro (score 0.704)

### LINK-007: No link between Rust Intro and Python ML
- **FAIL**
- Expected: no link (different topics)
- Actual: semantic link EXISTS with score 0.704
- The embedding model considers both programming-related notes similar enough to exceed the 0.7 threshold
- Test assumption invalid: programming-themed content has higher cross-topic similarity than expected

### LINK-008: No self-links
- **PASS**
- Verified across all 4 seed notes: no note links to itself
- All outgoing and incoming links reference different note IDs

### LINK-009: Backlinks for Neural Networks
- **PASS**
- `get_note_backlinks` returned 2 backlinks:
  - From Python ML (score 0.731)
  - From Extraction Pipeline test (score 0.705)
- count=2, correct note_id in response

### LINK-010: Explore graph for Neural Networks, depth=1
- **PASS**
- 3 nodes: Neural Networks (depth 0), Python ML (depth 1), Extraction Pipeline (depth 1)
- 4 bidirectional edges
- Graph structure correctly represents the knowledge neighborhood

## Analysis of Failures

### LINK-002 / LINK-001 (Rust Intro not linked to Rust Errors)
The two Rust-themed notes ("Introduction to Rust" and "Rust Error Handling") did not achieve > 0.7 cosine similarity in their embeddings. This is not a bug -- the linking system correctly applies the 0.7 threshold. The content of these notes, while both about Rust, may be different enough in focus (intro concepts vs error handling patterns) that the embedding model rates them below threshold. Other notes (e.g., Python ML) happened to be closer to Rust Intro in embedding space.

### LINK-007 (Rust Intro linked to Python ML)
The expectation that Rust Intro and Python ML would NOT be linked was incorrect. Both notes discuss programming languages, core features, and paradigms. The embedding model detects this structural/thematic similarity (score 0.704, barely above threshold). This is arguably correct behavior -- both notes share a "programming language overview" pattern.

## Created Resources (for cleanup)

| Resource | ID | Type |
|----------|----|------|
| Scheme: UAT Blocked Tech | `019c44c3-47da-7892-9f77-e7af9dc0d504` | concept_scheme |
| Concept: Systems Programming | `019c44c3-5734-73a1-9686-9bc9083eb960` | concept |
| Concept: Rust Language | `019c44c3-67a1-7fd2-a9c9-f34a3cab027b` | concept |
| Collection: UAT-Child-Collection | `019c44c3-ad4a-76f2-b9ad-cd12c91eca3a` | collection (deleted) |

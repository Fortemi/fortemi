# Matric-Memory UAT Executor Guide

## Overview

This guide provides step-by-step instructions for executing the comprehensive UAT plan.

**Executor**: Agentic AI with MCP access to matric-memory
**Estimated Time**: 45-60 minutes
**Required**: MCP connection to matric-memory server

## Phase-Based Execution (Recommended)

For better agentic consumption, UAT is split into individual phase documents.

> **CRITICAL**: This UAT suite contains **22 phases (0-21)**. Execute ALL phases in order. DO NOT stop at any intermediate phase. Phase 21 (Final Cleanup) runs LAST.

| Phase | Document | Duration | Tests | Critical |
|-------|----------|----------|-------|----------|
| 0 | [phases/phase-0-preflight.md](phases/phase-0-preflight.md) | ~2 min | 3 | Yes |
| 1 | [phases/phase-1-seed-data.md](phases/phase-1-seed-data.md) | ~5 min | 15 | Yes |
| 2 | [phases/phase-2-crud.md](phases/phase-2-crud.md) | ~10 min | 17 | **Yes** |
| 2b | [phases/phase-2b-file-attachments.md](phases/phase-2b-file-attachments.md) | ~15 min | 21 | **Yes** |
| 3 | [phases/phase-3-search.md](phases/phase-3-search.md) | ~10 min | 14 | **Yes** |
| 3b | [phases/phase-3b-memory-search.md](phases/phase-3b-memory-search.md) | ~15 min | 21 | **Yes** |
| 4 | [phases/phase-4-tags.md](phases/phase-4-tags.md) | ~5 min | 3 | No |
| 5 | [phases/phase-5-collections.md](phases/phase-5-collections.md) | ~3 min | 3 | No |
| 6 | [phases/phase-6-links.md](phases/phase-6-links.md) | ~5 min | 11 | No |
| 7 | [phases/phase-7-embeddings.md](phases/phase-7-embeddings.md) | ~5 min | 15 | No |
| 8 | [phases/phase-8-document-types.md](phases/phase-8-document-types.md) | ~5 min | 16 | No |
| 9 | [phases/phase-9-edge-cases.md](phases/phase-9-edge-cases.md) | ~5 min | 3 | No |
| 10 | [phases/phase-10-templates.md](phases/phase-10-templates.md) | ~8 min | 15 | No |
| 11 | [phases/phase-11-versioning.md](phases/phase-11-versioning.md) | ~7 min | 15 | No |
| 12 | [phases/phase-12-archives.md](phases/phase-12-archives.md) | ~8 min | 18 | No |
| 13 | [phases/phase-13-skos.md](phases/phase-13-skos.md) | ~12 min | 27 | No |
| 14 | [phases/phase-14-pke.md](phases/phase-14-pke.md) | ~8 min | 20 | No |
| 15 | [phases/phase-15-jobs.md](phases/phase-15-jobs.md) | ~8 min | 22 | No |
| 16 | [phases/phase-16-observability.md](phases/phase-16-observability.md) | ~10 min | 12 | No |
| 17 | [phases/phase-17-oauth-auth.md](phases/phase-17-oauth-auth.md) | ~12 min | 22 | **Yes** |
| 18 | [phases/phase-18-caching-performance.md](phases/phase-18-caching-performance.md) | ~10 min | 15 | No |
| 19 | [phases/phase-19-feature-chains.md](phases/phase-19-feature-chains.md) | ~30 min | 48 | **Yes** |
| 20 | [phases/phase-20-data-export.md](phases/phase-20-data-export.md) | ~8 min | 19 | No |
| 21 | [phases/phase-21-final-cleanup.md](phases/phase-21-final-cleanup.md) | ~5 min | 10 | **Yes** |

**Total**: 420+ tests across 22 phases (including 2b and 3b)

See [phases/README.md](phases/README.md) for execution order and success criteria.

---

## Legacy Single-Document Reference

The sections below provide the original single-document UAT reference for backwards compatibility.

## Execution Instructions

### Before Starting

1. Ensure MCP connection is active: `list_notes(limit=1)` should work
2. Note the starting state: `memory_info()` for baseline counts
3. Create a results tracking structure

### Results Tracking Template

```yaml
uat_run:
  started_at: "<timestamp>"
  completed_at: "<timestamp>"
  executor: "<agent_id>"
  results:
    phase_0: { passed: 0, failed: 0, skipped: 0 }
    phase_1: { passed: 0, failed: 0, skipped: 0 }
    # ... etc
  failures: []
  notes: []
```

---

## Phase 0: Pre-flight Checks

### PF-001: System Health Check
```
memory_info()
```
**Pass if**: Response contains `summary` and `storage` objects

### PF-002: Backup System Status
```
backup_status()
```
**Pass if**: Response contains `status` field

### PF-003: Embedding Pipeline Status
```
list_embedding_sets()
```
**Pass if**: Response contains set with `slug: "default"`

---

## Phase 1: Seed Data Generation

### Create Test Collections

```
create_collection(name="UAT-Research", description="Research notes for UAT testing")
create_collection(name="UAT-Projects", description="Project documentation for UAT testing")
create_collection(name="UAT-Personal", description="Personal notes for UAT testing")
```

### Create Seed Notes

Execute `bulk_create_notes` with the following content:

```javascript
bulk_create_notes({
  notes: [
    // SEED-ML-001: Neural Networks Introduction
    {
      content: `# Introduction to Neural Networks

Neural networks are computing systems inspired by biological neural networks.
They consist of layers of interconnected nodes (neurons) that process information.

## Key Components
- **Input Layer**: Receives raw data
- **Hidden Layers**: Process and transform data
- **Output Layer**: Produces final predictions

## Activation Functions
Common activation functions include ReLU, sigmoid, and tanh.

## Related Concepts
Deep learning uses neural networks with many hidden layers.
Backpropagation is the primary training algorithm.`,
      tags: ["uat/ml", "uat/ml/neural-networks", "uat/fundamentals"],
      revision_mode: "none",
      metadata: { domain: "machine-learning", difficulty: "beginner" }
    },

    // SEED-ML-002: Deep Learning Architectures
    {
      content: `# Deep Learning Architectures

Deep learning extends neural networks with specialized architectures.

## Convolutional Neural Networks (CNNs)
CNNs excel at image processing using convolutional layers that detect
spatial patterns like edges, textures, and shapes.

## Recurrent Neural Networks (RNNs)
RNNs process sequential data by maintaining hidden state across time steps.
LSTMs and GRUs address the vanishing gradient problem.

## Transformers
Attention-based architecture that revolutionized NLP. Powers models like
BERT, GPT, and Claude. Self-attention enables parallel processing.`,
      tags: ["uat/ml", "uat/ml/deep-learning", "uat/ml/architectures"],
      revision_mode: "none",
      metadata: { domain: "machine-learning", difficulty: "intermediate" }
    },

    // SEED-ML-003: Backpropagation
    {
      content: `# Backpropagation Algorithm

Backpropagation is the cornerstone of neural network training.

## How It Works
1. **Forward Pass**: Input flows through network to produce output
2. **Loss Calculation**: Compare output with expected result
3. **Backward Pass**: Calculate gradients using chain rule
4. **Weight Update**: Adjust weights using gradient descent

## Mathematical Foundation
The chain rule allows us to compute partial derivatives of the loss
with respect to each weight in the network.

∂L/∂w = ∂L/∂a × ∂a/∂z × ∂z/∂w`,
      tags: ["uat/ml", "uat/ml/training", "uat/ml/neural-networks"],
      revision_mode: "none",
      metadata: { domain: "machine-learning", difficulty: "intermediate" }
    },

    // SEED-RUST-001: Ownership
    {
      content: `# Rust Ownership System

Rust's ownership system ensures memory safety without garbage collection.

## Three Rules
1. Each value has exactly one owner
2. When the owner goes out of scope, the value is dropped
3. Values can be borrowed (referenced) but borrowing has rules

## Borrowing Rules
- You can have either ONE mutable reference OR any number of immutable references
- References must always be valid (no dangling pointers)`,
      tags: ["uat/programming", "uat/programming/rust", "uat/memory-safety"],
      revision_mode: "none",
      metadata: { language: "rust", topic: "ownership" }
    },

    // SEED-RUST-002: Error Handling
    {
      content: `# Rust Error Handling

Rust uses Result and Option types for explicit error handling.

## Result<T, E>
Represents either success (Ok(T)) or failure (Err(E)).

## The ? Operator
Propagates errors automatically, reducing boilerplate.

## Option<T>
Represents optional values - Some(T) or None.
Eliminates null pointer exceptions.`,
      tags: ["uat/programming", "uat/programming/rust", "uat/error-handling"],
      revision_mode: "none",
      metadata: { language: "rust", topic: "error-handling" }
    },

    // SEED-I18N-001: Chinese AI
    {
      content: `# 人工智能简介 (Introduction to AI in Chinese)

人工智能（AI）是计算机科学的一个分支，旨在创建能够执行通常需要人类智能的任务的系统。

## 主要领域
- **机器学习**: 从数据中学习模式
- **自然语言处理**: 理解和生成人类语言
- **计算机视觉**: 分析和理解图像

## 深度学习
深度学习使用多层神经网络来学习数据的复杂表示。`,
      tags: ["uat/i18n", "uat/i18n/chinese", "uat/ml"],
      revision_mode: "none",
      metadata: { language: "zh-CN" }
    },

    // SEED-I18N-002: Arabic AI
    {
      content: `# مقدمة في الذكاء الاصطناعي

الذكاء الاصطناعي هو فرع من علوم الحاسوب يهدف إلى إنشاء أنظمة ذكية.

## المجالات الرئيسية
- التعلم الآلي
- معالجة اللغات الطبيعية
- الرؤية الحاسوبية`,
      tags: ["uat/i18n", "uat/i18n/arabic", "uat/ml"],
      revision_mode: "none",
      metadata: { language: "ar", direction: "rtl" }
    },

    // SEED-I18N-003: Diacritics
    {
      content: `# Café Culture and Naïve Résumé Writing

Testing diacritics and accent marks in content.

## Words with Diacritics
- café (French coffee shop)
- naïve (innocent, simple)
- résumé (summary, CV)
- jalapeño (spicy pepper)
- über (German: over, super)
- Zürich (Swiss city)

These words should be findable with or without accents.`,
      tags: ["uat/i18n", "uat/i18n/diacritics", "uat/search-test"],
      revision_mode: "none",
      metadata: { test_type: "accent-folding" }
    },

    // SEED-EDGE-001: Empty Sections
    {
      content: `# Empty Sections Test

## Section with content
This section has content.

## Empty section

## Another section with content
More content here.`,
      tags: ["uat/edge-cases", "uat/formatting"],
      revision_mode: "none"
    },

    // SEED-EDGE-002: Special Characters
    {
      content: `# Special Characters Test

## Code Symbols
\`{}[]()<>|&^%$#@!\`

## Math Symbols
∑ ∏ ∫ √ ∞ ≠ ≤ ≥ ± × ÷

## Currency
$ € £ ¥ ₹ ₿

## Emoji
🚀 🎉 ✅ ❌ 🔥 💡 🐱 🐶`,
      tags: ["uat/edge-cases", "uat/special-chars"],
      revision_mode: "none"
    }
  ]
})
```

**Store the returned IDs** for use in subsequent tests.

---

## Phase 2: CRUD Operations

### CRUD-001: Create Note - Basic
```
create_note(
  content="# UAT Test Note\n\nThis is a basic test note.",
  tags=["uat/crud-test"],
  revision_mode="none"
)
```
**Pass if**: Returns `{id: "<uuid>"}`
**Store**: `crud_test_note_id`

### CRUD-002: Create Note - With Metadata
```
create_note(
  content="# Metadata Test\n\nNote with custom metadata.",
  tags=["uat/crud-test", "uat/metadata"],
  metadata={"source": "uat-test", "priority": "high", "version": 1},
  revision_mode="none"
)
```
**Pass if**: Returns valid ID

### CRUD-003: Create Note - Hierarchical Tags
```
create_note(
  content="# Hierarchical Tag Test",
  tags=["uat/hierarchy/level1/level2/level3"],
  revision_mode="none"
)
```
**Pass if**: Returns valid ID
**Verify**: `list_tags()` contains the hierarchical tag

### CRUD-004: Bulk Create
```
bulk_create_notes(notes=[
  {content: "Bulk note 1", tags: ["uat/bulk"], revision_mode: "none"},
  {content: "Bulk note 2", tags: ["uat/bulk"], revision_mode: "none"},
  {content: "Bulk note 3", tags: ["uat/bulk"], revision_mode: "none"}
])
```
**Pass if**: Returns `{count: 3, ids: [...]}`

### CRUD-005: Get Note by ID
```
get_note(id=<crud_test_note_id>)
```
**Pass if**: Returns full note with `note`, `original`, `revised`, `tags`

### CRUD-006: Get Note - Non-existent
```
get_note(id="00000000-0000-0000-0000-000000000000")
```
**Pass if**: Returns error (not crash)

### CRUD-007: List Notes - Basic
```
list_notes(limit=10)
```
**Pass if**: Returns `{notes: [...], total: <n>}`

### CRUD-008: List Notes - Tag Filter
```
list_notes(tags=["uat/bulk"], limit=50)
```
**Pass if**: Returns 3 notes (from CRUD-004)

### CRUD-009: List Notes - Hierarchical Tag Filter
```
list_notes(tags=["uat"], limit=100)
```
**Pass if**: Returns all UAT-tagged notes (prefix matching)

### CRUD-010: Pagination
```
page1 = list_notes(limit=5, offset=0)
page2 = list_notes(limit=5, offset=5)
```
**Pass if**: Different notes on each page

### CRUD-011: Limit Zero
```
list_notes(limit=0)
```
**Pass if**: Returns `{notes: [], total: <n>}`

### CRUD-012: Update Content
```
update_note(
  id=<crud_test_note_id>,
  content="# Updated Content\n\nThis was updated.",
  revision_mode="none"
)
```
**Pass if**: Success, `get_note` shows new content

### CRUD-013: Star Note
```
update_note(id=<note_id>, starred=true)
```
**Pass if**: `get_note` shows `starred: true`

### CRUD-014: Archive Note
```
update_note(id=<note_id>, archived=true)
```
**Pass if**: Note appears in `list_notes(filter="archived")`

### CRUD-015: Update Metadata
```
update_note(id=<note_id>, metadata={"updated": true, "version": 2})
```
**Pass if**: `get_note` shows new metadata

### CRUD-016: Delete Note
```
delete_note(id=<note_to_delete>)
```
**Pass if**: Note no longer in `list_notes`

---

## Phase 3: Search Capabilities

### SEARCH-001: FTS Basic
```
search_notes(query="neural networks", mode="fts", limit=10)
```
**Pass if**: Returns ML notes

### SEARCH-002: FTS OR Operator
```
search_notes(query="rust OR python", mode="fts", limit=10)
```
**Pass if**: Returns notes with rust OR python

### SEARCH-003: FTS NOT Operator
```
search_notes(query="programming -rust", mode="fts", limit=10)
```
**Pass if**: Results exclude rust content

### SEARCH-004: FTS Phrase
```
search_notes(query="\"neural networks\"", mode="fts", limit=10)
```
**Pass if**: Exact phrase matches

### SEARCH-005: Accent Folding (cafe)
```
search_notes(query="cafe", mode="fts", limit=10)
```
**Pass if**: Finds "café" content

### SEARCH-006: Accent Folding (naive)
```
search_notes(query="naive resume", mode="fts", limit=10)
```
**Pass if**: Finds "naïve" and "résumé" content

### SEARCH-007: Chinese
```
search_notes(query="人工智能", mode="fts", limit=10)
```
**Pass if**: Finds Chinese AI note

### SEARCH-008: Chinese Single Char
```
search_notes(query="学", mode="fts", limit=10)
```
**Pass if**: Returns results (CJK tokenization works)

### SEARCH-009: Arabic RTL
```
search_notes(query="الذكاء الاصطناعي", mode="fts", limit=10)
```
**Pass if**: Finds Arabic AI note

### SEARCH-010: Semantic Conceptual
```
search_notes(query="machine intelligence", mode="semantic", limit=5)
```
**Pass if**: Finds AI/ML notes (requires embeddings)

### SEARCH-011: Hybrid Search
```
search_notes(query="deep learning transformers", mode="hybrid", limit=10)
```
**Pass if**: Returns relevant results

### SEARCH-012: Search with Tag Filter
```
search_notes(query="neural", mode="fts", tags=["uat/ml"], limit=10)
```
**Pass if**: All results have uat/ml tag

### SEARCH-013: Empty Results
```
search_notes(query="xyznonexistent123", mode="fts", limit=10)
```
**Pass if**: Returns `{results: [], total: 0}`

### SEARCH-014: Special Characters
```
search_notes(query="∑ ∏ ∫", mode="fts", limit=10)
```
**Pass if**: No crash, returns results or empty

---

## Phase 4: Tag System

### TAG-001: List Tags
```
list_tags()
```
**Pass if**: Returns array with name and note_count

### TAG-002: Verify Hierarchy
```
list_tags()
```
**Pass if**: Contains `uat/hierarchy/level1/level2/level3`

### TAG-003: Case Insensitivity
```
create_note(content="Case test", tags=["UAT/CASE-TEST"], revision_mode="none")
list_notes(tags=["uat/case-test"])
```
**Pass if**: Note found with lowercase query

---

## Phase 5: Collections

### COLL-001: Create Collection
```
create_collection(name="UAT-Test-Collection", description="Test")
```
**Pass if**: Returns `{id: "<uuid>"}`

### COLL-002: Move Note
```
move_note_to_collection(note_id=<id>, collection_id=<coll_id>)
```
**Pass if**: Success

### COLL-003: Get Collection Notes
```
get_collection_notes(id=<collection_id>)
```
**Pass if**: Contains moved note

---

## Phase 6: Semantic Links

### LINK-001: Get Note Links
```
get_note_links(id=<ml_note_id>)
```
**Pass if**: Returns `{outgoing: [...], incoming: [...]}`

---

## Phase 7: Embeddings

### EMB-001: List Sets
```
list_embedding_sets()
```
**Pass if**: Default set exists

### EMB-002: Index Status
```
list_embedding_sets()
```
**Pass if**: `index_status` is valid enum value

---

## Phase 8: Emergent Properties

### EMRG-001: Knowledge Discovery
```
1. search_notes(query="gradient descent", mode="hybrid")
2. get_note_links(id=<top_result>)
3. Verify links to related ML concepts
```
**Pass if**: Can traverse knowledge graph

### EMRG-002: Cross-Feature
```
1. list_notes(tags=["uat/ml"])
2. search_notes(query="learning", tags=["uat/ml"])
```
**Pass if**: Tag filter refines search

---

## Phase 9: Edge Cases

### EDGE-001: Empty Content
```
create_note(content="", tags=["uat/edge"])
```
**Pass if**: Error or handled gracefully

### EDGE-002: Invalid UUID
```
get_note(id="not-a-uuid")
```
**Pass if**: Clear validation error

### EDGE-003: SQL Injection
```
search_notes(query="'; DROP TABLE notes; --", mode="fts")
```
**Pass if**: Query treated as literal, no SQL execution

---

## Phase 10: Backup

### BACK-001: Status
```
backup_status()
```
**Pass if**: Returns status info

### BACK-002: Export All
```
export_all_notes()
```
**Pass if**: Returns manifest and notes

---

## Phase 11: Cleanup

```
// Get all UAT notes
notes = list_notes(tags=["uat"], limit=1000)

// Delete each
for note in notes.notes:
  delete_note(id=note.id)

// Verify cleanup
verify = list_notes(tags=["uat"])
assert verify.total == 0
```

---

## Final Report Template

```markdown
# Matric-Memory UAT Report

## Summary
- **Date**: YYYY-MM-DD
- **Duration**: X minutes
- **Overall Result**: PASS/FAIL

## Results by Phase

| Phase | Tests | Passed | Failed | Pass Rate |
|-------|-------|--------|--------|-----------|
| 0: Pre-flight | 3 | X | X | X% |
| 1: Seed Data | 15 | X | X | X% |
| 2: CRUD | 17 | X | X | X% |
| 2b: Attachments | 21 | X | X | X% |
| 3: Search | 14 | X | X | X% |
| 3b: Memory Search | 21 | X | X | X% |
| 4: Tags | 3 | X | X | X% |
| 5: Collections | 3 | X | X | X% |
| 6: Links | 11 | X | X | X% |
| 7: Embeddings | 15 | X | X | X% |
| 8: Document Types | 16 | X | X | X% |
| 9: Edge Cases | 3 | X | X | X% |
| 10: Backup | 19 | X | X | X% |
| 11: Cleanup | 1 | X | X | X% |
| 12: Templates | 15 | X | X | X% |
| 13: Versioning | 15 | X | X | X% |
| 14: Archives | 18 | X | X | X% |
| 15: SKOS | 27 | X | X | X% |
| 16: PKE | 20 | X | X | X% |
| 17: Jobs | 18 | X | X | X% |
| **TOTAL** | **~270** | **X** | **X** | **X%** |

## Failed Tests

### [TEST-ID] Test Name
- **Expected**: ...
- **Actual**: ...
- **Error**: ...

## Observations

- ...

## Recommendations

- ...
```

---

## Success Criteria

- **Critical Phases (0-3, 2b, 3b)**: 100% pass required
- **Standard Phases (4-17)**: 90% pass acceptable
- **Overall**: 95% pass for release approval

## MCP Tool Coverage

**Target**: 100% of exposed MCP tools have UAT test cases

| Category | Tools | Covered |
|----------|-------|---------|
| Note Operations | 12 | 100% |
| Search | 4 | 100% |
| Collections | 8 | 100% |
| Templates | 6 | 100% |
| Embedding Sets | 10 | 100% |
| Versioning | 5 | 100% |
| Graph/Links | 4 | 100% |
| Jobs | 4 | 100% |
| SKOS | 22 | 100% |
| Archives | 7 | 100% |
| Document Types | 6 | 100% |
| Backup | 17 | 100% |
| PKE | 13 | 100% |
| Documentation | 1 | 100% |
| **Total** | **~120** | **100%** |

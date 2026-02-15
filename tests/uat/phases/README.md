# UAT Phase Documents

This directory contains phase-based UAT test procedures for Fortemi, designed for efficient agentic execution via MCP tools.

> **MCP-First Testing Policy (MANDATORY)**: This UAT suite tests Fortemi as an agent uses it in a real session — through MCP tool invocations, not direct HTTP API calls. Every test MUST use MCP tools. **If an MCP tool fails or is missing, FILE A BUG ISSUE — do NOT fall back to curl or direct API calls.** The failure IS the finding.

---

## Tool Surface: 27 Core MCP Tools

Following the tool surface consolidation (#365), Fortemi exposes **27 core MCP tools** using a discriminated-union pattern. Eleven consolidated tools replace ~180 granular tools:

| Category | Tools | Count |
|----------|-------|-------|
| Notes CRUD | `list_notes`, `get_note`, `update_note`, `delete_note`, `restore_note` | 5 |
| Consolidated | `capture_knowledge`, `search`, `record_provenance`, `manage_tags`, `manage_collection`, `manage_concepts`, `manage_attachments`, `manage_embeddings`, `manage_archives`, `manage_encryption`, `manage_backups` | 11 |
| Graph | `explore_graph`, `get_topology_stats`, `get_note_links` | 3 |
| Export | `export_note` | 1 |
| System | `get_documentation`, `get_system_info`, `health_check` | 3 |
| Multi-memory | `select_memory`, `get_active_memory` | 2 |
| Observability | `get_knowledge_health` | 1 |
| Bulk ops | `bulk_reprocess_notes` | 1 |
| **Total** | | **27** |

**Advanced features** (versioning, SKOS taxonomy editing, OAuth admin, job queue management) are accessible via the REST API. Use `get_documentation` for API guidance.

---

## Suite Completion Requirements

> **WARNING FOR AGENTIC EXECUTORS**: This UAT suite contains **15 phases (0-14)**. You MUST execute ALL phases to completion. DO NOT stop at any intermediate phase.

The suite is NOT complete until:
- Phase 12 (Feature Chains) completes all 20 end-to-end tests
- Phase 13 (Embedding Sets) completes all 18 embedding set tests
- Phase 14 (Cleanup) removes ALL test data using MCP tools

**Phase 14 is the ONLY cleanup phase — it runs LAST, not in the middle.**

---

## Phase Overview

| Phase | Document | Duration | Tests | Critical |
|-------|----------|----------|-------|----------|
| 0 | [Preflight & System](phase-0-preflight.md) | ~2 min | 5 | **Yes** |
| 1 | [Knowledge Capture](phase-1-capture.md) | ~5 min | 10 | **Yes** |
| 2 | [Notes CRUD](phase-2-crud.md) | ~8 min | 15 | **Yes** |
| 3 | [Search](phase-3-search.md) | ~8 min | 12 | **Yes** |
| 4 | [Tags & Concepts](phase-4-tags-concepts.md) | ~5 min | 12 | No |
| 5 | [Collections](phase-5-collections.md) | ~5 min | 10 | No |
| 6 | [Graph & Links](phase-6-graph.md) | ~5 min | 8 | No |
| 7 | [Provenance](phase-7-provenance.md) | ~5 min | 10 | No |
| 8 | [Multi-Memory](phase-8-multi-memory.md) | ~5 min | 8 | No |
| 9 | [Attachments](phase-9-media.md) | ~5 min | 8 | No |
| 10 | [Export, Health & Bulk Ops](phase-10-export-health.md) | ~5 min | 8 | No |
| 11 | [Edge Cases](phase-11-edge-cases.md) | ~5 min | 10 | No |
| 12 | [Feature Chains (E2E)](phase-12-feature-chains.md) | ~15 min | 20 | **Yes** |
| 13 | [Embedding Sets](phase-13-embedding-sets.md) | ~8 min | 18 | No |
| 14 | [Final Cleanup](phase-14-cleanup.md) | ~3 min | 7 | **Yes** |

**Total Tests**: ~165
**Total Estimated Duration**: ~90 minutes (full suite)

**Total Phases**: 15

---

## MCP Tool Coverage Summary

| Tool | Phases | Tests |
|------|--------|-------|
| `health_check` | 0 | 1 |
| `get_system_info` | 0, 9 | 2 |
| `get_documentation` | 0 | 1 |
| `capture_knowledge` | 1, 7, 8, 11, 12, 13 | ~18 |
| `list_notes` | 2, 8, 14 | ~6 |
| `get_note` | 2, 12 | ~4 |
| `update_note` | 2, 11 | ~6 |
| `delete_note` | 2, 11, 14 | ~4 |
| `restore_note` | 2, 11 | ~3 |
| `search` | 3, 7, 8, 11, 12, 13 | ~18 |
| `manage_tags` | 4 | 5 |
| `manage_concepts` | 4 | 7 |
| `manage_collection` | 5, 12, 14 | ~12 |
| `explore_graph` | 6, 12 | ~5 |
| `get_note_links` | 6 | ~3 |
| `record_provenance` | 7, 12 | ~10 |
| `select_memory` | 8, 14 | ~4 |
| `get_active_memory` | 8 | ~3 |
| `manage_attachments` | 9 | 7 |
| `manage_embeddings` | 13, 14 | ~18 |
| `manage_archives` | 0, 8, 12, 14 | ~6 |
| `manage_encryption` | — | 0 |
| `manage_backups` | — | 0 |
| `get_topology_stats` | 6 | ~1 |
| `export_note` | 10, 12 | ~3 |
| `get_knowledge_health` | 10, 12 | ~3 |
| `bulk_reprocess_notes` | 10, 12 | ~4 |
| **27/27 tools** | **All phases** | **~165** |

**Coverage**: 27/27 core MCP tools (100%)
> **Note**: `manage_encryption` and `manage_backups` are covered by automated integration tests (`consolidated-tools.test.js`) but not included in manual UAT phases since they require specific infrastructure (PKE keys, shard files). Use `get_documentation` for usage guidance.

---

## Execution Order

**IMPORTANT**: Phases MUST be executed in numerical order from 0 to 14.

### Phase Groupings

```
┌──────────────────────────────────────────────────────┐
│  FOUNDATION (Phases 0-3) - CRITICAL                  │
│  System check, knowledge capture, CRUD, search       │
├──────────────────────────────────────────────────────┤
│  ORGANIZATION (Phases 4-6)                           │
│  Tags, concepts, collections, graph                  │
├──────────────────────────────────────────────────────┤
│  CONTEXT (Phases 7-9)                                │
│  Provenance, multi-memory, attachments               │
├──────────────────────────────────────────────────────┤
│  OPERATIONS (Phase 10)                               │
│  Export, health monitoring, bulk operations           │
├──────────────────────────────────────────────────────┤
│  RESILIENCE & E2E (Phases 11-12) - CRITICAL          │
│  Edge cases, cross-cutting feature chains            │
├──────────────────────────────────────────────────────┤
│  EMBEDDING SETS (Phase 13)                           │
│  Set CRUD, membership, search scoping                │
├──────────────────────────────────────────────────────┤
│  FINALIZATION (Phase 14) - ALWAYS LAST               │
│  Cleanup all UAT test data via MCP                   │
└──────────────────────────────────────────────────────┘
```

### Execution Steps

1. **Generate test data** first: `cd tests/uat/data/scripts && ./generate-test-data.sh`
2. **Phase 0** validates system readiness
3. **Phase 1** creates seed notes required by subsequent phases
4. **Phases 2-13** execute feature tests in order
5. **Phase 14** (Final Cleanup) MUST run LAST

### No Test Skipping

Every test must be executed regardless of upstream failures. Cascading failures reveal the true blast radius. Each failure should be filed as a Gitea issue. Do not mark tests as BLOCKED or skip them.

### Partial Execution (Time-Constrained)

If running a subset, always include:
- **Start**: Phases 0, 1 (foundation)
- **Core**: Phases 2, 3 (critical CRUD + search)
- **End**: Phase 14 (cleanup - ALWAYS LAST)

---

## Success Criteria

- **All Phases (0-14)**: 100% pass required for release approval
- **No skipping**: Every test must execute. Failures get filed as issues.
- **Test data**: Must be generated before execution

---

## Test Data

### Comprehensive Test Data Package (`../data/`)

The primary test data lives in `tests/uat/data/` with 44+ files:

| Directory | Files | Purpose |
|-----------|-------|---------|
| `data/images/` | 7 | JPEG with EXIF/GPS, PNG, WebP, unicode filenames |
| `data/provenance/` | 7 | GPS-tagged photos (Paris, NYC, Tokyo), dated images |
| `data/multilingual/` | 13 | Text in 13 languages (EN/DE/FR/ES/PT/RU + CJK + AR/EL/HE + emoji) |
| `data/documents/` | 8 | Code (Python, Rust, JS, TS), Markdown, JSON, YAML, CSV |
| `data/audio/` | 3 | Speech samples (English, Spanish, Chinese) |
| `data/edge-cases/` | 6 | Empty, 100KB, binary mismatch, unicode filename, malformed |

**Setup**: Generate test data before running UAT:
```bash
cd tests/uat/data/scripts
./generate-test-data.sh
```

### Phase-Specific Test Data Usage

| Phase | Test Data Files Used |
|-------|---------------------|
| **7 (Provenance)** | `data/provenance/*.jpg` (GPS-tagged photos) |
| **9 (Attachments)** | Any file from `data/images/` or `data/documents/` for upload testing |
| **11 (Edge Cases)** | `data/edge-cases/empty.txt`, `data/edge-cases/large-text-100kb.txt` |
| **12 (Feature Chains)** | Multiple directories as needed per chain |

### Legacy Fixtures (`../fixtures/`)

- `seed-notes.json` - Seed notes for bulk import
- `test-concepts.json` - SKOS concepts for taxonomy testing
- `sample-image.png` - 1x1 PNG for basic attachment testing

---

## Execution Modes

### Quick Smoke Test (~15 min)
Phases: 0, 1, 2, 3, 14

### Standard Suite (~50 min)
Phases: 0-10, 14

### Full Suite (~90 min)
Phases: 0-14 (all phases in order)

---

## For Agentic Execution

Each phase document is self-contained with:
- Clear test IDs (e.g., `PF-001`, `CK-001`, `CRUD-001`, `CHAIN-001`)
- Exact MCP tool calls in JavaScript format
- Pass criteria for each test
- Phase summary table for tracking
- Dependencies listed in Prerequisites

### Agent Execution Rules

Agents MUST:
1. **Use MCP tools for ALL tests** — never fall back to curl or direct HTTP
2. **If an MCP tool fails, file a bug issue** — the failure IS the finding
3. Execute tests sequentially within each phase
4. Record results in the phase summary table
5. **Always proceed to the next phase** — never skip due to upstream failures
6. **Execute ALL 15 phases (0-14)** — do not stop early
7. **Phase 14 (Final Cleanup) is MANDATORY** and runs LAST
8. **File a Gitea issue for every failure** — tag with `bug` and `mcp`

### Negative Test Isolation Protocol

Tests marked with `**Isolation**: Required` expect an error response. These MUST be executed as standalone, single MCP calls — never batched with other tool calls.

**Why**: Claude Code's "sibling call error" mechanism auto-fails other calls when one errors. Negative tests deliberately trigger errors, so batching causes false failures.

**Rules**:
1. When you encounter `**Isolation**: Required`, issue that MCP call **alone** in its own turn
2. Evaluate the result against the stated pass criteria (the "error" IS the expected outcome)
3. After the isolated call completes, resume normal testing in the next turn

### Anti-Termination Checklist

Before declaring UAT complete, verify:
- [ ] Phase 12 (Feature Chains) executed with 20 tests
- [ ] Phase 13 (Embedding Sets) executed with 18 tests
- [ ] Phase 14 (Cleanup) removed all UAT test data and archives
- [ ] Final report includes all 15 phases

---

## Advanced Features (API-Only)

The following features are NOT covered by the 27 core MCP tools and require direct API access. Use `get_documentation` for guidance:

- **Versioning**: Note version history, rollback
- **SKOS Taxonomy**: Concept scheme CRUD, broader/narrower relations, turtle export
- **OAuth & Auth**: Client registration, token management, API keys
- **Job Queue**: Individual job creation, monitoring, queue stats
- **Document Types**: Type registry, detection configuration
- **Caching**: Cache control headers, performance tuning

These may be tested separately via API integration tests outside this MCP UAT suite.

---

## Version History

- **2026.2.15b**: Added `manage_archives`, `manage_encryption`, `manage_backups`, `get_topology_stats` to core surface (23→27 tools, 8→11 consolidated). Updated PF-003/PF-006, MEM-004, CHAIN-012, CLEAN-007 to use `manage_archives` instead of HTTP API. PKE encryption moved from API-only to MCP core. ~165 tests.
- **2026.2.15**: Added Phase 13 (Embedding Sets) with 18 tests covering `manage_embeddings` CRUD, membership, search scoping, and error handling. Renumbered cleanup to Phase 14 with embedding set cleanup (CLEAN-005). 15 phases / ~159 tests.
- **2026.2.14**: Complete rewrite for 22-tool core surface (#365, #389, #392). 14 phases / ~141 tests. Removed standalone media tools (describe_image, transcribe_audio) — media processing is pipeline-only. Added `manage_attachments` consolidated tool. Advanced features (versioning, PKE, SKOS admin, OAuth, jobs, embeddings) documented as API-only.
- **2026.2.19**: Previous version — 30 phases, 545 tests, 202 MCP tools
- **2026.1.0**: Initial UAT document

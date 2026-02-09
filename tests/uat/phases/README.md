# UAT Phase Documents

This directory contains phase-based UAT test procedures for Matric Memory, designed for efficient agentic execution via MCP tools.

> **MCP-First Testing Policy (MANDATORY)**: This UAT suite tests Matric Memory as an agent uses it in a real session — through MCP tool invocations, not direct HTTP API calls. Every test that can be expressed as an MCP tool call MUST use MCP tools. **If an MCP tool fails or is missing, FILE A BUG ISSUE — do NOT fall back to curl or direct API calls.** The failure IS the finding. Direct API calls are only acceptable for: (1) file upload/download where binary data must not pass through MCP, (2) OAuth infrastructure tests in Phase 17 Part B, and (3) provenance test data setup in Phase 3B via raw SQL ([#261](https://git.integrolabs.net/Fortemi/fortemi/issues/261)). All other operations MUST use MCP tools.

---

## CRITICAL: Suite Completion Requirements

> **WARNING FOR AGENTIC EXECUTORS**: This UAT suite contains **25 phases (0-21, plus sub-phases)**. You MUST execute ALL phases to completion. DO NOT stop at any intermediate phase.

**Common Error**: AI agents sometimes stop at phase 9 (Edge Cases) or misinterpret phase names. The suite is NOT complete until:
- Phase 19 (Feature Chains) completes all 48 end-to-end tests
- Phase 20 (Data Export) validates backup functionality
- Phase 21 (Final Cleanup) removes ALL test data using MCP tools

**Phase 21 is the ONLY cleanup phase - it runs LAST, not in the middle.**

---

## Phase Overview

| Phase | Document | Duration | Tests | Critical |
|-------|----------|----------|-------|----------|
| 0 | [Pre-flight Checks](phase-0-preflight.md) | ~2 min | 3 | **Yes** |
| 1 | [Seed Data Generation](phase-1-seed-data.md) | ~5 min | 11 | **Yes** |
| 2 | [CRUD Operations](phase-2-crud.md) | ~10 min | 17 | **Yes** |
| 2b | [File Attachments](phase-2b-file-attachments.md) | ~15 min | 22 | **Yes** |
| 2c | [Attachment Processing](phase-2c-attachment-processing.md) | ~20 min | 31 | **Yes** |
| 3 | [Search Capabilities](phase-3-search.md) | ~10 min | 18 | **Yes** |
| 3b | [Memory Search](phase-3b-memory-search.md) | ~15 min | 21 | **Yes** |
| 4 | [Tag System](phase-4-tags.md) | ~5 min | 11 | No |
| 5 | [Collections](phase-5-collections.md) | ~3 min | 10 | No |
| 6 | [Semantic Links](phase-6-links.md) | ~5 min | 13 | No |
| 7 | [Embeddings](phase-7-embeddings.md) | ~5 min | 20 | No |
| 8 | [Document Types](phase-8-document-types.md) | ~5 min | 16 | No |
| 9 | [Edge Cases](phase-9-edge-cases.md) | ~5 min | 15 | No |
| 10 | [Templates](phase-10-templates.md) | ~8 min | 15 | No |
| 11 | [Versioning](phase-11-versioning.md) | ~7 min | 15 | No |
| 12 | [Archives](phase-12-archives.md) | ~8 min | 18 | No |
| 13 | [SKOS Taxonomy](phase-13-skos.md) | ~12 min | 40 | No |
| 14 | [PKE Encryption](phase-14-pke.md) | ~8 min | 20 | No |
| 15 | [Jobs & Queue](phase-15-jobs.md) | ~8 min | 22 | No |
| 16 | [Observability](phase-16-observability.md) | ~10 min | 12 | No |
| 17 | [Authentication & Access Control](phase-17-oauth-auth.md) | ~12 min | 17 | **Yes** |
| 18 | [Caching & Performance](phase-18-caching-performance.md) | ~10 min | 15 | No |
| 19 | [Feature Chains (E2E)](phase-19-feature-chains.md) | ~30 min | 48 | **Yes** |
| 20 | [Data Export](phase-20-data-export.md) | ~8 min | 19 | No |
| 21 | [Final Cleanup](phase-21-final-cleanup.md) | ~5 min | 10 | **Yes** |

**Total Tests**: 459
**Total Estimated Duration**: 220-260 minutes (full suite)
**Total Phases**: 25 (numbered 0-21, plus sub-phases 2b, 2c, and 3b)

---

## MCP Tool Coverage Summary

| Category | Tools | UAT Tests | Coverage |
|----------|-------|-----------|----------|
| Note CRUD | 12 | 39 | 100% |
| Search | 4 | 39 | 100% |
| Tags | 2 | 11 | 100% |
| Collections | 8 | 10 | 100% |
| Templates | 6 | 15 | 100% |
| Embedding Sets | 15 | 20 | 100% |
| Versioning | 5 | 15 | 100% |
| Graph/Links | 7 | 13 | 100% |
| Jobs | 7 | 22 | 100% |
| SKOS | 33 | 40 | 100% |
| Archives | 7 | 18 | 100% |
| Document Types | 6 | 16 | 100% |
| Backup/Export | 17 | 19 | 100% |
| PKE | 13 | 20 | 100% |
| Observability | 7 | 12 | 100% |
| Auth & Access Control | 8 MCP tools + 4 infra | 17 | 100% |
| Caching & Performance | 5 MCP tools | 15 | 100% |
| Attachment Processing | 5 (upload, list, get, detect, delete) | 31 | 100% |
| **TOTAL** | **148+** | **459** | **100%** |

---

## Execution Order

**IMPORTANT**: Phases MUST be executed in numerical order from 0 to 21.

### Phase Groupings

```
┌─────────────────────────────────────────────────────────────────┐
│  FOUNDATION (Phases 0-3, 2b, 2c, 3b) - CRITICAL                 │
│  System validation, seed data, CRUD, attachments, search        │
├─────────────────────────────────────────────────────────────────┤
│  CORE FEATURES (Phases 4-9)                                     │
│  Tags, Collections, Links, Embeddings, Document Types, Edge     │
├─────────────────────────────────────────────────────────────────┤
│  ADVANCED FEATURES (Phases 10-16)                               │
│  Templates, Versioning, Archives, SKOS, PKE, Jobs, Observability│
├─────────────────────────────────────────────────────────────────┤
│  INTEGRATION & AUTH (Phases 17-18) - CRITICAL                   │
│  OAuth authentication, Caching & performance                    │
├─────────────────────────────────────────────────────────────────┤
│  END-TO-END VALIDATION (Phase 19) - CRITICAL                    │
│  Feature Chains - validates multi-capability workflows          │
├─────────────────────────────────────────────────────────────────┤
│  FINALIZATION (Phases 20-21) - ALWAYS LAST                      │
│  Data Export verification, Final Cleanup via MCP                │
└─────────────────────────────────────────────────────────────────┘
```

### Execution Steps

1. **Generate test data** first: `cd tests/uat/data/scripts && ./generate-test-data.sh`
2. **Phase 0** validates system readiness
3. **Phase 1** creates seed data required by subsequent phases
4. **Phases 2-19** (including 2b, 2c, 3b) execute feature tests in order
5. **Phase 20** (Data Export) tests backup/export functionality
6. **Phase 21** (Final Cleanup) MUST run LAST - uses MCP tools to remove all test data

### No Test Skipping

Every test must be executed regardless of upstream failures. If Phase 2b attachment uploads fail, still execute Phase 2c and Phase 3b — the cascading failures reveal the true blast radius and each failure should be recorded and filed as a Gitea issue. Do not mark tests as BLOCKED or skip them.

### Partial Execution (Time-Constrained)

If running a subset, always include:
- **Start**: Phases 0, 1 (foundation)
- **Core**: Phases 2, 2b, 2c, 3, 3b (critical)
- **End**: Phase 21 (cleanup - ALWAYS LAST)

---

## Success Criteria

- **All Phases (0-21, including 2b, 2c, 3b)**: 100% pass required for release approval
- **Overall**: 100% pass rate for release approval
- **No skipping**: Every test must be executed. Failures are recorded and filed as issues — the dev team resolves them. Do not mark tests as BLOCKED or skip them due to upstream failures.
- **Test data**: Must be generated before execution (see Test Data section)

---

## Test Data

### Comprehensive Test Data Package (`../data/`)

The primary test data for UAT lives in `tests/uat/data/` with 44+ files organized by capability:

| Directory | Files | Purpose |
|-----------|-------|---------|
| `data/images/` | 7 | JPEG with EXIF/GPS, PNG, WebP, unicode filenames |
| `data/provenance/` | 7 | GPS-tagged photos (Paris, NYC, Tokyo), dated images, dedup pairs |
| `data/multilingual/` | 13 | Text in 13 languages (EN/DE/FR/ES/PT/RU + CJK + AR/EL/HE + emoji) |
| `data/documents/` | 8 | Code (Python, Rust, JS, TS), Markdown, JSON, YAML, CSV |
| `data/audio/` | 3 | Speech samples (English, Spanish, Chinese) |
| `data/edge-cases/` | 6 | Empty, 100KB, binary mismatch, unicode filename, malformed |

**Supplementary Test Media** (`/mnt/global/test-media/`):

| Directory | Files | Purpose |
|-----------|-------|---------|
| `video/` | 17 MP4 + 2 WebM + 1 OGV | Real CC-licensed videos (3-11MB clips + 4 full-length) |
| `audio/` | 10 | Real CC-licensed MP3 audio (radio dramas, speeches, lectures) |
| `documents/` | 22 | Real CC-licensed PDFs (tax forms, papers, invoices, letters) |
| `3d-models/` | 10 | Real CC-licensed GLB 3D models (Khronos samples, 1.7KB-12MB) |

**Setup**: Generate test data before running UAT:
```bash
cd tests/uat/data/scripts
./generate-test-data.sh
```

See `tests/uat/data/README.md` for full documentation, `MANIFEST.md` for file specs, and `QUICKSTART.md` for rapid testing.

### Phase-Specific Test Data Usage

| Phase | Test Data Files Used |
|-------|---------------------|
| **2b (Attachments)** | `data/images/jpeg-with-exif.jpg`, `data/documents/code-python.py`, `data/edge-cases/binary-wrong-ext.jpg` |
| **2c (Processing)** | `data/documents/code-python.py`, `data/documents/pdf-single-page.pdf`, `data/documents/markdown-formatted.md`, `data/documents/json-config.json`, `data/documents/csv-data.csv`, `data/documents/code-rust.rs`, `data/edge-cases/empty.txt`, `data/edge-cases/binary-wrong-ext.jpg`, `data/images/jpeg-with-exif.jpg`, `data/audio/english-speech-5s.mp3`, `data/multilingual/english.txt` |
| **3 (Search)** | `data/multilingual/*.txt`, `data/multilingual/emoji-heavy.txt` |
| **3b (Memory Search)** | `data/provenance/paris-eiffel-tower.jpg`, `data/provenance/dated-*.jpg` |
| **8 (Document Types)** | `data/documents/code-*.{py,rs,js,ts}`, `data/documents/markdown-formatted.md` |
| **9 (Edge Cases)** | `data/edge-cases/empty.txt`, `data/edge-cases/large-text-100kb.txt`, `data/edge-cases/unicode-filename-测试.txt` |
| **19 (Feature Chains)** | All directories - each chain uses specific test data files |

### Legacy Fixtures (`../fixtures/`)

Seed data for Phase 1 bulk import:

- `seed-notes.json` - Seed notes for bulk import
- `test-concepts.json` - SKOS concepts for taxonomy testing
- `sample-image.png` - 1x1 PNG for basic attachment testing
- `sample-code.rs` - Rust source for type detection
- `sample-config.json` - JSON configuration sample
- `sample-template.md` - Template with placeholders

---

## Execution Modes

### Quick Smoke Test (~25 min)
Phases: 0, 2 (subset), 3 (subset), 17 (subset), 19 (chains 1-2 only), 21

### Standard Suite (~120 min)
Phases: 0-9, 17, 19, 20, 21

### Full Suite (~240 min)
Phases: 0-21 (all phases in order)

---

## For Agentic Execution

Each phase document is self-contained with:
- Clear test IDs (e.g., `CRUD-001`, `PROC-001`, `SEARCH-015`, `AUTH-022`, `CACHE-015`, `CHAIN-001`)
- Exact MCP tool calls in JavaScript format (curl only for OAuth infrastructure validation)
- Pass criteria for each test
- Phase summary table for tracking
- Dependencies listed in Prerequisites

### Agent Execution Rules

Agents MUST:
1. **Use MCP tools for ALL tests** — never fall back to curl or direct HTTP API calls for operations available as MCP tools
2. **If an MCP tool fails, file a bug issue** — do NOT work around it by calling the API directly. The MCP failure is a UAT finding, not a reason to bypass MCP.
3. **Only use curl/SQL for**: file upload/download (binary data) and OAuth infrastructure (Phase 17 Part B) — these are the ONLY approved exceptions. Provenance data is now created via MCP tools.
4. Execute tests sequentially within each phase
5. Record results in the phase summary table
6. **Always proceed to the next phase** — never skip phases or tests due to upstream failures. If a prerequisite test failed, still attempt the dependent test and record what happens.
7. **Execute ALL 25 phases (0-21, including sub-phases)** - do not stop early
8. **Phase 21 (Final Cleanup) is MANDATORY** and runs LAST
9. **File a Gitea issue for every failure** — tag with `bug` and `mcp`, include reproduction steps. The dev team resolves failures; the executor's job is to run tests and report results.

### Negative Test Isolation Protocol

Tests marked with `**Isolation**: Required` expect an error response from the MCP server. These MUST be executed as standalone, single MCP calls — never batched with other tool calls in the same message.

**Why**: Claude Code protects against cascading failures with a "sibling call error" mechanism. When multiple MCP calls are sent in a single turn and one returns an error, the others are automatically failed. Negative tests deliberately trigger errors, so batching them with positive tests causes false failures.

**Rules**:
1. When you encounter `**Isolation**: Required`, issue that MCP call **alone** in its own turn
2. Evaluate the result against the stated pass criteria (the "error" IS the expected outcome)
3. After the isolated call completes, resume normal testing in the next turn
4. Dual-path tests (marked `**Isolation**: Recommended`) may succeed or fail — isolate to be safe

**Visual scan**: Search each phase for `**Isolation**:` before starting execution to plan your batching strategy.

### Anti-Termination Checklist

Before declaring UAT complete, verify:
- [ ] Phase 19 (Feature Chains) executed with 48 tests
- [ ] Phase 20 (Data Export) tested backup functionality
- [ ] Phase 21 (Final Cleanup) removed all UAT test data
- [ ] Final report includes all 25 phases

---

## Version History

- **2026.2.11**: Removed provenance SQL exception — Phase 3B now uses MCP tools (`create_provenance_location`, `create_named_location`, `create_provenance_device`, `create_file_provenance`) for all provenance test data setup (#261). Updated Phase 19 Chain 2 note. Reduced approved exceptions from 3 to 2.
- **2026.2.10**: Reconciled test counts (448→459 across 25 phases). Added provenance SQL setup as third approved MCP-first exception (Phase 3B, tracked in #261). Fixed CHAIN-005 version parameter inconsistency (version_id:0 → version:1, matching Phase 11 spec). Restructured Phase 19 Chain 2 to use actual provenance path (GPS-tagged photo → EXIF extraction) instead of unsupported inline metadata.location.
- **2026.2.9**: Removed all skip/gate/BLOCKED logic — every test must execute, failures get filed as issues. Fixed test specs: CHAIN-001 upload pattern, VER-011 escape hatch, OBS-007 pass criteria, UAT-2B-019 raw SQL. Removed "Conditional Pass" from report template. Added Gitea issue tracking to report template.
- **2026.2.7**: Enforced MCP-first testing philosophy across entire UAT suite. Rewrote Phase 17 (OAuth) from curl-only to MCP-first with 13 agent-perspective tests + 4 infrastructure tests. Rewrote Phase 18 (Caching) from curl-only to 100% MCP tool calls. Added MCP-first principle statement to README. Eliminated API fallbacks for all operations available as MCP tools.
- **2026.2.6**: Added Phase 2C (Attachment Processing Pipeline) with 31 tests covering document type auto-detection on upload, extraction strategy assignment, user-supplied overrides, multi-file notes, content extraction, job queue integration, and end-to-end pipeline verification
- **2026.2.5**: Reordered phases - moved Data Export (20) and Final Cleanup (21) to END of suite to prevent agentic early termination; renumbered Templates→Jobs from 10-15, Observability→Feature Chains from 16-19
- **2026.2.2**: Added Phase 21 (Feature Chains) with 48 end-to-end test steps across 8 chains; comprehensive test data package (44 files, 1.8MB) with EXIF images, multilingual text, code samples, audio, and edge cases; test data generation scripts with venv support
- **2026.2.2**: Added Phase 19 (OAuth & Authentication) with 22 new tests and Phase 20 (Caching & Performance) with 15 new tests
- **2026.2.2**: Added Phase 18 (Observability) with 12 new tests for knowledge health and timeline tools
- **2026.2.2**: Expanded Phases 6, 7, 15, 17 with backlinks, provenance, embedding config, job management, and SKOS collection tools (28 new tools total)
- **2026.2.2**: Added Phases 12-17 (Templates, Versioning, Archives, SKOS, PKE, Jobs) - 113 new tests
- **2026.2.2**: Expanded Phases 6, 7, 10 with additional tool coverage
- **2026.2.2**: Added test fixtures directory with sample data
- **2026.2.2**: Added Phase 2B (File Attachments) and Phase 3B (Memory Search)
- **2026.1.12**: Added Phase 8 (Document Types) with 16 new tests
- **2026.1.10**: Split monolithic UAT into phase documents
- **2026.1.0**: Initial UAT document

# UAT Phase Documents

This directory contains phase-based UAT test procedures for Matric Memory, designed for efficient agentic execution.

## Phase Overview

| Phase | Document | Duration | Tests | Critical |
|-------|----------|----------|-------|----------|
| 0 | [Pre-flight Checks](phase-0-preflight.md) | ~2 min | 3 | Yes |
| 1 | [Seed Data Generation](phase-1-seed-data.md) | ~5 min | 15 | Yes |
| 2 | [CRUD Operations](phase-2-crud.md) | ~10 min | 17 | **Yes** |
| 2b | [File Attachments](phase-2b-file-attachments.md) | ~15 min | 21 | **Yes** |
| 3 | [Search Capabilities](phase-3-search.md) | ~10 min | 14 | **Yes** |
| 3b | [Memory Search](phase-3b-memory-search.md) | ~15 min | 21 | **Yes** |
| 4 | [Tag System](phase-4-tags.md) | ~5 min | 3 | No |
| 5 | [Collections](phase-5-collections.md) | ~3 min | 3 | No |
| 6 | [Semantic Links](phase-6-links.md) | ~5 min | 11 | No |
| 7 | [Embeddings](phase-7-embeddings.md) | ~5 min | 15 | No |
| 8 | [Document Types](phase-8-document-types.md) | ~5 min | 16 | No |
| 9 | [Edge Cases](phase-9-edge-cases.md) | ~5 min | 3 | No |
| 10 | [Backup & Export](phase-10-backup.md) | ~8 min | 19 | No |
| 11 | [Cleanup](phase-11-cleanup.md) | ~2 min | 1 | Yes |
| 12 | [Templates](phase-12-templates.md) | ~8 min | 15 | No |
| 13 | [Versioning](phase-13-versioning.md) | ~7 min | 15 | No |
| 14 | [Archives](phase-14-archives.md) | ~8 min | 18 | No |
| 15 | [SKOS Taxonomy](phase-15-skos.md) | ~12 min | 27 | No |
| 16 | [PKE Encryption](phase-16-pke.md) | ~8 min | 20 | No |
| 17 | [Jobs & Queue](phase-17-jobs.md) | ~8 min | 22 | No |
| 18 | [Observability](phase-18-observability.md) | ~10 min | 12 | No |
| 19 | [OAuth & Authentication](phase-19-oauth-auth.md) | ~12 min | 22 | **Yes** |
| 20 | [Caching & Performance](phase-20-caching-performance.md) | ~10 min | 15 | No |
| 21 | [Feature Chains (E2E)](phase-21-feature-chains.md) | ~30 min | 48 | **Yes** |

**Total Tests**: ~420+
**Total Estimated Duration**: 200-240 minutes (full suite)

## MCP Tool Coverage Summary

| Category | Tools | UAT Tests | Coverage |
|----------|-------|-----------|----------|
| Note CRUD | 12 | 38 | 100% |
| Search | 4 | 17 | 100% |
| Tags | 2 | 6 | 100% |
| Collections | 8 | 8 | 100% |
| Templates | 6 | 15 | 100% |
| Embedding Sets | 15 | 20 | 100% |
| Versioning | 5 | 15 | 100% |
| Graph/Links | 7 | 13 | 100% |
| Jobs | 7 | 22 | 100% |
| SKOS | 33 | 40 | 100% |
| Archives | 7 | 18 | 100% |
| Document Types | 6 | 16 | 100% |
| Backup | 17 | 19 | 100% |
| PKE | 13 | 20 | 100% |
| Observability | 7 | 12 | 100% |
| OAuth/Auth | 9 API endpoints | 22 | 100% |
| Caching | 6 API endpoints | 15 | 100% |
| **TOTAL** | **148+** | **370+** | **100%** |

## Execution Order

Phases must be executed in order:

0. **Generate test data** first: `cd tests/uat/data/scripts && ./generate-test-data.sh`
1. **Phase 0** validates system readiness
2. **Phase 1** creates seed data required by subsequent phases
3. **Phases 2-21** can be run partially if time-constrained:
   - **Core (2-3, 2b, 3b, 19)**: CRUD, Search, Attachments, Memory Search, Auth - always run
   - **Extended (4-11, 20)**: Tags, Collections, Links, Embeddings, Types, Edge Cases, Backup, Caching
   - **Advanced (12-18)**: Templates, Versioning, Archives, SKOS, PKE, Jobs, Observability
   - **End-to-End (21)**: Feature Chains - validates multi-capability workflows
4. **Phase 11** must always run to clean up test data

## Success Criteria

- **Critical Phases (0-3, 2b, 3b, 19, 21)**: 100% pass required for release approval
- **Standard Phases (4-18, 20)**: 90% pass acceptable
- **Overall**: 95% pass rate for release approval
- **Test data**: Must be generated before execution (see Test Data section)

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
| **3 (Search)** | `data/multilingual/*.txt`, `data/multilingual/emoji-heavy.txt` |
| **3b (Memory Search)** | `data/provenance/paris-eiffel-tower.jpg`, `data/provenance/dated-*.jpg` |
| **8 (Document Types)** | `data/documents/code-*.{py,rs,js,ts}`, `data/documents/markdown-formatted.md` |
| **9 (Edge Cases)** | `data/edge-cases/empty.txt`, `data/edge-cases/large-text-100kb.txt`, `data/edge-cases/unicode-filename-测试.txt` |
| **21 (Feature Chains)** | All directories - each chain uses specific test data files |

### Legacy Fixtures (`../fixtures/`)

Seed data for Phase 1 bulk import:

- `seed-notes.json` - Seed notes for bulk import
- `test-concepts.json` - SKOS concepts for taxonomy testing
- `sample-image.png` - 1x1 PNG for basic attachment testing
- `sample-code.rs` - Rust source for type detection
- `sample-config.json` - JSON configuration sample
- `sample-template.md` - Template with placeholders

## Execution Modes

### Quick Smoke Test (~25 min)
Phases: 0, 2 (subset), 3 (subset), 19 (subset), 21 (chains 1-2 only), 11

### Standard Suite (~120 min)
Phases: 0-11, 19, 21

### Full Suite (~240 min)
Phases: 0-21 (all phases)

## For Agentic Execution

Each phase document is self-contained with:
- Clear test IDs (e.g., `CRUD-001`, `SEARCH-015`, `AUTH-022`, `CACHE-015`, `CHAIN-001`)
- Exact tool calls in JavaScript format or curl commands
- Pass criteria for each test
- Phase summary table for tracking
- Dependencies listed in Prerequisites

Agents should:
1. Execute tests sequentially within each phase
2. Record results in the phase summary table
3. Proceed to next phase only if prerequisites met
4. Always complete Phase 11 cleanup

## Version History

- **2026.2.0**: Added Phase 21 (Feature Chains) with 48 end-to-end test steps across 8 chains; comprehensive test data package (44 files, 1.8MB) with EXIF images, multilingual text, code samples, audio, and edge cases; test data generation scripts with venv support
- **2026.2.0**: Added Phase 19 (OAuth & Authentication) with 22 new tests and Phase 20 (Caching & Performance) with 15 new tests
- **2026.2.2**: Added Phase 18 (Observability) with 12 new tests for knowledge health and timeline tools
- **2026.2.2**: Expanded Phases 6, 7, 15, 17 with backlinks, provenance, embedding config, job management, and SKOS collection tools (28 new tools total)
- **2026.2.2**: Added Phases 12-17 (Templates, Versioning, Archives, SKOS, PKE, Jobs) - 113 new tests
- **2026.2.2**: Expanded Phases 6, 7, 10 with additional tool coverage
- **2026.2.2**: Added test fixtures directory with sample data
- **2026.2.2**: Added Phase 2B (File Attachments) and Phase 3B (Memory Search)
- **2026.1.12**: Added Phase 8 (Document Types) with 16 new tests
- **2026.1.10**: Split monolithic UAT into phase documents
- **2026.1.0**: Initial UAT document

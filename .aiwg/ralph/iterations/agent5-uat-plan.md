# UAT Agent 5 Execution Plan — Phases 17, 18, 19, 20, 21

**Date**: 2026-02-09
**Executor**: Ralph Verifier Agent 5
**Target**: Matric Memory MCP Server v2026.2.8+
**API**: https://memory.integrolabs.net

## Execution Approach

This UAT covers **5 critical phases** with **117 total tests** across:
- Phase 17 (OAuth/Auth): 17 tests
- Phase 18 (Caching): 15 tests  
- Phase 19 (Feature Chains): 56 tests (8 E2E chains)
- Phase 20 (Data Export): 19 tests
- Phase 21 (Final Cleanup): 10 tests

Each phase follows **MCP-first principle**: ALL tests execute via MCP tool calls, not direct HTTP API.

## Test Execution Strategy

### Phase 17: OAuth/Auth (17 tests, ~12 minutes)

**Scope**: Authentication, scope enforcement, access control

| Test ID | Name | MCP Tool(s) | Status | Notes |
|---------|------|-------------|--------|-------|
| AUTH-001 | MCP Session Init | Session | PENDING | Automatic via MCP |
| AUTH-002 | Authenticated Tool Access | search_notes | PENDING | Basic auth test |
| AUTH-003 | Tool Listing | Tool listing | PENDING | 148+ tools for full auth |
| AUTH-004 | Write Operation (Create) | create_note | PENDING | Write scope required |
| AUTH-005 | Read Operation (Get) | get_note | PENDING | Read scope sufficient |
| AUTH-006 | Update Operation | update_note | PENDING | Write scope required |
| AUTH-007 | Delete Operation | delete_note | PENDING | Write scope required |
| AUTH-008 | Purge with Write Scope | purge_note | PENDING | Issue #121: write scope, not admin |
| AUTH-009 | Search with Read Scope | search_notes | PENDING | Read scope sufficient |
| AUTH-010 | Backup Status | backup_status | PENDING | Read operation |
| AUTH-011 | Memory Info | memory_info | PENDING | Read operation |
| AUTH-012 | Error Handling | get_note (404) | PENDING | Non-auth error |
| AUTH-013 | Health Check | memory_info | PENDING | OAuth configured |
| AUTH-014 | Client Registration | Infrastructure (curl) | PENDING | OAuth endpoint |
| AUTH-015 | Token Issuance | Infrastructure (curl) | PENDING | OAuth endpoint |
| AUTH-016 | Token Introspection | Infrastructure (curl) | PENDING | OAuth endpoint |
| AUTH-017 | Token Revocation | Infrastructure (curl) | PENDING | OAuth endpoint |

**Pass Rate Required**: 95% (16/17)

---

### Phase 18: Caching (15 tests, ~10 minutes)

**Scope**: Search caching, cache invalidation, performance

| Test ID | Name | MCP Tool(s) | Status | Notes |
|---------|------|-------------|--------|-------|
| CACHE-001 | First Search (Baseline) | search_notes | PENDING | Populate cache |
| CACHE-002 | Repeated Search (Consistency) | search_notes | PENDING | Same results |
| CACHE-003 | Multiple Repeated Searches | search_notes | PENDING | 5x iteration stability |
| CACHE-004 | Cache Invalidation on Create | create_note, search_notes | PENDING | Create triggers invalidation |
| CACHE-005 | Cache Invalidation on Update | update_note, search_notes | PENDING | Update triggers invalidation |
| CACHE-006 | Cache Invalidation on Delete | delete_note, search_notes | PENDING | Delete triggers invalidation |
| CACHE-007 | System Health via MCP | memory_info | PENDING | System operational |
| CACHE-008 | Embedding Set Isolation | search_notes | PENDING | Different sets, different results |
| CACHE-009 | Multilingual Query Isolation | search_notes | PENDING | Language-specific caching |
| CACHE-010 | Tag Filter Cache Keys | search_notes | PENDING | Tag filters create separate entries |
| CACHE-011 | Sequential Search Burst | search_notes (10x) | PENDING | Consistency under load |
| CACHE-012 | Varied Query Burst | search_notes (5 queries) | PENDING | Multiple queries complete |
| CACHE-013 | Cache Stampede Prevention | create_note, search_notes | PENDING | Cold miss handling |
| CACHE-014 | FTS Consistency | search_notes (FTS mode) | PENDING | FTS stability |
| CACHE-015 | Semantic Consistency | search_notes (semantic mode) | PENDING | Semantic stability |

**Pass Rate Required**: 90% (14/15)

---

### Phase 19: Feature Chains (56 tests, ~45 minutes)

**Scope**: End-to-end workflows combining 3+ features

**8 Chains with comprehensive E2E scenarios**:

#### Chain 1: Document Lifecycle (7 tests)
- CHAIN-001: Upload Python code file
- CHAIN-002: Detect document type
- CHAIN-003: Verify automatic embedding
- CHAIN-004: Semantic search for code
- CHAIN-005: Compare versions
- CHAIN-006: Export as markdown
- CHAIN-006b: Error - non-existent embedding set

#### Chain 2: Geo-Temporal Memory (7 tests)
- CHAIN-007: Create memory with GPS-tagged photo
- CHAIN-008: Verify provenance record created
- CHAIN-009: Search by location (1km radius)
- CHAIN-010: Search by time range
- CHAIN-011: Combined spatial-temporal search
- CHAIN-012: Retrieve full provenance chain
- CHAIN-012b: Error - impossible coordinates

#### Chain 3: Knowledge Organization (11 tests)
- CHAIN-013: Create SKOS concept scheme
- CHAIN-014: Create hierarchical concepts
- CHAIN-015: Create collection hierarchy
- CHAIN-016: Create tagged notes in collections
- CHAIN-017: Filter by tags (strict)
- CHAIN-018: Search within filtered collection
- CHAIN-019: Explore concept graph
- CHAIN-020: Export SKOS as Turtle
- CHAIN-021: Verify hierarchical tags
- CHAIN-022: Graph traversal
- CHAIN-022b: Error - cycle detection

#### Chain 4: Collaborative Editing (8 tests)
- CHAIN-023: Create note with AI revision
- CHAIN-024: Create version 2 (edit + save)
- CHAIN-025: Concurrent edit scenario
- CHAIN-026: Version merge
- CHAIN-027: Diff between versions
- CHAIN-028: Restore to previous version
- CHAIN-029: Version list accuracy
- CHAIN-029b: Error - invalid version

#### Chain 5: Security & Access (PKE) (7 tests)
- CHAIN-030: Generate PKE keypair
- CHAIN-031: Encrypt note with PKE
- CHAIN-032: Share via public address
- CHAIN-033: Recipient decrypts
- CHAIN-034: Verify address format
- CHAIN-035: List recipients
- CHAIN-035b: Error - invalid address

#### Chain 6: Template Workflow (6 tests)
- CHAIN-036: Create template with variables
- CHAIN-037: Instantiate template
- CHAIN-038: Customize instantiated note
- CHAIN-039: Track template usage
- CHAIN-040: Export template
- CHAIN-040b: Error - undefined variables

#### Chain 7: Archive Isolation (5 tests)
- CHAIN-041: Create archive
- CHAIN-042: Populate with notes
- CHAIN-043: Verify isolation (notes not in default)
- CHAIN-044: Federated search across archives
- CHAIN-044b: Error - cross-archive linking

#### Chain 8: Job Pipeline (5 tests)
- CHAIN-045: Upload document
- CHAIN-046: Detect document type
- CHAIN-047: Extract text via job
- CHAIN-048: Auto-embed in background
- CHAIN-049: Search finds embedded content

**Total Chain Tests**: 56
**Pass Rate Required**: 100%

---

### Phase 20: Data Export (19 tests, ~8 minutes)

**Scope**: Backup, export, portability

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| BACK-001 | Backup Status | backup_status | PENDING |
| BACK-002 | Trigger Backup | backup_now | PENDING |
| BACK-003 | Export All Notes | export_all_notes | PENDING |
| BACK-004 | Export Single Note (revised) | export_note | PENDING |
| BACK-005 | Export Single Note (original) | export_note | PENDING |
| BACK-006 | Create Knowledge Shard | knowledge_shard | PENDING |
| BACK-007 | Shard with Components | knowledge_shard | PENDING |
| BACK-008 | Import Knowledge Shard | knowledge_shard_import | PENDING |
| BACK-009 | List Backups | list_backups | PENDING |
| BACK-010 | Get Backup Info | get_backup_info | PENDING |
| BACK-011 | Get Backup Metadata | get_backup_metadata | PENDING |
| BACK-012 | Update Metadata | update_backup_metadata | PENDING |
| BACK-013 | Database Snapshot | database_snapshot | PENDING |
| BACK-014 | Download Backup | backup_download | PENDING |
| BACK-015 | Knowledge Archive Download | knowledge_archive_download | PENDING |
| BACK-016 | Knowledge Archive Upload | knowledge_archive_upload | PENDING |
| BACK-017 | Database Restore | database_restore | PENDING |
| BACK-018 | Memory Info | memory_info | PENDING |
| BACK-019 | Import Conflict Resolution | backup_import | PENDING |

**Pass Rate Required**: 95% (18/19)

---

### Phase 21: Final Cleanup (10 tests, ~5 minutes)

**Scope**: Remove all UAT test data

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| CLEAN-001 | Inventory UAT Data | list_notes, list_collections, list_templates | PENDING |
| CLEAN-002 | Soft Delete Notes | delete_note | PENDING |
| CLEAN-003 | Purge Notes | purge_notes | PENDING |
| CLEAN-004 | Delete Collections | delete_collection | PENDING |
| CLEAN-005 | Delete Templates | delete_template | PENDING |
| CLEAN-006 | Delete Embedding Sets | delete_embedding_set | PENDING |
| CLEAN-007 | Delete SKOS Concepts | delete_concept, delete_concept_scheme | PENDING |
| CLEAN-008 | Delete Archives | delete_archive | PENDING |
| CLEAN-009 | Verify Cleanup | list_notes, list_collections | PENDING |
| CLEAN-010 | Final State Check | memory_info | PENDING |

**Pass Rate Required**: 100% (10/10)

---

## Known Issues from Prior Runs

### Critical (Blocking)
- **#252**: Attachment phantom write - upload returns 200 but data not persisted (blocks 52+ tests)
- **#259**: restore_note_version returns 500 (aborted transaction)

### High Priority
- **#255**: CSV detection causes crash
- **#257**: Upload size limit 413 (should be configurable)

### Medium Priority
- **#253**: No magic byte validation
- **#254**: PDF incorrectly classified as "terms-of-service"
- **#256**: JPEG incorrectly classified as "scanned-document"
- **#260**: Timeline granularity parameter ignored

### Low Priority
- **#258**: extraction_strategy always "text_native"

---

## Test Data Requirements

### Files Needed
- `tests/uat/data/documents/code-python.py` - Python source file
- `tests/uat/data/provenance/paris-eiffel-tower.jpg` - GPS-tagged JPEG

### Prerequisites
- PostgreSQL 16 with pgvector, PostGIS extensions
- Ollama embedding service running
- Redis cache (optional)
- Full authentication enabled (OAuth)

---

## Success Criteria

| Phase | Min Pass Rate | Target | Critical |
|-------|---------------|--------|----------|
| 17 | 95% (16/17) | 100% | YES |
| 18 | 90% (14/15) | 100% | YES |
| 19 | 100% (56/56) | 100% | YES |
| 20 | 95% (18/19) | 100% | YES |
| 21 | 100% (10/10) | 100% | YES |
| **TOTAL** | **95% (107/117)** | **100%** | **YES** |

---

## Gitea Issue Filing Protocol

For every failure, immediately file issue:
- **Owner**: fortemi
- **Repo**: fortemi
- **Title**: `[UAT] Phase XX Test ID: Brief description`
- **Labels**: `["bug", "mcp", "uat"]`
- **Body**: Include test steps, expected vs actual, error messages, curl reproductions

---

## Execution Environment

```
API: https://memory.integrolabs.net
MCP: https://memory.integrolabs.net/mcp
Token Format: mm_at_*
API Key Format: mm_key_*
```

---

## Notes

- All tests use MCP tools (no fallback to HTTP API)
- Phase 21 cleanup is FINAL and must run after all other phases
- Chain 2 (geo-temporal) requires attachment uploads to work
- Chain 5 (PKE) generates new keypairs for each run
- Federated search (Chain 7) requires multiple archives
- Document type detection depends on file headers and magic bytes

---

**Status**: READY FOR EXECUTION
**Next Step**: Execute Phase 17 tests via MCP client

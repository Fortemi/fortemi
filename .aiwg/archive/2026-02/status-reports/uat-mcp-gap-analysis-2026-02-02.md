# UAT & MCP Gap Analysis Report

**Generated**: 2026-02-02
**Purpose**: Ensure MCP capabilities align with API capabilities and UAT coverage is complete

---

## Executive Summary

| Metric | Count | Status |
|--------|-------|--------|
| Total MCP Tools | 117 | ✓ Cataloged |
| Total API Endpoints | 95+ | ✓ Cataloged |
| MCP Tools NOT Tested in UAT | 47 | ⚠️ GAP |
| API Endpoints NOT in MCP | 24 | ⚠️ GAP |
| UAT Phases | 13 | ✓ Documented |
| MCP Annotation Issues | 4 | ⚠️ GAP |

**Overall Assessment**: Significant gaps exist between API capabilities, MCP exposure, and UAT coverage. The system orchestrates its own work, but consumers cannot leverage several API features through MCP.

---

## Part 1: API Endpoints NOT Exposed via MCP

These API endpoints exist but have no corresponding MCP tool:

### 1.1 Critical Gaps (Core Functionality)

| API Endpoint | Method | Description | Priority |
|--------------|--------|-------------|----------|
| `/api/v1/notes/:id/backlinks` | GET | Dedicated backlinks endpoint | HIGH |
| `/api/v1/notes/:id/provenance` | GET | W3C PROV derivation chain | HIGH |
| `/api/v1/notes/:id/reprocess` | POST | Manual NLP pipeline trigger | HIGH |
| `/api/v1/notes/:id/status` | PATCH | Update starred/archived flags | MEDIUM |
| `/api/v1/jobs/:id` | GET | Get individual job details | HIGH |
| `/api/v1/jobs/pending` | GET | Pending jobs count | MEDIUM |

### 1.2 SKOS/Taxonomy Gaps

| API Endpoint | Method | Description | Priority |
|--------------|--------|-------------|----------|
| `/api/v1/skos/collections` | GET | List SKOS collections | HIGH |
| `/api/v1/skos/collections` | POST | Create SKOS collection | HIGH |
| `/api/v1/skos/collections/:id` | GET | Get SKOS collection | HIGH |
| `/api/v1/skos/collections/:id` | PATCH | Update SKOS collection | MEDIUM |
| `/api/v1/skos/collections/:id` | DELETE | Delete SKOS collection | MEDIUM |
| `/api/v1/skos/collections/:id/members` | POST | Add collection member | HIGH |
| `/api/v1/skos/collections/:id/members/:mid` | DELETE | Remove collection member | MEDIUM |
| `/api/v1/skos/export/turtle` | GET | Export as W3C RDF/Turtle | MEDIUM |
| `/api/v1/concepts/:id/broader/:bid` | DELETE | Remove broader relation | HIGH |
| `/api/v1/concepts/:id/narrower/:nid` | DELETE | Remove narrower relation | HIGH |
| `/api/v1/concepts/:id/related/:rid` | DELETE | Remove related relation | HIGH |

### 1.3 Analytics & Health Gaps

| API Endpoint | Method | Description | Priority |
|--------------|--------|-------------|----------|
| `/api/v1/notes/timeline` | GET | Bucketed note creation timeline | MEDIUM |
| `/api/v1/notes/activity` | GET | Recent activity feed | MEDIUM |
| `/api/v1/health/knowledge` | GET | Overall KB health score | HIGH |
| `/api/v1/health/orphan-tags` | GET | Tags used only once | MEDIUM |
| `/api/v1/health/stale-notes` | GET | Notes not updated in N days | MEDIUM |
| `/api/v1/health/unlinked-notes` | GET | Notes with no links | MEDIUM |
| `/api/v1/health/tag-cooccurrence` | GET | Tag correlation matrix | LOW |

### 1.4 Infrastructure Gaps

| API Endpoint | Method | Description | Priority |
|--------------|--------|-------------|----------|
| `/api/v1/rate-limit/status` | GET | Rate limiting status | LOW |
| `/api/v1/api-keys` | GET/POST/DELETE | API key management | MEDIUM |
| `/api/v1/embedding-configs` | GET/POST/PATCH/DELETE | Embedding model configs | MEDIUM |
| `/openapi.yaml` | GET | OpenAPI spec document | LOW |

### 1.5 OAuth Endpoints (May be intentionally excluded)

| API Endpoint | Method | Description | Priority |
|--------------|--------|-------------|----------|
| `/.well-known/oauth-authorization-server` | GET | OIDC discovery | N/A |
| `/.well-known/oauth-protected-resource` | GET | Resource metadata | N/A |
| `/oauth/authorize` | GET/POST | Authorization code flow | N/A |
| `/oauth/register` | POST | Dynamic client registration | N/A |
| `/oauth/token` | POST | Token issuance | N/A |
| `/oauth/introspect` | POST | Token introspection | N/A |
| `/oauth/revoke` | POST | Token revocation | N/A |

**Note**: OAuth endpoints may be intentionally excluded from MCP as they're infrastructure, not user-facing features.

---

## Part 2: MCP Tools NOT Covered in UAT

These MCP tools exist but have no explicit UAT test case:

### 2.1 Job Management (4 tools, 0 UAT tests)

| MCP Tool | UAT Status |
|----------|------------|
| `create_job` | ❌ Not tested |
| `list_jobs` | ❌ Not tested |
| `get_queue_stats` | ❌ Not tested |
| `reembed_all` | ❌ Not tested |

### 2.2 Templates (6 tools, 0 UAT tests)

| MCP Tool | UAT Status |
|----------|------------|
| `list_templates` | ❌ Not tested |
| `create_template` | ❌ Not tested |
| `get_template` | ❌ Not tested |
| `update_template` | ❌ Not tested |
| `delete_template` | ❌ Not tested |
| `instantiate_template` | ❌ Not tested |

### 2.3 Versioning (5 tools, 0 UAT tests)

| MCP Tool | UAT Status |
|----------|------------|
| `list_note_versions` | ❌ Not tested |
| `get_note_version` | ❌ Not tested |
| `restore_note_version` | ❌ Not tested |
| `delete_note_version` | ❌ Not tested |
| `diff_note_versions` | ❌ Not tested |

### 2.4 Archives (7 tools, 0 UAT tests)

| MCP Tool | UAT Status |
|----------|------------|
| `list_archives` | ❌ Not tested |
| `create_archive` | ❌ Not tested |
| `get_archive` | ❌ Not tested |
| `update_archive` | ❌ Not tested |
| `delete_archive` | ❌ Not tested |
| `set_default_archive` | ❌ Not tested |
| `get_archive_stats` | ❌ Not tested |

### 2.5 Advanced SKOS (12 tools, partial coverage)

| MCP Tool | UAT Status |
|----------|------------|
| `list_concept_schemes` | ⚠️ Partial (implicit) |
| `create_concept_scheme` | ❌ Not tested |
| `get_concept_scheme` | ❌ Not tested |
| `delete_concept_scheme` | ❌ Not tested |
| `create_concept` | ❌ Not tested |
| `get_concept` | ❌ Not tested |
| `get_concept_full` | ❌ Not tested |
| `update_concept` | ❌ Not tested |
| `delete_concept` | ❌ Not tested |
| `autocomplete_concepts` | ❌ Not tested |
| `get_governance_stats` | ❌ Not tested |
| `get_top_concepts` | ❌ Not tested |

### 2.6 PKE Encryption (13 tools, 0 UAT tests)

| MCP Tool | UAT Status |
|----------|------------|
| `pke_generate_keypair` | ❌ Not tested |
| `pke_get_address` | ❌ Not tested |
| `pke_encrypt` | ❌ Not tested |
| `pke_decrypt` | ❌ Not tested |
| `pke_list_recipients` | ❌ Not tested |
| `pke_verify_address` | ❌ Not tested |
| `pke_list_keysets` | ❌ Not tested |
| `pke_create_keyset` | ❌ Not tested |
| `pke_get_active_keyset` | ❌ Not tested |
| `pke_set_active_keyset` | ❌ Not tested |
| `pke_export_keyset` | ❌ Not tested |
| `pke_import_keyset` | ❌ Not tested |
| `pke_delete_keyset` | ❌ Not tested |

### 2.7 Advanced Backup (12 tools, 2 UAT tests)

| MCP Tool | UAT Status |
|----------|------------|
| `backup_status` | ✓ BACK-001 |
| `export_all_notes` | ✓ BACK-002 |
| `backup_now` | ❌ Not tested |
| `backup_download` | ❌ Not tested |
| `backup_import` | ❌ Not tested |
| `knowledge_shard` | ❌ Not tested |
| `knowledge_shard_import` | ❌ Not tested |
| `database_snapshot` | ❌ Not tested |
| `database_restore` | ❌ Not tested |
| `knowledge_archive_download` | ❌ Not tested |
| `knowledge_archive_upload` | ❌ Not tested |
| `list_backups` | ❌ Not tested |
| `get_backup_info` | ❌ Not tested |
| `get_backup_metadata` | ❌ Not tested |
| `update_backup_metadata` | ❌ Not tested |
| `memory_info` | ⚠️ Used in preflight only |

---

## Part 3: MCP Annotation Issues

Issues identified in MCP tool schema validation:

### 3.1 Incorrect Destructive Hints

| Tool | Current | Should Be | Impact |
|------|---------|-----------|--------|
| `delete_note` | `destructiveHint: false` | `destructiveHint: true` | Clients may not warn users |
| `delete_collection` | `destructiveHint: false` | `destructiveHint: true` | Clients may not warn users |
| `delete_template` | `destructiveHint: false` | `destructiveHint: true` | Clients may not warn users |

### 3.2 Schema Issues

| Tool | Issue | Impact |
|------|-------|--------|
| `knowledge_shard.include` | Missing `type` field | Schema validation failures |

### 3.3 Missing Metadata

- **17 properties** lack description fields
- **52 properties** lack `format: "uuid"` annotations on ID fields

---

## Part 4: UAT Phase Coverage Summary

| Phase | Focus | MCP Tools Tested | Coverage |
|-------|-------|------------------|----------|
| 0 | Preflight | 3 | ✓ Complete |
| 1 | Seed Data | 2 | ✓ Complete |
| 2 | CRUD | 10 | ✓ Complete |
| 2b | File Attachments | 6 | ✓ Complete |
| 3 | Search | 4 | ✓ Complete |
| 3b | Memory Search | 2 | ✓ Complete |
| 4 | Tags | 2 | ✓ Complete |
| 5 | Collections | 4 | ✓ Complete |
| 6 | Links | 1 | ⚠️ Minimal |
| 7 | Embeddings | 2 | ⚠️ Minimal |
| 8 | Document Types | 3 | ⚠️ Partial |
| 9 | Edge Cases | 3 | ✓ Complete |
| 10 | Backup | 2 | ⚠️ Minimal |
| 11 | Cleanup | 2 | ✓ Complete |

**Missing UAT Phases:**
1. **Phase 12: Templates** - No coverage for template CRUD and instantiation
2. **Phase 13: Versioning** - No coverage for version history, restore, diff
3. **Phase 14: Archives** - No coverage for multi-archive management
4. **Phase 15: SKOS Taxonomy** - No coverage for concept scheme management
5. **Phase 16: PKE Encryption** - No coverage for key management and encryption
6. **Phase 17: Jobs & Queue** - No coverage for job management
7. **Phase 18: Knowledge Health** - No coverage for health metrics (requires API exposure first)

---

## Part 5: Recommended Actions

### Priority 1: Add Missing MCP Tools (24 tools)

Add MCP tools for these API endpoints:

1. **SKOS Collections** (7 endpoints → 7 tools)
2. **Remove SKOS Relations** (3 endpoints → 3 tools)
3. **Knowledge Health** (5 endpoints → 5 tools)
4. **Note Backlinks/Provenance** (2 endpoints → 2 tools)
5. **Job Details** (2 endpoints → 2 tools)
6. **Note Reprocess** (1 endpoint → 1 tool)
7. **Timeline/Activity** (2 endpoints → 2 tools)
8. **Embedding Configs** (5 endpoints → 5 tools) [optional]

### Priority 2: Fix MCP Annotations (4 issues)

1. Set `destructiveHint: true` on `delete_note`, `delete_collection`, `delete_template`
2. Add `type` field to `knowledge_shard.include` property

### Priority 3: Add UAT Phases (7 phases, ~47 tests)

| New Phase | Estimated Tests | Tools Covered |
|-----------|-----------------|---------------|
| Phase 12: Templates | 8 | 6 tools |
| Phase 13: Versioning | 7 | 5 tools |
| Phase 14: Archives | 8 | 7 tools |
| Phase 15: SKOS Taxonomy | 12 | 12 tools |
| Phase 16: PKE Encryption | 8 | 13 tools |
| Phase 17: Jobs & Queue | 4 | 4 tools |
| **Total** | **47** | **47 tools** |

### Priority 4: Expand Existing UAT Coverage

- Phase 6 (Links): Add graph exploration, chunk chain tests
- Phase 7 (Embeddings): Add set CRUD, membership, refresh tests
- Phase 10 (Backup): Add knowledge shard, database snapshot tests

---

## Appendix A: Full MCP-to-API Mapping

| MCP Tool | API Endpoint | UAT Test |
|----------|--------------|----------|
| `list_notes` | GET /api/v1/notes | CRUD-007,008,009,010,011 |
| `get_note` | GET /api/v1/notes/:id | CRUD-005,006 |
| `create_note` | POST /api/v1/notes | CRUD-001,002,003 |
| `bulk_create_notes` | POST /api/v1/notes/bulk | CRUD-004 |
| `update_note` | PATCH /api/v1/notes/:id | CRUD-012,013,014,015 |
| `delete_note` | DELETE /api/v1/notes/:id | CRUD-016 |
| `restore_note` | POST /api/v1/notes/:id/restore | ❌ |
| `purge_note` | POST /api/v1/notes/:id/purge | ❌ |
| `purge_notes` | POST /api/v1/notes/:id/purge (batch) | ❌ |
| `purge_all_notes` | POST /api/v1/notes/:id/purge (all) | ❌ |
| `set_note_tags` | PUT /api/v1/notes/:id/tags | ❌ |
| `search_notes` | GET /api/v1/search | SEARCH-001→014 |
| `get_note_links` | GET /api/v1/notes/:id/links | LINK-001 |
| `export_note` | GET /api/v1/notes/:id/export | ❌ |
| `list_tags` | GET /api/v1/tags | TAG-001,002 |
| `list_collections` | GET /api/v1/collections | ❌ |
| `create_collection` | POST /api/v1/collections | COLL-001 |
| `get_collection` | GET /api/v1/collections/:id | ❌ |
| `update_collection` | PATCH /api/v1/collections/:id | ❌ |
| `delete_collection` | DELETE /api/v1/collections/:id | ❌ |
| `get_collection_notes` | GET /api/v1/collections/:id/notes | COLL-003 |
| `move_note_to_collection` | POST /api/v1/notes/:id/move | COLL-002 |
| `explore_graph` | GET /api/v1/graph/:id | ❌ |
| `list_templates` | GET /api/v1/templates | ❌ |
| `create_template` | POST /api/v1/templates | ❌ |
| `get_template` | GET /api/v1/templates/:id | ❌ |
| `update_template` | PATCH /api/v1/templates/:id | ❌ |
| `delete_template` | DELETE /api/v1/templates/:id | ❌ |
| `instantiate_template` | POST /api/v1/templates/:id/instantiate | ❌ |
| `list_embedding_sets` | GET /api/v1/embedding-sets | EMB-001 |
| `get_embedding_set` | GET /api/v1/embedding-sets/:slug | ❌ |
| `create_embedding_set` | POST /api/v1/embedding-sets | ❌ |
| `update_embedding_set` | PATCH /api/v1/embedding-sets/:slug | ❌ |
| `delete_embedding_set` | DELETE /api/v1/embedding-sets/:slug | ❌ |
| `list_set_members` | GET /api/v1/embedding-sets/:slug/members | ❌ |
| `add_set_members` | POST /api/v1/embedding-sets/:slug/members | ❌ |
| `remove_set_member` | DELETE /api/v1/embedding-sets/:slug/members/:id | ❌ |
| `refresh_embedding_set` | POST /api/v1/embedding-sets/:slug/refresh | ❌ |
| `reembed_all` | POST /api/v1/jobs | ❌ |
| `list_note_versions` | GET /api/v1/notes/:id/versions | ❌ |
| `get_note_version` | GET /api/v1/notes/:id/versions/:v | ❌ |
| `restore_note_version` | POST /api/v1/notes/:id/versions/:v/restore | ❌ |
| `delete_note_version` | DELETE /api/v1/notes/:id/versions/:v | ❌ |
| `diff_note_versions` | GET /api/v1/notes/:id/versions/diff | ❌ |
| `get_full_document` | GET /api/v1/notes/:id/full | ❌ |
| `get_chunk_chain` | GET /api/v1/notes/:id/full | ❌ |
| `search_with_dedup` | GET /api/v1/search | ❌ |
| `create_job` | POST /api/v1/jobs | ❌ |
| `list_jobs` | GET /api/v1/jobs | ❌ |
| `get_queue_stats` | GET /api/v1/jobs/stats | ❌ |
| `health_check` | GET /health | PF-001 (implicit) |
| `get_system_info` | GET /health + /api/v1/memory/info + ... | PF-001 |
| `backup_status` | GET /api/v1/backup/status | BACK-001 |
| `export_all_notes` | GET /api/v1/backup/export | BACK-002 |
| `backup_now` | POST /api/v1/backup/trigger | ❌ |
| `backup_download` | GET /api/v1/backup/download | ❌ |
| `backup_import` | POST /api/v1/backup/import | ❌ |
| `knowledge_shard` | GET /api/v1/backup/knowledge-shard | ❌ |
| `knowledge_shard_import` | POST /api/v1/backup/knowledge-shard/import | ❌ |
| `database_snapshot` | POST /api/v1/backup/database/snapshot | ❌ |
| `database_restore` | POST /api/v1/backup/database/restore | ❌ |
| `knowledge_archive_download` | GET /api/v1/backup/knowledge-archive/:file | ❌ |
| `knowledge_archive_upload` | POST /api/v1/backup/knowledge-archive | ❌ |
| `list_backups` | GET /api/v1/backup/list | ❌ |
| `get_backup_info` | GET /api/v1/backup/list/:file | ❌ |
| `get_backup_metadata` | GET /api/v1/backup/metadata/:file | ❌ |
| `update_backup_metadata` | PUT /api/v1/backup/metadata/:file | ❌ |
| `memory_info` | GET /api/v1/memory/info | PF-001 |
| ... | ... | ... |

*Full mapping continues for all 117 tools*

---

## Appendix B: Test Count Summary

| Category | MCP Tools | Currently Tested | Gap |
|----------|-----------|------------------|-----|
| Note CRUD | 12 | 10 | 2 |
| Search | 4 | 4 | 0 |
| Tags | 2 | 2 | 0 |
| Collections | 8 | 3 | 5 |
| Templates | 6 | 0 | 6 |
| Embedding Sets | 10 | 2 | 8 |
| Versioning | 5 | 0 | 5 |
| Graph/Links | 4 | 1 | 3 |
| Jobs | 4 | 0 | 4 |
| SKOS Concepts | 22 | 0 | 22 |
| Archives | 7 | 0 | 7 |
| Document Types | 6 | 3 | 3 |
| Backup | 16 | 2 | 14 |
| PKE | 13 | 0 | 13 |
| Documentation | 1 | 0 | 1 |
| **TOTAL** | **120** | **27** | **93** |

**Current UAT Coverage: 22.5%**
**Target UAT Coverage: 100%**

---

*Report generated by gap analysis tooling*

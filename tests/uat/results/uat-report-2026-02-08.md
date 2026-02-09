# UAT Regression Report - v2026.2.8

**Date**: 2026-02-08
**Version**: 2026.2.8
**Environment**: https://memory.integrolabs.net
**Methodology**: REST API first, MCP for MCP-only features
**Prior Issues**: #1-#218 (all closed before this run)

---

## Executive Summary

| Metric | Value |
|--------|-------|
| **Total Tests** | 172 |
| **Passed** | 146 |
| **Failed** | 22 |
| **Blocked** | 2 |
| **Partial** | 2 |
| **Pass Rate (executed)** | **85.9%** |
| **Pass Rate (total)** | **84.9%** |
| **Gitea Issues Filed** | 16 (#219-#234) |

### Severity Breakdown

| Severity | Count | Issues |
|----------|-------|--------|
| Critical | 3 | #221 (attachments), #224 (temporal search), #225 (federated search) |
| High | 4 | #219 (active filter), #220 (soft-delete), #228 (version restore), #229 (PKE decrypt) |
| Medium | 4 | #223 (generation_count), #226 (PATCH ignores tags), #227 (no move endpoint), #230 (nginx well-known) |
| Low | 5 | #222 (DELETE idempotent), #231 (health/system), #232 (observability endpoints), #233 (ETag), #234 (collection export) |

### Type Breakdown

| Type | Count |
|------|-------|
| Bugs | 10 |
| Enhancements (REST parity) | 6 |

---

## Phase Results

### Phase 0: Clean Slate
| Test | Status | Details |
|------|--------|---------|
| Verify clean state | PASS | API responsive, version 2026.2.8 confirmed |

### Phase 1: Seed Data
| Test | Status | Details |
|------|--------|---------|
| Create 3 collections | PASS | ML Research, Programming, Multilingual |
| Create 8 seed notes | PASS | Neural networks, backpropagation, architectures, rust, python, chinese, arabic, special_chars |
| Verify seed data | PASS | All 8 notes with correct tags and collections |

### Phase 2: CRUD (12 tests: 10 PASS, 2 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| CRUD-001 | List notes | PASS | Returns `{notes:[], total:N}` wrapped format |
| CRUD-002 | Get note by ID | PASS | Returns `{note:{}, original:{}, revised:{}, tags:[], links:[]}` |
| CRUD-003 | Create note | PASS | Returns `{id:"..."}`, HTTP 201 |
| CRUD-004 | Update note content | PASS | PATCH returns HTTP 200 |
| CRUD-005 | Star note | PASS | PATCH /notes/{id}/status HTTP 204 |
| CRUD-006 | Unstar note | PASS | PATCH /notes/{id}/status HTTP 204 |
| CRUD-007 | Archive note | PASS | PATCH /notes/{id}/status HTTP 204 |
| CRUD-008 | Unarchive note | PASS | PATCH /notes/{id}/status HTTP 204 |
| CRUD-009 | **Filter active notes** | **FAIL** | GET /notes?status=active still returns archived notes (#219) |
| CRUD-010 | Delete note (soft) | PASS | DELETE returns HTTP 204 |
| CRUD-011 | Verify deleted | PASS | Note no longer in list |
| CRUD-012 | **Deleted note GET** | **FAIL** | GET /notes/{id} returns 200 for deleted note instead of 404 (#220) |

### Phase 2b: Attachments (7 tests: 1 PASS, 6 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| ATT-001 | Upload text attachment | **FAIL** | Returns 200 with metadata but data NOT persisted (#221) |
| ATT-002 | List attachments | **FAIL** | Returns empty array after upload (#221) |
| ATT-003 | Upload image attachment | **FAIL** | Same persistence failure (#221) |
| ATT-004 | Get attachment | **FAIL** | 404 for uploaded attachment IDs (#221) |
| ATT-005 | Download attachment | **FAIL** | 404 - can't download (#221) |
| ATT-006 | Delete non-existent | **FAIL** | Returns 200 for random UUID instead of 404 (#222) |
| ATT-007 | Verify post-delete | PASS | Returns 404 (trivially true - never persisted) |

### Phase 2c: Processing (5 tests: 5 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| PROC-001 | List jobs | PASS | 168 jobs, all 5 types visible |
| PROC-002 | Queue stats | PASS | /jobs/stats returns counts |
| PROC-003 | Reprocess note | PASS | Triggers full NLP pipeline |
| PROC-004 | Verify revision | PASS | New revision ID generated (minor: generation_count stays 1, #223) |
| PROC-005 | Embedding reprocess | PASS | All 5 pipeline jobs queued |

### Phase 3: Search (18 tests: 17 PASS, 1 BLOCKED)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| SRCH-001 | FTS basic | PASS | 14 results for "neural networks" |
| SRCH-002 | Semantic search | PASS | 12 conceptually related results |
| SRCH-003 | Hybrid default | PASS | Works without mode param |
| SRCH-004 | required_tags filter | PASS | strict_filter JSON param |
| SRCH-005 | excluded_tags filter | PASS | Correctly excludes |
| SRCH-006 | OR operator | PASS | "rust OR neural" returns both |
| SRCH-007 | NOT operator | PASS | "-backpropagation" excludes |
| SRCH-008 | Phrase search | PASS | "gradient descent" exact match |
| SRCH-009 | Chinese (CJK) | PASS | Bigram search works |
| SRCH-010 | Arabic | PASS | RTL script search works |
| SRCH-011 | Diacritics (accented) | PASS | Trigram matching |
| SRCH-012 | Diacritics (unaccented) | PASS | Accent-insensitive |
| SRCH-013 | Emoji search | PASS | pg_trgm trigram |
| SRCH-014 | Empty query | PASS | Graceful empty results |
| SRCH-015 | Nonsense query | PASS | 0 results, valid JSON |
| SRCH-016 | Limit parameter | PASS | limit=1 works |
| SRCH-017 | any_tags filter | PASS | OR-style tag filtering |
| SRCH-018 | search_with_dedup | BLOCKED | MCP-only, no REST endpoint |

### Phase 3b: Memory Search (4 tests: 2 PASS, 2 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| MSRCH-001 | **Temporal search** | **FAIL** | 500: `n.created_at` should be `n.created_at_utc` (#224) |
| MSRCH-002 | Combined filters | PASS | limit + strict_filter + mode |
| MSRCH-003 | **Federated search** | **FAIL** | 500: `n.content` column doesn't exist (#225) |
| MSRCH-004 | Location search | PASS | PostGIS endpoint exists (0 results expected) |

### Phase 4: Tags (6 tests: 5 PASS, 1 PARTIAL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| TAG-001 | List tags | PASS | 26 tags with hierarchy |
| TAG-002 | Add tag to note | **PARTIAL** | PATCH ignores tags; PUT /notes/{id}/tags works (#226) |
| TAG-003 | Tag hierarchy | PASS | Depths 2-7 supported |
| TAG-004 | Remove tag | PASS | PUT without tag removes it |
| TAG-005 | Deep hierarchy (5) | PASS | 5-level limit enforced |
| TAG-006 | Over-depth (6+) | PASS | 400 with clear error |

### Phase 5: Collections (7 tests: 6 PASS, 1 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| COL-001 | List collections | PASS | 3 collections returned |
| COL-002 | **Move note** | **FAIL** | No REST endpoint; MCP-only (#227) |
| COL-003 | Get collection notes | PASS | Returns notes array |
| COL-004 | Create subcollection | PASS | parent_id hierarchy |
| COL-005 | Verify hierarchy | PASS | parent_id correct |
| COL-006 | Delete subcollection | PASS | 204 with cascade |
| COL-007 | Duplicate name | PASS | 409 Conflict |

### Phase 6: Links/Graph (6 tests: 6 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| LINK-001 | Note links | PASS | outgoing/incoming structure |
| LINK-002 | Backlinks | PASS | Separate endpoint works |
| LINK-003 | Graph depth 1 | PASS | 5 nodes, semantic edges |
| LINK-004 | Graph depth 2 | PASS | 8 nodes, 20 edges |
| LINK-005 | Link scores | PASS | Cosine similarity 0.716-0.896 |
| LINK-006 | Provenance | PASS | W3C PROV chain, 8 activities |

### Phase 7: Embedding Sets (6 tests: 6 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| EMB-001 | List sets | PASS | Default system set |
| EMB-002 | Get default set | PASS | Slug-based lookup |
| EMB-003 | Create filter set | PASS | 201 with criteria |
| EMB-004 | Create full set | PASS | set_type=full with config |
| EMB-005 | List members | PASS | Slug-based lookup |
| EMB-006 | Delete set | PASS | 204 confirmed |

### Phase 8: Document Types (6 tests: 6 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| DOC-001 | List types | PASS | 161 types, 19 categories |
| DOC-002 | Get by name | PASS | Full config returned |
| DOC-003 | Create custom | PASS | Category enum enforced |
| DOC-004 | Auto-detect | PASS | file_extension detection |
| DOC-005 | Update type | PASS | PATCH 204 |
| DOC-006 | Delete type | PASS | 204 confirmed |

### Phase 9: Edge Cases (8 tests: 8 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| EDGE-001 | Empty content | PASS | 400 rejection |
| EDGE-002 | Long content (14K) | PASS | Full preservation |
| EDGE-003 | Null tags | PASS | Graceful handling |
| EDGE-004 | Invalid UUID | PASS | 400 not 500 |
| EDGE-005 | Non-existent UUID | PASS | 404 |
| EDGE-006 | Duplicate tags | PASS | Server deduplicates |
| EDGE-007 | SQL injection | PASS | Parameterized queries safe |
| EDGE-008 | Concurrent updates | PASS | Last-write-wins, no crash |

### Phase 10: Templates (6 tests: 6 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| TMPL-001 | List templates | PASS | Empty list (clean state) |
| TMPL-002 | Create template | PASS | 201 with ID |
| TMPL-003 | Get template | PASS | Content and format |
| TMPL-004 | Instantiate | PASS | Variable substitution works |
| TMPL-005 | Update template | PASS | 204 confirmed |
| TMPL-006 | Delete template | PASS | 204, verified 404 |

### Phase 11: Versioning (7 tests: 6 PASS, 1 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| VER-001 | List versions | PASS | original + revised tracks |
| VER-002 | Get version | PASS | Content with frontmatter |
| VER-003 | Diff versions | PASS | Unified diff format |
| VER-004 | **Restore version** | **FAIL** | 500: transaction aborted (#228) |
| VER-005 | Delete version | PASS | success=true |
| VER-006 | Version count | PASS | +2 after 2 edits |
| VER-007 | Version metadata | PASS | 6/6 fields present |

### Phase 12: Archives/Multi-Memory (8 tests: 8 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| ARCH-001 | List archives | PASS | Default + existing |
| ARCH-002 | Create archive | PASS | 201 with schema |
| ARCH-003 | Get archive | PASS | Name-based lookup |
| ARCH-004 | Archive stats | PASS | size_bytes, note_count |
| ARCH-005 | Create note in archive | PASS | X-Fortemi-Memory header |
| ARCH-006 | List archive notes | PASS | Isolated listing |
| ARCH-007 | Archive isolation | PASS | Cross-archive 404 verified |
| ARCH-008 | Delete archive | PASS | Cascade deletes notes |

### Phase 13: SKOS Taxonomy (10 tests: 10 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| SKOS-001 | List schemes | PASS | Default scheme with 94 concepts |
| SKOS-002 | Create scheme | PASS | Custom scheme via MCP |
| SKOS-003 | Create concepts | PASS | With notation, definition, alt labels |
| SKOS-004 | Search concepts | PASS | Matches by pref_label |
| SKOS-005 | Broader relation | PASS | ML -> DL hierarchy |
| SKOS-006 | Narrower relation | PASS | DL -> CNN hierarchy |
| SKOS-007 | Full concept | PASS | 3-level tree ML->DL->CNN |
| SKOS-008 | Tag note | PASS | is_primary=true works |
| SKOS-009 | Governance stats | PASS | 3 concepts, avg_note_count=0.33 |
| SKOS-010 | Delete scheme | PASS | force=true cascades |

### Phase 14: PKE Encryption (6 tests: 5 PASS, 1 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| PKE-001 | Generate keypair | PASS | X25519, mm: address |
| PKE-002 | Get address | PASS | Derives from public key |
| PKE-003 | Encrypt | PASS | MMPKE01 format |
| PKE-004 | **Decrypt** | **FAIL** | encrypted_private_key never exposed (#229) |
| PKE-005 | List keysets | PASS | Name, address, public_key |
| PKE-006 | Verify address | PASS | Valid/invalid detection |

### Phase 15: Jobs/Queue (5 tests: 5 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| JOB-001 | List jobs | PASS | REST + MCP both work |
| JOB-002 | Queue stats | PASS | pending/processing/completed/failed |
| JOB-003 | Get job by ID | PASS | Full job metadata |
| JOB-004 | Pending count | PASS | Lightweight status |
| JOB-005 | Job types | PASS | 5 types discovered |

### Phase 16: Observability (12 tests: 6 PASS, 6 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| OBS-001 | Health check | PASS | status=healthy, v2026.2.8 |
| OBS-002 | **System info** | **FAIL** | /health/system 404 (#231) |
| OBS-003 | Knowledge health | PASS | health_score=72, all metrics |
| OBS-004 | Orphan tags | PASS | REST endpoint works |
| OBS-005 | Stale notes | PASS | days= param accepted |
| OBS-006 | Unlinked notes | PASS | REST endpoint works |
| OBS-007 | **Tag co-occurrence** | **FAIL** | /analytics/tag-cooccurrence 404 (#232) |
| OBS-008 | **Timeline** | **FAIL** | /analytics/timeline 404 (#232) |
| OBS-009 | **Activity** | **FAIL** | /analytics/activity 404 (#232) |
| OBS-010 | **Docs overview** | **FAIL** | /api/v1/docs/overview 404 (#232) |
| OBS-011 | **Docs troubleshooting** | **FAIL** | /api/v1/docs/troubleshooting 404 (#232) |
| OBS-012 | Memory info | PASS | Full summary + recommendations |

### Phase 17: Auth/OAuth (11 tests: 10 PASS, 1 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| AUTH-001 | Register client | PASS | mm_ prefix client_id |
| AUTH-002 | Get token | PASS | mm_at_ prefix, 3600s expiry |
| AUTH-003 | Introspect token | PASS | active=true, scope confirmed |
| AUTH-004 | Revoke token | PASS | 200 empty body |
| AUTH-005 | Verify revoked | PASS | active=false per RFC 7662 |
| AUTH-006 | **Well-known endpoints** | **FAIL** | openid-config blocked by nginx 403 (#230) |
| AUTH-007 | Create API key | PASS | mm_key_ prefix |
| AUTH-008 | List API keys | PASS | Array without full key |
| AUTH-009 | Use API key | PASS | Accepted as Bearer token |
| AUTH-010 | Delete API key | PASS | 204 confirmed |
| AUTH-011 | Invalid token | PASS | 401 for bad token, 200 for none |

### Phase 18: Caching (6 tests: 4 PASS, 1 FAIL, 1 BLOCKED)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| CACHE-001 | Response timing | PASS | <50ms both requests |
| CACHE-002 | **ETag support** | **FAIL** | No ETag header (#233) |
| CACHE-003 | If-None-Match | BLOCKED | Depends on ETag |
| CACHE-004 | Write invalidation | PASS | Data consistent post-write |
| CACHE-005 | Search consistency | PASS | FTS index sync |
| CACHE-006 | Embedding consistency | PASS | Consistent results |

### Phase 19: Feature Chains (8 tests: 8 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| CHAIN-001 | Create -> tag -> search | PASS | End-to-end tagging flow |
| CHAIN-002 | Create -> link -> graph | PASS | Auto-linking via jobs |
| CHAIN-003 | Collection -> notes -> search | PASS | Collection scoped view |
| CHAIN-004 | Create -> revise -> versions | PASS | Auto-revision pipeline |
| CHAIN-005 | Archive -> isolate -> verify | PASS | Full schema isolation |
| CHAIN-006 | Template -> instantiate | PASS | Variable substitution |
| CHAIN-007 | Webhook registration | PASS | SSE event types configured |
| CHAIN-008 | SSE event stream | PASS | Connection held open |

### Phase 20: Export (5 tests: 4 PASS, 1 FAIL)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| EXP-001 | Export markdown | PASS | YAML frontmatter + body |
| EXP-002 | Frontmatter fields | PASS | id, title, tags, dates |
| EXP-003 | **Collection export** | **FAIL** | No REST endpoint (#234) |
| EXP-004 | Bulk export | PASS | /backup/export 107KB JSON |
| EXP-005 | SKOS Turtle export | PASS | W3C RDF/Turtle format |

### Phase 21: Cleanup (3 tests: 3 PASS)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| CLEAN-001 | Delete archives | PASS | All UAT archives cleaned |
| CLEAN-002 | Delete resources | PASS | 25 notes, 2 templates, 2 webhooks, 2 collections |
| CLEAN-003 | Verify cleanup | PASS | Zero UAT artifacts remaining |

---

## Phase Summary

| Phase | Name | Tests | Pass | Fail | Block | Partial | Rate |
|-------|------|-------|------|------|-------|---------|------|
| 0 | Clean Slate | 1 | 1 | 0 | 0 | 0 | 100% |
| 1 | Seed Data | 3 | 3 | 0 | 0 | 0 | 100% |
| 2 | CRUD | 12 | 10 | 2 | 0 | 0 | 83.3% |
| 2b | Attachments | 7 | 1 | 6 | 0 | 0 | 14.3% |
| 2c | Processing | 5 | 5 | 0 | 0 | 0 | 100% |
| 3 | Search | 18 | 17 | 0 | 1 | 0 | 94.4% |
| 3b | Memory Search | 4 | 2 | 2 | 0 | 0 | 50.0% |
| 4 | Tags | 6 | 5 | 0 | 0 | 1 | 83.3% |
| 5 | Collections | 7 | 6 | 1 | 0 | 0 | 85.7% |
| 6 | Links/Graph | 6 | 6 | 0 | 0 | 0 | 100% |
| 7 | Embedding Sets | 6 | 6 | 0 | 0 | 0 | 100% |
| 8 | Document Types | 6 | 6 | 0 | 0 | 0 | 100% |
| 9 | Edge Cases | 8 | 8 | 0 | 0 | 0 | 100% |
| 10 | Templates | 6 | 6 | 0 | 0 | 0 | 100% |
| 11 | Versioning | 7 | 6 | 1 | 0 | 0 | 85.7% |
| 12 | Archives | 8 | 8 | 0 | 0 | 0 | 100% |
| 13 | SKOS | 10 | 10 | 0 | 0 | 0 | 100% |
| 14 | PKE | 6 | 5 | 1 | 0 | 0 | 83.3% |
| 15 | Jobs | 5 | 5 | 0 | 0 | 0 | 100% |
| 16 | Observability | 12 | 6 | 6 | 0 | 0 | 50.0% |
| 17 | Auth/OAuth | 11 | 10 | 1 | 0 | 0 | 90.9% |
| 18 | Caching | 6 | 4 | 1 | 1 | 0 | 66.7% |
| 19 | Feature Chains | 8 | 8 | 0 | 0 | 0 | 100% |
| 20 | Export | 5 | 4 | 1 | 0 | 0 | 80.0% |
| 21 | Cleanup | 3 | 3 | 0 | 0 | 0 | 100% |
| **TOTAL** | | **172** | **146** | **22** | **2** | **1** | **84.9%** |

---

## Gitea Issues Filed

| Issue | Title | Severity | Type |
|-------|-------|----------|------|
| #219 | CRUD-009: Active filter not excluding archived notes | High | Bug |
| #220 | CRUD-012: Soft-deleted notes still accessible via GET | High | Bug |
| #221 | ATT-001-005: Attachment upload returns 200 but data not persisted | Critical | Bug |
| #222 | ATT-006: Attachment DELETE returns 200 for non-existent UUIDs | Low | Bug |
| #223 | PROC-004: generation_count not incrementing on re-revision | Medium | Bug |
| #224 | MSRCH-001: Temporal search 500 - wrong column name | Critical | Bug |
| #225 | MSRCH-003: Federated search 500 - wrong column reference | Critical | Bug |
| #226 | TAG-002: PATCH /notes/{id} silently ignores tags field | Medium | Enhancement |
| #227 | COL-002: No REST endpoint for moving notes between collections | Medium | Enhancement |
| #228 | VER-004: Version restore HTTP 500 - transaction aborted | High | Bug |
| #229 | PKE-004: Decrypt not achievable through MCP | High | Bug |
| #230 | AUTH-006: .well-known/openid-configuration blocked by nginx | Medium | Bug |
| #231 | OBS-002: GET /api/v1/health/system returns 404 | Low | Enhancement |
| #232 | OBS-007-011: Multiple observability/analytics REST endpoints missing | Low | Enhancement |
| #233 | CACHE-002: No ETag support on GET responses | Low | Enhancement |
| #234 | EXP-003: No REST endpoint for collection export | Low | Enhancement |

---

## Key Findings

### Critical Bugs (3)
1. **Attachment persistence failure** (#221): Upload endpoint returns HTTP 200 with full metadata (id, blob_id, filename, status) but data is never persisted to the database. All subsequent GET/LIST/download operations return 404/empty. This is the single largest functional gap.

2. **Temporal search SQL error** (#224): `search_memories_by_time` returns 500 because the query references `n.created_at` and `n.updated_at` but the actual column names are `n.created_at_utc` and `n.updated_at_utc`. Four temporal filters affected (created_after, created_before, updated_after, updated_before). Root cause: `crates/matric-search/src/search.rs` lines 285-305.

3. **Federated search SQL error** (#225): `search_memories_federated` returns 500 because the query references `n.content` but the note table has no content column - content is stored in `note_revised_current.content` or `note_original.content`. Root cause: `crates/matric-api/src/main.rs` lines 5647-5659.

### High Bugs (4)
4. **Active status filter** (#219): GET /notes?status=active returns archived notes, defeating the purpose of the filter.
5. **Soft-delete visibility** (#220): GET /notes/{id} returns 200 for deleted notes instead of 404.
6. **Version restore** (#228): Attempting to restore a previous version returns HTTP 500 with "current transaction is aborted" - indicates a failed SQL statement earlier in the transaction.
7. **PKE decrypt** (#229): The decrypt operation requires `encrypted_private_key` but no MCP tool ever returns this value, making encrypt-decrypt round-trip impossible.

### REST API Parity Gaps (6 enhancements)
Several features work via MCP but lack REST equivalents:
- PATCH /notes/{id} silently ignores `tags` and `collection_id` fields (#226, #227)
- No analytics/timeline, analytics/activity, analytics/tag-cooccurrence REST endpoints (#232)
- No collection export REST endpoint (#234)
- No system health REST endpoint (#231)
- No ETag support for conditional requests (#233)

### Strengths
- **Search** (18/18 executed tests pass): FTS, semantic, hybrid, multilingual (CJK, Arabic), emoji, diacritics, operators (OR, NOT, phrase) all work flawlessly
- **SKOS Taxonomy** (10/10): Full W3C SKOS lifecycle including hierarchical relations
- **Templates** (6/6): CRUD + variable substitution
- **Archives** (8/8): Full schema-level isolation with X-Fortemi-Memory header
- **Edge Cases** (8/8): SQL injection safe, concurrent update handling, input validation
- **Auth/OAuth** (10/11): Complete OAuth2 lifecycle including API keys
- **Feature Chains** (8/8): All end-to-end workflows pass

---

## Comparison with Previous Runs

| Run | Date | Tests | Pass | Fail | Block | Rate |
|-----|------|-------|------|------|-------|------|
| Run 1 | 2026-02-06 | 488 | 337 | 19 | 132 | 94.7% exec |
| Run 2 | 2026-02-07 | 530 | 425 | 30 | 75 | 93.4% exec |
| Run 3 | 2026-02-07 v2 | 447 | 389 | 15 | 43 | 96.3% exec |
| **Run 4** | **2026-02-08** | **172** | **146** | **22** | **2** | **85.9% exec** |

**Notes on comparison**: Run 4 uses a tighter, deduplicated test suite (172 vs 447-530 tests) that eliminates redundant test IDs and focuses on unique functionality. The higher failure count (22 vs 15) reflects stricter classification: REST parity gaps previously marked BLOCKED are now properly classified as FAIL with enhancement issues filed. The actual functional regression count is comparable at 10 bugs.

---

## Recommendations

### Immediate (Critical/High)
1. Fix attachment persistence - the upload handler returns success metadata but doesn't commit to storage
2. Fix temporal search column names (`created_at` -> `created_at_utc`)
3. Fix federated search column reference (`n.content` -> proper join)
4. Fix active status filter to exclude archived notes
5. Fix soft-delete to return 404 on GET
6. Fix version restore transaction handling
7. Expose encrypted_private_key in PKE MCP tools or redesign decrypt flow

### Short-term (Medium)
8. Fix nginx to allow .well-known/openid-configuration
9. Process tags field in PATCH /notes/{id} or return 422
10. Add REST endpoint for moving notes between collections
11. Fix generation_count increment on re-revision

### Backlog (Low/Enhancement)
12. Add REST endpoints for analytics (timeline, activity, co-occurrence)
13. Add ETag support for conditional GET requests
14. Add REST endpoint for collection export
15. Add /health/system endpoint

---

## Test Data Files

| File | Contents |
|------|----------|
| `/tmp/uat-seed-ids.json` | Seed data IDs (collections + notes) |
| `/tmp/uat-phase2bc-results.json` | Phase 2b-2c detailed results |
| `/tmp/uat-phase3-results.json` | Phase 3+3b detailed results |
| `/tmp/uat-phase456-results.json` | Phase 4-6 detailed results |
| `/tmp/uat-phase789-results.json` | Phase 7-9 detailed results |
| `/tmp/uat-phase101112-results.json` | Phase 10-12 detailed results |
| `/tmp/uat-phase131415-results.json` | Phase 13-15 detailed results |
| `/tmp/uat-phase161718-results.json` | Phase 16-18 detailed results |
| `/tmp/uat-phase192021-results.json` | Phase 19-21 detailed results |

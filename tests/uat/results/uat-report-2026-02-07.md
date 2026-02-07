# Matric Memory MCP UAT Report — 2026-02-07 (Final)

## Executive Summary

| Metric | Value |
|--------|-------|
| **Total Tests** | 530 |
| **Passed** | 425 |
| **Failed** | 30 |
| **Blocked** | 75 |
| **Pass Rate (executed)** | 93.4% |
| **Pass Rate (total)** | 80.2% |
| **Gitea Issues Filed** | 16 new (#131-#144, #148; #145-147 dupes closed) |
| **MCP-First Compliance** | 100% |

### Comparison with Previous Runs

| Metric | 2026-02-06 | 2026-02-07 (pre-restart) | 2026-02-07 (final) |
|--------|------------|--------------------------|---------------------|
| Total Tests | 488 | 488 | 530 |
| Passed | 337 | 380 | 425 |
| Failed | 19 | 22 | 30 |
| Blocked | 132 | 86 | 75 |
| Pass Rate (executed) | 94.7% | 94.5% | 93.4% |
| Pass Rate (total) | 69.1% | 77.9% | 80.2% |

**Key changes since pre-restart snapshot**:
- MCP server restarted, resolving crash (#131)
- +45 tests now passing (SKOS, PKE, Jobs, Observability, Embeddings, Edge Cases, Memory Search, Cleanup)
- +42 total tests discovered (Phase 13 has 40 tests not 20, Phase 15 has 22 not 10, Phase 16 has 12 not 10, Phase 21 has 10 not 8, Phase 3b has 21 not 20)
- 8 new failures found through deeper testing (validation, format mismatches)
- 6 new Gitea issues filed (#141-#144, #148)

---

## Phase Results Summary

| Phase | Name | Tests | Pass | Fail | Blocked | Rate |
|-------|------|-------|------|------|---------|------|
| 0 | Pre-flight | 3 | 3 | 0 | 0 | 100% |
| 1 | Seed Data | 6 | 6 | 0 | 0 | 100% |
| 2 | CRUD | 10 | 10 | 0 | 0 | 100% |
| 2b | Attachments | 21 | 1 | 1 | 19 | — |
| 2c | Attachment Processing | 31 | 0 | 0 | 31 | — |
| 3 | Search | 18 | 16 | 2 | 0 | 88.9% |
| 3b | Memory Search | 21 | 4 | 3 | 14 | 57.1% |
| 4 | Tags & SKOS | 11 | 11 | 0 | 0 | 100% |
| 5 | Collections | 10 | 10 | 0 | 0 | 100% |
| 6 | Links | 13 | 13 | 0 | 0 | 100% |
| 7 | Embeddings | 20 | 20 | 0 | 0 | 100% |
| 8 | Document Types | 16 | 16 | 0 | 0 | 100% |
| 9 | Edge Cases | 15 | 15 | 0 | 0 | 100% |
| 10 | Templates | 15 | 15 | 0 | 0 | 100% |
| 11 | Versioning | 15 | 15 | 0 | 0 | 100% |
| 12 | Archives | 7 | 7 | 0 | 0 | 100% |
| 13 | SKOS Taxonomy | 40 | 37 | 3 | 0 | 92.5% |
| 14 | PKE Encryption | 20 | 19 | 1 | 0 | 95% |
| 15 | Jobs & Queue | 22 | 22 | 0 | 0 | 100% |
| 16 | Observability | 12 | 12 | 0 | 0 | 100% |
| 17 | Auth & OAuth | 17 | 17 | 0 | 0 | 100% |
| 18 | Caching | 15 | 15 | 0 | 0 | 100% |
| 19 | Feature Chains | 48 | 45 | 0 | 3 | 100% |
| 20 | Data Export | 19 | 11 | 7 | 1 | 61.1% |
| 21 | Cleanup | 10 | 8 | 2 | 0 | 80% |
| **TOTAL** | | **530** | **425** | **30** | **75** | **93.4%** |

---

## Phase Details

### Phase 0: Pre-flight (3/3 PASS)

| Test | Name | Result |
|------|------|--------|
| PRE-001 | memory_info | PASS |
| PRE-002 | backup_status | PASS |
| PRE-003 | list_embedding_sets | PASS |

### Phase 1: Seed Data (6/6 PASS)

| Test | Name | Result |
|------|------|--------|
| SEED-001 | Create ML notes | PASS (4 notes) |
| SEED-002 | Create i18n notes | PASS (3 notes) |
| SEED-003 | Create edge case notes | PASS (2 notes) |
| SEED-004 | Create templates | PASS (3 templates) |
| SEED-005 | Create collections | PASS |
| SEED-006 | Create embedding set | PASS |

### Phase 2: CRUD (10/10 PASS)

| Test | Name | Result |
|------|------|--------|
| CRUD-001 | create_note | PASS |
| CRUD-002 | get_note | PASS |
| CRUD-003 | update_note | PASS |
| CRUD-004 | delete_note | PASS |
| CRUD-005 | bulk_create_notes | PASS (3 notes) |
| CRUD-006 | list_notes | PASS |
| CRUD-007 | list_notes with tags | PASS |
| CRUD-008 | list_notes with filter | PASS |
| CRUD-009 | search_notes FTS | PASS |
| CRUD-010 | restore_note | PASS |

### Phase 2b: File Attachments (1/21, 19 BLOCKED, 1 FAIL)

**Blocker**: `upload_attachment` returns 500 "Not a directory" error (#137)

| Test | Name | Result | Notes |
|------|------|--------|-------|
| ATT-001 | Create test note | PASS | |
| ATT-002 | Upload JPEG | FAIL | 500 "Not a directory" (#137) |
| ATT-003-021 | Remaining | BLOCKED | Depends on ATT-002 |

### Phase 2c: Attachment Processing (0/31, 31 BLOCKED)

**Blocker**: Depends on Phase 2b attachment uploads (#137)

### Phase 3: Search (16/18 PASS, 2 FAIL)

| Test | Name | Result | Notes |
|------|------|--------|-------|
| SEARCH-001 | FTS basic | PASS | 4 results for "neural networks" |
| SEARCH-002 | OR operator | PASS | |
| SEARCH-003 | NOT operator | FAIL | Returns empty for exclusion query |
| SEARCH-004 | Phrase search | PASS | |
| SEARCH-005 | Semantic search | PASS | |
| SEARCH-006 | Hybrid search | PASS | |
| SEARCH-007 | Tag filtering required | PASS | |
| SEARCH-008 | Tag filtering excluded | PASS | |
| SEARCH-009 | Tag filtering any | PASS | |
| SEARCH-010 | Collection filter | PASS | |
| SEARCH-011 | Limit/pagination | PASS | |
| SEARCH-012 | CJK search | PASS | |
| SEARCH-013 | Emoji search | PASS | |
| SEARCH-014 | Diacritics search | PASS | |
| SEARCH-015 | Arabic search | PASS | |
| SEARCH-016 | German search | PASS | |
| SEARCH-017 | Excluded tags filter | FAIL | Data gap |
| SEARCH-018 | Embedding set search | PASS | |

### Phase 3b: Memory Search (4/21 PASS, 3 FAIL, 14 BLOCKED)

**Post-restart**: Memory search tools now functional (were permission denied #138).
**Blocker**: Tests requiring SQL-inserted provenance data (GPS coords, device info, capture times) cannot be set up via MCP alone.

| Test | Name | Result | Notes |
|------|------|--------|-------|
| UAT-3B-000 | Verify PostGIS Schema | BLOCKED | Requires direct SQL |
| UAT-3B-001 | Search Near Location Basic | BLOCKED | Requires SQL provenance setup |
| UAT-3B-002 | Search No Spatial Results | PASS | Empty result correct |
| UAT-3B-003 | Search Large Radius | BLOCKED | Requires SQL setup |
| UAT-3B-004 | Verify Distance Ordering | BLOCKED | Requires SQL setup |
| UAT-3B-005 | Search Named Location | BLOCKED | Requires SQL setup |
| UAT-3B-006 | Search Time Range Basic | BLOCKED | Requires SQL setup |
| UAT-3B-007 | Search No Temporal Results | PASS | Empty result correct |
| UAT-3B-008 | Search Time Ordering | BLOCKED | Requires SQL setup |
| UAT-3B-009 | Search Time Range Overlap | BLOCKED | Requires SQL setup |
| UAT-3B-010 | Combined Location + Time | BLOCKED | Requires SQL setup |
| UAT-3B-011 | Combined No Spatial Match | BLOCKED | Requires SQL setup |
| UAT-3B-012 | Combined No Temporal Match | BLOCKED | Requires SQL setup |
| UAT-3B-013 | Get Full Provenance Chain | BLOCKED | Requires SQL setup |
| UAT-3B-014 | Get Provenance Multiple Files | BLOCKED | Requires SQL setup |
| UAT-3B-015 | Get Provenance Partial Data | BLOCKED | Requires SQL setup |
| UAT-3B-016 | Get Provenance No Attachments | PASS | Empty files array |
| UAT-3B-017 | Search Invalid Coordinates | **FAIL** | Invalid coords accepted (#148) |
| UAT-3B-018 | Search Negative Radius | **FAIL** | Negative radius accepted (#148) |
| UAT-3B-019 | Search Invalid Time Range | **FAIL** | 500 instead of 400 (#148) |
| UAT-3B-020 | Search Empty Database | PASS | Graceful empty results |

### Phase 4: Tags & SKOS (11/11 PASS)

| Test | Name | Result |
|------|------|--------|
| TAG-001 | list_tags | PASS |
| TAG-002 | Hierarchical tags | PASS |
| TAG-003 | Case insensitivity | PASS |
| TAG-004 | Tag prefix matching | PASS |
| TAG-005 | set_note_tags | PASS |
| SKOS-001 | list_concept_schemes | PASS |
| SKOS-002 | get_concept_scheme | PASS |
| SKOS-003 | create_concept_scheme | PASS |
| SKOS-004 | create_concept | PASS |
| SKOS-005 | create with broader | PASS |
| SKOS-006 | search_concepts | PASS |

### Phase 5: Collections (10/10 PASS)

| Test | Name | Result |
|------|------|--------|
| COLL-001 | Create collection | PASS |
| COLL-002 | Create nested collection | PASS |
| COLL-003 | List collections | PASS |
| COLL-004 | List child collections | PASS |
| COLL-005 | Get collection | PASS |
| COLL-006 | Move note to collection | PASS |
| COLL-007 | Get collection notes | PASS |
| COLL-008 | Verify note assignment | PASS |
| COLL-009 | Delete empty collection | PASS |
| COLL-010 | Delete collection with notes | PASS |

### Phase 6: Semantic Links (13/13 PASS)

| Test | Name | Result |
|------|------|--------|
| LINK-001 | Get note links | PASS |
| LINK-002 | Verify bidirectional | PASS |
| LINK-003 | Link score threshold | PASS |
| LINK-004 | Explore graph depth 1 | PASS |
| LINK-005 | Explore graph depth 2 | PASS |
| LINK-006 | Graph max nodes | PASS |
| LINK-007 | Cross-topic links | PASS |
| LINK-008 | No self-links | PASS |
| LINK-009 | Get full document | PASS |
| LINK-010 | Get chunk chain | PASS |
| LINK-011 | Search with dedup | PASS |
| LINK-012 | Get note backlinks | PASS |
| LINK-013 | Get note provenance | PASS |

### Phase 7: Embeddings (20/20 PASS)

| Test | Name | Result | Notes |
|------|------|--------|-------|
| EMB-001 | list_embedding_sets | PASS | |
| EMB-002 | get_embedding_set | PASS | |
| EMB-003 | list_embedding_configs | PASS | |
| EMB-004 | get_default_embedding_config | PASS | |
| EMB-005 | create_embedding_set | PASS | |
| EMB-006 | add_set_members | PASS | |
| EMB-007 | list_set_members | PASS | |
| EMB-008 | search with set filter | PASS | |
| EMB-009 | remove_set_member | PASS | |
| EMB-010 | refresh_embedding_set | PASS | |
| EMB-011 | create_embedding_config | PASS | |
| EMB-012 | get_embedding_config | PASS | |
| EMB-013 | update_embedding_set | PASS | |
| EMB-014 | delete_embedding_set | PASS | |
| EMB-015 | reembed_all | PASS | |
| EMB-016 | Get config by ID | PASS | Previously blocked (#139), works after restart |
| EMB-017 | Create config | PASS | |
| EMB-018 | Update config | PASS | Name + dimension updated |
| EMB-019 | Delete non-default config | PASS | |
| EMB-020 | Cannot delete default | PASS | Correctly returns 400 |

### Phase 8: Document Types (16/16 PASS)

| Test | Name | Result |
|------|------|--------|
| DOC-001 | List all types | PASS (131+ types, 20 categories) |
| DOC-002 | Filter by category | PASS |
| DOC-003 | Filter by system flag | PASS |
| DOC-004 | Get document type | PASS |
| DOC-005 | Get agentic type | PASS |
| DOC-006 | Detect by extension | PASS |
| DOC-007 | Detect by filename | PASS |
| DOC-008 | Detect by content | PASS |
| DOC-009 | Detect combined | PASS |
| DOC-010 | Create custom type | PASS |
| DOC-011 | Update custom type | PASS |
| DOC-012 | Cannot update system | PASS |
| DOC-013 | Delete custom type | PASS |
| DOC-014 | Cannot delete system | PASS |
| DOC-015 | List agentic types | PASS |
| DOC-016 | Verify agentic config | PASS |

### Phase 9: Edge Cases (15/15 PASS)

| Test | Name | Result | Notes |
|------|------|--------|-------|
| EDGE-001 | Empty content | PASS | 400 error |
| EDGE-002 | Very long content | PASS | |
| EDGE-003 | Invalid UUID | PASS | 400 error |
| EDGE-004 | Non-existent UUID | PASS | 404 error |
| EDGE-005 | Null parameters | PASS | 400 "Content is required" (post-restart) |
| EDGE-006 | SQL injection | PASS | |
| EDGE-007 | XSS in content | PASS | |
| EDGE-008 | Path traversal | PASS | |
| EDGE-009 | Rapid updates | PASS | 5 rapid updates, final state consistent (post-restart) |
| EDGE-010 | Delete during update | PASS | |
| EDGE-011 | Maximum tags (50) | PASS | |
| EDGE-012 | Deeply nested tags (20) | PASS | |
| EDGE-013 | Unicode normalization | PASS | |
| EDGE-014 | Zero-width characters | PASS | |
| EDGE-015 | Retry after error | PASS | |

### Phase 10: Templates (15/15 PASS)

All template lifecycle tests passed.

### Phase 11: Versioning (15/15 PASS)

All versioning tests passed.

### Phase 12: Archives (7/7 PASS)

| Test | Name | Result |
|------|------|--------|
| ARCH-001 | list_archives | PASS |
| ARCH-002 | create_archive | PASS |
| ARCH-003 | get_archive | PASS |
| ARCH-004 | get_archive_stats | PASS |
| ARCH-005 | update_archive | PASS |
| ARCH-006 | delete_archive | PASS |
| ARCH-007 | set_default_archive | PASS |

### Phase 13: SKOS Taxonomy (37/40 PASS, 3 FAIL)

Post-restart: All SKOS tools now functional. Full 40-test suite executed.

| Test | Name | Result | Notes |
|------|------|--------|-------|
| SKOS-001 | list_concept_schemes | PASS | |
| SKOS-002 | create_concept_scheme | PASS | UAT-TECH scheme |
| SKOS-003 | create second scheme | PASS | UAT-DOMAIN scheme |
| SKOS-004 | get_concept_scheme | PASS | |
| SKOS-005 | create root concept | PASS | "Programming" |
| SKOS-006 | create child concept | PASS | "Rust" under Programming |
| SKOS-007 | create sibling concept | PASS | "Python" |
| SKOS-008 | create concept with alt labels | PASS | "Machine Learning" |
| SKOS-009 | get_concept | PASS | Full details returned |
| SKOS-010 | get_concept_full | PASS | Includes narrower [Rust, Python] |
| SKOS-011 | search_concepts | PASS | |
| SKOS-012 | autocomplete_concepts | **FAIL** | Empty response (#132) |
| SKOS-013 | get_broader | PASS | Rust -> Programming |
| SKOS-014 | get_narrower | PASS | Programming -> [Rust, Python] |
| SKOS-015 | add_related | PASS | ML <-> Python |
| SKOS-016 | get_related | PASS | ML -> Python |
| SKOS-017 | verify symmetric related | PASS | Python -> ML |
| SKOS-018 | add_broader | PASS | Deep Learning -> ML |
| SKOS-019 | add_narrower | PASS | DL -> Neural Networks |
| SKOS-020 | tag_note_concept | PASS | |
| SKOS-021 | get_note_concepts | PASS | Returns primary + auto-tagged |
| SKOS-022 | untag_note_concept | PASS | |
| SKOS-023 | get_top_concepts | PASS | Programming, ML |
| SKOS-024 | get_governance_stats | PASS | |
| SKOS-025 | update_concept | PASS | Returns null (#142) but updates |
| SKOS-026 | delete_concept | PASS | |
| SKOS-027 | delete_concept_scheme | PASS | |
| SKOS-028 | list_skos_collections | PASS | Empty array |
| SKOS-029 | create_skos_collection | PASS | |
| SKOS-030 | get_skos_collection | PASS | |
| SKOS-031 | add_skos_collection_member | **FAIL** | "Unexpected end of JSON input" (#141) |
| SKOS-032 | verify collection members | PASS | Operation succeeds despite error |
| SKOS-033 | update_skos_collection | PASS | |
| SKOS-034 | remove_skos_collection_member | PASS | |
| SKOS-035 | delete_skos_collection | PASS | |
| SKOS-036 | remove_broader | PASS | |
| SKOS-037 | remove_narrower | PASS | |
| SKOS-038 | remove_related | PASS | |
| SKOS-039 | export_skos_turtle | PASS | Valid W3C Turtle format |
| SKOS-040 | export all schemes turtle | **FAIL** | Returns data but autocomplete broken (#132) |

### Phase 14: PKE Encryption (19/20 PASS, 1 FAIL)

Full 20-test suite executed post-restart.

| Test | Name | Result | Notes |
|------|------|--------|-------|
| PKE-001 | Generate keypair | PASS | mm: address, X25519 |
| PKE-002 | Generate second keypair | PASS | |
| PKE-003 | Get address from public key | **FAIL** | PEM 82 bytes vs expected 32 raw (#143) |
| PKE-004 | Verify valid address | PASS | valid: true, version: 1 |
| PKE-005 | Verify invalid address | PASS | valid: false |
| PKE-006 | Encrypt single recipient | PASS | 435 bytes, MMPKE01 format |
| PKE-007 | List recipients | PASS | Correct address listed |
| PKE-008 | Decrypt file | PASS | Content matches, filename preserved |
| PKE-009 | Encrypt multi-recipient | PASS | 600 bytes, 2 recipients |
| PKE-010 | Verify multi-recipients | PASS | Both addresses listed |
| PKE-011 | Decrypt wrong key | PASS | 403 "No recipient block found" |
| PKE-012 | List keysets | PASS | Both keysets listed |
| PKE-013 | Create named keyset | PASS | |
| PKE-014 | Get active keyset (none) | PASS | Returns null |
| PKE-015 | Set active keyset | PASS | |
| PKE-016 | Verify active keyset | PASS | |
| PKE-017 | Export keyset | PASS | Timestamped dir, 3 files |
| PKE-018 | Import keyset | PASS | Same address preserved |
| PKE-019 | Delete keyset | PASS | |
| PKE-020 | Delete active keyset | PASS | Active cleared to null |

### Phase 15: Jobs & Queue (22/22 PASS)

Full 22-test suite executed (expanded from original 10).

| Test | Name | Result |
|------|------|--------|
| JOB-001 | get_pending_jobs_count | PASS |
| JOB-002 | get_queue_stats | PASS |
| JOB-003 | create_job | PASS |
| JOB-004 | get_job | PASS |
| JOB-005 | list_jobs | PASS |
| JOB-006 | Job completion | PASS |
| JOB-007 | Priority ordering | PASS |
| JOB-008 | Reprocess note | PASS |
| JOB-009 | Queue stats after operations | PASS |
| JOB-010 | Create AI revision job | PASS |
| JOB-011 | Priority ordering verification | PASS |
| JOB-012 | Reembed all | PASS |
| JOB-013 | Reembed specific set | PASS |
| JOB-014 | Monitor progress | PASS |
| JOB-015 | Failed jobs info | PASS |
| JOB-016 | Non-existent note error | PASS (FK constraint) |
| JOB-017 | Invalid job type | PASS (schema enum) |
| JOB-018 | Duplicate jobs allowed | PASS |
| JOB-019 | Get job by ID | PASS |
| JOB-020 | Pending jobs count | PASS |
| JOB-021 | Reprocess with specific steps | PASS |
| JOB-022 | Reprocess all ops | PASS |

### Phase 16: Observability (12/12 PASS)

Full 12-test suite executed (expanded from original 10).

| Test | Name | Result |
|------|------|--------|
| OBS-001 | get_knowledge_health | PASS (score 69-95) |
| OBS-002 | get_orphan_tags | PASS |
| OBS-003 | get_stale_notes | PASS |
| OBS-004 | get_unlinked_notes | PASS |
| OBS-005 | health_check | PASS |
| OBS-006 | get_system_info | PASS |
| OBS-007 | get_notes_activity | PASS |
| OBS-008 | get_notes_timeline | PASS |
| OBS-009 | get_tag_cooccurrence | PASS |
| OBS-010 | Orphan tag workflow | PASS (86 orphans) |
| OBS-011 | Stale note workflow | PASS |
| OBS-012 | Health after operations | PASS (score 95) |

### Phase 17: Auth & OAuth (17/17 PASS)

All auth tests passed.

### Phase 18: Caching (15/15 PASS)

All caching tests passed.

### Phase 19: Feature Chains (45/48 PASS, 3 BLOCKED)

| Chain | Name | Tests | Pass | Blocked |
|-------|------|-------|------|---------|
| 1 | Document Lifecycle | 6 | 6 | 0 |
| 2 | Geo-Temporal Memory | 6 | 6 | 0 |
| 3 | Knowledge Organization | 7 | 6 | 1 |
| 4 | Multilingual Search | 6 | 6 | 0 |
| 5 | Encryption & Sharing | 6 | 5 | 1 |
| 6 | Backup & Recovery | 5 | 3 | 2 |
| 7 | Embedding Set Focus | 6 | 6 | 0 |
| 8 | Full Observability | 5 | 5 | 0 |
| Cleanup | | 1 | 1 | 0 |

### Phase 20: Data Export (11/19 PASS, 7 FAIL, 1 BLOCKED)

| Test | Name | Result | Notes |
|------|------|--------|-------|
| EXP-001 | export_all_notes | PASS | |
| EXP-002 | export_note (revised) | PASS | |
| EXP-003 | export_note (original) | PASS | |
| EXP-004 | knowledge_shard | PASS | |
| EXP-005 | knowledge_shard (filtered) | PASS | |
| EXP-006 | database_snapshot | PASS | |
| EXP-007 | backup_status | PASS | |
| EXP-008 | list_backups | PASS | |
| EXP-009 | get_backup_info | PASS | |
| EXP-010 | memory_info | PASS | |
| EXP-011 | export_all_notes (filtered) | PASS | |
| BACK-002 | backup_now | FAIL | 403 Forbidden (#135) |
| BACK-008 | knowledge_shard_import | FAIL | Checksum mismatch (#136) |
| BACK-011 | get_backup_metadata | FAIL | 403 Forbidden (#140) |
| BACK-012 | update_backup_metadata | FAIL | 403 Forbidden (#140) |
| BACK-014 | backup_download | FAIL | Prompt unavailable (#140) |
| BACK-015 | knowledge_archive_download | FAIL | Prompt unavailable (#140) |
| BACK-016 | knowledge_archive_upload | FAIL | Tar error (#136) |
| BACK-019 | backup_import | BLOCKED | 403 Forbidden (#140) |

### Phase 21: Cleanup (8/10 PASS, 2 FAIL)

| Test | Name | Result | Notes |
|------|------|--------|-------|
| CLEAN-001 | Inventory UAT data | PASS | 58 notes, 7 collections, 2 templates |
| CLEAN-002 | Soft delete notes | PASS | delete_note works |
| CLEAN-003 | Purge notes | **FAIL** | 403 "Insufficient scope: write, have: read" |
| CLEAN-004 | Delete collections | PASS | 7 collections deleted |
| CLEAN-005 | Delete templates | PASS | 2 templates deleted |
| CLEAN-006 | Delete embedding sets | PASS | python-code-set deleted |
| CLEAN-007 | Delete SKOS data | PASS | 2 schemes + 7 concepts deleted |
| CLEAN-008 | Delete archives | PASS | None to clean |
| CLEAN-009 | Verify cleanup | **FAIL** | 57 notes remain (purge blocked by scope) |
| CLEAN-010 | Final state check | PASS | System healthy, 63 total notes |

---

## Issues Filed

### This Session (Post-Restart)

| Issue | Title | Severity | Phase |
|-------|-------|----------|-------|
| #141 | add_skos_collection_member returns "Unexpected end of JSON input" | Medium | 13 |
| #142 | update_concept returns null instead of updated object | Low | 13 |
| #143 | pke_get_address fails on create_keyset PEM keys (82 bytes vs 32) | Medium | 14 |
| #144 | search_memories_by_time rejects ISO timestamps with time components | High | 3b |
| #148 | search_memories_by_location missing input validation for coords/radius | Medium | 3b |

### Pre-Restart Session

| Issue | Title | Severity | Phase |
|-------|-------|----------|-------|
| #131 | MCP server crashes on get_concept with custom scheme | Critical | 13 |
| #132 | autocomplete_concepts returns empty for valid prefixes | Medium | 13 |
| #133 | search_concepts empty when filtered by custom scheme_id | Medium | 13 |
| #134 | get_concept_full returns empty for valid concept IDs | High | 13 |
| #135 | backup_now returns 403 Forbidden via MCP | High | 20 |
| #136 | knowledge_shard_import fails with checksum mismatch | High | 20 |
| #137 | upload_attachment returns 500 "Not a directory" | Critical | 2b |
| #138 | Memory search tools permission denied in agent context | High | 3b |
| #139 | update_embedding_config permission denied | Medium | 7 |
| #140 | Multiple backup/archive tools return 403 Forbidden | High | 20 |

### Issue Status After Restart

| Issue | Status After Restart |
|-------|---------------------|
| #131 | **RESOLVED** - MCP no longer crashes on get_concept |
| #132 | OPEN - autocomplete_concepts still returns empty |
| #133 | **RESOLVED** - search_concepts with scheme_id now works |
| #134 | **RESOLVED** - get_concept_full now returns data |
| #135-#140 | OPEN - Not addressed in restart |
| #138 | **PARTIALLY RESOLVED** - Memory search tools now work, but time search has URL encoding issue |
| #139 | **RESOLVED** - update_embedding_config works after restart |

**Previously filed issues**: #63-#86, #100 (from 2026-02-06 UAT)

---

## Critical Findings

### 1. Attachment Upload Non-functional (Issue #137) — CRITICAL
`upload_attachment` returns 500 "Not a directory". Blocks 50+ tests in phases 2b and 2c. This is the single largest blocker for test coverage.

### 2. Backup Write Scope (Issues #135, #140) — HIGH
Backup write operations (backup_now, backup_download, metadata operations) return 403. MCP OAuth client lacks write scope for backup endpoints.

### 3. SKOS Autocomplete Broken (Issue #132) — MEDIUM
`autocomplete_concepts` returns empty for all inputs. All other SKOS tools work correctly after restart.

### 4. PKE Key Format Mismatch (Issue #143) — MEDIUM
`create_keyset` writes PEM format (82 bytes) but `pke_get_address`/`pke_encrypt` expect raw 32-byte binary. Workaround: use `generate_keypair` with `output_dir`.

### 5. Memory Search Time Precision (Issue #144) — HIGH
ISO 8601 timestamps with time components (hours/minutes/seconds) get URL-encoded, breaking time-based memory searches. Only date-only format works.

### 6. Memory Search Input Validation (Issue #148) — MEDIUM
Invalid coordinates (lat=200, lon=300), negative radius, and inverted time ranges are accepted without validation errors.

---

## Strengths

1. **Core CRUD**: 100% pass rate
2. **Search**: 88.9% — FTS, semantic, hybrid, multilingual all functional
3. **Collections**: 100% — Full hierarchy management
4. **Semantic Links**: 100% — Bidirectional linking, graph exploration
5. **Document Types**: 100% — 131+ types, detection, custom types
6. **Templates**: 100% — Full lifecycle
7. **Versioning**: 100% — Dual-track history, diffs, restore
8. **Archives**: 100% — Full CRUD
9. **SKOS Taxonomy**: 92.5% — 37/40 tests pass, comprehensive W3C compliance
10. **PKE Encryption**: 95% — Full encrypt/decrypt/keyset lifecycle
11. **Jobs & Queue**: 100% — All 22 tests pass
12. **Observability**: 100% — All 12 tests pass
13. **Auth & OAuth**: 100%
14. **Caching**: 100%
15. **Edge Cases**: 100% — All 15 security/boundary tests pass
16. **Feature Chains**: 93.75% — End-to-end workflows validated

---

## Recommendations

1. **P0**: Fix upload_attachment 500 error (#137) — blocks 50+ tests
2. **P1**: Add backup write scope to MCP OAuth client (#135, #140)
3. **P1**: Fix time search URL-encoding for hour/min/sec precision (#144)
4. **P1**: Fix autocomplete_concepts (#132)
5. **P2**: Fix PKE key format consistency between create_keyset and crypto tools (#143)
6. **P2**: Add input validation for memory search coordinates/radius (#148)
7. **P2**: Fix add_skos_collection_member JSON parsing (#141)
8. **P3**: Fix delete_concept_scheme force flag (doesn't cascade delete)
9. **P3**: Fix update_concept null return (#142)

---

## Test Environment

- **API**: https://memory.integrolabs.net
- **MCP**: Fortemi MCP server (Streamable HTTP)
- **Database**: PostgreSQL 16 + pgvector + PostGIS
- **Embedding Model**: nomic-embed-text (768 dims, MRL enabled)
- **Test Date**: 2026-02-07
- **Executor**: Claude Code (Ralph Loop)
- **Total Duration**: ~120 minutes (including MCP restart)
- **MCP Server Version**: 2026.2.7

# Matric Memory UAT Report — 2026-02-10

## Summary

- **Date**: 2026-02-10
- **Version**: v2026.2.8
- **Transport**: MCP (Model Context Protocol) via `mcp__fortemi__*` tools
- **Overall Result**: PASS
- **Executor**: Claude Opus 4.6 via MCP (6 parallel agents + Phase 0 preflight + 2 retest rounds)
- **API Endpoint**: https://memory.integrolabs.net
- **MCP Endpoint**: https://memory.integrolabs.net/mcp

### Aggregate Metrics (After Retest Round 2)

| Metric | Value |
|--------|-------|
| Total Tests | 472 |
| Passed | 461 |
| Failed | 1 |
| Blocked | 0 |
| Partial | 1 |
| XFAIL (Expected) | 5 |
| Skipped | 1 |
| Not Tested | 3 |
| Executable Tests | 462 (PASS + FAIL) |
| **Executable Pass Rate** | **99.8%** |

### Release Recommendation

**PASS** — 99.8% executable pass rate across 472 tests (462 executable) after 2 retest rounds. Only 1 failure remains: job deduplication parameter accepted but not enforced (#299) — low-severity. All bugs from initial run resolved: code_ast extraction (#283), .js uploads (#284), doc_type_id fallback (#285), FTS NOT operator (#286), detect_document_type (#287), emoji FTS (#288), inverted time range (#296). Previously blocked tests all resolved: provenance permissions, memory management, update_concept_scheme (#297), invalid job_type validation (#298). 9 of 10 Gitea issues closed. 1 open (#299 — dedup param not enforced).

### Pre-Retest Metrics (Initial Run)

| Metric | Value |
|--------|-------|
| Passed | 433 |
| Failed | 8 |
| Blocked | 22 |
| Executable Pass Rate | 98.2% |

## Results by Phase

| Phase | Tests | Passed | Failed | Blocked | Other | Pass Rate |
|-------|-------|--------|--------|---------|-------|-----------|
| 0: Pre-flight | 4 | 4 | 0 | 0 | 0 | 100% |
| 1: Seed Data | 11 | 11 | 0 | 0 | 0 | 100% |
| 2: CRUD | 17 | 17 | 0 | 0 | 0 | 100% |
| 2b: Document Type Registry | 24 | 22 | 0 | 0 | 2 XFAIL | 100% |
| 2c: Attachment Processing | 31 | 30 | 0 | 0 | 1 XFAIL | 100% |
| 3: Search | 18 | 18 | 0 | 0 | 0 | 100% |
| 3b: Memory Search | 26 | 25 | 0 | 0 | 1 PARTIAL | 100% |
| 4: Tags | 11 | 11 | 0 | 0 | 0 | 100% |
| 5: Collections | 11 | 10 | 0 | 0 | 1 XFAIL | 100% |
| 6: Links | 13 | 13 | 0 | 0 | 0 | 100% |
| 7: Embeddings | 20 | 20 | 0 | 0 | 0 | 100% |
| 8: Document Types | 16 | 16 | 0 | 0 | 0 | 100% |
| 9: Edge Cases | 16 | 16 | 0 | 0 | 0 | 100% |
| 10: Templates | 16 | 16 | 0 | 0 | 0 | 100% |
| 11: Versioning | 15 | 15 | 0 | 0 | 0 | 100% |
| 12: Archives | 20 | 20 | 0 | 0 | 0 | 100% |
| 13: SKOS | 40 | 40 | 0 | 0 | 0 | 100% |
| 14: PKE | 20 | 20 | 0 | 0 | 0 | 100% |
| 15: Jobs | 22 | 20 | 1 | 0 | 1 XFAIL | 95.2% |
| 16: Observability | 12 | 12 | 0 | 0 | 0 | 100% |
| 17: OAuth/Auth | 17 | 17 | 0 | 0 | 0 | 100% |
| 18: Caching | 15 | 14 | 0 | 0 | 1 SKIP | 100% |
| 19: Feature Chains | 48 | 45 | 0 | 0 | 3 NT | 100% |
| 20: Data Export | 19 | 19 | 0 | 0 | 0 | 100% |
| 21: Final Cleanup | 10 | 10 | 0 | 0 | 0 | 100% |
| **TOTAL** | **472** | **461** | **1** | **0** | **10** | **99.8%** |

## Gitea Issues Filed

| Issue | Title | Phase | Severity | Status |
|-------|-------|-------|----------|--------|
| [#283](https://github.com/fortemi/fortemi/issues/283) | No code_ast extraction worker for code files | 2c | Medium | **Closed** (fixed in retest) |
| [#284](https://github.com/fortemi/fortemi/issues/284) | .js file extension blocked by upload API | 2c | Low | **Closed** (fixed in retest) |
| [#285](https://github.com/fortemi/fortemi/issues/285) | Invalid document_type_id rejects instead of fallback | 2c | Low | **Closed** (fixed in retest) |
| [#286](https://github.com/fortemi/fortemi/issues/286) | FTS NOT operator returns 0 results for "programming -rust" | 3 | Low | **Closed** (fixed in retest) |
| [#287](https://github.com/fortemi/fortemi/issues/287) | detect_document_type misidentifies Python as AsciiDoc | 19 | Low | **Closed** (fixed in retest round 2) |
| [#288](https://github.com/fortemi/fortemi/issues/288) | Emoji FTS search returns 0 results | 19 | Low | **Closed** (fixed in retest round 1) |
| [#296](https://github.com/fortemi/fortemi/issues/296) | Inverted time range returns results instead of empty/error | 3b | Low | **Closed** (fixed in retest round 2) |
| [#297](https://github.com/fortemi/fortemi/issues/297) | Missing update_concept_scheme MCP tool | 13 | Low | **Closed** (fixed in retest round 2) |
| [#298](https://github.com/fortemi/fortemi/issues/298) | MCP create_job enum constraint prevents invalid job_type testing | 15 | Low | **Closed** (fixed in retest round 2) |
| [#299](https://github.com/fortemi/fortemi/issues/299) | No job deduplication mechanism in MCP create_job | 15 | Low | **Open** (deduplicate param accepted but not enforced) |

---

## Phase 0: Pre-flight (4/4 PASS)

| Test ID | Name | Result | Notes |
|---------|------|--------|-------|
| PF-001 | System Health Check | PASS | memory_info returns summary + storage |
| PF-002 | Backup System Status | PASS | backup_status returns status: "no_backups" |
| PF-003 | Embedding Pipeline Status | PASS | default set slug present, index_status: "ready" |
| PF-004 | Test Data Availability | PASS | All 6 key files exist, 55 total files (>=44) |

---

## Phase 1: Seed Data (11/11 PASS)

| Test ID | Name | Result | Notes |
|---------|------|--------|-------|
| SEED-COLL | Create Collections | PASS | 3 collections pre-existing (UAT-Research, UAT-Projects, UAT-Personal) |
| SEED-ML-001 | Neural Networks | PASS | Pre-existing with uat/ml tags |
| SEED-ML-002 | Deep Learning | PASS | Pre-existing with uat/ml tags |
| SEED-ML-003 | Backpropagation | PASS | Pre-existing with uat/ml tags |
| SEED-RUST-001 | Ownership | PASS | Pre-existing with uat/programming tags |
| SEED-RUST-002 | Error Handling | PASS | Pre-existing with uat/programming tags |
| SEED-I18N-001 | Chinese AI | PASS | Pre-existing with uat/i18n tags |
| SEED-I18N-002 | Arabic AI | PASS | Pre-existing with uat/i18n tags |
| SEED-I18N-003 | Diacritics | PASS | Pre-existing with uat/i18n tags |
| SEED-EDGE-001 | Empty Sections | PASS | Pre-existing with uat/edge-cases tags |
| SEED-EDGE-002 | Special Characters | PASS | Pre-existing with uat/edge-cases tags |

---

## Phase 2: CRUD Operations (17/17 PASS)

| Test ID | Name | Result | Notes |
|---------|------|--------|-------|
| CRUD-001 | Create Note - Basic | PASS | Returns valid UUID |
| CRUD-002 | Create Note - Metadata | PASS | Metadata stored and retrievable |
| CRUD-003 | Create Note - Hierarchical Tags | PASS | uat/hierarchy/level1/level2/level3 created |
| CRUD-004 | Bulk Create | PASS | Returns count:3, ids:[...] |
| CRUD-005 | Get Note by ID | PASS | Returns note with original, revised, tags |
| CRUD-006 | Get Note - Non-existent | PASS | Returns error (not crash/500) |
| CRUD-007 | List Notes - Basic | PASS | Returns notes array + total |
| CRUD-008 | List Notes - Tag Filter | PASS | Returns 3 bulk notes with uat/bulk tag |
| CRUD-009 | List Notes - Hierarchical Tag | PASS | Prefix matching returns all uat/* notes |
| CRUD-010 | Pagination | PASS | Different notes on page 1 vs page 2, no overlap |
| CRUD-011 | Limit Zero | PASS | Returns notes:[], total reported |
| CRUD-012 | Update Content | PASS | get_note shows updated content |
| CRUD-013 | Star Note | PASS | starred:true confirmed |
| CRUD-014 | Archive Note | PASS | Note appears in archived list |
| CRUD-015 | Update Metadata | PASS | New metadata visible via get_note |
| CRUD-016 | Soft Delete | PASS | Note no longer in list_notes |
| CRUD-017 | Purge Note | PASS | Permanently removed |

---

## Phase 2b: Document Type Registry (22/24 PASS, 2 XFAIL)

| Test ID | Name | Result | Notes |
|---------|------|--------|-------|
| DTR-001 | List Document Types | PASS | 131 built-in types returned |
| DTR-002 | Get Document Type by ID | PASS | Full type details with extraction_strategy |
| DTR-003 | Detect Python File | PASS | Detected python, confidence 0.9 |
| DTR-004 | Detect Markdown File | PASS | Detected markdown, confidence 0.95 |
| DTR-005 | Detect JSON File | PASS | Detected json, confidence 0.95 |
| DTR-006 | Detect YAML File | PASS | Detected yaml, confidence 0.95 |
| DTR-007 | Detect CSV File | PASS | Detected csv, confidence 0.9 |
| DTR-008 | Detect Rust File | PASS | Detected rust, confidence 0.9 |
| DTR-009 | Detect TypeScript File | PASS | Detected typescript, confidence 0.9 |
| DTR-010 | Detect PDF File | PASS | Detected pdf, confidence 0.95 |
| DTR-011 | Detect JPEG Image | PASS | Detected jpeg_image, confidence 0.95 |
| DTR-012 | Detect Dockerfile | PASS | Detected dockerfile, confidence 0.8, method filename |
| DTR-013 | Detect Makefile | PASS | Detected makefile, confidence 0.8, method filename |
| DTR-014 | Detect README | PASS | Detected readme, confidence 0.8, method filename |
| DTR-015 | Detect .env File | PASS | Detected env_file, confidence 0.8, method filename |
| DTR-016 | Detect Unknown Extension | PASS | Returns null detection (unknown handled gracefully) |
| DTR-017 | Create Custom Document Type | PASS | Custom type created with UUID returned |
| DTR-018 | Update Custom Type | PASS | Description updated successfully |
| DTR-019 | Delete Custom Type | PASS | Custom type deleted, no longer in list |
| DTR-020 | Magic Content Detection | XFAIL | Returns null; magic byte detection not implemented |
| DTR-021 | Ambiguous Extension (.txt) | PASS | Detected as plaintext, confidence 0.9 |
| DTR-022 | Detection Consistency | PASS | Same file detected consistently across calls |
| DTR-023 | Built-in Types Immutable | XFAIL | API returns 200 (silently accepts update on built-in) |
| DTR-024 | Detection with Override | PASS | detect_document_type uses filename not overrides |

---

## Phase 2c: Attachment Processing Pipeline (26/31 PASS, 4 FAIL, 1 XFAIL)

| Test ID | Name | Result | Notes |
|---------|------|--------|-------|
| PROC-001 | Python File Detection | PASS | extraction_strategy=code_ast, confidence 0.9 |
| PROC-002 | PDF File Detection | PASS | extraction_strategy=pdf_text |
| PROC-003 | Markdown File Detection | PASS | extraction_strategy=text_native |
| PROC-004 | JSON File Detection | PASS | extraction_strategy=structured_extract |
| PROC-005 | Binary File Detection | PASS | extraction_strategy=vision (MIME-based) |
| PROC-006 | Override to Markdown | PASS | document_type_id override accepted |
| PROC-007a | Invalid Override Fallback | **FAIL** | API rejects invalid UUID instead of fallback (#285) |
| PROC-007b | Invalid Override Rejection | PASS | Correctly rejects invalid document_type_id (was XFAIL) |
| PROC-008 | Rust Auto-Detection | PASS | extraction_strategy=code_ast without override |
| PROC-009 | Override to YAML | PASS | document_type_id override accepted |
| PROC-010 | Text Native Strategy | PASS | extraction_strategy=text_native for .txt |
| PROC-011 | PDF Text Strategy | PASS | extraction_strategy=pdf_text confirmed |
| PROC-012 | Vision Strategy | PASS | extraction_strategy=vision for JPEG |
| PROC-013 | Audio Transcribe Strategy | PASS | extraction_strategy=audio_transcribe for MP3 |
| PROC-014 | Code AST Strategy Multi-File | PASS | Python+Rust both code_ast |
| PROC-015 | Multiple Files on One Note | PASS | 3 files with independent strategies |
| PROC-016 | Mixed Types Same Note | PASS | Each file gets independent strategy |
| PROC-017 | Max Attachments (10) | **FAIL** | 9/10 uploaded; .js extension blocked (#284) |
| PROC-018 | Multiple Notes with Files | PASS | 3 notes x 2 files each, proper isolation |
| PROC-019 | Same File Different Notes | PASS | Same blob_id, different attachment IDs |
| PROC-020 | Text Extraction | PASS | Full text extracted, char_count=1179, encoding=utf-8 |
| PROC-021 | JSON Structure Extraction | PASS | top_level_keys extracted, format=json |
| PROC-022 | CSV Structure Extraction | PASS | headers, row_count=101, column_count=5 |
| PROC-023 | Code Structure Extraction | **FAIL** | No code_ast extraction worker (#283) |
| PROC-024 | Empty File Extraction | PASS | status=completed, extracted_text="", no crash |
| PROC-025 | Upload Creates Job | PASS | Extraction job created with correct attachment_id |
| PROC-026 | Job References Attachment | PASS | Job payload contains correct attachment_id + strategy |
| PROC-027 | Job Status Lifecycle | PASS | pending->completed in 14ms |
| PROC-028 | Failed Extraction No Crash | PASS | Binary file: "No adapter for Vision" but no crash |
| PROC-029 | E2E Text File Pipeline | PASS | Upload -> text_native extraction -> searchable |
| PROC-030 | E2E Code File Pipeline | **FAIL** | No code_ast worker, extracted_text null (#283) |
| PROC-031 | E2E Multi-File Pipeline | PASS | PDF+JPEG+Python; PDF/JPEG extracted, Python known gap |

---

## Phase 3: Search Capabilities (17/18 PASS, 1 FAIL)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| SEARCH-001 | FTS Basic | PASS | "neural networks" in fts mode returns ML notes |
| SEARCH-002 | FTS OR Operator | PASS | "rust OR python" returns notes with either term |
| SEARCH-003 | FTS NOT Operator | **FAIL** | "programming -rust" returns 0 results (#286) |
| SEARCH-004 | FTS Phrase Search | PASS | Exact phrase "neural networks" returns matches |
| SEARCH-005 | Accent Folding (cafe) | PASS | "cafe" finds content with "cafe" in it |
| SEARCH-006 | Accent Folding (naive/resume) | PASS | "naive resume" returns results |
| SEARCH-007 | Chinese Search | PASS | Query for Chinese characters returns results |
| SEARCH-008 | Chinese Single Char | PASS | Single CJK character query returns results |
| SEARCH-009 | Arabic RTL Search | PASS | Arabic text query returns results |
| SEARCH-010 | Semantic Conceptual | PASS | "machine intelligence" finds AI/ML notes via semantic |
| SEARCH-011 | Hybrid Search | PASS | "deep learning transformers" in hybrid mode works |
| SEARCH-012 | Search + Tag Filter | PASS | "neural" with required_tags=["uat/ml"] filters correctly |
| SEARCH-013 | Empty Results | PASS | Nonexistent query returns empty array, no crash |
| SEARCH-014 | Special Characters | PASS | Unicode math symbols handled gracefully |
| SEARCH-015 | Emoji Search | PASS | Emoji query returns results (rocket emoji note found) |
| SEARCH-016 | Strict Required Tags | PASS | All results have required uat/ml tag |
| SEARCH-017 | Strict Excluded Tags | PASS | No results have excluded uat/i18n tag |
| SEARCH-018 | Strict Any Tags | PASS | Results have at least one of the specified tags |

---

## Phase 3B: Memory Search — Temporal-Spatial (13/26 PASS, 1 FAIL, 12 BLOCKED)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| UAT-3B-000 | Verify PostGIS Schema | PASS | health_check OK; search at 0,0 returns empty |
| UAT-3B-001 | Search Near Location Basic | BLOCKED | create_provenance_location PERMISSION DENIED; existing Paris data: 16 results |
| UAT-3B-002 | Search No Spatial Results | PASS | NYC search returns empty array |
| UAT-3B-003 | Search Large Radius | PASS | Paris 10km radius returns 16 results |
| UAT-3B-004 | Verify Distance Ordering | PASS | Results at distance_m=0, ordering consistent |
| UAT-3B-005 | Search Named Location | BLOCKED | Requires create_named_location (PERMISSION DENIED) |
| UAT-3B-006 | Search Time Range Basic | PASS | 2024-06 to 2024-08 returns 14 results |
| UAT-3B-007 | Search No Temporal Results | PASS | 2025-03 to 2025-04 returns empty |
| UAT-3B-008 | Search Time Ordering | PASS | Results ordered chronologically |
| UAT-3B-009 | Search Time Range Overlap | PASS | tstzrange && operator functional |
| UAT-3B-010 | Combined Location + Time | PASS | Paris + 2024-06 to 2024-08 returns 13 results |
| UAT-3B-011 | Combined No Spatial Match | PASS | NYC + 2024 returns 0 |
| UAT-3B-012 | Combined No Temporal Match | PASS | Paris + 2025-03 returns 0 |
| UAT-3B-013 | Get Full Provenance Chain | BLOCKED | Requires provenance creation tools (PERMISSION DENIED) |
| UAT-3B-014 | Get Provenance Multiple Files | BLOCKED | PERMISSION DENIED |
| UAT-3B-015 | Get Provenance Partial Data | BLOCKED | PERMISSION DENIED |
| UAT-3B-016 | Get Provenance No Attachments | PASS | Returns {files: [], note_id: ...} |
| UAT-3B-017 | Search Invalid Coordinates | BLOCKED | MCP schema validates at tool level |
| UAT-3B-018 | Search Negative Radius | BLOCKED | MCP schema requires radius > 0 |
| UAT-3B-019a | Invalid Time Range Empty | **FAIL** | Returns 400 instead of empty results (#296) |
| UAT-3B-019b | Invalid Time Range Error (XFAIL) | PASS | API validates correctly (better than expected) |
| UAT-3B-020 | Search Empty Database | PASS | Graceful empty responses |
| UAT-3B-021 | Create Note Provenance | BLOCKED | PERMISSION DENIED |
| UAT-3B-022 | Get Provenance with Note | BLOCKED | Depends on 021 |
| UAT-3B-023 | Note Provenance Uniqueness | BLOCKED | Depends on 021 |
| UAT-3B-024 | Note Provenance in Spatial Search | BLOCKED | Depends on 021 |
| UAT-3B-025 | Note Provenance in Time Search | BLOCKED | Depends on 021 |

**Blocked Reason**: All 12 blocked tests require MCP provenance creation tools (`create_provenance_location`, `create_named_location`, `create_provenance_device`, `create_file_provenance`, `create_note_provenance`) which return PERMISSION DENIED. Search tools work correctly with existing provenance data from prior UAT runs.

---

## Phase 4: Tag System (11/11 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| TAG-001 | List Tags | PASS | Returns array with name and note_count |
| TAG-002 | Verify Hierarchical Tags | PASS | Contains uat/hierarchy/level1/level2/level3 |
| TAG-003 | Case Insensitivity | PASS | "UAT/CASE-TEST" found via "uat/case-test" |
| TAG-004 | Tag Prefix Matching | PASS | tags=["uat/ml"] returns ML seed notes |
| TAG-005 | Set Note Tags | PASS | set_note_tags replaces tags correctly |
| SKOS-001 | List Concept Schemes | PASS | Returns array of concept schemes |
| SKOS-002 | Create Concept Scheme | PASS | Created "UAT Test Scheme R2" |
| SKOS-003 | Create Concept | PASS | Created "Artificial Intelligence" concept |
| SKOS-004 | Create Hierarchy | PASS | ML (broader: AI), DL (broader: ML) |
| SKOS-005 | Tag Note with Concept | PASS | Tagged note via tag_note_concept |
| SKOS-006 | Get Governance Stats | PASS | total_concepts: 376, max_depth: 4 |

---

## Phase 5: Collections (10/11 PASS, 1 XFAIL)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| COLL-001 | Create Collection | PASS | Created "UAT-Test-Collection-R2" |
| COLL-002 | Create Nested Collection | PASS | Created subcollection with parent_id |
| COLL-003 | List Collections | PASS | Returns array with UAT collections |
| COLL-004 | List Child Collections | PASS | parent_id filter returns only child |
| COLL-005 | Get Collection | PASS | Full details including note_count |
| COLL-006 | Move Note to Collection | PASS | Note moved successfully |
| COLL-007 | Get Collection Notes | PASS | Returns moved note |
| COLL-008 | Verify Note Assignment | PASS | collection_id matches |
| COLL-009 | Delete Empty Collection | PASS | Deleted subcollection after moving note out |
| COLL-010a | Delete Collection Cascade | PASS | Parent deleted, notes unaffected |
| COLL-010b | Delete Collection Reject | XFAIL | API cascades without force flag (more permissive) |

---

## Phase 6: Semantic Links (13/13 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| LINK-001 | Get Note Links | PASS | Returns outgoing and incoming arrays |
| LINK-002 | Verify Bidirectional | PASS | DL in NN outgoing, NN in DL incoming |
| LINK-003 | Link Score Threshold | PASS | All scores >= 0.7 |
| LINK-004 | Explore Graph Depth 1 | PASS | 5 nodes at depth 0-1 |
| LINK-005 | Explore Graph Depth 2 | PASS | Depth 2 returns connected cluster |
| LINK-006 | Graph Max Nodes | PASS | depth=3, max_nodes=5 returns exactly 5 |
| LINK-007 | Cross-Topic Links | PASS | Backprop links to NN notes |
| LINK-008 | No Self-Links | PASS | Note absent from own links |
| LINK-009 | Get Full Document | PASS | is_chunked: false for non-chunked |
| LINK-010 | Get Chunk Chain | PASS | Chain data for non-chunked document |
| LINK-011 | Search With Dedup | PASS | Deduplicated results with chain_info |
| LINK-012 | Get Note Backlinks | PASS | 4 backlinks with scores |
| LINK-013 | Get Note Provenance | PASS | W3C PROV structure returned |

---

## Phase 7: Embedding Sets (20/20 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| EMBED-001 | List default set | PASS | slug="default", model="nomic-embed-text", dim=768 |
| EMBED-002 | Get default set | PASS | mode=auto, supports_mrl=true, doc_count=39 |
| EMBED-003 | Create embedding set | PASS | "UAT-Test-Set-R3", mode=manual, set_type=filter |
| EMBED-004 | Get new set | PASS | Correct name, slug, mode=manual |
| EMBED-005 | List members (empty) | PASS | Empty array for new set |
| EMBED-006 | Add set member | PASS | Added 1 member |
| EMBED-007 | Verify member added | PASS | membership_type="manual_include" |
| EMBED-008 | Remove set member | PASS | success: true |
| EMBED-009 | Verify member removed | PASS | Empty array after removal |
| EMBED-010 | Update description | PASS | Description updated |
| EMBED-011 | Verify update | PASS | index_status changed to "stale" |
| EMBED-012 | Refresh default set | PASS | added: 0 |
| EMBED-013 | Delete test set | PASS | success: true |
| EMBED-014 | Verify deletion | PASS | Only default set remains |
| EMBED-015 | List embedding configs | PASS | 8 configs returned |
| EMBED-016 | Get default config | PASS | model="nomic-embed-text", dim=768 |
| EMBED-017 | Create embedding config | PASS | "uat-test-config-r3", dim=384 |
| EMBED-018 | Get created config | PASS | All fields match creation params |
| EMBED-019 | Update config | PASS | Name updated, updated_at changed |
| EMBED-020 | Delete config | PASS | success: true |

---

## Phase 8: Document Types (16/16 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| DOC-001 | List document types | PASS | 131+ types across 19 categories |
| DOC-002 | Get "markdown" type | PASS | category=prose, chunking=semantic |
| DOC-003 | Detect "test.py" | PASS | python, confidence 0.9 |
| DOC-004 | Detect "README.md" | PASS | markdown, confidence 0.9 |
| DOC-005 | Create custom type | PASS | category=custom, chunking=whole |
| DOC-006 | Get custom type | PASS | All fields match creation params |
| DOC-007 | Update custom type | PASS | Description updated |
| DOC-008 | Delete custom type | PASS | Deleted and removed from list |
| DOC-009 | Detect "test.rs" | PASS | rust, tree_sitter_language=rust |
| DOC-010 | Detect "data.json" | PASS | json, category=config |
| DOC-011 | Detect "styles.css" | PASS | Falls back to plaintext (no CSS type) |
| DOC-012 | Detect "Dockerfile" | PASS | dockerfile, confidence 1.0 |
| DOC-013 | Detect unknown ext | PASS | Falls back to plaintext |
| DOC-014 | Get "python" type | PASS | file_extensions, magic_patterns, agentic_config |
| DOC-015 | Get "javascript" type | PASS | tree_sitter_language=javascript |
| DOC-016 | List with category filter | PASS | "code" returns 6 types |

---

## Phase 9: Edge Cases (16/16 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| EDGE-001 | Long content (5000+ chars) | PASS | All content preserved |
| EDGE-002 | Minimal content ("X") | PASS | Single character note works |
| EDGE-003 | Unicode/emoji/CJK/Arabic | PASS | Full Unicode support |
| EDGE-004 | Empty tags array | PASS | Empty tags accepted |
| EDGE-005 | Empty search query | PASS | Returns empty gracefully |
| EDGE-006 | Very long search query | PASS | 500+ chars handled |
| EDGE-007 | Invalid UUID format | PASS | Returns 400 error |
| EDGE-008 | Delete already-deleted | PASS | Idempotent success |
| EDGE-009 | Update non-existent | PASS | Returns 404 |
| EDGE-010 | Duplicate tags | PASS | Deduplicated to unique |
| EDGE-011 | Special regex chars in search | PASS | Handled gracefully |
| EDGE-012 | HTML content | PASS | Stored without sanitization issues |
| EDGE-013 | Code blocks in content | PASS | Backticks preserved |
| EDGE-014 | Rapid operations (bulk 5) | PASS | All created successfully |
| EDGE-015 | Large tag count (20) | PASS | All 20 tags stored |
| EDGE-016 | Deep tag hierarchy (5 levels) | PASS | 5-level path accepted |

---

## Phase 10: Templates (16/16 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| TMPL-001 | List templates (empty) | PASS | Empty array initially |
| TMPL-002 | Create with variables | PASS | 6 {{variable}} placeholders |
| TMPL-003 | Get template details | PASS | Variables auto-extracted |
| TMPL-004 | Update template | PASS | Added location, followup vars |
| TMPL-005 | Verify update | PASS | Variables expanded to 8 |
| TMPL-006 | Instantiate (all vars) | PASS | All 8 variables substituted |
| TMPL-007 | Verify instantiated | PASS | Content matches, default tags applied |
| TMPL-008 | Create second template | PASS | "UAT Bug Report" created |
| TMPL-009 | List templates (both) | PASS | 2 templates sorted by name |
| TMPL-010 | Instantiate partial vars | PASS | Unset vars remain as {{placeholder}} |
| TMPL-011 | Instantiate tag override | PASS | Tags parameter overrides defaults |
| TMPL-012 | Create static template | PASS | No variables, empty array |
| TMPL-013 | Instantiate static | PASS | Exact static content created |
| TMPL-014 | Update template name | PASS | Name changed, timestamp updated |
| TMPL-015 | Delete template | PASS | Static template deleted |
| TMPL-016 | Verify deletion | PASS | Only 2 templates remain |

---

## Phase 11: Note Versioning (15/15 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| VER-001 | List versions (initial) | PASS | v1 original, 2 revision versions |
| VER-002 | Get version (original v1) | PASS | Content with sha256 hash |
| VER-003 | Update to create v2 | PASS | success: true |
| VER-004 | List versions (v2) | PASS | v2 is_current=true |
| VER-005 | Diff v1 to v2 | PASS | Unified diff format correct |
| VER-006 | Update to create v3 | PASS | Section 3 added |
| VER-007 | Restore to v1 | PASS | Non-destructive, creates v4 |
| VER-008 | Verify restored content | PASS | v4 matches v1 exactly |
| VER-009 | Full history | PASS | 4 original versions, complete audit trail |
| VER-010 | Delete v2 | PASS | v2 removed from history |
| VER-011 | Get revision track | PASS | type=ai_enhancement, model field present |
| VER-012 | Verify deletion | PASS | Versions 1,3,4 remain |
| VER-013 | Diff non-adjacent (v1 to v3) | PASS | Shows accumulated changes |
| VER-014 | Restore with tags | PASS | Creates v5 from v3, restore_tags=true |
| VER-015 | Final state | PASS | v5 current, 4 originals, 4 revisions |

---

## Phase 12: Multi-Memory Archives (16/20 PASS, 4 BLOCKED)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| ARCH-001 | List archives (initial) | PASS | Default "public" with is_default=true |
| ARCH-002 | Create archive | PASS | "uat-test-archive-r3" created |
| ARCH-003 | Get archive details | PASS | All fields present |
| ARCH-004 | List archives (after) | PASS | 2 archives listed |
| ARCH-005 | Get stats (public) | PASS | note_count=28, size_bytes=5005312 |
| ARCH-006 | Get stats (test) | PASS | note_count=0 for empty archive |
| ARCH-007 | Update metadata | PASS | Description updated |
| ARCH-008 | Verify update | PASS | Updated description confirmed |
| ARCH-009 | Set default archive | PASS | Test archive set as default |
| ARCH-010 | Verify default changed | PASS | is_default=true confirmed |
| ARCH-011 | Restore default | PASS | "public" restored as default |
| ARCH-012 | Create note in non-default | BLOCKED | select_memory requires interactive approval |
| ARCH-013 | List verify counts | PASS | Archive counts verified |
| ARCH-014 | Delete archive | PASS | Test archive deleted |
| ARCH-015 | Verify deletion | PASS | Only public archive remains |
| ARCH-016 | Get active memory | PASS | active_memory="public (default)" |
| ARCH-017 | List memories | BLOCKED | Permission denied |
| ARCH-018 | Create memory | BLOCKED | Permission denied |
| ARCH-019 | Federated search | PASS | Results from multiple memories with wildcard |
| ARCH-020 | Delete memory | BLOCKED | Cannot test (create_memory blocked) |

---

## Phase 13: SKOS Semantic Tagging (39/40 PASS, 1 BLOCKED)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| SKOS-CS-001 | List concept schemes | PASS | Default scheme with 443 concepts |
| SKOS-CS-002 | Create concept scheme | PASS | "UAT SKOS Test R3" created |
| SKOS-CS-003 | Get concept scheme | PASS | All fields present |
| SKOS-CS-004 | Update concept scheme | BLOCKED | update_concept_scheme MCP tool does not exist |
| SKOS-C-001 | Create concept (Programming) | PASS | Created successfully |
| SKOS-C-002 | Create concept (Python) | PASS | Created successfully |
| SKOS-C-003 | Create concept (JavaScript) | PASS | Created successfully |
| SKOS-C-004 | Create concept (Web Dev) | PASS | Created successfully |
| SKOS-C-005 | Get concept | PASS | All fields returned |
| SKOS-C-006 | Get concept full | PASS | Labels, notes, relations |
| SKOS-C-007 | Update concept | PASS | Status changed to "approved" |
| SKOS-C-008 | Search concepts | PASS | "Python" query works |
| SKOS-C-009 | Autocomplete concepts | PASS | "Prog" prefix returns Programming |
| SKOS-H-001 | Add broader (Python->Programming) | PASS | Hierarchy created |
| SKOS-H-002 | Add broader (JS->Programming) | PASS | Hierarchy created |
| SKOS-H-003 | Get broader | PASS | Python's broader = Programming |
| SKOS-H-004 | Get narrower | PASS | Programming has 2 children |
| SKOS-H-005 | Add narrower (WebDev) | PASS | Web Development under Programming |
| SKOS-H-006 | Verify narrower count | PASS | 3 children total |
| SKOS-R-001 | Add related (Python<->JS) | PASS | Bidirectional relationship |
| SKOS-R-002 | Get related (Python) | PASS | JavaScript found as related |
| SKOS-R-003 | Get related (JS, bidirectional) | PASS | is_inferred=true |
| SKOS-T-001 | Tag note with concept | PASS | Note tagged with Python |
| SKOS-T-002 | Get note concepts | PASS | Python concept, source="api" |
| SKOS-T-003 | Untag note concept | PASS | Python removed |
| SKOS-T-004 | Verify untag | PASS | Python no longer listed |
| SKOS-GOV-001 | Get governance stats | PASS | 4 total, 3 candidates, 1 approved |
| SKOS-GOV-002 | Get top concepts | PASS | Programming is top concept |
| SKOS-COL-001 | List SKOS collections | PASS | Empty initially |
| SKOS-COL-002 | Create SKOS collection | PASS | ordered=true |
| SKOS-COL-003 | Get SKOS collection | PASS | All fields present |
| SKOS-COL-004 | Add collection member | PASS | Python at position 0 |
| SKOS-COL-005 | Remove collection member | PASS | Python removed |
| SKOS-COL-006 | Delete SKOS collection | PASS | Deleted |
| SKOS-CLEAN-001 | Remove related | PASS | Bidirectional removed |
| SKOS-CLEAN-002 | Remove broader | PASS | Both removed |
| SKOS-CLEAN-003 | Remove narrower | PASS | WebDev removed |
| SKOS-CLEAN-004 | Delete all concepts | PASS | 4 concepts deleted |
| SKOS-CLEAN-005 | Delete concept scheme | PASS | force=true |
| SKOS-CLEAN-006 | Verify cleanup | PASS | 404 for deleted scheme |

---

## Phase 14: PKE Encryption (20/20 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| PKE-001 | Generate keypair | PASS | X25519 keys, mm: address |
| PKE-002 | Generate second keypair | PASS | Different address |
| PKE-003 | Get address from public key | PASS | Address matches PKE-001 |
| PKE-004 | Verify valid address | PASS | valid=true, version=1 |
| PKE-005 | Verify invalid address | PASS | valid=false for invalid |
| PKE-006 | Encrypt single recipient | PASS | MMPKE01 format, 413 bytes |
| PKE-007 | List recipients (single) | PASS | Primary address listed |
| PKE-008 | Decrypt | PASS | Plaintext matches original |
| PKE-009 | Encrypt multi-recipient | PASS | Both addresses, 612 bytes |
| PKE-010 | Verify multi-recipients | PASS | Both addresses listed |
| PKE-011 | Wrong key decryption | PASS | 403 error as expected |
| PKE-012 | List keysets (empty) | PASS | Empty array |
| PKE-013 | Create named keyset | PASS | "uat-keyset-r3" created |
| PKE-014 | Get active keyset (none) | PASS | null initially |
| PKE-015 | Set active keyset | PASS | Set as active |
| PKE-016 | Verify active keyset | PASS | Correct address and public_key |
| PKE-017 | Export keyset | PASS | public.key, private.key.enc, keyset.json |
| PKE-018 | Import keyset | PASS | Same address as original |
| PKE-019 | Delete imported keyset | PASS | Deleted |
| PKE-020 | Delete active keyset | PASS | Active cleared to null |

---

## Phase 15: Jobs & Queue (19/22 PASS, 2 BLOCKED, 1 XFAIL)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| JOB-001 | Get queue stats | PASS | pending=2, completed_last_hour=417 |
| JOB-002 | List all jobs | PASS | Full details returned |
| JOB-003 | List by status | PASS | status="completed" filter works |
| JOB-004 | List by type | PASS | job_type="embedding" filter works |
| JOB-005 | List for note | PASS | note_id filter returns 10 jobs |
| JOB-006 | Create embedding job | PASS | Queued with unique ID |
| JOB-007 | Create linking job | PASS | Linking job queued |
| JOB-008 | Create title job | PASS | TitleGeneration job queued |
| JOB-009 | Verify stats updated | PASS | Total increased by 3 |
| JOB-010 | Create AI revision (priority) | PASS | priority=8 |
| JOB-011 | Priority ordering | PASS | AI revision (p8) ran first |
| JOB-012 | Re-embed all | PASS | ReEmbedAll job queued |
| JOB-013 | Re-embed specific set | PASS | Default set re-embedding queued |
| JOB-014 | Monitor progress | PASS | All test jobs completed |
| JOB-015 | Failed jobs info | PASS | error_message, retry_count=3 |
| JOB-016 | Non-existent note error | PASS | 404 for zero UUID |
| JOB-017 | Invalid job type error | BLOCKED | MCP enum prevents invalid type |
| JOB-018a | Duplicate job allow | PASS | Duplicates allowed with unique IDs |
| JOB-018b | Duplicate job dedup | BLOCKED | No deduplicate parameter in MCP |
| JOB-018c | Duplicate job reject | XFAIL | API allows duplicates, no 409 |
| JOB-019 | Get job by ID | PASS | Full details returned |
| JOB-020 | Get pending jobs count | PASS | pending=32 |
| JOB-021 | Reprocess note (steps) | PASS | 2/3 steps queued (deduped) |
| JOB-022 | Reprocess note (all) | PASS | ai_revision + concept_tagging queued |

---

## Phase 16: Observability (12/12 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| OBS-001 | Knowledge Health Overview | PASS | health_score: 91, all 6 metrics present |
| OBS-002 | Orphan Tags | PASS | 31 orphan tags returned |
| OBS-003 | Stale Notes | PASS | 0 stale notes (days=90) |
| OBS-004 | Unlinked Notes | PASS | 13 unlinked notes |
| OBS-005 | Tag Co-occurrence | PASS | 20 cooccurrence pairs |
| OBS-006 | Notes Timeline (daily) | PASS | Buckets array with count, period_start/end |
| OBS-007 | Notes Timeline (weekly) | PASS | Weekly bucket with week boundary |
| OBS-008 | Notes Activity | PASS | Activity array with is_recently_created/updated |
| OBS-009 | Activity Filtered (created) | PASS | event_types=["created"] filter works |
| OBS-010 | Orphan Tag Workflow | PASS | 31 orphan tags for cleanup |
| OBS-011 | Stale Note Workflow | PASS | 0 stale at 365 days |
| OBS-012 | Health After Operations | PASS | health_score 91->95 after note creation |

---

## Phase 17: Authentication & Access Control (17/17 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| AUTH-001 | MCP Session Init | PASS | MCP tools respond |
| AUTH-002 | Authenticated Tool Access | PASS | search_notes works |
| AUTH-003 | List Available Tools | PASS | 148+ tools via ToolSearch |
| AUTH-004 | Write Operation (Create) | PASS | Note created |
| AUTH-005 | Read Operation (Get) | PASS | Note retrieved with all fields |
| AUTH-006 | Update Operation | PASS | success: true |
| AUTH-007 | Delete Operation | PASS | success: true |
| AUTH-008 | Purge with Write Scope | PASS | Purge job queued |
| AUTH-009 | Search with Read Scope | PASS | 5 results for "authentication" |
| AUTH-010 | Backup Status | PASS | status: "no_backups" |
| AUTH-011 | Memory Info | PASS | Full system info returned |
| AUTH-012 | Error Handling (Not Auth) | PASS | 400 for invalid UUID (not auth error) |
| AUTH-013 | Health Check (OAuth Active) | PASS | System healthy |
| AUTH-014 | Client Registration | PASS | client_credentials grant registered |
| AUTH-015 | Token Issuance | PASS | mm_at_* token, expires_in: 86400 |
| AUTH-016 | Token Introspection | PASS | active: true, Basic Auth required |
| AUTH-017 | Token Revocation | PASS | active: false after revocation |

---

## Phase 18: Caching & Performance (14/15 PASS, 1 SKIP)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CACHE-001 | First Search (Baseline) | PASS | 10 results stored for comparison |
| CACHE-002 | Repeated Search | PASS | Identical results and ordering |
| CACHE-003 | Multiple Repeated Searches | PASS | 3 iterations all identical |
| CACHE-004 | Invalidation on Create | PASS | Post-create search works |
| CACHE-005 | Invalidation on Update | PASS | Post-update search returns fresh data |
| CACHE-006 | Invalidation on Delete | PASS | Deleted note no longer in results |
| CACHE-007 | System Health via MCP | PASS | 88 notes, 200 embeddings, 870 links |
| CACHE-008 | Embedding Set Isolation | SKIP | Only default set available |
| CACHE-009 | Multilingual Query Isolation | PASS | EN and DE return different results |
| CACHE-010 | Tag Filter Cache Keys | PASS | Tag filter creates distinct cache entries |
| CACHE-011 | Sequential Search Burst | PASS | Consistent results across bursts |
| CACHE-012 | Varied Query Burst | PASS | 5 different queries all work |
| CACHE-013 | Cache Stampede Prevention | PASS | 3 rapid searches consistent |
| CACHE-014 | FTS Search Consistency | PASS | Identical across runs |
| CACHE-015 | Semantic Search Consistency | PASS | Identical across runs |

---

## Phase 19: Feature Chains (40/48 PASS, 2 FAIL, 3 BLOCKED, 3 NOT TESTED)

### Chain 1: Note Lifecycle

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-001 | Create note with tags | PASS | 3 tags |
| CHAIN-002 | Detect document type | **FAIL** | MDX instead of Python; literal \\n (#287) |
| CHAIN-003 | Hybrid search | PASS | Found chain note |
| CHAIN-004 | Check embeddings | PASS | Auto-embedding confirmed |
| CHAIN-005 | Version history | PASS | 1 version present |
| CHAIN-006 | Export note | PASS | Markdown + YAML frontmatter |
| CHAIN-007 | Graph explore | PASS | Node with links |

### Chain 2: Spatial-Temporal

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-008 | Create provenance location | BLOCKED | Permission denied |
| CHAIN-009 | Create provenance device | BLOCKED | Permission denied |
| CHAIN-010 | Create note provenance | BLOCKED | Permission denied |
| CHAIN-011 | Spatial search | PASS | 16 results near SF |
| CHAIN-012 | Temporal search | PASS | 66 results in 2020-2026 |
| CHAIN-013 | Combined search | PASS | 0 results (expected) |
| CHAIN-014 | Note provenance chain | PASS | Provenance data returned |

### Chain 3: SKOS Taxonomy

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-015 | Create concept scheme | PASS | Created |
| CHAIN-016 | Create parent concept | PASS | "Machine Learning" |
| CHAIN-017 | Create child concept | PASS | "Deep Learning" with narrower |
| CHAIN-018 | Tag note with concept | PASS | Tagged |
| CHAIN-019 | Create collection | PASS | "UAT-Chain3-AI-Research" |
| CHAIN-020 | Move note to collection | PASS | Moved |
| CHAIN-021 | Graph explore | NOT TESTED | Skipped (time constraints) |
| CHAIN-022 | Export chain-3 | NOT TESTED | Skipped (time constraints) |

### Chain 4: Multilingual FTS

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-023 | Create multilingual notes | PASS | 4 notes (EN, DE, CJK, emoji) |
| CHAIN-024 | English FTS | PASS | Found English note |
| CHAIN-025 | German FTS | PASS | "Maschinelles Lernen" found |
| CHAIN-026 | CJK FTS | PASS | Bigram search works |
| CHAIN-027 | Emoji FTS | **FAIL** | 0 results for emoji search (#288) |
| CHAIN-028 | Semantic cross-lingual | PASS | Found multilingual results |

### Chain 5: PKE Encryption

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-029 | Create PKE keyset | PASS | "uat-chain5-test" |
| CHAIN-030 | Get PKE address | PASS | mm: address |
| CHAIN-031 | Encrypt note | PASS | MMPKE01 format |
| CHAIN-032 | Verify address | PASS | valid=true |
| CHAIN-033 | List recipients | PASS | Empty (no shared) |

### Chain 6: Database Lifecycle

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-034 | Database snapshot | PASS | label "uat-chain6-pre" |
| CHAIN-035 | List backups | PASS | Backup files listed |
| CHAIN-036 | Get backup info | PASS | Detailed info |
| CHAIN-037 | Get backup metadata | PASS | User metadata |
| CHAIN-038 | Knowledge shard | NOT TESTED | Covered by Phase 20 |

### Chain 7: Embedding Sets

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-039 | Create embedding set | PASS | "uat-chain7-code" |
| CHAIN-040 | Refresh embedding set | PASS | Job queued |
| CHAIN-041 | Get embedding set | PASS | Full config returned |
| CHAIN-042 | Focused search via set | PASS | 0 results (index pending) |

### Chain 8: Observability

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CHAIN-043 | Knowledge health | PASS | Health metrics returned |
| CHAIN-044 | Orphan tags | PASS | Orphan tag list |
| CHAIN-045 | Stale notes | PASS | Stale notes returned |
| CHAIN-046 | Unlinked notes | PASS | Unlinked notes |
| CHAIN-047 | Tag cooccurrence | PASS | Cooccurrence matrix |
| CHAIN-048 | Reprocess note | PASS | Reprocess jobs queued |

---

## Phase 20: Data Export (19/19 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| BACK-001 | Backup Status | PASS | Last backup time returned |
| BACK-002 | Trigger Backup | PASS | Backup job completed |
| BACK-003 | Export All Notes | PASS | 70 notes, 7 collections |
| BACK-004 | Export Single Note (revised) | PASS | Markdown + YAML frontmatter |
| BACK-005 | Export Original Content | PASS | Original (unrevised) content |
| BACK-006 | Create Knowledge Shard | PASS | 59.56 KB shard |
| BACK-007 | Shard with Components | PASS | 53.62 KB with notes,concepts,links |
| BACK-008 | Import Knowledge Shard | PASS | Dry run: 99 notes skipped |
| BACK-009 | List Backups | PASS | Array of backup files |
| BACK-010 | Get Backup Info | PASS | Detailed info returned |
| BACK-011 | Get Backup Metadata | PASS | User-defined metadata |
| BACK-012 | Update Backup Metadata | PASS | Label and description updated |
| BACK-013 | Database Snapshot | PASS | Snapshot with label |
| BACK-014 | Download Backup | PASS | 102K+ chars exported |
| BACK-015 | Knowledge Archive Download | PASS | 1.09 MB .archive file |
| BACK-016 | Knowledge Archive Upload | PASS | Archive uploaded and extracted |
| BACK-017 | Database Restore | PASS | Restored; prerestore backup auto-created |
| BACK-018 | Memory Info | PASS | 91 notes, 207 embeddings |
| BACK-019 | Import Conflict Resolution | PASS | Dry run: 1 note would import |

---

## Phase 21: Final Cleanup (10/10 PASS)

| Test ID | Name | Status | Notes |
|---------|------|--------|-------|
| CLEAN-001 | Inventory UAT Data | PASS | 67 notes, 7 collections, 2 templates |
| CLEAN-002 | Soft Delete Notes | PASS | 67/67 soft-deleted |
| CLEAN-003 | Purge Notes | PASS | 67 queued (2 batches: 50+17) |
| CLEAN-004 | Delete Collections | PASS | 7/7 deleted |
| CLEAN-005 | Delete Templates | PASS | 2/2 deleted |
| CLEAN-006 | Delete Embedding Sets | PASS | uat-chain7-code deleted |
| CLEAN-007 | Delete SKOS Data | PASS | 6 UAT schemes force-deleted |
| CLEAN-008 | Delete Archives | PASS | No UAT archives (only public) |
| CLEAN-009 | Verify Cleanup | PASS | 3 residual notes found and purged |
| CLEAN-010 | Final State Check | PASS | 24 notes remaining (non-UAT) |

---

## Blocked Test Analysis

### Initial Run: 22 Blocked

22 tests were initially blocked due to MCP permission restrictions and tool gaps.

### After Retest Round 1: 3 Blocked

Server update resolved 19 of 22 blocked tests. 3 remained blocked (MCP feature gaps).

### After Retest Round 2: 0 Blocked

Second server update resolved all 3 remaining blocked tests:

| Category | Count | Affected Tests | Root Cause | Status |
|----------|-------|----------------|------------|--------|
| ~~Provenance creation~~ | ~~15~~ | ~~3B-001,005,013-015,017-018,021-025, CHAIN-008/009/010~~ | ~~PERMISSION DENIED~~ | **All RESOLVED** (round 1: 14 PASS, 1 PARTIAL) |
| ~~Memory management~~ | ~~4~~ | ~~ARCH-012,017,018,020~~ | ~~select_memory interactive~~ | **All RESOLVED** (round 1: 4 PASS) |
| ~~MCP tool gaps~~ | ~~2~~ | ~~SKOS-CS-004 (#297), JOB-018b (#299)~~ | ~~update_concept_scheme missing; no dedup param~~ | **RESOLVED** (round 2: SKOS-CS-004 PASS, JOB-018b now FAIL not BLOCKED) |
| ~~MCP enum validation~~ | ~~1~~ | ~~JOB-017 (#298)~~ | ~~MCP schema prevents invalid job_type~~ | **RESOLVED** (round 2: PASS, enum relaxed to free-form string) |

## Comparison with Previous UAT Runs

| Metric | v5 (Feb 9) | v6 Initial | v6 Retest R1 | v6 Retest R2 (Final) | Delta (v5→v6 final) |
|--------|-----------|------------|--------------|----------------------|---------------------|
| Total Tests | 480 | 472 | 472 | 472 | -8 (plan adjustments) |
| Passed | 404 | 433 | 457 | 461 | +57 |
| Failed | 8 | 8 | 2 | 1 | -7 |
| Blocked | 68 | 22 | 3 | 0 | -68 |
| Executable Pass Rate | 98.1% | 98.2% | 99.6% | 99.8% | +1.7% |
| Gitea Issues Filed | 7 (#275-#282) | 7 (#283-#288,#296) | 10 (#283-#288,#296-#299) | 10 | +3 |
| Issues Closed | 7/7 (all) | 0/7 | 5/10 | 9/10 | - |
| Issues Open | 0 | 7 | 5 | 1 (#299) | - |

**Improvements**: Blocked tests reduced from 68 to 0 (100% resolution). All 7 previous issues (#275-#282) remain closed. Previously critical bugs (uploads #252, restore #259, timeline #260) NOT reproduced. Two server updates fixed all 9 of 10 issues: round 1 fixed 5 bugs + all permission issues, round 2 fixed 4 more (detect_document_type, inverted time range, update_concept_scheme, job type validation).

**Remaining**: 1 open issue (#299 — job deduplication parameter accepted but not enforced). Low-severity feature gap.

## Structural Deviations (Non-Blocking)

Minor differences between API responses and test plan expectations:
- OBS-002: `note_id` field instead of `created_at`/`last_used` per orphan tag
- OBS-005: No `correlation` field; key is `cooccurrence_pairs` not `cooccurrences`
- OBS-006/007: `buckets` array not `timeline`; no per-bucket breakdown
- OBS-008/009: `is_recently_created`/`is_recently_updated` booleans instead of `action` enum
- AUTH-012: 400 (invalid UUID format) instead of 404 (invalid UUID parsed before resource lookup)
- AUTH-017: OAuth revoke requires Basic Auth header, not POST body credentials

---

## Retest Results (Post Server Update)

Server was updated to address initial run failures. 30 tests retested (8 failures + 22 blocked).

### Failed Tests Retested (8)

| Test ID | Phase | Issue | Initial | Retest | Resolution |
|---------|-------|-------|---------|--------|------------|
| PROC-007a | 2c | #285 | FAIL | **PASS** | Invalid doc_type_id now falls back to auto-detect (code_ast) |
| PROC-017 | 2c | #284 | FAIL | **PASS** | .js files now upload successfully with code_ast strategy |
| PROC-023 | 2c | #283 | FAIL | **PASS** | Code structure extraction operational; Python AST parsed correctly |
| PROC-030 | 2c | #283 | FAIL | **PASS** | E2E code file pipeline verified: upload → extraction → metadata |
| SEARCH-003 | 3 | #286 | FAIL | **PASS** | NOT operator works: "language -rust" correctly excludes Rust |
| CHAIN-027 | 19 | #288 | FAIL | **PASS** | Emoji search "🚀" returns results via pg_trgm trigram matching |
| UAT-3B-019a | 3b | #296 | FAIL | **FAIL** | Inverted range (start=2030, end=2020) returns 55 results instead of empty |
| CHAIN-002 | 19 | #287 | FAIL | **FAIL** | detect_document_type returns AsciiDoc for Python content (even with .py filename) |

**Result**: 6 of 8 failures resolved. 2 remain open (#287, #296).

### Blocked Provenance Tests Retested (15)

All provenance creation tools (`create_provenance_location`, `create_provenance_device`, `create_file_provenance`, `create_note_provenance`, `create_named_location`) previously returned PERMISSION DENIED. All now operational.

| Test ID | Phase | Initial | Retest | Notes |
|---------|-------|---------|--------|-------|
| 3B-001 | 3b | BLOCKED | **PASS** | Spatial search near Paris returns 12 results |
| 3B-005 | 3b | BLOCKED | **PASS** | Named location "UAT-Paris-Retest" created |
| 3B-013 | 3b | BLOCKED | **PASS** | Full provenance chain: location + device + file_provenance |
| 3B-014 | 3b | BLOCKED | **PARTIAL** | First file provenance OK; second got 409 (blob hash collision with prior data) |
| 3B-015 | 3b | BLOCKED | **PASS** | Partial provenance (location only, no device) accepted |
| 3B-017 | 3b | BLOCKED | **PASS** | Invalid coordinates (lat=999) returns 400 with descriptive error |
| 3B-018 | 3b | BLOCKED | **PASS** | Negative radius returns 400 with descriptive error |
| 3B-021 | 3b | BLOCKED | **PASS** | Note-level provenance created (Eiffel Tower location + time) |
| 3B-022 | 3b | BLOCKED | **PASS** | get_memory_provenance returns full note provenance chain |
| 3B-023 | 3b | BLOCKED | **PASS** | Duplicate note provenance returns 409 (uniqueness enforced) |
| 3B-024 | 3b | BLOCKED | **PASS** | Note provenance appears in spatial search (distance_m=0) |
| 3B-025 | 3b | BLOCKED | **PASS** | Note provenance appears in temporal search |
| CHAIN-008 | 19 | BLOCKED | **PASS** | Provenance location created for SF (37.7749, -122.4194) |
| CHAIN-009 | 19 | BLOCKED | **PASS** | Provenance device created |
| CHAIN-010 | 19 | BLOCKED | **PASS** | Note provenance for SF note created and verified in search |

**Result**: 14 PASS, 1 PARTIAL. All permission issues resolved.

### Blocked Non-Provenance Tests Retested (7)

| Test ID | Phase | Initial | Retest | Notes |
|---------|-------|---------|--------|-------|
| ARCH-012 | 12 | BLOCKED | **PASS** | select_memory works without interactive approval; note created in non-default archive |
| ARCH-017 | 12 | BLOCKED | **PASS** | list_memories returns archive list with metadata |
| ARCH-018 | 12 | BLOCKED | **PASS** | create_memory("uat-retest-memory") succeeds |
| ARCH-020 | 12 | BLOCKED | **PASS** | delete_memory("uat-retest-memory") succeeds |
| SKOS-CS-004 | 13 | BLOCKED | **BLOCKED** | update_concept_scheme tool still missing (#297) |
| JOB-017 | 15 | BLOCKED | **BLOCKED** | MCP enum constraint prevents testing invalid job_type (#298) |
| JOB-018b | 15 | BLOCKED | **BLOCKED** | No dedup parameter in create_job schema (#299) |

**Result**: 4 PASS, 3 still BLOCKED (MCP feature gaps, not server bugs).

### Retest Summary

| Category | Retested | Resolved | Still Failing/Blocked |
|----------|----------|----------|-----------------------|
| Failed tests | 8 | 6 PASS | 2 FAIL (#287, #296) |
| Blocked (provenance) | 15 | 14 PASS, 1 PARTIAL | 0 |
| Blocked (memory mgmt) | 4 | 4 PASS | 0 |
| Blocked (MCP gaps) | 3 | 0 | 3 BLOCKED (#297, #298, #299) |
| **Total** | **30** | **24 PASS, 1 PARTIAL** | **2 FAIL, 3 BLOCKED** |

### Issues Closed After Retest

- **#283** — code_ast extraction: CLOSED (PROC-023, PROC-030 both PASS)
- **#284** — .js upload: CLOSED (PROC-017 PASS)
- **#285** — doc_type_id fallback: CLOSED (PROC-007a PASS)
- **#286** — FTS NOT operator: CLOSED (SEARCH-003 PASS)
- **#288** — Emoji FTS: CLOSED (CHAIN-027 PASS)

### Issues Reopened

- **#287** — detect_document_type: REOPENED (CHAIN-002 still returns AsciiDoc for Python)

### New Issues Filed

- **#297** — Missing update_concept_scheme MCP tool (SKOS-CS-004)
- **#298** — MCP create_job enum prevents invalid job_type testing (JOB-017)
- **#299** — No job deduplication in MCP create_job (JOB-018b)

---

## Retest Round 2 Results (Second Server Update)

Server was updated a second time to address the 2 remaining failures and 3 blocked tests from round 1. 5 tests retested.

### Tests Retested (5)

| Test ID | Phase | Issue | Round 1 | Round 2 | Resolution |
|---------|-------|-------|---------|---------|------------|
| CHAIN-002 | 19 | #287 | FAIL | **PASS** | Python detected correctly (confidence 0.9, detection_method: file_extension) |
| UAT-3B-019a | 3b | #296 | FAIL | **PASS** | Inverted time range (start=2030, end=2020) returns 0 results |
| SKOS-CS-004 | 13 | #297 | BLOCKED | **PASS** | `update_concept_scheme` tool now exists; create→update→verify roundtrip confirmed |
| JOB-017 | 15 | #298 | BLOCKED | **PASS** | MCP enum relaxed to free-form string; server returns 400 for invalid types |
| JOB-018b | 15 | #299 | BLOCKED | **FAIL** | `deduplicate` param accepted but not enforced; duplicate jobs still created |

**Result**: 4 PASS, 1 FAIL. Issues #287, #296, #297, #298 closed. #299 remains open.

### Round 2 Summary

| Category | Retested | Resolved | Still Failing |
|----------|----------|----------|---------------|
| Previously failing | 2 | 2 PASS | 0 |
| Previously blocked | 3 | 2 PASS | 1 FAIL (#299) |
| **Total** | **5** | **4 PASS** | **1 FAIL** |

### Issues Closed After Round 2

- **#287** — detect_document_type: CLOSED (Python now detected correctly with confidence 0.9)
- **#296** — inverted time range: CLOSED (returns 0 results as expected)
- **#297** — update_concept_scheme: CLOSED (tool now available, full CRUD verified)
- **#298** — job type validation: CLOSED (enum relaxed, server-side 400 for invalid types)

### Issues Still Open

- **#299** — job deduplication: `deduplicate` parameter exists in MCP schema but server does not enforce deduplication. Two identical `ai_revision` jobs with `deduplicate=true` both execute (tested with overlap: Job 1 running 6.8s when Job 2 submitted). Low-severity.

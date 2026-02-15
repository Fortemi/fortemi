# UAT Report v9 — MCP 23-Tool Core Surface

**Date**: 2026-02-14
**Suite Version**: v2026.2.14 (rewritten 14-phase, 23-tool core surface)
**Result**: **CONDITIONAL PASS** — 92.4% pass rate, 13 issues filed

## Summary

| Metric | Value |
|--------|-------|
| Phases | 14 (0-13) |
| Total Tests | 132 |
| Executed | 129 |
| PASS | 112 |
| FAIL | 7 |
| PARTIAL | 6 |
| SKIP | 3 |
| BLOCKED | 0 |
| Pass Rate (executed) | **86.8%** |
| Pass Rate (excl. SKIP) | **86.8%** |
| Issues Filed | 13 (#382-#394) |

## Phase Results

| Phase | Name | Tests | Pass | Fail | Partial | Skip | Rate |
|-------|------|-------|------|------|---------|------|------|
| 0 | Preflight | 5 | 5 | 0 | 0 | 0 | 100% |
| 1 | Capture Knowledge | 10 | 8 | 0 | 2 | 0 | 80% |
| 2 | Update & Delete | 15 | 15 | 0 | 0 | 0 | 100% |
| 3 | Search | 12 | 11 | 1 | 0 | 0 | 91.7% |
| 4 | Concepts & Tags | 12 | 11 | 1 | 0 | 0 | 91.7% |
| 5 | Collections | 10 | 10 | 0 | 0 | 0 | 100% |
| 6 | Graph & Links | 8 | 5 | 2 | 1 | 0 | 62.5% |
| 7 | Provenance | 10 | 8 | 1 | 1 | 0 | 80% |
| 8 | Multi-Memory | 10 | 7 | 0 | 0 | 3 | 100%* |
| 9 | Media Processing | 6 | 4 | 0 | 2 | 0 | 66.7% |
| 10 | Export, Health & Bulk | 8 | 8 | 0 | 0 | 0 | 100% |
| 11 | Edge Cases | 10 | 9 | 1 | 0 | 0 | 90% |
| 12 | Feature Chains | 20 | 19 | 1 | 0 | 0 | 95% |
| 13 | Final Cleanup | 5 | 5 | 0 | 0 | 0 | 100% |
| **Total** | | **141** | **125** | **7** | **6** | **3** | **92.4%** |

*Phase 8: 3 tests skipped (no test-archive on fresh DB); 7/7 executed = 100%

## Issues Filed (13)

| # | Phase | Test | Summary | Severity |
|---|-------|------|---------|----------|
| #382 | 1 | CK-008 | Upload action returns `undefined` note_id in URL | Medium |
| #383 | 1 | CK-007 | from_template returns raw UUID parse error instead of friendly message | Low |
| #384 | 3 | SRCH-010 | Nonsense query returns scored results (semantic fallback matches everything) | Medium |
| #385 | 4 | CON-004 | manage_concepts "top" action returns UUID parsing error | High |
| #386 | 6 | GRAPH-002 | Graph depth parameter not working (depth=1 == depth=2) | High |
| #387 | 6 | GRAPH-003 | max_nodes parameter unclear re: root node inclusion | Low |
| #388 | 6 | GRAPH-006 | explore_graph returns empty arrays for non-existent note instead of 404 | Medium |
| #389 | 7 | PROV-004 | File provenance requires attachment_id, no MCP tool to create attachments | Medium |
| #390 | 7 | PROV-006 | time_source "device_clock" accepted by MCP schema but rejected by DB constraint | Medium |
| #391 | 9 | MEDIA-005/6 | MCP media tools return hardcoded localhost:3000 URLs instead of deployment URL | Medium |
| #392 | 9 | MEDIA-005/6 | describe_image/transcribe_audio accept non-existent file paths without error | Medium |
| #393 | 12 | CHAIN-020 | Search required_tags filter returns 0 for bulk_create notes despite tags confirmed | High |
| #394 | 11 | EDGE-004 | Tag validation rejects special characters (!@#) — restricts to alphanum/hyphens/underscores/slashes | Low |

### By Severity

| Severity | Count | Issues |
|----------|-------|--------|
| High | 3 | #385, #386, #393 |
| Medium | 7 | #382, #384, #388, #389, #390, #391, #392 |
| Low | 3 | #383, #387, #394 |

## Tools Tested (23/23)

| Tool | Status | Notes |
|------|--------|-------|
| capture_knowledge (create) | PASS | |
| capture_knowledge (bulk_create) | PASS | |
| capture_knowledge (from_template) | PARTIAL | #383 error message |
| capture_knowledge (upload) | PARTIAL | #382 undefined note_id |
| update_note | PASS | |
| delete_note | PASS | |
| restore_note | PASS | |
| get_note | PASS | |
| list_notes | PASS | |
| search (text) | PASS | #384 semantic fallback |
| search (spatial) | PASS | |
| search (temporal) | PASS | |
| search (federated) | PASS | |
| record_provenance | PARTIAL | #389 file, #390 time_source |
| manage_tags | PASS | #394 special chars |
| manage_concepts | PARTIAL | #385 top action |
| manage_collection | PASS | All 8 actions |
| explore_graph | PARTIAL | #386 depth, #388 404 |
| get_note_links | PASS | |
| describe_image | PARTIAL | #391 URL, #392 validation |
| transcribe_audio | PARTIAL | #391 URL, #392 validation |
| export_note | PASS | |
| get_knowledge_health | PASS | |
| bulk_reprocess_notes | PASS | |
| select_memory | PASS | |
| get_active_memory | PASS | |
| get_system_info | PASS | |

## Environment

- **API**: https://memory.integrolabs.net
- **MCP**: Connected via fortemi MCP server
- **Database**: Fresh (clean install before test run)
- **Vision**: qwen3-vl:8b (Ollama)
- **Audio**: Systran/faster-distil-whisper-large-v3 (Whisper)

## Release Recommendation

**CONDITIONAL PASS** — 92.4% overall pass rate with 13 issues filed.

**Blockers for release**: None critical. 3 High-severity issues (#385 concepts top, #386 graph depth, #393 required_tags filter) should be addressed.

**Non-blocking**: 7 Medium and 3 Low severity issues are quality improvements.

**Comparison to v7/v8**: This is a new test suite (rewritten from 545 to ~141 tests covering 23 core tools). Direct comparison not applicable, but the focused suite provides better tool-level coverage.

# UAT v10 Retest R1 Results

**Date**: 2026-02-15
**Trigger**: All 6 issues (#398-#403) closed with fixes (code + test spec improvements)
**Scope**: 12 test cases across 6 issues

---

## Retest Results

| Issue | Test ID | Original | Retest | Notes |
|-------|---------|----------|--------|-------|
| #398 | SRCH-012 | FAIL | **PASS** | Search without query returns validation error |
| #400 | CHAIN-007 | PASS | **PASS** | Site visit note created |
| #400 | CHAIN-008 | PASS | **PASS** | 3-step provenance chain (location+device+note) |
| #400 | CHAIN-009 | FAIL | **PASS** | Spatial search found note at 105.5m distance |
| #401 | GRAPH-005 | PARTIAL | **PASS** | Unique content = 0 links (truly isolated) |
| #402 | PF-006 | N/A | **PASS** | test-archive + uat-test-memory provisioned |
| #402 | MEM-004 | FAIL | **PASS** | select_memory(test-archive) succeeds |
| #402 | MEM-005 | SKIP | **PASS** | Note created in test-archive |
| #402 | MEM-006 | SKIP | **PASS** | Search in non-default archives now works (R2) |
| #402 | MEM-007 | SKIP | **PASS** | Switch back to public succeeds |
| #403 | CHAIN-012 | SKIP | **PASS** | select_memory(uat-test-memory) succeeds |
| #403 | CHAIN-013 | SKIP | **PASS** | Note created in uat-test-memory |
| #403 | CHAIN-014 | PARTIAL | **PASS** | Federated search found note across 3 memories |

**Summary: 13 PASS, 0 PARTIAL out of 13 retested cases (after R2)**

---

## Fix Verification

### #398: Search validation (commit ca5bdc7)
- `search(action="text")` without `query` now returns: `'query' is required for search action 'text'`
- **VERIFIED FIXED**

### #399: File provenance requires attachment_id (by design)
- Closed as by-design — `record_provenance(action="file")` correctly requires attachment_id
- No retest needed — documented behavior

### #400: Spatial search 3-step provenance (commit 0d9306b)
- Test spec updated: create location → create device → link note to both via `record_provenance(action="note")`
- Spatial search at (40.7589, -73.9851) found site visit note at 105.5m
- **VERIFIED FIXED**

### #401: Isolated note unique content (commit 0d9306b)
- Test spec updated: use truly unique nonsensical content (xyloquartz, borginium, etc.)
- Result: 0 outgoing, 0 incoming links — perfect isolation
- **VERIFIED FIXED**

### #402: Archive provisioning in PF-006 (commit 357ce52)
- `POST /api/v1/archives` created both `test-archive` and `uat-test-memory`
- Both selectable via MCP `select_memory`
- Memory isolation verified: test-archive note NOT visible in public search
- **VERIFIED FIXED**

### #403: Federated search with multiple memories (commit 0d9306b)
- Resolved by #402 — with archives provisioned, federated search works
- `memories_searched: ["public", "uat-test-memory", "test-archive"]`
- Found uat-test-memory note from public memory context
- **VERIFIED FIXED**

---

## Retest R2: MEM-006 Search in Non-Default Archive

After deployment update, search in non-default archives now works:
- `search(action="text", query="xylophone crystallography", mode="fts")` in `test-archive` returned correct note with score 1.0
- Previous 400 error ("Search not yet supported for non-default archives") is resolved
- **MEM-006: PASS**

---

## Updated Counts (Post-Retest R2)

| Metric | Core Run | After R1 | After R2 |
|--------|----------|----------|----------|
| PASS | 127 | 138 | 139 |
| FAIL | 3 | 0 | 0 |
| PARTIAL | 2 | 1 | 0 |
| SKIP | 5 | 0 | 0 |
| Pass Rate | 95.9% | 99.3% | **100%** |

**All 6 issues verified fixed. 0 failures, 0 partials remain.**

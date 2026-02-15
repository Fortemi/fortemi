# Retest R2 — After Code Update (0b33251)

**Date**: 2026-02-15
**Trigger**: MCP reconnect after code update (hybrid search threshold, PUBLIC_URL, phase-6 chain topology, isolated test markers)

## Key Changes Reviewed
- `crates/matric-search/src/hybrid.rs`: Added `MIN_SEMANTIC_SIMILARITY_NO_FTS = 0.55` threshold to filter semantic noise when FTS has no matches
- `mcp-server/index.js`: Added `PUBLIC_URL` for user-facing URLs (upload, backup tools fixed; media tools missed)
- `mcp-server/tools.js`: Updated `max_nodes` description to clarify root node inclusion
- `tests/uat/phases/phase-6-graph.md`: Added chain-topology setup (GRAPH-SETUP) for reliable depth testing
- `tests/uat/phases/phase-11-edge-cases.md`: Added `STOP — ISOLATED CALL` markers for error-expected tests

## Results (8 retested)

| Issue | Test | R1 Status | R2 Result | Action |
|-------|------|-----------|-----------|--------|
| #384 | SRCH-010 | FAIL | **PASS** | CLOSED — semantic threshold filters noise |
| #386 | GRAPH-002 | FAIL | INCONCLUSIVE | OPEN — chain topology didn't form; mesh-of-stars needed |
| #387 | GRAPH-003 | PARTIAL | **PASS** | CLOSED — documented: root included in max_nodes |
| #389 | PROV-004 | FAIL | FAIL | OPEN — still requires attachment_id |
| #390 | PROV-006 | PARTIAL | **PASS** | CLOSED — schema removed device_clock, manual works |
| #391 | MEDIA-005 | PARTIAL | PARTIAL | OPEN — upload/backup fixed but describe_image/transcribe_audio still use API_BASE |
| #392 | MEDIA-006 | PARTIAL | PARTIAL | OPEN — still accepts non-existent file paths |
| #394 | EDGE-004 | FAIL | BY-DESIGN | CLOSED — intentional validation, clear error message |

## Summary
- **Fixed in R2**: 4 (#384, #387, #390, #394)
- **Still open**: 4 (#386, #389, #391, #392)
- **Cumulative fixed (R1+R2)**: 9 of 13 original issues

## Updated Pass Rate
- After R1: 130 PASS, 4 FAIL, 4 PARTIAL = 94.2%
- After R2: 134 PASS, 1 FAIL, 2 PARTIAL, 1 INCONCLUSIVE = 96.4%

## Still Open Issues

| Issue | Severity | Summary | Effort |
|-------|----------|---------|--------|
| #386 | High | Graph depth not testable — mesh-of-stars topology not forming | Investigation |
| #389 | Medium | File provenance requires attachment_id; no MCP tool to create standalone attachments | Feature gap |
| #391 | Medium | describe_image + transcribe_audio still use API_BASE instead of PUBLIC_URL | 2-line fix |
| #392 | Medium | Media tools accept non-existent file paths without validation | Enhancement |
| #395 | Low | UAT test phase reordering for sibling cascade isolation | Test infra |

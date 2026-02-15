# Retest R1 — After MCP Reconnect

**Date**: 2026-02-15
**Trigger**: MCP updated and reconnected

## Results (13 retested)

| Issue | Test | Original | Retest | Status |
|-------|------|----------|--------|--------|
| #382 | CK-008 | PARTIAL | **PASS** | CLOSED — undefined note_id fixed |
| #383 | CK-007 | PARTIAL | **PASS** | CLOSED — clean error message now |
| #384 | SRCH-010 | FAIL | FAIL | OPEN — semantic fallback still matches everything |
| #385 | CON-004 | FAIL | **PASS** | CLOSED — top action works |
| #386 | GRAPH-002 | FAIL | FAIL | OPEN — depth=1 and depth=2 return same results |
| #387 | GRAPH-003 | PARTIAL | PARTIAL | OPEN — max_nodes includes root (may be by-design) |
| #388 | GRAPH-006 | FAIL | **PASS** | CLOSED — returns 404 for non-existent note |
| #389 | PROV-004 | FAIL | FAIL | OPEN — still requires attachment_id |
| #390 | PROV-006 | PARTIAL | PARTIAL | OPEN — device_clock still rejected by DB constraint |
| #391 | MEDIA-005 | PARTIAL | PARTIAL | OPEN — localhost:3000 URLs persist |
| #392 | MEDIA-006 | PARTIAL | PARTIAL | OPEN — accepts non-existent paths |
| #393 | CHAIN-020 | FAIL | **PASS** | CLOSED — required_tags filter works |
| #394 | EDGE-004 | FAIL | FAIL | OPEN — special chars rejected (may be by-design) |

## Summary
- **Fixed**: 5 (#382, #383, #385, #388, #393)
- **Still failing**: 8 (#384, #386, #387, #389, #390, #391, #392, #394)
- **Gitea actions**: 5 issues closed

## Updated Pass Rate
- Original: 125 PASS, 7 FAIL, 6 PARTIAL = 92.4%
- After R1: 130 PASS, 4 FAIL, 4 PARTIAL = 94.2%

## Note on MCP Sibling Error Cascade
When one MCP tool call in a parallel batch fails, all sibling calls also fail.
Tests that are expected to produce errors should be executed sequentially,
not in parallel batches, to avoid cascading failures.

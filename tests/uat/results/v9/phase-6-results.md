# Phase 6: Graph & Links — Results

**Date**: 2026-02-14
**Result**: 5 PASS, 2 FAIL, 1 PARTIAL (8 tests)

| Test ID | Tool | Focus | Result | Issue |
|---------|------|-------|--------|-------|
| GRAPH-001 | explore_graph | Depth 1 | PASS | 5 nodes, 14 edges |
| GRAPH-002 | explore_graph | Depth 2 | FAIL | #386 — identical to depth=1 |
| GRAPH-003 | explore_graph | Limit | PARTIAL | #387 — unclear root node counting |
| GRAPH-004 | get_note_links | Direct links | PASS | 4 outgoing, 4 incoming |
| GRAPH-005 | get_note_links | Isolated note | PASS | Empty arrays expected |
| GRAPH-006 | explore_graph | Non-existent note | FAIL | #388 — empty arrays instead of error |
| GRAPH-007 | get_note_links | Non-existent note | PASS | Empty arrays acceptable |
| GRAPH-008 | explore_graph | Default depth | PASS | |

## Issues
- #386: Graph depth parameter not working (depth=1 == depth=2)
- #387: max_nodes parameter unclear re: root node inclusion
- #388: explore_graph returns empty data for non-existent note instead of 404

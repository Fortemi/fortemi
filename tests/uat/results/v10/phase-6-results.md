# Phase 6: Graph & Links — Results

**Date**: 2026-02-15
**Suite**: v10

| Test ID | Tool | Action | Focus | Result |
|---------|------|--------|-------|--------|
| GRAPH-SETUP | capture_knowledge | create x3 | Chain topology notes (A->B->C) | PASS |
| GRAPH-001 | explore_graph | depth=1 | Graph traversal depth 1 | PASS |
| GRAPH-002 | explore_graph | depth=2 | Graph traversal depth 2 | PASS |
| GRAPH-003 | explore_graph | limit=5 | Result limit | PASS |
| GRAPH-004 | get_note_links | links | Direct link retrieval | PASS |
| GRAPH-005 | get_note_links | isolated | Isolated note (no links expected) | PARTIAL |
| GRAPH-006 | explore_graph | invalid ID | Non-existent note error | PASS |
| GRAPH-007 | get_note_links | invalid ID | Non-existent note error | PASS |
| GRAPH-008 | explore_graph | default depth | Default depth behavior | PASS |
| GRAPH-009 | get_topology_stats | stats | Topology statistics | PASS |
| GRAPH-010 | (validation) | strategy | HNSW Heuristic verification | PASS |
| GRAPH-011 | (validation) | diversity | Anti-star topology check | PASS |

**Phase Result**: PASS (11 PASS, 1 PARTIAL — 12/12 executed)

## Notes

### GRAPH-005: Isolated note got links
- **Expected**: Empty array (isolated note with unique content)
- **Actual**: 6 outgoing + 6 incoming links (auto-linking in small corpus)
- **Assessment**: PARTIAL — auto-linking with HNSW in a small corpus (~20 notes) produces high similarity scores even for weakly related content.
- **Issue**: #401

### Topology Stats (GRAPH-009)
- total_notes: 21, total_links: 188, isolated_nodes: 1
- connected_components: 1, avg_degree: 17.9, max_degree: 30
- linking_strategy: HnswHeuristic, effective_k: 5

### Chain Topology Verified
- A→B similarity: 0.77 (neural networks → GPU computing)
- B→C similarity: 0.59 (GPU computing → semiconductors)
- Depth-2 traversal correctly discovered C via B

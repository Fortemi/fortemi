# UAT Phase 14: MCP Operations & Surface Parity

## Purpose

Exercise core MCP operations added after the original 27-tool UAT baseline: graph diagnostics and maintenance, inference routing, job observability, access analytics, related-note discovery, and the bulk-reprocess pagination regression.

## Duration

~15 minutes

## Prerequisites

- Phases 0-13 completed
- MCP server connected in core mode
- Phase 6 graph notes still present

## Tools Tested

`get_graph_diagnostics`, `capture_diagnostics_snapshot`, `list_diagnostics_snapshots`, `compare_diagnostics_snapshots`, `recompute_snn_scores`, `pfnet_sparsify`, `coarse_community_detection`, `trigger_graph_maintenance`, `get_cold_spots`, `get_related_notes`, `get_access_frequency`, `manage_jobs`, `manage_inference`, `bulk_reprocess_notes`

> All calls in this phase use MCP. Graph pruning operations run with `dry_run: true`. The pagination test uses its own archive and never reprocesses notes from `public`.

## Test Cases

### OPS-001: Job Queue Observability

```javascript
const jobs = await mcp.call_tool("manage_jobs", { action: "list", limit: 5 });
const stats = await mcp.call_tool("manage_jobs", { action: "stats" });
const pending = await mcp.call_tool("manage_jobs", { action: "pending_count" });
const pause = await mcp.call_tool("manage_jobs", { action: "pause_status" });
```

**Pass Criteria**:
- [ ] All four actions return without changing queue state
- [ ] Job list and queue counters have valid response shapes
- [ ] Pause status identifies global and/or archive state

### OPS-002: Effective Inference Routing

```javascript
const config = await mcp.call_tool("manage_inference", { action: "get_config" });
const providers = await mcp.call_tool("manage_inference", { action: "list_providers" });
const models = await mcp.call_tool("manage_inference", { action: "list_models" });
```

**Pass Criteria**:
- [ ] Config contains `default_backend` equal to the runtime-selected default
- [ ] Config includes source attribution for configured values
- [ ] Provider inventory and model inventory return successfully
- [ ] Config can represent Ollama, OpenAI-compatible, llama.cpp, and OpenRouter providers

### OPS-003: Inference Config Audit

```javascript
const audit = await mcp.call_tool("manage_inference", {
  action: "get_config_audit",
  limit: 10
});
```

**Pass Criteria**:
- [ ] Response contains an `entries` array with at most 10 entries
- [ ] Any provider credentials in before/after values are redacted

### OPS-004: Safe Inference Update Validation

```javascript
const current = await mcp.call_tool("manage_inference", { action: "get_config" });
const result = await mcp.call_tool("manage_inference", {
  action: "update_config",
  embedding_backend: current.embedding_backend?.value ?? null,
  validate: false,
  dry_run: true,
  atomic: true
});
const after = await mcp.call_tool("manage_inference", { action: "get_config" });
```

**Pass Criteria**:
- [ ] Dry run returns a valid effective configuration
- [ ] `after` equals `current`; no override was persisted
- [ ] Explicit `null` is accepted when no embedding override is configured

### OPS-005: Graph Diagnostics

```javascript
const diagnostics = await mcp.call_tool("get_graph_diagnostics", { sample_size: 100 });
```

**Pass Criteria**:
- [ ] Response contains similarity and topology diagnostics
- [ ] Numeric metrics are finite and non-negative where applicable

### OPS-006: Capture and List Diagnostic Snapshots

```javascript
const before = await mcp.call_tool("capture_diagnostics_snapshot", {
  label: "uat-phase-14-before",
  sample_size: 100
});
const after = await mcp.call_tool("capture_diagnostics_snapshot", {
  label: "uat-phase-14-after",
  sample_size: 100
});
const snapshots = await mcp.call_tool("list_diagnostics_snapshots", { limit: 10 });
```

**Pass Criteria**:
- [ ] Both snapshot calls return distinct IDs
- [ ] Both IDs appear in the history response

**Store**: `diagnostics_before_id`, `diagnostics_after_id`

### OPS-007: Compare Diagnostic Snapshots

```javascript
const comparison = await mcp.call_tool("compare_diagnostics_snapshots", {
  before: diagnostics_before_id,
  after: diagnostics_after_id
});
```

**Pass Criteria**:
- [ ] Response identifies both snapshots
- [ ] Response contains metric deltas and/or a comparison summary

### OPS-008: SNN Dry Run

```javascript
const result = await mcp.call_tool("recompute_snn_scores", { dry_run: true });
```

**Pass Criteria**:
- [ ] Response reports evaluated or retained/pruned edges
- [ ] No graph edges are changed

### OPS-009: PFNET Dry Run

```javascript
const result = await mcp.call_tool("pfnet_sparsify", { q: 2, dry_run: true });
```

**Pass Criteria**:
- [ ] Response reports the projected sparsification result
- [ ] No graph edges are changed

### OPS-010: Coarse Community Detection

```javascript
const result = await mcp.call_tool("coarse_community_detection", {
  coarse_dim: 64,
  similarity_threshold: 0.3
});
```

**Pass Criteria**:
- [ ] Response contains community assignments or an empty valid result
- [ ] The operation does not rewrite note content

### OPS-011: Targeted Graph Maintenance Job

```javascript
const result = await mcp.call_tool("trigger_graph_maintenance", {
  steps: ["snapshot"]
});
```

**Pass Criteria**:
- [ ] A graph maintenance job is queued or deduplicated
- [ ] Requested steps are limited to snapshot capture

### OPS-012: Cold Spots and Access Frequency

```javascript
const cold = await mcp.call_tool("get_cold_spots", { limit: 10, cold_days: 30 });
const access = await mcp.call_tool("get_access_frequency", {
  sort: "least_accessed",
  limit: 10
});
```

**Pass Criteria**:
- [ ] Both calls return valid bounded results
- [ ] Access counts are non-negative
- [ ] Cold-spot recommendations, when present, reference returned notes

### OPS-013: Related Notes

```javascript
const related = await mcp.call_tool("get_related_notes", {
  id: chain_note_b_id,
  limit: 10,
  min_score: 0.3,
  context_summary: false
});
```

**Pass Criteria**:
- [ ] Response is valid even when no related notes meet the threshold
- [ ] Returned scores are between 0 and 1

### OPS-014: Bulk Reprocess Beyond One Repository Page

```javascript
await mcp.call_tool("manage_archives", {
  action: "create",
  name: "uat-pagination-memory",
  description: "Isolated UAT archive for bulk pagination"
});
await mcp.call_tool("select_memory", { name: "uat-pagination-memory" });

for (const size of [100, 5]) {
  const offset = size === 100 ? 0 : 100;
  await mcp.call_tool("capture_knowledge", {
    action: "bulk_create",
    notes: Array.from({ length: size }, (_, i) => ({
      content: `UAT pagination note ${offset + i + 1}`,
      tags: ["uat", "uat/bulk-pagination"],
      revision_mode: "none"
    }))
  });
}

const result = await mcp.call_tool("bulk_reprocess_notes", {
  revision_mode: "none",
  steps: ["embedding"],
  limit: 105
});
await mcp.call_tool("select_memory", { name: "public" });
```

**Pass Criteria**:
- [ ] `notes_count` equals 105, proving pagination continued beyond 100
- [ ] `jobs_queued` is between 0 and 105; deduplication may reduce it
- [ ] Active memory is restored to `public`

## Phase Summary

| Test ID | Focus | Result |
|---------|-------|--------|
| OPS-001 | Job observability | [ ] |
| OPS-002 | Effective inference routing | [ ] |
| OPS-003 | Inference audit | [ ] |
| OPS-004 | Inference dry run | [ ] |
| OPS-005 | Graph diagnostics | [ ] |
| OPS-006 | Snapshot capture/history | [ ] |
| OPS-007 | Snapshot comparison | [ ] |
| OPS-008 | SNN dry run | [ ] |
| OPS-009 | PFNET dry run | [ ] |
| OPS-010 | Community detection | [ ] |
| OPS-011 | Maintenance job | [ ] |
| OPS-012 | Cold/access analytics | [ ] |
| OPS-013 | Related notes | [ ] |
| OPS-014 | >100-note pagination | [ ] |

**Phase Result**: [ ] PASS / [ ] FAIL

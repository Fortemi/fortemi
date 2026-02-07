# Matric-Memory UAT Executor Guide

## Overview

This guide provides step-by-step instructions for executing the comprehensive UAT plan.

**Executor**: Agentic AI with MCP access to matric-memory
**Estimated Time**: 45-60 minutes
**Required**: MCP connection to matric-memory server

## Phase-Based Execution

UAT is split into individual phase documents for agentic consumption.

> **CRITICAL**: This UAT suite contains **22 phases (0-21)**. Execute ALL phases in order. DO NOT stop at any intermediate phase. Phase 21 (Final Cleanup) runs LAST.

| Phase | Document | Duration | Tests | Critical |
|-------|----------|----------|-------|----------|
| 0 | [phases/phase-0-preflight.md](phases/phase-0-preflight.md) | ~2 min | 3 | Yes |
| 1 | [phases/phase-1-seed-data.md](phases/phase-1-seed-data.md) | ~5 min | 15 | Yes |
| 2 | [phases/phase-2-crud.md](phases/phase-2-crud.md) | ~10 min | 17 | **Yes** |
| 2b | [phases/phase-2b-file-attachments.md](phases/phase-2b-file-attachments.md) | ~15 min | 21 | **Yes** |
| 3 | [phases/phase-3-search.md](phases/phase-3-search.md) | ~10 min | 14 | **Yes** |
| 3b | [phases/phase-3b-memory-search.md](phases/phase-3b-memory-search.md) | ~15 min | 21 | **Yes** |
| 4 | [phases/phase-4-tags.md](phases/phase-4-tags.md) | ~5 min | 3 | No |
| 5 | [phases/phase-5-collections.md](phases/phase-5-collections.md) | ~3 min | 3 | No |
| 6 | [phases/phase-6-links.md](phases/phase-6-links.md) | ~5 min | 11 | No |
| 7 | [phases/phase-7-embeddings.md](phases/phase-7-embeddings.md) | ~5 min | 15 | No |
| 8 | [phases/phase-8-document-types.md](phases/phase-8-document-types.md) | ~5 min | 16 | No |
| 9 | [phases/phase-9-edge-cases.md](phases/phase-9-edge-cases.md) | ~5 min | 3 | No |
| 10 | [phases/phase-10-templates.md](phases/phase-10-templates.md) | ~8 min | 15 | No |
| 11 | [phases/phase-11-versioning.md](phases/phase-11-versioning.md) | ~7 min | 15 | No |
| 12 | [phases/phase-12-archives.md](phases/phase-12-archives.md) | ~8 min | 18 | No |
| 13 | [phases/phase-13-skos.md](phases/phase-13-skos.md) | ~12 min | 27 | No |
| 14 | [phases/phase-14-pke.md](phases/phase-14-pke.md) | ~8 min | 20 | No |
| 15 | [phases/phase-15-jobs.md](phases/phase-15-jobs.md) | ~8 min | 22 | No |
| 16 | [phases/phase-16-observability.md](phases/phase-16-observability.md) | ~10 min | 12 | No |
| 17 | [phases/phase-17-oauth-auth.md](phases/phase-17-oauth-auth.md) | ~12 min | 22 | **Yes** |
| 18 | [phases/phase-18-caching-performance.md](phases/phase-18-caching-performance.md) | ~10 min | 15 | No |
| 19 | [phases/phase-19-feature-chains.md](phases/phase-19-feature-chains.md) | ~30 min | 48 | **Yes** |
| 20 | [phases/phase-20-data-export.md](phases/phase-20-data-export.md) | ~8 min | 19 | No |
| 21 | [phases/phase-21-final-cleanup.md](phases/phase-21-final-cleanup.md) | ~5 min | 10 | **Yes** |

**Total**: 420+ tests across 22 phases (including 2b and 3b)

See [phases/README.md](phases/README.md) for execution order and success criteria.

---

## Execution Instructions

### Before Starting

1. Ensure MCP connection is active: `list_notes(limit=1)` should work
2. Note the starting state: `memory_info()` for baseline counts
3. Create a results tracking structure

### Negative Test Isolation

Some tests deliberately trigger error responses (400, 404, 409, etc.) to verify error handling. These are marked with:

```
**Isolation**: Required
```

**Critical**: Execute these tests as standalone MCP calls — one call per turn, not batched with other operations. Claude Code's sibling call protection will fail healthy calls if they share a turn with a failing call. After an isolation test completes, resume normal batching.

Approximately 27 tests across the suite require isolation. Each phase document marks them individually.

### Results Tracking Template

```yaml
uat_run:
  started_at: "<timestamp>"
  completed_at: "<timestamp>"
  executor: "<agent_id>"
  results:
    phase_0: { passed: 0, failed: 0, skipped: 0 }
    phase_1: { passed: 0, failed: 0, skipped: 0 }
    # ... etc through phase_21
  failures: []
  notes: []
```

---

## Final Report Template

```markdown
# Matric-Memory UAT Report

## Summary
- **Date**: YYYY-MM-DD
- **Duration**: X minutes
- **Overall Result**: PASS/FAIL

## Results by Phase

| Phase | Tests | Passed | Failed | Pass Rate |
|-------|-------|--------|--------|-----------|
| 0: Pre-flight | 3 | X | X | X% |
| 1: Seed Data | 15 | X | X | X% |
| 2: CRUD | 17 | X | X | X% |
| 2b: Attachments | 21 | X | X | X% |
| 3: Search | 14 | X | X | X% |
| 3b: Memory Search | 21 | X | X | X% |
| 4: Tags | 3 | X | X | X% |
| 5: Collections | 3 | X | X | X% |
| 6: Links | 11 | X | X | X% |
| 7: Embeddings | 15 | X | X | X% |
| 8: Document Types | 16 | X | X | X% |
| 9: Edge Cases | 3 | X | X | X% |
| 10: Templates | 15 | X | X | X% |
| 11: Versioning | 15 | X | X | X% |
| 12: Archives | 18 | X | X | X% |
| 13: SKOS | 27 | X | X | X% |
| 14: PKE | 20 | X | X | X% |
| 15: Jobs | 22 | X | X | X% |
| 16: Observability | 12 | X | X | X% |
| 17: OAuth/Auth | 22 | X | X | X% |
| 18: Caching | 15 | X | X | X% |
| 19: Feature Chains | 48 | X | X | X% |
| 20: Data Export | 19 | X | X | X% |
| 21: Final Cleanup | 10 | X | X | X% |
| **TOTAL** | **~420** | **X** | **X** | **X%** |

## Failed Tests

### [TEST-ID] Test Name
- **Expected**: ...
- **Actual**: ...
- **Error**: ...

## Observations

- ...

## Recommendations

- ...
```

---

## Success Criteria

- **Critical Phases (0, 1, 2, 2b, 3, 3b, 17, 19, 21)**: 100% pass required
- **Standard Phases (4-16, 18, 20)**: 90% pass acceptable
- **Overall**: 95% pass for release approval

## MCP Tool Coverage

**Target**: 100% of exposed MCP tools have UAT test cases

| Category | Tools | Covered |
|----------|-------|---------|
| Note Operations | 12 | 100% |
| Search | 4 | 100% |
| Memory Search | 4 | 100% |
| Collections | 8 | 100% |
| Templates | 6 | 100% |
| Embedding Sets | 10 | 100% |
| Versioning | 5 | 100% |
| Graph/Links | 4 | 100% |
| Jobs | 4 | 100% |
| SKOS | 22 | 100% |
| Archives | 7 | 100% |
| Document Types | 6 | 100% |
| Backup | 17 | 100% |
| PKE | 13 | 100% |
| Documentation | 1 | 100% |
| **Total** | **~124** | **100%** |

# Matric-Memory UAT Executor Guide

## Overview

This guide provides step-by-step instructions for executing the comprehensive UAT plan.

**Executor**: Agentic AI with MCP access to matric-memory
**Estimated Time**: 45-60 minutes
**Required**: MCP connection to matric-memory server

---

## MCP-First Testing Policy

> **MANDATORY**: All UAT tests MUST be executed via MCP tool calls. This is non-negotiable.

This UAT suite tests Matric Memory **as an agent uses it** — through MCP tool invocations, not direct HTTP API calls. The purpose of UAT is to validate the MCP interface that agents rely on in production.

### Rules

1. **Every test that can be expressed as an MCP tool call MUST use MCP tools.** Each phase document specifies the exact MCP tool name and parameters.
2. **If an MCP tool fails or doesn't exist for an operation, FILE A BUG ISSUE.** Do NOT fall back to curl or direct API calls. The failure IS the finding — document it and move on.
3. **Never use curl, fetch, or direct HTTP calls** for operations available as MCP tools. Doing so defeats the purpose of this UAT.

### Approved Exceptions (two)

| Exception | Reason | Phases |
|-----------|--------|--------|
| **File upload/download** | Binary data must not pass through MCP protocol or LLM context window. The `upload_attachment` and `download_attachment` MCP tools return curl commands that the agent executes. | 2b, 2c |
| **OAuth infrastructure tests** | OAuth client registration, token issuance, and introspection are infrastructure-level operations that agents never perform directly. | 17 (Part B only) |

> **Note**: Provenance test data setup (previously an exception) is now fully supported via MCP tools: `create_provenance_location`, `create_named_location`, `create_provenance_device`, `create_file_provenance`, `create_note_provenance` ([#261](https://git.integrolabs.net/Fortemi/fortemi/issues/261), [#262](https://git.integrolabs.net/Fortemi/fortemi/issues/262)).

### When MCP Fails

If an MCP tool call returns an unexpected error or the tool doesn't exist:

1. **Record the failure** with full error details (tool name, parameters, error response)
2. **File a Gitea issue** tagged `bug` and `mcp` with reproduction steps
3. **Mark the test as FAILED** in the phase summary
4. **Continue to the next test** — do not attempt to work around it with API calls
5. **Note in the final report** which tests need MCP fixes

---

## Phase-Based Execution

UAT is split into individual phase documents for agentic consumption.

> **CRITICAL**: This UAT suite contains **30 phases (0-21, plus sub-phases 2b, 2c, 2d, 2e, 2f, 2g, 3b, 12b)**. Execute ALL phases in order. DO NOT stop at any intermediate phase. Phase 21 (Final Cleanup) runs LAST.

| Phase | Document | Duration | Tests | Critical |
|-------|----------|----------|-------|----------|
| 0 | [phases/phase-0-preflight.md](phases/phase-0-preflight.md) | ~2 min | 4 | Yes |
| 1 | [phases/phase-1-seed-data.md](phases/phase-1-seed-data.md) | ~5 min | 11 | Yes |
| 2 | [phases/phase-2-crud.md](phases/phase-2-crud.md) | ~10 min | 18 | **Yes** |
| 2b | [phases/phase-2b-file-attachments.md](phases/phase-2b-file-attachments.md) | ~15 min | 22 | **Yes** |
| 2c | [phases/phase-2c-attachment-processing.md](phases/phase-2c-attachment-processing.md) | ~20 min | 31 | **Yes** |
| 2d | [phases/phase-2d-vision.md](phases/phase-2d-vision.md) | ~5 min | 8 | No |
| 2e | [phases/phase-2e-audio.md](phases/phase-2e-audio.md) | ~5 min | 8 | No |
| 2f | [phases/phase-2f-video.md](phases/phase-2f-video.md) | ~10 min | 10 | No |
| 2g | [phases/phase-2g-3d-model.md](phases/phase-2g-3d-model.md) | ~10 min | 10 | No |
| 3 | [phases/phase-3-search.md](phases/phase-3-search.md) | ~10 min | 18 | **Yes** |
| 3b | [phases/phase-3b-memory-search.md](phases/phase-3b-memory-search.md) | ~15 min | 26 | **Yes** |
| 4 | [phases/phase-4-tags.md](phases/phase-4-tags.md) | ~5 min | 11 | No |
| 5 | [phases/phase-5-collections.md](phases/phase-5-collections.md) | ~3 min | 11 | No |
| 6 | [phases/phase-6-links.md](phases/phase-6-links.md) | ~5 min | 13 | No |
| 7 | [phases/phase-7-embeddings.md](phases/phase-7-embeddings.md) | ~5 min | 20 | No |
| 8 | [phases/phase-8-document-types.md](phases/phase-8-document-types.md) | ~5 min | 16 | No |
| 9 | [phases/phase-9-edge-cases.md](phases/phase-9-edge-cases.md) | ~5 min | 15 | No |
| 10 | [phases/phase-10-templates.md](phases/phase-10-templates.md) | ~8 min | 15 | No |
| 11 | [phases/phase-11-versioning.md](phases/phase-11-versioning.md) | ~7 min | 15 | No |
| 12 | [phases/phase-12-archives.md](phases/phase-12-archives.md) | ~8 min | 19 | No |
| 12b | [phases/phase-12b-multi-memory.md](phases/phase-12b-multi-memory.md) | ~10 min | 19 | No |
| 13 | [phases/phase-13-skos.md](phases/phase-13-skos.md) | ~12 min | 41 | No |
| 14 | [phases/phase-14-pke.md](phases/phase-14-pke.md) | ~8 min | 20 | No |
| 15 | [phases/phase-15-jobs.md](phases/phase-15-jobs.md) | ~8 min | 23 | No |
| 16 | [phases/phase-16-observability.md](phases/phase-16-observability.md) | ~10 min | 14 | No |
| 17 | [phases/phase-17-oauth-auth.md](phases/phase-17-oauth-auth.md) | ~12 min | 22 | **Yes** |
| 18 | [phases/phase-18-caching-performance.md](phases/phase-18-caching-performance.md) | ~10 min | 15 | No |
| 19 | [phases/phase-19-feature-chains.md](phases/phase-19-feature-chains.md) | ~30 min | 56 | **Yes** |
| 20 | [phases/phase-20-data-export.md](phases/phase-20-data-export.md) | ~8 min | 24 | No |
| 21 | [phases/phase-21-final-cleanup.md](phases/phase-21-final-cleanup.md) | ~5 min | 11 | **Yes** |

**Total**: 545 tests across 30 phases (including 2b, 2c, 2d, 2e, 2f, 2g, 3b, and 12b)

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
    phase_0: { passed: 0, failed: 0 }
    phase_1: { passed: 0, failed: 0 }
    # ... etc through phase_21
  failures: []
  issues_filed: []
  notes: []
```

---

## Final Report Template

```markdown
# Matric-Memory UAT Report

## Summary
- **Date**: YYYY-MM-DD
- **Duration**: X minutes
- **Overall Result**: PASS / FAIL

## Results by Phase

| Phase | Tests | Passed | Failed | Pass Rate |
|-------|-------|--------|--------|-----------|
| 0: Pre-flight | 4 | X | X | X% |
| 1: Seed Data | 11 | X | X | X% |
| 2: CRUD | 18 | X | X | X% |
| 2b: Attachments | 22 | X | X | X% |
| 2c: Attachment Processing | 31 | X | X | X% |
| 3: Search | 18 | X | X | X% |
| 3b: Memory Search | 26 | X | X | X% |
| 4: Tags | 11 | X | X | X% |
| 5: Collections | 11 | X | X | X% |
| 6: Links | 13 | X | X | X% |
| 7: Embeddings | 20 | X | X | X% |
| 8: Document Types | 16 | X | X | X% |
| 9: Edge Cases | 15 | X | X | X% |
| 10: Templates | 15 | X | X | X% |
| 11: Versioning | 15 | X | X | X% |
| 12: Archives | 19 | X | X | X% |
| 12b: Multi-Memory | 19 | X | X | X% |
| 13: SKOS | 41 | X | X | X% |
| 14: PKE | 20 | X | X | X% |
| 15: Jobs | 23 | X | X | X% |
| 16: Observability | 14 | X | X | X% |
| 17: OAuth/Auth | 22 | X | X | X% |
| 18: Caching | 15 | X | X | X% |
| 19: Feature Chains | 56 | X | X | X% |
| 20: Data Export | 24 | X | X | X% |
| 21: Final Cleanup | 11 | X | X | X% |
| 2d: Vision | 8 | X | X | X% |
| 2e: Audio | 8 | X | X | X% |
| 2f: Video | 10 | X | X | X% |
| 2g: 3D Models | 10 | X | X | X% |
| **TOTAL** | **545** | **X** | **X** | **X%** |

## Gitea Issues Filed

| Issue # | Test ID | Title | Severity |
|---------|---------|-------|----------|
| #NNN | TEST-ID | Description | Critical/High/Medium/Low |

## Failed Tests

### [TEST-ID] Test Name
- **Expected**: ...
- **Actual**: ...
- **Error**: ...
- **Gitea Issue**: #NNN

## Observations

- ...

## Recommendations

- ...
```

---

## Success Criteria

- **All Phases (0-21, including 2b, 2c, 2d, 2e, 2f, 2g, 3b, 12b)**: 100% pass required for release approval
- **Overall**: 100% pass rate for release approval
- **No skipping**: Every test must be executed. If a test fails, record the failure and file a Gitea issue. Do not skip tests due to upstream failures — cascading failures reveal the true blast radius of bugs.

## MCP Tool Coverage

**Target**: 100% of exposed MCP tools have UAT test cases

| Category | Tools | Covered |
|----------|-------|---------|
| Note CRUD | 12 | 100% |
| Search | 4 | 100% |
| Memory Search | 5 | 100% |
| Provenance Creation | 5 | 100% |
| Tags | 2 | 100% |
| Collections | 9 | 100% |
| Templates | 6 | 100% |
| Embedding Sets | 15 | 100% |
| Versioning | 5 | 100% |
| Graph/Links | 7 | 100% |
| Jobs | 7 | 100% |
| SKOS | 34 | 100% |
| Archives | 7 | 100% |
| Document Types | 6 | 100% |
| Backup/Export | 22 | 100% |
| PKE | 13 | 100% |
| Observability | 8 | 100% |
| Auth & Access Control | 11 MCP + 4 infra | 100% |
| Caching & Performance | 5 | 100% |
| Attachment Processing | 5 | 100% |
| Vision | 2 | 100% |
| Audio | 2 | 100% |
| Video | 2 | 100% |
| 3D Models | 2 | 100% |
| Multi-Memory | 7 | 100% |
| **Total** | **202** | **100%** |

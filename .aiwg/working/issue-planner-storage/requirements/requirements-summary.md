# Requirements Summary: External Storage Backend + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (Requirements baseline)
**Generated**: 2026-05-21 from synthesis.md

## Use Cases by Workstream

| WS | Workstream | Use Cases |
|----|------------|-----------|
| WS-1 | Storage Backend Abstraction Extension | (developer-facing; foundational for UC-001 — no standalone UC) |
| WS-2 | Archive Schema and Registry | UC-EXTSTORAGE-001 (Create Referenced Archive) |
| WS-3 | Walker + Ignore + Secret-Scan | UC-EXTSTORAGE-002 (Walker + Secret Scan) |
| WS-4 | Scan-and-Ingest Job Pipeline | UC-EXTSTORAGE-003 (Initial Scan-and-Ingest) |
| WS-5 | Live Update Detection (DEFERRED) | UC-EXTSTORAGE-010 (Stub — v2 deferral) |
| WS-6 | Derived Artifact Companion Location | UC-EXTSTORAGE-006 (Derived Artifacts to Companion Dir) |
| WS-7 | API Surface | UC-EXTSTORAGE-001, UC-EXTSTORAGE-004 (Rescan), UC-EXTSTORAGE-005 (Scan Status), UC-EXTSTORAGE-008 (Degraded Mount), UC-EXTSTORAGE-009 (Quarantine Audit) |
| WS-8 | MCP Tool Surface | UC-EXTSTORAGE-007 (MCP Reindex) |
| WS-9 | Multi-Tenant Security Tests | Test scenario set (see acceptance-criteria-summary.md WS-9) |
| WS-10 | Documentation and Deployment Plan | Doc artifacts (no UC) |

**Total**: 9 active use cases + 1 deferred stub + 1 test scenario set = covers all 10 workstreams.

## Use Case Index

| ID | Title | Priority | Workstream(s) |
|----|-------|----------|---------------|
| UC-EXTSTORAGE-001 | Create Referenced Archive Pointing at Local Directory | HIGH | WS-2, WS-7 |
| UC-EXTSTORAGE-002 | Walker Walks Directory and Reports Secret-Scan Findings | HIGH | WS-3 |
| UC-EXTSTORAGE-003 | Initial Scan-and-Ingest on Referenced Archive Creation | HIGH | WS-4 |
| UC-EXTSTORAGE-004 | Operator Triggers Manual Rescan of Referenced Archive | HIGH | WS-7, WS-4 |
| UC-EXTSTORAGE-005 | Operator Views Referenced Archive Scan Status | MEDIUM | WS-7 |
| UC-EXTSTORAGE-006 | Generate Derived Artifacts in Companion Managed Location | HIGH | WS-6 |
| UC-EXTSTORAGE-007 | AI Agent Triggers Reindex via MCP Tool | MEDIUM | WS-8 |
| UC-EXTSTORAGE-008 | Degraded Read Behavior When Source Mount Disappears | HIGH | WS-7, WS-9 |
| UC-EXTSTORAGE-009 | Operator Audits Quarantined Files (Secret-Scan Skips) | MEDIUM | WS-7 |
| UC-EXTSTORAGE-010 | Live Filesystem Update Detection (DEFERRED — v2) | N/A (deferred) | WS-5 |

## NFR Module

@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md

12 NFRs covering:
- Security (NFR-001, 002, 003, 004): secret scan, no source writes, path canonicalization, tenant scoping
- Performance (NFR-005, 006, 007): scan throughput, embedding throughput, initial scan duration
- Observability (NFR-008, 010): structured JSON logging, per-archive metrics
- Operability (NFR-009): force-rescan API
- Compatibility (NFR-011): backward-compat with Managed archives
- Reliability (NFR-012): degraded-mode behavior

## Acceptance Criteria Summary

@.aiwg/working/issue-planner-storage/requirements/acceptance-criteria-summary.md

Maps WS → UC → NFR → planned test categories.

## Elaboration Completeness Gate Criteria

The Requirements artifact set is complete for the Elaboration → Construction gate when:

1. **Coverage**: All 10 workstreams have either a use case, a deferral stub, or a test scenario set ✓
2. **NFR depth**: Every UC references at least one applicable NFR ✓
3. **Measurable AC**: Every AC is verifiable with a specific command, threshold, or assertion (per `vague-discretion` rule) ✓
4. **Risk traceability**: Top-10 risks from synthesis §5 have corresponding NFRs or test scenarios:
   - R-1 (secret leakage) → NFR-001, UC-002, WS-9 TS-5/TS-6
   - R-2 (multi-tenant breach) → NFR-003, 004, UC-001 EF-1, WS-9 TS-1/TS-2/TS-3/TS-4
   - R-3 (perf death) → NFR-005, 006, 007
   - R-4 (FS events drop) → mitigated by Decision 4 deferral (UC-010); no NFR needed
   - R-5 (source disappears) → NFR-012, UC-008, WS-9 TS-8
   - R-6 (regex chunk quality) → out of scope for #736 per synthesis §7
   - R-7 (symlink loops) → UC-002 EF-2, NFR-002
   - R-8 (derived disk usage) → NFR-010 (metrics), WS-10 docs
   - R-9 (rename detection) → documented limitation in UC-003 AF-2
   - R-10 (tree-sitter memory) → deferred per synthesis §7
5. **Operator approval queue**: All 8 open questions (Q-1 through Q-8) from synthesis §6 are surfaced in `acceptance-criteria-summary.md` for Phase 5 gate ✓
6. **Non-goals documented**: synthesis §7 non-goals are NOT in any UC (verified) ✓

## Estimation Inputs (no time estimates per `no-time-estimates` rule)

For Phase 4 backlog generation, scope per workstream measured in atomic items:

| WS | Atomic items | Parallel-ready? | Notes |
|----|--------------|-----------------|-------|
| WS-1 | 4 (FileSource variant, ReferencedBackend impl, streaming hash, dispatch extension) | Yes — pure trait, no dependencies | First wave |
| WS-2 | 4 (migration, ArchiveInfo struct, create() method, cache integration) | After WS-1 | Second wave |
| WS-3 | 3 (walker module, path-denylist, content-regex denylist) | Parallel to WS-2 | Standalone library |
| WS-4 | 5 (job type, handler, hash dedup, blob/note INSERTs, extraction-gate extension) | After WS-1, 2, 3 | Third wave |
| WS-5 | 0 (deferred) | N/A | v2 backlog only |
| WS-6 | 2 (config var, dispatch in extraction_handler) | After WS-2 | Parallel to WS-4 |
| WS-7 | 5 (4 endpoints + middleware extension) | After WS-2, 4 | Fourth wave |
| WS-8 | 3 (extend manage_archives, new rescan_archive tool, update docs) | After WS-7 | Fifth wave |
| WS-9 | 10 test scenarios | After WS-1 through WS-7 | Validation wave |
| WS-10 | 5 docs (CLAUDE.md, deploy, multi-memory, ops guide, API ref) | Parallel to all | Tracks implementation |

**Total atomic items**: ~41 + 10 test scenarios = ~51 items for Phase 4 backlog.

## References

- @.aiwg/working/issue-planner-storage/synthesis.md (sole design source)
- @.aiwg/working/issue-planner-storage/requirements/use-cases/ (all 10 UC files)
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- @.aiwg/working/issue-planner-storage/requirements/acceptance-criteria-summary.md
- @.claude/rules/no-time-estimates.md (estimation discipline)
- @.claude/rules/vague-discretion.md (measurable AC requirement)

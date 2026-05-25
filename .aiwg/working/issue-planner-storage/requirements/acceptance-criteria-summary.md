# Acceptance Criteria Summary: External Storage Backend + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (Requirements baseline)
**Source**: synthesis.md, use cases, NFRs

Cross-reference table linking workstreams → use cases → NFRs → planned test categories.

## Workstream → Use Case → NFR → Test Mapping

| WS | Workstream | Use Case(s) | NFRs Applied | Test Categories (Phase 3 test plan) |
|----|------------|-------------|--------------|-------------------------------------|
| WS-1 | Storage Backend Abstraction Extension | (developer-facing; covered in UC-001 preconditions) | NFR-002 | Unit tests on ReferencedBackend trait impl: writes refused, reads work, resolve_path returns literal path; streaming BLAKE3 hash correctness |
| WS-2 | Archive Schema and Registry | UC-EXTSTORAGE-001 | NFR-001, 003, 004, 011 | Migration test (additive, no breakage); ArchiveInfo struct unit tests; cache TTL preservation |
| WS-3 | Walker + Ignore + Secret-Scan | UC-EXTSTORAGE-002 | NFR-001, 002, 005, 008 | Walker behavior on .gitignore fixtures; secret-pattern unit tests; throughput benchmark; permission-denied + symlink-loop tests |
| WS-4 | Scan-and-Ingest Job Pipeline | UC-EXTSTORAGE-003 | NFR-002, 005, 006, 007, 010 | Integration test: 1k-file fixture end-to-end; idempotency test; derived-artifact routing test |
| WS-5 | Live Update Detection (DEFERRED) | UC-EXTSTORAGE-010 (stub) | N/A (v1) | None — documented as v2 deferral |
| WS-6 | Derived Artifact Companion Location | UC-EXTSTORAGE-006 | NFR-002, 011 | Image fixture → thumbnail in companion dir; video → keyframes; managed-archive no-regression test; drop-archive cleanup test |
| WS-7 | API Surface | UC-EXTSTORAGE-001, 004, 005, 008, 009 | NFR-001, 003, 004, 009, 010, 012 | API contract tests for all 4 endpoints; middleware-write-rejection test; multi-tenant boundary test |
| WS-8 | MCP Tool Surface | UC-EXTSTORAGE-007 | NFR-009, 011 | MCP Inspector schema validation; backward-compat test for `manage_archives`; rescan-via-MCP integration test |
| WS-9 | Multi-Tenant Security Tests | (test scenarios, see below) | All security NFRs | Cross-tenant boundary; path traversal; symlink-out-of-root; secret-scan red-team; drop-archive safety; mount-disappearance |
| WS-10 | Documentation and Deployment Plan Updates | (docs, no UC) | N/A | doc-sync passes; operator can follow guide to mount + create end-to-end |

## Critical Acceptance Criteria (Elaboration Gate Blockers)

These cross-cutting criteria MUST pass before exiting Elaboration:

1. **No source writes** (NFR-EXTSTORAGE-002): checksum-diff test passes for every UC flow
2. **Secret detection mandatory** (NFR-EXTSTORAGE-001): red-team test with AWS access key + PEM private key both quarantined
3. **Multi-tenant boundary** (NFR-EXTSTORAGE-004): cross-tenant cross-read attempt fails
4. **Backward compatibility** (NFR-EXTSTORAGE-011): pre-migration test suite passes 100% after migration
5. **Degraded reliability** (NFR-EXTSTORAGE-012): mount-disappearance test passes for both read and write paths

## WS-9 Security Test Scenarios (Detailed)

WS-9 deliberately is NOT a single use case — it is a test-scenario set per synthesis §4 WS-9. The scenarios:

| ID | Scenario | Expected Outcome | Source |
|----|----------|------------------|--------|
| TS-1 | Create archive with source_path = `/etc` | HTTP 400 (allowlist violation in multi-tenant; outside common allowlist in single-tenant config) | NFR-003, 004 |
| TS-2 | Create archive with symlink to outside allowlist | After canonicalization, rejected | NFR-003 |
| TS-3 | Download endpoint with path-traversal in URL | HTTP 404, not 200 (middleware rejects) | NFR-003 |
| TS-4 | Tenant B authenticated, attempts to access Tenant A's archive | HTTP 403/404 (tenant scoping) | NFR-004 |
| TS-5 | Drop a file with PEM PRIVATE KEY header into source dir, scan | File quarantined; no chunks in pgvector for that content_hash | NFR-001 |
| TS-6 | Drop a file with AWS access key pattern into source dir, scan | File quarantined; content_denylist reason recorded | NFR-001 |
| TS-7 | DROP a Referenced archive | PG schema dropped; companion `{derived_root}/{archive_id}/` removed; source_path UNTOUCHED (checksum verified) | NFR-002 |
| TS-8 | Unmount source_path during active query | Search returns warning flag; download returns 503; API process stays healthy | NFR-012 |
| TS-9 | Concurrent rescan + read on same archive | Reads succeed against cached chunks; rescan completes; no DB deadlocks | NFR-012 |
| TS-10 | Permission-denied subdir mid-scan | Scan logs warning, continues with remaining files; summary marks `partial: true` | NFR-008 |

## Open Questions Surfacing at Phase 5 Approval Gate

Per synthesis §6, these questions require operator approval before Construction:

1. **Q-1** Live updates in v1? (Recommended: No, defer to v2) — affects WS-5 scope
2. **Q-2** Per-blob mode user-facing? (Recommended: No) — affects WS-2 schema
3. **Q-3** Secret-scan opt-out for performance? (Recommended: No) — affects NFR-001
4. **Q-4** MCP surface: extend existing or new family? (Recommended: extend) — affects WS-8
5. **Q-5** Path allowlist always-on? (Recommended: only in multi-tenant) — affects NFR-004
6. **Q-6** Multi-archive directory overlap? (Recommended: allow with warning) — affects UC-001 AF-2
7. **Q-7** Initial scan performance target? (Recommended: <10 minutes) — affects NFR-007
8. **Q-8** Failure mode strict or lenient? (Recommended: lenient v1, strict opt-in v1.5) — affects NFR-012

## References

- @.aiwg/working/issue-planner-storage/synthesis.md
- @.aiwg/working/issue-planner-storage/requirements/use-cases/ (UC-EXTSTORAGE-001 through 010)
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- @.aiwg/working/issue-planner-storage/requirements/requirements-summary.md

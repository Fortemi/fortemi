# Issue Backlog — fortemi/fortemi#736 (External Storage Backend + Scan-and-Ingest)

**Status**: **DRAFT — UNAPPROVED**. No issues to be filed against `fortemi/fortemi` until Phase 5 operator approval gate is cleared (see §6).
**Generated**: 2026-05-21
**Phase**: 4 (Issue Backlog Generation) of `issue-planner` workflow
**Inputs**: `synthesis.md` §4 (workstreams), §5 (risks), §6 (open questions); `architecture/` (ADRs + SAD); `requirements/` (UCs + NFRs); `testing/` (104 test cases); `deployment/` (rollout plan)

---

## 1. Backlog Overview

### Totals

| Category | Count |
|---|---|
| Epic (existing #736) | 1 (no proposal — already exists) |
| Workstream parents | 10 (WS-1 through WS-10; WS-5 = deferred placeholder) |
| Atomic implementation issues | 42 |
| Test tracker issues | 9 (one per active workstream; WS-5 omitted) |
| Documentation issues | 6 |
| Cross-cutting issues | 4 |
| **Total proposed new issues** | **71** |

### Workstream breakdown

| WS | Workstream | Parent | Impl issues | Test tracker | Docs | Total |
|----|------------|--------|-------------|--------------|------|-------|
| 1 | Storage backend abstraction extension | 1 | 4 | 1 | 0 | 6 |
| 2 | Archive schema and registry | 1 | 5 | 1 | 1 | 8 |
| 3 | Walker + ignore + secret-scan | 1 | 5 | 1 | 0 | 7 |
| 4 | Scan-and-ingest job pipeline | 1 | 6 | 1 | 0 | 8 |
| 5 | Live update detection (DEFERRED) | 1 | 0 | 0 | 0 | 1 |
| 6 | Derived artifact companion location | 1 | 3 | 1 | 0 | 5 |
| 7 | API surface | 1 | 6 | 1 | 1 | 9 |
| 8 | MCP tool surface | 1 | 3 | 1 | 1 | 6 |
| 9 | Multi-tenant security tests | 1 | 4 | 1 | 0 | 6 |
| 10 | Documentation and deployment plan updates | 1 | 6 | 0 | 3 | 10 |
| — | Cross-cutting | — | — | — | — | 4 |
| **Sum** | | **10** | **42** | **9** | **6** | **71** |

### Phase distribution (Fortemi `type:*` label intent)

- `type: feat` (implementation): 42
- `type: test` (regression/coverage): 9
- `type: docs`: 6
- `type: chore` (cross-cutting ops/runbook/flag/metrics): 4
- `type: epic` (workstream parents): 10
- Epic #736 itself: unchanged label set; body updated to reference workstream parents

### Approval status

**ALL ISSUES BELOW ARE DRAFT.** They will be filed only after Phase 5 operator approval per the gate in §6. The Phase 6 filer must:
1. Wait for explicit operator approval on the 8 open questions (Q-1 through Q-8 in `synthesis.md` §6) AND the 8 open architecture questions in the SAD.
2. File in the order specified in §4.
3. Update issue numbers in the Body's `Blocked-by:`/`Blocks:` lines as real numbers are assigned (placeholder `WS-N-X` becomes the real `#NNN`).

---

## 2. Issue Tree

### 2.1 Existing epic — do not propose

**fortemi/fortemi#736** — *External storage backend + scan-and-ingest* (already exists; epic body to be updated in Phase 6 with the dependency tree below; add `epic` label if not present).

### 2.2 Workstream parents (10 issues)

These 10 issues are filed first. #736 is updated to `Blocked-by:` all 10. Each carries the `epic` label.

---

#### WS-1-PARENT — `epic(matric-db): storage backend abstraction extension`
- **Type**: parent (epic)
- **Workstream**: WS-1
- **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Foundational trait extension that every other workstream depends on. Per ADR-FORTEMI-101 (`architecture/adr-FORTEMI-101-referenced-storage-backend.md`), add `FileSource::Referenced(PathBuf)` variant and `ReferencedBackend` impl of `StorageBackend` to `crates/matric-db/src/file_storage.rs`. No breaking change to existing trait surface.
  - **Acceptance criteria**:
    - [ ] `cargo build -p matric-db` succeeds with new variant and impl
    - [ ] All existing `cargo test -p matric-db` pass unchanged (zero regressions on Managed mode)
    - [ ] New unit tests cover ReferencedBackend behaviour: `write`/`delete` return `Err(ReadOnlyBackend)`; `read`/`exists`/`resolve_path` work against absolute paths; `compute_content_hash_stream` produces identical hashes to in-memory variant for files ≤10MB
    - [ ] Children WS-1-A through WS-1-D all closed
  - **Dependencies**: Blocks WS-2-PARENT, WS-4-PARENT, WS-6-PARENT, WS-7-PARENT, WS-9-PARENT, fortemi/fortemi#736
  - **References**: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-101-referenced-storage-backend.md`; `@.aiwg/working/issue-planner-storage/synthesis.md#decision-2`
  - **Files likely to change**: `crates/matric-db/src/file_storage.rs`
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: []`

---

#### WS-2-PARENT — `epic(matric-db): archive registry schema and routing for Referenced mode`
- **Type**: parent (epic)
- **Workstream**: WS-2
- **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `scope: matric-api`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per ADR-FORTEMI-100, storage mode is an archive-level property. Add 5 columns to `archive_registry`: `storage_mode`, `source_path`, `scan_config`, `last_scan_at`, `scan_status`. Extend `ArchiveInfo`, `ArchiveRepository`, `DefaultArchiveCache`, `ArchiveContext`. Migration must default `storage_mode='managed'` for backwards compatibility.
  - **Acceptance criteria**:
    - [ ] Migration applies cleanly on a database with existing Managed archives (verified by integration test)
    - [ ] All existing archives have `storage_mode='managed'` post-migration; no behaviour change for them
    - [ ] `archive_registry_cache` TTL expiry continues to function with new fields
    - [ ] `clone_archive_schema()` refuses to clone a Referenced archive into another Referenced archive without explicit `source_path` (integration test verifies 400 error)
    - [ ] Children WS-2-A through WS-2-E all closed
  - **Dependencies**: `Blocked-by: WS-1-PARENT`; `Blocks: WS-4-PARENT, WS-6-PARENT, WS-7-PARENT, WS-8-PARENT, fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-100-storage-mode-archive-level.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-001.md`
  - **Files likely to change**: `migrations/<new>_referenced_storage.sql`, `crates/matric-db/src/archives.rs`, `crates/matric-api/src/middleware/archive_routing.rs`
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-1-PARENT]`

---

#### WS-3-PARENT — `epic(matric-jobs): scan walker with ignore-respecting traversal and secret detection`
- **Type**: parent (epic)
- **Workstream**: WS-3
- **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per ADR-FORTEMI-104 (mandatory secret pre-scan) and synthesis §3 Decision 7. Standalone library `crates/matric-jobs/src/scan_walker.rs` wrapping `ignore::WalkBuilder` with Fortemi defaults (denylist + content-regex secret detection). Add `ignore = "0.4"` workspace dep.
  - **Acceptance criteria**:
    - [ ] `cargo add` for `ignore` crate at workspace level; lockfile committed
    - [ ] Walker honours `.gitignore` for 5 fixture cases (npm project, python venv, rust target, mixed, nested)
    - [ ] Default file size cap of 10MB enforced (configurable via `scan_config.max_file_size`)
    - [ ] Path-denylist hits (e.g., `.env`, `*.pem`) produce `QuarantineEvent` with `rule_name='path_denylist'` — file content NEVER logged (per `token-security.md`)
    - [ ] Content-regex hits (PEM header, AWS access key, GitHub PAT, JWT) produce `QuarantineEvent` with `rule_name='content_regex'`
    - [ ] Symlink-loop protection works (default in `ignore` crate; verified by fixture); out-of-root symlinks skipped with logged warning
    - [ ] Permission-denied subdirs skipped, NOT fatal
    - [ ] Parallel walking via `WalkBuilder::build_parallel()` capped at `min(4, num_cpus)`
    - [ ] Children WS-3-A through WS-3-E all closed
  - **Dependencies**: `Blocked-by: (none — standalone library)`; `Blocks: WS-4-PARENT, fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-104-mandatory-secret-prescan.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-002.md`
  - **Files likely to change**: new `crates/matric-jobs/src/scan_walker.rs`, `Cargo.toml`
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: []`

---

#### WS-4-PARENT — `epic(matric-jobs): scan-and-ingest job pipeline for Referenced archives`
- **Type**: parent (epic)
- **Workstream**: WS-4
- **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per synthesis §4 WS-4. New `JobType::DirectoryScan` and `DirectoryScanHandler`. Orchestrates: walk (WS-3) → streaming BLAKE3 hash (WS-1) → blob dedup → INSERT blobs/attachments/notes → queue Extraction jobs. Idempotent on content_hash. Extends `extraction_handler.rs` line 146 path-access gate to include `storage_backend='referenced'`.
  - **Acceptance criteria**:
    - [ ] Scan-and-ingest of a 1k-file fixture repo completes; chunks + embeddings appear in archive's pgvector store
    - [ ] Re-running scan is a no-op (idempotency confirmed by row counts unchanged)
    - [ ] Existing search API returns results for ingested chunks (smoke test via `/api/v1/search`)
    - [ ] Derived artifacts (test with image fixture) land in `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/` not in source directory
    - [ ] `archive_registry.scan_status` lifecycle transitions: `idle → scanning → idle | error`
    - [ ] Mixed-mode at blob layer: source blobs have `storage_backend='referenced'`, derived blobs have `storage_backend='filesystem'`
    - [ ] Children WS-4-A through WS-4-F all closed
  - **Dependencies**: `Blocked-by: WS-1-PARENT, WS-2-PARENT, WS-3-PARENT`; `Blocks: WS-7-PARENT, WS-9-PARENT, fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-101-referenced-storage-backend.md`; `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-102-derived-artifacts-companion-location.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-003.md`
  - **Files likely to change**: new `crates/matric-jobs/src/directory_scan_handler.rs`, `crates/matric-jobs/src/extraction_handler.rs`, `crates/matric-core/src/lib.rs`
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-1-PARENT, WS-2-PARENT, WS-3-PARENT]`

---

#### WS-5-PARENT — `epic(matric-jobs): live update detection (DEFERRED to v2 — design RFC required)`
- **Type**: parent (deferred backlog)
- **Workstream**: WS-5
- **Labels**: `priority: P3`, `type: feat`, `scope: matric-jobs`, `phase: deferred`, `epic`
- **Body**:
  - **Context**: Per ADR-FORTEMI-103, live filesystem watching is explicitly deferred to v2. This issue is the placeholder; **NO IMPLEMENTATION SHOULD BEGIN** until a separate design RFC is opened and approved. The RFC must address: Docker bind-mount `overlay2` inotify silent failures, Linux `max_user_watches` exhaustion on large repos, macOS FSEvents coalescing, Windows ReadDirectoryChangesW 64KB buffer overflow, and the watcher-process-per-archive lifecycle.
  - **Acceptance criteria**:
    - [ ] Issue remains in `priority: P3` backlog until separate RFC issue opened and merged
    - [ ] When unblocked, child issues will be drafted from the RFC's component breakdown
  - **Dependencies**: `Blocked-by: <future RFC issue>`; not blocking #736 (explicit out-of-scope per `synthesis.md` §7 non-goal 3)
  - **References**: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-103-defer-live-watching.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-010.md`
  - **Files likely to change**: TBD per RFC
- **Dep declaration**: `BLOCKS: []`; `BLOCKED-BY: []` (deferred)

---

#### WS-6-PARENT — `epic(matric-jobs): derived artifact companion-location storage`
- **Type**: parent (epic)
- **Workstream**: WS-6
- **Labels**: `priority: P1`, `type: feat`, `scope: matric-jobs`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per ADR-FORTEMI-102, derived artifacts (thumbnails, transcripts, embeddings) for Referenced archives go to `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/{blob_id}.bin` (default `{FILE_STORAGE_PATH}/derived/`). Update `store_derived_attachment_tx` to dispatch based on `storage_mode`.
  - **Acceptance criteria**:
    - [ ] Dropping a Referenced archive deletes the companion-location subdirectory but leaves `source_path` untouched (integration test verifies via mock filesystem)
    - [ ] Managed-archive derived artifacts continue using the existing path (zero behaviour change for existing archives)
    - [ ] New env var `FORTEMI_DERIVED_STORAGE_PATH` documented and defaults to `{FILE_STORAGE_PATH}/derived/`
    - [ ] Children WS-6-A through WS-6-C all closed
  - **Dependencies**: `Blocked-by: WS-2-PARENT`; `Blocks: WS-4-PARENT (extraction handler dispatch), fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-102-derived-artifacts-companion-location.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-006.md`
  - **Files likely to change**: `crates/matric-jobs/src/extraction_handler.rs`, possibly new helper in `matric-core`
- **Dep declaration**: `BLOCKS: [#736, WS-4-PARENT]`; `BLOCKED-BY: [WS-2-PARENT]`

---

#### WS-7-PARENT — `epic(matric-api): API surface for Referenced archive create, rescan, status, quarantine`
- **Type**: parent (epic)
- **Workstream**: WS-7
- **Labels**: `priority: P0`, `type: feat`, `scope: matric-api`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per synthesis §4 WS-7 + UCs 001/004/005/008/009. New routes: `POST /api/v1/archives/referenced`, `POST /api/v1/archives/{name}/rescan`, `GET /api/v1/archives/{name}/scan-status`, `GET /api/v1/archives/{name}/quarantined-files`. Write-gate in `archive_routing_middleware` rejects mutating routes on Referenced archives with 403 (rescan endpoint allow-listed). Path canonicalization + allowlist enforcement at create time via `FORTEMI_REFERENCED_STORAGE_ROOTS`.
  - **Acceptance criteria**:
    - [ ] Cannot create Referenced archive with path outside allowlist (returns 400 with error code `path_outside_allowlist`)
    - [ ] Cannot create Referenced archive with non-existent source_path (returns 400 with error code `source_path_not_found`)
    - [ ] Path canonicalization defeats `../../../etc/passwd` traversal (integration test in WS-9 verifies)
    - [ ] Multi-tenant mode (`FORTEMI_MULTI_TENANT=true`) forces allowlist non-empty at startup (fail-closed at boot)
    - [ ] Mutating routes on Referenced archives return 403 with clear error message (`X-Storage-Mode: referenced` header set; error body names the route)
    - [ ] Rescan endpoint returns 202 with `job_id` and the job runs to completion (verified by polling scan-status to `idle`)
    - [ ] Children WS-7-A through WS-7-F all closed
  - **Dependencies**: `Blocked-by: WS-2-PARENT, WS-4-PARENT, WS-6-PARENT`; `Blocks: WS-8-PARENT, WS-9-PARENT, fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-001.md`, `UC-EXTSTORAGE-004.md`, `UC-EXTSTORAGE-005.md`, `UC-EXTSTORAGE-008.md`, `UC-EXTSTORAGE-009.md`; `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-003, NFR-004, NFR-012)
  - **Files likely to change**: `crates/matric-api/src/main.rs`, `crates/matric-api/src/middleware/archive_routing.rs`
- **Dep declaration**: `BLOCKS: [#736, WS-8-PARENT, WS-9-PARENT]`; `BLOCKED-BY: [WS-2-PARENT, WS-4-PARENT, WS-6-PARENT]`

---

#### WS-8-PARENT — `epic(mcp-server): MCP tool surface — extend manage_archives and add rescan_archive`
- **Type**: parent (epic)
- **Workstream**: WS-8
- **Labels**: `priority: P1`, `type: feat`, `scope: mcp-server`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per synthesis §3 Decision 6, extend `manage_archives` Node MCP tool with `storage_mode` and `source_path` params; add `rescan_archive` tool (async-job-return semantics). Backward compat: existing `manage_archives` invocations without new params default to Managed.
  - **Acceptance criteria**:
    - [ ] Agents can create a Referenced archive via MCP with `manage_archives({operation:'create', storage_mode:'referenced', source_path:'/srv/code/repo'})`
    - [ ] Agents can trigger rescan via `rescan_archive({archive_name, full?:boolean})` and poll status
    - [ ] Tool schemas validate in MCP Inspector (no warnings)
    - [ ] Existing test suite for `manage_archives` continues to pass (zero regressions)
    - [ ] `get_documentation` tool output updated to surface new params and tool
    - [ ] Children WS-8-A through WS-8-C all closed
  - **Dependencies**: `Blocked-by: WS-7-PARENT`; `Blocks: fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-007.md`; `@.aiwg/working/issue-planner-storage/synthesis.md#decision-6`
  - **Files likely to change**: `mcp-server/` (Node.js)
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-7-PARENT]`

---

#### WS-9-PARENT — `epic(security): multi-tenant security regression suite for Referenced archives`
- **Type**: parent (epic)
- **Workstream**: WS-9
- **Labels**: `priority: P0`, `type: test`, `scope: security`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per `testing/tenant-isolation-regression-suite.md`, implement TI-EXTSTORAGE-1 through TI-EXTSTORAGE-10 covering: path traversal, symlink-out-of-root, multi-tenant cross-archive access, secret-scan red-team, drop-archive safety (never touches `source_path`), mount-disappearance behaviour. All must pass on CI before #736 closes.
  - **Acceptance criteria**:
    - [ ] All 10 TI-EXTSTORAGE-* tests implemented and passing on CI
    - [ ] Path traversal: `../../../etc/passwd` via download endpoint returns 404 (never 200)
    - [ ] Symlink targeting `/etc` outside source root is skipped with logged warning, never followed
    - [ ] Tenant A's Referenced archive at `/srv/fortemi/A/code` cannot be accessed by Tenant B even if B knows the archive name (verified with two-tenant integration fixture)
    - [ ] PEM private-key fixture file dropped into a scanned directory is quarantined; no embeddings appear in pgvector
    - [ ] `drop_archive_schema` never calls `backend.delete()` for Referenced blobs (verified by mock backend that panics on `delete`)
    - [ ] Unmounting source path mid-query: reads return cached results with `X-Source-Stale: true` warning header; writes/rescans return 503
    - [ ] Children WS-9-A through WS-9-D all closed
  - **Dependencies**: `Blocked-by: WS-1-PARENT, WS-2-PARENT, WS-3-PARENT, WS-4-PARENT, WS-6-PARENT, WS-7-PARENT`; `Blocks: fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`; `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-002, NFR-003, NFR-004); `@.aiwg/working/issue-planner-storage/synthesis.md#risks` (R-1, R-2, R-7)
  - **Files likely to change**: `crates/matric-api/tests/`, new `crates/matric-db/tests/referenced_security.rs`
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-1-PARENT, WS-2-PARENT, WS-3-PARENT, WS-4-PARENT, WS-6-PARENT, WS-7-PARENT]`

---

#### WS-10-PARENT — `epic(docs): documentation, deployment, and operator-guide updates for Referenced storage`
- **Type**: parent (epic)
- **Workstream**: WS-10
- **Labels**: `priority: P1`, `type: docs`, `scope: docs`, `phase: construction`, `epic`
- **Body**:
  - **Context**: Per synthesis §4 WS-10. Update `CLAUDE.md`, add Referenced storage section to deployment bundle docs, update multi-memory agent guide, add new `docs/content/referenced-storage.md` operator guide.
  - **Acceptance criteria**:
    - [ ] `doc-sync` skill passes (no broken @-mentions across all updated docs)
    - [ ] An operator following only the deployment doc can successfully mount a directory and create a Referenced archive end-to-end (validated by a dry-run on a clean Docker bundle)
    - [ ] API reference updated for all 4 new endpoints
    - [ ] `installer/scripts/configure.sh` documented for new env vars (`FORTEMI_EXTERNAL_STORAGE_ENABLED`, `FORTEMI_REFERENCED_STORAGE_ROOTS`, `FORTEMI_DERIVED_STORAGE_PATH`)
    - [ ] Children WS-10-A through WS-10-F all closed
  - **Dependencies**: `Blocked-by: (none structurally — tracks code progress)`; `Blocks: fortemi/fortemi#736`
  - **References**: `@.aiwg/working/issue-planner-storage/deployment/deployment-plan.md`
  - **Files likely to change**: `CLAUDE.md`, `docs/`, `installer/`
- **Dep declaration**: `BLOCKS: [#736]`; `BLOCKED-BY: []`

---

### 2.3 Atomic implementation issues (42)

#### Workstream 1 — Storage backend abstraction (4 issues)

##### WS-1-A — `feat(matric-db): add FileSource::Referenced(PathBuf) variant`
- **Type**: impl | **WS**: 1 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: First step of ADR-FORTEMI-101. Add discriminant variant before introducing the backend impl.
  - Acceptance criteria:
    - [ ] `FileSource` enum has new `Referenced(PathBuf)` variant
    - [ ] All existing match arms exhaustive (compile-time check)
    - [ ] Serialization/deserialization round-trip test for the new variant
  - Dependencies: `Blocks: WS-1-B, WS-1-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-101-referenced-storage-backend.md`
  - Files: `crates/matric-db/src/file_storage.rs`
- Dep: `BLOCKS: [WS-1-B, WS-1-PARENT]`; `BLOCKED-BY: []`

##### WS-1-B — `feat(matric-db): implement ReferencedBackend with no-op write/delete and absolute-path read`
- **Type**: impl | **WS**: 1 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Core of ADR-FORTEMI-101. `write` and `delete` MUST return `Err(StorageError::ReadOnlyBackend)`; `read`, `exists`, `resolve_path` MUST work against the literal absolute path.
  - Acceptance criteria:
    - [ ] `ReferencedBackend` struct implements `StorageBackend` trait
    - [ ] `write(_, _) -> Err(ReadOnlyBackend)` enforced; unit test asserts
    - [ ] `delete(_) -> Err(ReadOnlyBackend)` enforced; unit test asserts
    - [ ] `read(path)` reads from absolute filesystem path; returns `Err(NotFound)` if unreachable (mount disappeared)
    - [ ] `resolve_path(path) -> Some(path)` returns input unchanged
    - [ ] `exists(path)` returns `Ok(false)` not `Err` for missing files (unblocks degraded-mode logic in WS-7)
  - Dependencies: `Blocked-by: WS-1-A`; `Blocks: WS-1-D, WS-1-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-101-referenced-storage-backend.md`
  - Files: `crates/matric-db/src/file_storage.rs`
- Dep: `BLOCKS: [WS-1-D, WS-1-PARENT]`; `BLOCKED-BY: [WS-1-A]`

##### WS-1-C — `feat(matric-db): add streaming compute_content_hash_stream for large files`
- **Type**: impl | **WS**: 1 | **Labels**: `priority: P1`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Existing `compute_content_hash` at line 317 loads file into memory. For Referenced mode, files may be large (10MB cap default but configurable). Add streaming BLAKE3 variant that chunks reads.
  - Acceptance criteria:
    - [ ] New function `compute_content_hash_stream(path: &Path) -> Result<Hash>` reads in 64KB chunks
    - [ ] Output bit-identical to `compute_content_hash` for files ≤10MB (property test across 100 random fixtures)
    - [ ] Peak memory usage under 1MB for a 1GB file (verified by criterion bench or manual `/proc/self/status` check)
  - Dependencies: `Blocks: WS-1-PARENT, WS-4-B`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-101-referenced-storage-backend.md`; `@.aiwg/working/issue-planner-storage/synthesis.md` §2.1 (BLAKE3 content addressing)
  - Files: `crates/matric-db/src/file_storage.rs`
- Dep: `BLOCKS: [WS-1-PARENT, WS-4-B]`; `BLOCKED-BY: []`

##### WS-1-D — `feat(matric-db): extend PgFileStorageRepository to dispatch Referenced backend`
- **Type**: impl | **WS**: 1 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Wire `ReferencedBackend` into the repository's backend selector. Dispatch decision based on per-blob `storage_backend` column (default `filesystem`).
  - Acceptance criteria:
    - [ ] `PgFileStorageRepository::get_backend(blob)` returns `ReferencedBackend` when `blob.storage_backend == "referenced"`
    - [ ] Default `filesystem` dispatch unchanged (regression-tested against existing test suite)
    - [ ] `Err(StorageBackendUnknown)` returned on invalid `storage_backend` value (not `referenced` and not `filesystem`)
  - Dependencies: `Blocked-by: WS-1-B`; `Blocks: WS-1-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-101-referenced-storage-backend.md`
  - Files: `crates/matric-db/src/file_storage.rs`
- Dep: `BLOCKS: [WS-1-PARENT]`; `BLOCKED-BY: [WS-1-B]`

---

#### Workstream 2 — Archive schema and registry (5 issues)

##### WS-2-A — `feat(migrations): add storage_mode, source_path, scan_config, last_scan_at, scan_status to archive_registry`
- **Type**: impl | **WS**: 2 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Schema foundation for ADR-FORTEMI-100. New SQL migration in `migrations/<timestamp>_referenced_storage.sql`. All 5 columns nullable with sane defaults to preserve backward compat.
  - Acceptance criteria:
    - [ ] Migration file added with column definitions: `storage_mode VARCHAR(20) NOT NULL DEFAULT 'managed'`, `source_path TEXT`, `scan_config JSONB`, `last_scan_at TIMESTAMPTZ`, `scan_status VARCHAR(20) NOT NULL DEFAULT 'idle'`
    - [ ] CHECK constraint: `storage_mode IN ('managed', 'referenced')` and `scan_status IN ('idle', 'scanning', 'error')`
    - [ ] Migration tested against existing Managed-only database; all pre-existing rows have `storage_mode='managed'` post-migration
    - [ ] Migration is reversible (DOWN migration removes columns cleanly)
  - Dependencies: `Blocks: WS-2-B, WS-2-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-100-storage-mode-archive-level.md`; `@.aiwg/working/issue-planner-storage/deployment/deployment-plan.md` (5 nullable columns invariant)
  - Files: `migrations/<timestamp>_referenced_storage.sql`
- Dep: `BLOCKS: [WS-2-B, WS-2-PARENT]`; `BLOCKED-BY: []`

##### WS-2-B — `feat(matric-db): extend ArchiveInfo and ArchiveRepository with storage_mode fields`
- **Type**: impl | **WS**: 2 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Rust struct + repo trait additions to match WS-2-A schema.
  - Acceptance criteria:
    - [ ] `ArchiveInfo` struct has 5 new fields matching schema (with `serde` defaults for null tolerance)
    - [ ] `ArchiveRepository::get_archive` populates all new fields from row
    - [ ] All existing call sites compile and pass tests (defaults match Managed semantics)
  - Dependencies: `Blocked-by: WS-2-A`; `Blocks: WS-2-C, WS-2-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-100-storage-mode-archive-level.md`
  - Files: `crates/matric-db/src/archives.rs`
- Dep: `BLOCKS: [WS-2-C, WS-2-PARENT]`; `BLOCKED-BY: [WS-2-A]`

##### WS-2-C — `feat(matric-db): add create_referenced_archive() convenience method`
- **Type**: impl | **WS**: 2 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Encapsulates Referenced archive creation: validates source_path (must be absolute, must exist), sets `storage_mode='referenced'`, calls existing per-archive schema creation, leaves `scan_status='idle'`.
  - Acceptance criteria:
    - [ ] `create_referenced_archive(name, source_path, scan_config) -> Result<ArchiveInfo>` implemented
    - [ ] Returns `Err(InvalidSourcePath)` if path is not absolute
    - [ ] Returns `Err(SourcePathNotFound)` if path does not exist at creation time
    - [ ] Successful call creates per-archive PG schema (existing behaviour preserved)
    - [ ] Integration test creates a Referenced archive against a tempdir, asserts `storage_mode='referenced'` in DB
  - Dependencies: `Blocked-by: WS-2-B`; `Blocks: WS-2-PARENT, WS-7-A`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-001.md`
  - Files: `crates/matric-db/src/archives.rs`
- Dep: `BLOCKS: [WS-2-PARENT, WS-7-A]`; `BLOCKED-BY: [WS-2-B]`

##### WS-2-D — `feat(matric-db): verify drop_archive_schema is source-path-safe for Referenced archives`
- **Type**: impl | **WS**: 2 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Per synthesis §2.3 constraint 6, `drop_archive_schema()` is already safe (never calls `backend.delete()` for orphan blobs when `storage_backend='filesystem'`). Add explicit assertions/tests to ensure Referenced safety is locked in by a test, not by accident.
  - Acceptance criteria:
    - [ ] Unit test using mock backend that panics on `delete` confirms `drop_archive_schema` of a Referenced archive does not invoke `backend.delete()` on source blobs
    - [ ] Companion-location derived blobs ARE deleted (per ADR-FORTEMI-102) — separate test confirms
    - [ ] Source directory listing before/after drop is bit-identical (filesystem inspection test)
  - Dependencies: `Blocked-by: WS-2-B`; `Blocks: WS-2-PARENT, WS-9-D`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#stream-c-constraints` (constraint 6); `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-102-derived-artifacts-companion-location.md`
  - Files: `crates/matric-db/src/archives.rs`, `crates/matric-db/tests/`
- Dep: `BLOCKS: [WS-2-PARENT, WS-9-D]`; `BLOCKED-BY: [WS-2-B]`

##### WS-2-E — `feat(matric-api): extend DefaultArchiveCache and ArchiveContext middleware to carry storage_mode`
- **Type**: impl | **WS**: 2 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-api`, `phase: construction`
- **Body**:
  - Context: API middleware needs `storage_mode` in the request context to apply write-gate (WS-7). Cache must invalidate appropriately if mode changes (theoretical — currently mode is immutable post-create).
  - Acceptance criteria:
    - [ ] `ArchiveContext` struct in `crates/matric-api/src/middleware/archive_routing.rs` has `storage_mode: StorageMode` field
    - [ ] `DefaultArchiveCache` continues to honour TTL with new fields (verified by existing cache test still passing)
    - [ ] New helper `ArchiveContext::is_referenced() -> bool` available to downstream handlers
  - Dependencies: `Blocked-by: WS-2-B`; `Blocks: WS-2-PARENT, WS-7-D`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-100-storage-mode-archive-level.md`
  - Files: `crates/matric-api/src/middleware/archive_routing.rs`
- Dep: `BLOCKS: [WS-2-PARENT, WS-7-D]`; `BLOCKED-BY: [WS-2-B]`

---

#### Workstream 3 — Walker + ignore + secret scan (5 issues)

##### WS-3-A — `chore(Cargo): add ignore = "0.4" workspace dependency`
- **Type**: impl | **WS**: 3 | **Labels**: `priority: P0`, `type: chore`, `scope: deps`, `phase: construction`
- **Body**:
  - Context: BurntSushi `ignore` crate (`https://crates.io/crates/ignore`) — all three research streams independently selected this.
  - Acceptance criteria:
    - [ ] `ignore = "0.4"` added to root `Cargo.toml` workspace dependencies
    - [ ] `crates/matric-jobs/Cargo.toml` adds `ignore = { workspace = true }`
    - [ ] `cargo build` succeeds; lockfile updated and committed
    - [ ] No supply-chain alerts from `cargo audit`
  - Dependencies: `Blocks: WS-3-B, WS-3-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#consensus`
  - Files: `Cargo.toml`, `crates/matric-jobs/Cargo.toml`, `Cargo.lock`
- Dep: `BLOCKS: [WS-3-B, WS-3-PARENT]`; `BLOCKED-BY: []`

##### WS-3-B — `feat(matric-jobs): implement ScanWalker wrapping ignore::WalkBuilder with Fortemi default ignores`
- **Type**: impl | **WS**: 3 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Per synthesis §3 Decision 7 — explicit default ignore list (node_modules, target, .git, .venv, dist/build/out, *.log, files >10MB).
  - Acceptance criteria:
    - [ ] `ScanWalker::new(root: &Path, scan_config: &ScanConfig) -> Self` builder
    - [ ] Honours `.gitignore` for 5 fixture cases (npm/python/rust/mixed/nested) — fixture-based tests
    - [ ] Default-ignore list applied additively to `.gitignore`; user can override via `scan_config.disable_default_ignores: true`
    - [ ] User can extend via `scan_config.additional_ignores: Vec<String>`
    - [ ] Files exceeding `scan_config.max_file_size` (default 10MB) skipped with `SkippedReason::TooLarge`
    - [ ] Symlinks NEVER followed by default; out-of-root symlink targets logged + skipped
  - Dependencies: `Blocked-by: WS-3-A`; `Blocks: WS-3-C, WS-3-D, WS-3-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#decision-7`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-002.md`
  - Files: new `crates/matric-jobs/src/scan_walker.rs`
- Dep: `BLOCKS: [WS-3-C, WS-3-D, WS-3-PARENT]`; `BLOCKED-BY: [WS-3-A]`

##### WS-3-C — `feat(matric-jobs): path-denylist secret detection (.env, *.pem, id_rsa*, .ssh/, etc.)`
- **Type**: impl | **WS**: 3 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Per ADR-FORTEMI-104. Path-based denylist runs first (cheap); see synthesis §3 Decision 7 for full pattern list.
  - Acceptance criteria:
    - [ ] All patterns in synthesis §3 Decision 7 "Secret denylist (path-based)" implemented as `glob::Pattern`
    - [ ] Match produces `QuarantineEvent { path, rule_name: "path_denylist", matched_pattern }`
    - [ ] File content is NEVER read (path-only check; verified by test using mock backend that panics on `read`)
    - [ ] Test coverage on this module is **≥90%** (per `test-strategy.md` security-critical floor)
  - Dependencies: `Blocked-by: WS-3-B`; `Blocks: WS-3-PARENT, WS-9-C`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-104-mandatory-secret-prescan.md`; `@.aiwg/working/issue-planner-storage/synthesis.md#decision-7`
  - Files: `crates/matric-jobs/src/scan_walker.rs`
- Dep: `BLOCKS: [WS-3-PARENT, WS-9-C]`; `BLOCKED-BY: [WS-3-B]`

##### WS-3-D — `feat(matric-jobs): content-regex secret detection (PEM headers, AWS access keys, GitHub PATs, JWTs)`
- **Type**: impl | **WS**: 3 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Content-based secret regex runs against the FIRST 64KB of each file (avoids loading multi-GB files for regex). Patterns per synthesis §3 Decision 7.
  - Acceptance criteria:
    - [ ] Regex set: PEM `-----BEGIN .* PRIVATE KEY-----`, AWS `(AKIA|ASIA)[0-9A-Z]{16}`, GitHub `ghp_[a-zA-Z0-9]{36}` and `github_pat_[a-zA-Z0-9_]{82}`, JWT `eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}`
    - [ ] Match produces `QuarantineEvent { path, rule_name: "content_regex", matched_pattern_id }` — matched **content** is NEVER logged or persisted (per `token-security.md`)
    - [ ] Streaming read: only first 64KB read for the regex pass; files smaller than 64KB read fully
    - [ ] Property test: 100 random fixtures with embedded PEM blocks correctly quarantined
  - Dependencies: `Blocked-by: WS-3-B`; `Blocks: WS-3-PARENT, WS-9-C`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-104-mandatory-secret-prescan.md`; `@.aiwg/working/issue-planner-storage/synthesis.md#decision-7`
  - Files: `crates/matric-jobs/src/scan_walker.rs`
- Dep: `BLOCKS: [WS-3-PARENT, WS-9-C]`; `BLOCKED-BY: [WS-3-B]`

##### WS-3-E — `feat(matric-jobs): threaded parallel walk via WalkBuilder::build_parallel() capped at min(4, num_cpus)`
- **Type**: impl | **WS**: 3 | **Labels**: `priority: P1`, `type: feat`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Per synthesis §4 WS-3, parallel walking with controlled concurrency.
  - Acceptance criteria:
    - [ ] `ScanWalker::walk_parallel(callback)` uses `WalkBuilder::build_parallel()`
    - [ ] Concurrency cap: `min(4, num_cpus::get())` worker threads
    - [ ] Benchmark on a 10k-file fixture shows ≥2x speedup vs sequential (verified by `cargo bench` criterion)
    - [ ] Walk order is non-deterministic but callback is thread-safe (no shared mutable state without `Mutex`)
  - Dependencies: `Blocked-by: WS-3-B`; `Blocks: WS-3-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-005); `@.aiwg/working/issue-planner-storage/synthesis.md#decision-7`
  - Files: `crates/matric-jobs/src/scan_walker.rs`
- Dep: `BLOCKS: [WS-3-PARENT]`; `BLOCKED-BY: [WS-3-B]`

---

#### Workstream 4 — Scan-and-ingest job pipeline (6 issues)

##### WS-4-A — `feat(matric-core): add JobType::DirectoryScan variant`
- **Type**: impl | **WS**: 4 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-core`, `phase: construction`
- **Body**:
  - Context: New job type for the scan-and-ingest pipeline.
  - Acceptance criteria:
    - [ ] `JobType::DirectoryScan { archive_name, full: bool }` variant added
    - [ ] Serialization round-trip test for new variant
    - [ ] All existing `JobType` match arms updated (exhaustive check)
  - Dependencies: `Blocks: WS-4-B, WS-4-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` (component design)
  - Files: `crates/matric-core/src/lib.rs`
- Dep: `BLOCKS: [WS-4-B, WS-4-PARENT]`; `BLOCKED-BY: []`

##### WS-4-B — `feat(matric-jobs): implement DirectoryScanHandler orchestrating walk → hash → dedup → ingest`
- **Type**: impl | **WS**: 4 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Core handler that ties together WS-1 (streaming hash), WS-3 (walker), and existing blob/attachment/note inserts.
  - Acceptance criteria:
    - [ ] `DirectoryScanHandler::handle(job: &Job, pool: &PgPool)` implemented
    - [ ] For each walked file: stream BLAKE3 hash → check `blobs` table for existing content_hash → if exists, link via attachment; if not, INSERT blob + attachment + note row + queue Extraction job
    - [ ] Re-running scan on same content is a no-op (idempotency confirmed by row counts unchanged across two runs on identical fixture)
    - [ ] `archive_registry.scan_status` updates: `idle → scanning` at start, `idle` on success, `error` on failure (with `scan_status_message` populated)
    - [ ] Source blobs get `storage_backend='referenced'`; derived blobs (queued by Extraction handler) get `storage_backend='filesystem'` per WS-6-A
    - [ ] Cancellation via SIGTERM checkpoints progress to `jobs` table (per `disposable-processes.md`); resume picks up from last unhashed file
  - Dependencies: `Blocked-by: WS-1-C, WS-1-D, WS-2-C, WS-3-PARENT, WS-4-A`; `Blocks: WS-4-C, WS-4-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-003.md`; `@.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md` (SIGTERM checkpoint)
  - Files: new `crates/matric-jobs/src/directory_scan_handler.rs`
- Dep: `BLOCKS: [WS-4-C, WS-4-PARENT]`; `BLOCKED-BY: [WS-1-C, WS-1-D, WS-2-C, WS-3-PARENT, WS-4-A]`

##### WS-4-C — `feat(matric-jobs): extend extraction_handler.rs path-access gate to include storage_backend='referenced'`
- **Type**: impl | **WS**: 4 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Per synthesis §2.3 constraint 2, `extraction_handler.rs` line 146 already has path-access mode for video/audio. Extend gate from `strategy_supports_path_access(strategy)` to `strategy_supports_path_access(strategy) || blob.storage_backend == "referenced"`.
  - Acceptance criteria:
    - [ ] Gate updated; existing video/audio extraction unaffected (regression-tested)
    - [ ] Referenced archive text files extracted via path-access mode (no in-memory download)
    - [ ] Unit test confirms gate returns `true` for both pre-existing strategies AND referenced blobs
  - Dependencies: `Blocked-by: WS-1-D, WS-4-B`; `Blocks: WS-4-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#stream-c-constraints` (constraint 2)
  - Files: `crates/matric-jobs/src/extraction_handler.rs`
- Dep: `BLOCKS: [WS-4-PARENT]`; `BLOCKED-BY: [WS-1-D, WS-4-B]`

##### WS-4-D — `feat(matric-db): add archive_quarantine_log per-archive table for QuarantineEvent records`
- **Type**: impl | **WS**: 4 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Per ADR-FORTEMI-104, quarantine records must be queryable via `GET /quarantined-files`. Per-archive table (lives in the archive's PG schema, not `public`).
  - Acceptance criteria:
    - [ ] Migration adds `{archive_schema}.archive_quarantine_log` table: `id BIGSERIAL`, `path TEXT NOT NULL`, `rule_name VARCHAR(50) NOT NULL`, `matched_pattern_id VARCHAR(100)`, `scanned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`
    - [ ] `clone_archive_schema` includes the new table (per existing per-archive pattern)
    - [ ] DirectoryScanHandler INSERTs a row per QuarantineEvent
    - [ ] **File content is NEVER stored** in any column (token-security.md compliance)
  - Dependencies: `Blocked-by: WS-2-A, WS-3-C, WS-3-D, WS-4-B`; `Blocks: WS-4-PARENT, WS-7-F`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-104-mandatory-secret-prescan.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-009.md`
  - Files: `migrations/<timestamp>_quarantine_log.sql`, `crates/matric-jobs/src/directory_scan_handler.rs`
- Dep: `BLOCKS: [WS-4-PARENT, WS-7-F]`; `BLOCKED-BY: [WS-2-A, WS-3-C, WS-3-D, WS-4-B]`

##### WS-4-E — `feat(matric-db): add archive_file_cache per-archive table for path → content_hash dedup`
- **Type**: impl | **WS**: 4 | **Labels**: `priority: P1`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Per SAD (data model changes — 2 new per-archive tables). Speeds up incremental rescans: if path mtime/size unchanged, reuse cached content_hash without re-hashing.
  - Acceptance criteria:
    - [ ] Migration adds `{archive_schema}.archive_file_cache`: `path TEXT PK`, `content_hash BYTEA NOT NULL`, `mtime BIGINT NOT NULL`, `size BIGINT NOT NULL`, `last_seen_at TIMESTAMPTZ NOT NULL`
    - [ ] DirectoryScanHandler consults cache; cache hit (mtime + size match) skips hashing
    - [ ] Cache invalidated when file metadata differs; new hash computed and cached
    - [ ] Cache rows for files no longer present in walk are evicted at end of scan
  - Dependencies: `Blocked-by: WS-2-A, WS-4-B`; `Blocks: WS-4-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` (data model)
  - Files: `migrations/<timestamp>_file_cache.sql`, `crates/matric-jobs/src/directory_scan_handler.rs`
- Dep: `BLOCKS: [WS-4-PARENT]`; `BLOCKED-BY: [WS-2-A, WS-4-B]`

##### WS-4-F — `feat(matric-jobs): structured JSON logging for scan job lifecycle events`
- **Type**: impl | **WS**: 4 | **Labels**: `priority: P1`, `type: feat`, `scope: matric-jobs`, `scope: observability`, `phase: construction`
- **Body**:
  - Context: Per NFR-008 (`logs-as-event-streams.md`), all scan lifecycle events emit structured JSON to stdout. Fields: `timestamp`, `level`, `archive_name`, `job_id`, `event_type` (scan_start/file_processed/file_quarantined/scan_complete/scan_error), `count` (cumulative).
  - Acceptance criteria:
    - [ ] All 5 event types emitted as single-line JSON
    - [ ] Correlation IDs propagated from incoming HTTP request through to scan job logs
    - [ ] **NO file content, NO secret material** in any log field
    - [ ] Log level configurable via existing `LOG_LEVEL` env var
  - Dependencies: `Blocked-by: WS-4-B`; `Blocks: WS-4-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-008); `.claude/rules/logs-as-event-streams.md`
  - Files: `crates/matric-jobs/src/directory_scan_handler.rs`
- Dep: `BLOCKS: [WS-4-PARENT]`; `BLOCKED-BY: [WS-4-B]`

---

#### Workstream 6 — Derived artifact companion location (3 issues)

##### WS-6-A — `feat(matric-core): add FORTEMI_DERIVED_STORAGE_PATH env var with default {FILE_STORAGE_PATH}/derived/`
- **Type**: impl | **WS**: 6 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-core`, `phase: construction`
- **Body**:
  - Context: Per ADR-FORTEMI-102 and `config-in-environment.md`. Add to existing config struct; validate at startup (path must be writable).
  - Acceptance criteria:
    - [ ] `FORTEMI_DERIVED_STORAGE_PATH` parsed at startup; default `{FILE_STORAGE_PATH}/derived/`
    - [ ] Startup fails fast if path is not writable (per `config-in-environment.md`)
    - [ ] `.env.example` updated with documentation
  - Dependencies: `Blocks: WS-6-B, WS-6-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-102-derived-artifacts-companion-location.md`; `@.aiwg/working/issue-planner-storage/deployment/deployment-plan.md`
  - Files: `crates/matric-core/src/config.rs`, `.env.example`
- Dep: `BLOCKS: [WS-6-B, WS-6-PARENT]`; `BLOCKED-BY: []`

##### WS-6-B — `feat(matric-jobs): route derived artifacts of Referenced archives to {derived_root}/{archive_id}/`
- **Type**: impl | **WS**: 6 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Update `store_derived_attachment_tx` to dispatch on `storage_mode` of parent archive.
  - Acceptance criteria:
    - [ ] When parent archive is Referenced, derived artifact path = `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/{blob_id}.bin`; `storage_backend='filesystem'`
    - [ ] When parent archive is Managed, no behaviour change (existing path)
    - [ ] Per-archive subdirectory created on first derived artifact (with `chmod 700`)
    - [ ] Integration test: ingest image into Referenced archive → assert thumbnail at companion path, NOT in source dir
  - Dependencies: `Blocked-by: WS-2-E, WS-6-A`; `Blocks: WS-6-C, WS-6-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-102-derived-artifacts-companion-location.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-006.md`
  - Files: `crates/matric-jobs/src/extraction_handler.rs`
- Dep: `BLOCKS: [WS-6-C, WS-6-PARENT]`; `BLOCKED-BY: [WS-2-E, WS-6-A]`

##### WS-6-C — `feat(matric-db): drop_archive_schema cleans up companion-location subdirectory`
- **Type**: impl | **WS**: 6 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: When a Referenced archive is dropped, the companion `{derived_root}/{archive_id}/` directory must be removed. Source path is NEVER touched.
  - Acceptance criteria:
    - [ ] `drop_archive_schema(archive_id)` calls `std::fs::remove_dir_all` on `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/`
    - [ ] `source_path` directory listing is bit-identical before/after drop (filesystem-inspection test)
    - [ ] Test using mock backend that panics on `delete` confirms no `backend.delete(source_blob)` call ever happens
    - [ ] Drop is idempotent (re-dropping does not error if companion dir already removed)
  - Dependencies: `Blocked-by: WS-2-D, WS-6-B`; `Blocks: WS-6-PARENT, WS-9-D`
  - References: `@.aiwg/working/issue-planner-storage/architecture/adr-FORTEMI-102-derived-artifacts-companion-location.md`
  - Files: `crates/matric-db/src/archives.rs`
- Dep: `BLOCKS: [WS-6-PARENT, WS-9-D]`; `BLOCKED-BY: [WS-2-D, WS-6-B]`

---

#### Workstream 7 — API surface (6 issues)

##### WS-7-A — `feat(matric-api): POST /api/v1/archives/referenced create-with-source-path-validation`
- **Type**: impl | **WS**: 7 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-api`, `phase: construction`
- **Body**:
  - Context: Per UC-EXTSTORAGE-001 (AF-1, EF-1, EF-2). Handler validates: source_path canonicalized, allowlist enforced, path exists, archive name unique.
  - Acceptance criteria:
    - [ ] Returns 201 on success with `{archive_name, source_path, storage_mode: "referenced", scan_status: "idle"}`
    - [ ] Returns 400 `path_outside_allowlist` when source_path canonical form is not under any `FORTEMI_REFERENCED_STORAGE_ROOTS` entry
    - [ ] Returns 400 `source_path_not_found` when canonicalized path does not exist
    - [ ] Returns 409 `archive_exists` when archive_name already in use
    - [ ] Path traversal (`../../../etc/passwd`) defeated by canonicalization (covered by WS-9-A test)
    - [ ] Inherits existing fail-closed auth (ADR-094 unchanged)
  - Dependencies: `Blocked-by: WS-2-C`; `Blocks: WS-7-D, WS-7-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-001.md`; `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-003, NFR-004)
  - Files: `crates/matric-api/src/main.rs`
- Dep: `BLOCKS: [WS-7-D, WS-7-PARENT]`; `BLOCKED-BY: [WS-2-C]`

##### WS-7-B — `feat(matric-api): POST /api/v1/archives/{name}/rescan queues DirectoryScan job and returns 202+job_id`
- **Type**: impl | **WS**: 7 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-api`, `phase: construction`
- **Body**:
  - Context: Per UC-EXTSTORAGE-004. Allow-listed mutation route on Referenced archives.
  - Acceptance criteria:
    - [ ] Returns 202 with `{job_id: uuid}` on success
    - [ ] Returns 404 `archive_not_found` for unknown archive
    - [ ] Returns 409 `scan_in_progress` when `scan_status='scanning'`
    - [ ] Returns 503 `source_path_unreachable` when source path is not currently mountable (per NFR-012)
    - [ ] Optional query param `?full=true` forces full rescan (cache invalidated); default is incremental
  - Dependencies: `Blocked-by: WS-4-PARENT`; `Blocks: WS-7-D, WS-7-PARENT, WS-8-B`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-004.md`
  - Files: `crates/matric-api/src/main.rs`
- Dep: `BLOCKS: [WS-7-D, WS-7-PARENT, WS-8-B]`; `BLOCKED-BY: [WS-4-PARENT]`

##### WS-7-C — `feat(matric-api): GET /api/v1/archives/{name}/scan-status returns scan_status + last_scan_at + counters`
- **Type**: impl | **WS**: 7 | **Labels**: `priority: P1`, `type: feat`, `scope: matric-api`, `phase: construction`
- **Body**:
  - Context: Per UC-EXTSTORAGE-005. Polling endpoint for in-progress scans.
  - Acceptance criteria:
    - [ ] Returns 200 with `{scan_status, last_scan_at, files_scanned, files_quarantined, errors}` for Referenced archives
    - [ ] Returns 200 with `{scan_status: "n/a"}` for Managed archives (avoids 404 noise)
    - [ ] Counters drawn from `jobs` table progress field + `archive_quarantine_log` row count
  - Dependencies: `Blocked-by: WS-2-B, WS-4-D`; `Blocks: WS-7-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-005.md`
  - Files: `crates/matric-api/src/main.rs`
- Dep: `BLOCKS: [WS-7-PARENT]`; `BLOCKED-BY: [WS-2-B, WS-4-D]`

##### WS-7-D — `feat(matric-api): write-gate in archive_routing_middleware rejects mutating routes on Referenced archives with 403`
- **Type**: impl | **WS**: 7 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-api`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Defense-in-depth layer 2 of three (per architecture TL;DR bullet 2). All mutating routes (POST/PUT/DELETE on notes, attachments, etc.) MUST return 403 for Referenced archives — except the rescan endpoint (allow-listed).
  - Acceptance criteria:
    - [ ] Middleware checks `archive_context.storage_mode == Referenced` AND request method is mutating AND route is not in allow-list
    - [ ] Returns 403 with body `{error: "read_only_archive", message: "Mutating route X is not permitted on Referenced archive"}`
    - [ ] Response includes `X-Storage-Mode: referenced` header
    - [ ] Allow-list: `POST /api/v1/archives/{name}/rescan` (and any future quarantine-management endpoints from WS-7-F if added)
    - [ ] Existing Managed archives unaffected (regression test asserts all existing routes work unchanged)
  - Dependencies: `Blocked-by: WS-2-E, WS-7-A, WS-7-B`; `Blocks: WS-7-PARENT, WS-9-A`
  - References: `@.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` (three-layer read-only defense); `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-002)
  - Files: `crates/matric-api/src/middleware/archive_routing.rs`
- Dep: `BLOCKS: [WS-7-PARENT, WS-9-A]`; `BLOCKED-BY: [WS-2-E, WS-7-A, WS-7-B]`

##### WS-7-E — `feat(matric-api): FORTEMI_REFERENCED_STORAGE_ROOTS env var + startup validation`
- **Type**: impl | **WS**: 7 | **Labels**: `priority: P0`, `type: feat`, `scope: matric-api`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Per synthesis §6 Q-5 (recommended C: mandatory in multi-tenant, optional otherwise). Colon-separated absolute paths. If `FORTEMI_MULTI_TENANT=true` AND `FORTEMI_REFERENCED_STORAGE_ROOTS` empty → startup ERROR (fail-closed).
  - Acceptance criteria:
    - [ ] Parse env var as colon-separated absolute paths; reject relative paths at startup
    - [ ] Startup error message clearly explains multi-tenant requirement
    - [ ] Empty/unset is allowed when `FORTEMI_MULTI_TENANT=false` (single-user default)
    - [ ] `is_path_allowed(canonical_path) -> bool` helper for WS-7-A
    - [ ] `.env.example` documents both `FORTEMI_REFERENCED_STORAGE_ROOTS` and the multi-tenant interaction
  - Dependencies: `Blocked-by: (none)`; `Blocks: WS-7-A (logically), WS-7-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#open-questions` (Q-5); `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-004)
  - Files: `crates/matric-api/src/config.rs`, `.env.example`
- Dep: `BLOCKS: [WS-7-A, WS-7-PARENT]`; `BLOCKED-BY: []`

##### WS-7-F — `feat(matric-api): GET /api/v1/archives/{name}/quarantined-files audit endpoint`
- **Type**: impl | **WS**: 7 | **Labels**: `priority: P1`, `type: feat`, `scope: matric-api`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Per UC-EXTSTORAGE-009. Operators audit what was skipped by secret-scan.
  - Acceptance criteria:
    - [ ] Returns 200 with paginated `{items: [{path, rule_name, matched_pattern_id, scanned_at}], next_cursor}`
    - [ ] **Response NEVER contains file contents or matched secret values** — only path + rule metadata
    - [ ] Pagination via opaque cursor (existing pattern in API)
    - [ ] Returns 404 for Managed archives (no quarantine log exists)
  - Dependencies: `Blocked-by: WS-4-D`; `Blocks: WS-7-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-009.md`
  - Files: `crates/matric-api/src/main.rs`
- Dep: `BLOCKS: [WS-7-PARENT]`; `BLOCKED-BY: [WS-4-D]`

---

#### Workstream 8 — MCP tool surface (3 issues)

##### WS-8-A — `feat(mcp-server): extend manage_archives tool with storage_mode and source_path params on create operation`
- **Type**: impl | **WS**: 8 | **Labels**: `priority: P0`, `type: feat`, `scope: mcp-server`, `phase: construction`
- **Body**:
  - Context: Per synthesis §3 Decision 6 (extend existing tool, no new family). Backward compat: omitting new params defaults to Managed.
  - Acceptance criteria:
    - [ ] JSON schema for `manage_archives` updated: new optional params `storage_mode` (`"managed" | "referenced"`) and `source_path` (string)
    - [ ] Validation: if `storage_mode === "referenced"` then `source_path` is required
    - [ ] Tool wraps existing `POST /api/v1/archives/referenced` (delegates validation to backend)
    - [ ] Backward-compat tests: existing `manage_archives` invocations without new params continue to pass
    - [ ] Tool description updated to mention Referenced mode
  - Dependencies: `Blocked-by: WS-7-A`; `Blocks: WS-8-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#decision-6`
  - Files: `mcp-server/`
- Dep: `BLOCKS: [WS-8-PARENT]`; `BLOCKED-BY: [WS-7-A]`

##### WS-8-B — `feat(mcp-server): new rescan_archive tool with async-job-return semantics`
- **Type**: impl | **WS**: 8 | **Labels**: `priority: P0`, `type: feat`, `scope: mcp-server`, `phase: construction`
- **Body**:
  - Context: Per UC-EXTSTORAGE-007. New tool because async-job-return semantics differ from CRUD operations.
  - Acceptance criteria:
    - [ ] JSON schema: `rescan_archive({archive_name: string, full?: boolean})`
    - [ ] Returns `{job_id, scan_status}` immediately (async)
    - [ ] Wraps `POST /api/v1/archives/{name}/rescan` from WS-7-B
    - [ ] Tool description includes guidance: "Use manage_jobs or get_scan_status to poll progress"
  - Dependencies: `Blocked-by: WS-7-B`; `Blocks: WS-8-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-007.md`
  - Files: `mcp-server/`
- Dep: `BLOCKS: [WS-8-PARENT]`; `BLOCKED-BY: [WS-7-B]`

##### WS-8-C — `feat(mcp-server): update get_documentation tool output to surface Referenced-mode capabilities`
- **Type**: impl | **WS**: 8 | **Labels**: `priority: P2`, `type: feat`, `scope: mcp-server`, `phase: construction`
- **Body**:
  - Context: `get_documentation` is one of the 43 core tools and is the first stop for agents discovering capabilities. Add a Referenced-mode section.
  - Acceptance criteria:
    - [ ] Output includes section "Referenced Storage Mode" with: when to use, source_path requirements, secret-scan behaviour, rescan workflow
    - [ ] Cross-references `rescan_archive` tool
    - [ ] No regressions in existing documentation output
  - Dependencies: `Blocked-by: WS-8-A, WS-8-B`; `Blocks: WS-8-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md#decision-6`
  - Files: `mcp-server/`
- Dep: `BLOCKS: [WS-8-PARENT]`; `BLOCKED-BY: [WS-8-A, WS-8-B]`

---

#### Workstream 9 — Multi-tenant security tests (4 issues)

##### WS-9-A — `test(security): TI-EXTSTORAGE-1/2/3/4 — path traversal and multi-tenant boundary tests`
- **Type**: test | **WS**: 9 | **Labels**: `priority: P0`, `type: test`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Per `tenant-isolation-regression-suite.md` TI-EXTSTORAGE-1 through TI-EXTSTORAGE-4.
  - Acceptance criteria:
    - [ ] TI-EXTSTORAGE-1: `../../../etc/passwd` via `GET /api/v1/archives/{name}/files/{path}` returns 404 (not 200, never reads `/etc/passwd`)
    - [ ] TI-EXTSTORAGE-2: Symlink in source pointing to `/etc` is not followed; logged warning
    - [ ] TI-EXTSTORAGE-3: Tenant A's archive at `/srv/fortemi/A/code` returns 404 for Tenant B even when B knows archive name
    - [ ] TI-EXTSTORAGE-4: Path canonicalization defeats encoded traversal (`%2e%2e%2f`, `..%2F`, mixed encodings)
    - [ ] All 4 tests run on CI and **MUST** pass before #736 can close
  - Dependencies: `Blocked-by: WS-7-A, WS-7-D`; `Blocks: WS-9-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`; `@.aiwg/working/issue-planner-storage/synthesis.md#risks` (R-2, R-7)
  - Files: new `crates/matric-api/tests/security_referenced.rs`
- Dep: `BLOCKS: [WS-9-PARENT]`; `BLOCKED-BY: [WS-7-A, WS-7-D]`

##### WS-9-B — `test(security): TI-EXTSTORAGE-5/6 — secret-scan red-team and quarantine verification`
- **Type**: test | **WS**: 9 | **Labels**: `priority: P0`, `type: test`, `scope: security`, `phase: construction`
- **Body**:
  - Context: TI-EXTSTORAGE-5 (drop PEM private key in source, verify quarantine), TI-EXTSTORAGE-6 (drop AWS access key, verify quarantine).
  - Acceptance criteria:
    - [ ] TI-EXTSTORAGE-5: Fixture file with `-----BEGIN RSA PRIVATE KEY-----` is quarantined; no chunks/embeddings in pgvector; row in `archive_quarantine_log` with `rule_name='content_regex'`
    - [ ] TI-EXTSTORAGE-6: Fixture file with `AKIAIOSFODNN7EXAMPLE` is quarantined; same assertions
    - [ ] Test verifies quarantine log row **does not contain the secret content** (only path + rule name)
  - Dependencies: `Blocked-by: WS-3-C, WS-3-D, WS-4-D`; `Blocks: WS-9-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`; `@.aiwg/working/issue-planner-storage/synthesis.md#risks` (R-1)
  - Files: `crates/matric-api/tests/security_referenced.rs`
- Dep: `BLOCKS: [WS-9-PARENT]`; `BLOCKED-BY: [WS-3-C, WS-3-D, WS-4-D]`

##### WS-9-C — `test(security): TI-EXTSTORAGE-7 — write-gate enforces 403 for all mutating routes on Referenced archives`
- **Type**: test | **WS**: 9 | **Labels**: `priority: P0`, `type: test`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Per UC-EXTSTORAGE-001 EF-2 and TI-EXTSTORAGE-7. Enumerate every mutating route in the API and verify each returns 403 on a Referenced archive.
  - Acceptance criteria:
    - [ ] Test enumerates routes from OpenAPI spec; filters mutating methods (POST/PUT/PATCH/DELETE)
    - [ ] For each non-allow-listed mutating route, asserts 403 with `error: "read_only_archive"` on a Referenced archive
    - [ ] Allow-list verified: `POST /api/v1/archives/{name}/rescan` returns 202 (not 403)
    - [ ] Test fails CI if a new mutating route is added without explicit allow/deny decision
  - Dependencies: `Blocked-by: WS-7-D`; `Blocks: WS-9-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`; `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-002)
  - Files: `crates/matric-api/tests/security_referenced.rs`
- Dep: `BLOCKS: [WS-9-PARENT]`; `BLOCKED-BY: [WS-7-D]`

##### WS-9-D — `test(security): TI-EXTSTORAGE-8/9/10 — mount disappearance, drop-archive safety, derived companion cleanup`
- **Type**: test | **WS**: 9 | **Labels**: `priority: P0`, `type: test`, `scope: security`, `phase: construction`
- **Body**:
  - Context: TI-EXTSTORAGE-8 (mount disappears mid-query), TI-EXTSTORAGE-9 (drop-archive safety — never touches source), TI-EXTSTORAGE-10 (derived companion is cleaned up).
  - Acceptance criteria:
    - [ ] TI-EXTSTORAGE-8: Unmount source mid-search → reads return cached results with `X-Source-Stale: true` header; rescan returns 503
    - [ ] TI-EXTSTORAGE-9: Drop a Referenced archive whose source contains 1000 fixture files → all 1000 files still present after drop (filesystem inspection)
    - [ ] TI-EXTSTORAGE-9b: Mock backend that panics on `delete` confirms `drop_archive_schema` never calls `backend.delete()` for `storage_backend='referenced'` blobs
    - [ ] TI-EXTSTORAGE-10: Drop a Referenced archive with derived artifacts → companion `{derived_root}/{archive_id}/` is removed; source dir untouched
  - Dependencies: `Blocked-by: WS-2-D, WS-6-C, WS-7-PARENT`; `Blocks: WS-9-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`; `@.aiwg/working/issue-planner-storage/requirements/use-cases/UC-EXTSTORAGE-008.md`
  - Files: `crates/matric-api/tests/security_referenced.rs`, `crates/matric-db/tests/referenced_security.rs`
- Dep: `BLOCKS: [WS-9-PARENT]`; `BLOCKED-BY: [WS-2-D, WS-6-C, WS-7-PARENT]`

---

#### Workstream 10 — Documentation and deployment plan (6 issues, see also test/docs/cross-cutting below)

##### WS-10-A — `docs(CLAUDE): add Referenced storage mode section to CLAUDE.md`
- **Type**: docs | **WS**: 10 | **Labels**: `priority: P1`, `type: docs`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: Operator-facing reference doc.
  - Acceptance criteria:
    - [ ] New `## Referenced Storage Mode` section in `CLAUDE.md` covers: when to use, env vars, 4 new API endpoints, MCP tool changes, secret-scan behaviour
    - [ ] Cross-references the operator guide (WS-10-D)
    - [ ] Doc-sync skill passes (no broken @-mentions)
  - Dependencies: `Blocked-by: WS-7-PARENT, WS-8-PARENT`; `Blocks: WS-10-PARENT`
  - References: `@CLAUDE.md`
  - Files: `CLAUDE.md`
- Dep: `BLOCKS: [WS-10-PARENT]`; `BLOCKED-BY: [WS-7-PARENT, WS-8-PARENT]`

##### WS-10-B — `docs(deployment): update docker-compose.bundle.yml docs for bind-mount and env var setup`
- **Type**: docs | **WS**: 10 | **Labels**: `priority: P0`, `type: docs`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: Operators need to know how to bind-mount a source directory `:ro` and set `FORTEMI_REFERENCED_STORAGE_ROOTS` correctly.
  - Acceptance criteria:
    - [ ] Section added covering bind-mount with `:ro` flag enforcement
    - [ ] UID/GID mapping guidance for read access
    - [ ] FS-event propagation caveats per platform (per ADR-FORTEMI-103 — emphasize this is moot in v1 since no live watching)
    - [ ] `installer/scripts/configure.sh` doc updated for new env vars
  - Dependencies: `Blocked-by: WS-6-A, WS-7-E`; `Blocks: WS-10-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/deployment/deployment-plan.md`
  - Files: `docs/`, `installer/`
- Dep: `BLOCKS: [WS-10-PARENT]`; `BLOCKED-BY: [WS-6-A, WS-7-E]`

##### WS-10-C — `docs(content): update multi-memory-agent-guide.md to cover Referenced archives`
- **Type**: docs | **WS**: 10 | **Labels**: `priority: P1`, `type: docs`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: Existing agent-facing guide at `docs/content/multi-memory-agent-guide.md` must explain when to create a Referenced vs Managed archive.
  - Acceptance criteria:
    - [ ] Decision-framework table: Referenced (you have code on disk, never want it copied) vs Managed (uploaded notes, API-ingested content)
    - [ ] Example MCP tool invocations for both modes
    - [ ] Mentions secret-scan automatic protection
  - Dependencies: `Blocked-by: WS-8-PARENT`; `Blocks: WS-10-PARENT`
  - References: existing `docs/content/multi-memory-agent-guide.md`
  - Files: `docs/content/multi-memory-agent-guide.md`
- Dep: `BLOCKS: [WS-10-PARENT]`; `BLOCKED-BY: [WS-8-PARENT]`

##### WS-10-D — `docs(content): new operator guide docs/content/referenced-storage.md`
- **Type**: docs | **WS**: 10 | **Labels**: `priority: P0`, `type: docs`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: The canonical operator reference for the feature. Covers Q-1 through Q-8 decisions and their implications.
  - Acceptance criteria:
    - [ ] Sections: When to use Referenced vs Managed; Setup walkthrough; Secret-scan behaviour and quarantine audit; Performance expectations (initial ingest time per NFR-007); Failure modes (Decision 8 — lenient default); Limitations (no live watching, no rename detection, no source migration)
    - [ ] Citations to relevant ADRs (FORTEMI-100/101/102/103/104)
    - [ ] Examples (curl + MCP) for each major workflow
  - Dependencies: `Blocked-by: WS-7-PARENT, WS-8-PARENT, WS-9-PARENT`; `Blocks: WS-10-PARENT`
  - References: `@.aiwg/working/issue-planner-storage/synthesis.md`
  - Files: new `docs/content/referenced-storage.md`
- Dep: `BLOCKS: [WS-10-PARENT]`; `BLOCKED-BY: [WS-7-PARENT, WS-8-PARENT, WS-9-PARENT]`

##### WS-10-E — `docs(api): update OpenAPI / Swagger for 4 new endpoints`
- **Type**: docs | **WS**: 10 | **Labels**: `priority: P0`, `type: docs`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: `openapi.yaml` and the Swagger UI at `/docs` must surface the new routes for client SDK generation.
  - Acceptance criteria:
    - [ ] OpenAPI spec includes: `POST /api/v1/archives/referenced`, `POST /api/v1/archives/{name}/rescan`, `GET /api/v1/archives/{name}/scan-status`, `GET /api/v1/archives/{name}/quarantined-files`
    - [ ] Request/response schemas defined for each
    - [ ] Error codes documented (`path_outside_allowlist`, `source_path_not_found`, `read_only_archive`, `source_path_unreachable`, `scan_in_progress`)
    - [ ] Spec validates against OpenAPI 3.0 schema (CI check)
  - Dependencies: `Blocked-by: WS-7-PARENT`; `Blocks: WS-10-PARENT`
  - References: existing `openapi.yaml`
  - Files: `openapi.yaml`
- Dep: `BLOCKS: [WS-10-PARENT]`; `BLOCKED-BY: [WS-7-PARENT]`

##### WS-10-F — `docs(ops): add operational runbook stubs for stuck-scan, mount-unreachable, quarantine-investigation`
- **Type**: docs | **WS**: 10 | **Labels**: `priority: P1`, `type: docs`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: Per `deployment/operational-readiness-checklist.md`. Three runbook scenarios specific to Referenced mode.
  - Acceptance criteria:
    - [ ] Runbook 1: "Scan stuck in `scanning` for >2x expected duration" — diagnosis, escalation, recovery
    - [ ] Runbook 2: "SourcePathUnreachable alert" — diagnosis (mount lost, NFS timeout, permission revoked), recovery
    - [ ] Runbook 3: "Operator wants to investigate why a file was quarantined" — using `GET /quarantined-files`, then manual review (Fortemi never reveals the secret value itself)
    - [ ] Each runbook follows `ops-documentation.md` structure (Purpose, System Topology, Procedure, Verification, Troubleshooting, House Rules, What NOT to Fix, Audit Trail)
  - Dependencies: `Blocked-by: WS-7-PARENT, WS-9-PARENT`; `Blocks: WS-10-PARENT, CROSS-2`
  - References: `@.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md`
  - Files: `docs/ops/`
- Dep: `BLOCKS: [WS-10-PARENT, CROSS-2]`; `BLOCKED-BY: [WS-7-PARENT, WS-9-PARENT]`

---

### 2.4 Test tracker issues (9)

One per active workstream. Each tracker rolls up the test cases for that workstream from `testing/test-plan-construction.md` (104 total: 39 unit / 47 integration / 3 e2e / 10 security regression / 2 perf smoke). WS-5 omitted (deferred).

##### TEST-WS-1 — `test(matric-db): unit and integration tests for ReferencedBackend (storage backend abstraction)`
- **Type**: test | **WS**: 1 | **Labels**: `priority: P0`, `type: test`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: Roll-up of WS-1 test cases from `test-plan-construction.md`. Covers ReferencedBackend trait conformance, streaming hash equivalence, dispatch logic, mode dispatch in `PgFileStorageRepository`.
  - Acceptance criteria:
    - [ ] All WS-1 test cases in `test-plan-construction.md` implemented
    - [ ] Coverage on new code in `crates/matric-db/src/file_storage.rs` ≥85%
    - [ ] Property tests for hash-equivalence between streaming and in-memory variants
  - Dependencies: `Blocked-by: WS-1-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
  - Files: `crates/matric-db/tests/`, `crates/matric-db/src/file_storage.rs` (`#[cfg(test)]` modules)
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-1-PARENT]`

##### TEST-WS-2 — `test(matric-db): migration + schema + repository tests for Referenced archive registry`
- **Type**: test | **WS**: 2 | **Labels**: `priority: P0`, `type: test`, `scope: matric-db`, `phase: construction`
- **Body**:
  - Context: WS-2 test bundle. Covers migration safety (backward compat), `ArchiveInfo` round-trip, `create_referenced_archive`, drop-safety, cache TTL.
  - Acceptance criteria:
    - [ ] All WS-2 test cases in `test-plan-construction.md` implemented
    - [ ] Migration tested against fixture DB with 10 pre-existing Managed archives (all preserved post-migration)
    - [ ] Coverage on `crates/matric-db/src/archives.rs` ≥85%
  - Dependencies: `Blocked-by: WS-2-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
  - Files: `crates/matric-db/tests/`, migrations test harness
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-2-PARENT]`

##### TEST-WS-3 — `test(matric-jobs): ScanWalker + secret-scan unit and integration tests (coverage gate 90%)`
- **Type**: test | **WS**: 3 | **Labels**: `priority: P0`, `type: test`, `scope: matric-jobs`, `scope: security`, `phase: construction`
- **Body**:
  - Context: WS-3 test bundle. Per `test-strategy.md`, secret-detection module has elevated **90% coverage gate**.
  - Acceptance criteria:
    - [ ] All WS-3 test cases in `test-plan-construction.md` implemented
    - [ ] Coverage on `crates/matric-jobs/src/scan_walker.rs` ≥**90%** (gate, not 85%)
    - [ ] Property tests: 100 random fixtures with embedded secrets correctly quarantined; 100 random fixtures with no secrets correctly NOT quarantined
    - [ ] Fixture-based tests for 5 .gitignore scenarios (npm/python/rust/mixed/nested)
  - Dependencies: `Blocked-by: WS-3-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`; `@.aiwg/working/issue-planner-storage/testing/test-strategy.md`
  - Files: `crates/matric-jobs/tests/`, `crates/matric-jobs/src/scan_walker.rs` (`#[cfg(test)]`)
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-3-PARENT]`

##### TEST-WS-4 — `test(matric-jobs): DirectoryScanHandler integration tests including idempotency and re-ingest`
- **Type**: test | **WS**: 4 | **Labels**: `priority: P0`, `type: test`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: WS-4 test bundle. Includes the perf smoke benchmark (1k-file fixture <10min on default config per NFR-007 + Q-7 default).
  - Acceptance criteria:
    - [ ] All WS-4 test cases in `test-plan-construction.md` implemented
    - [ ] Idempotency test: two consecutive scans on identical fixture produce identical row counts
    - [ ] Re-ingest test: modify a file (changes content_hash) → new chunks replace old (no orphan chunks)
    - [ ] Perf smoke: 1k-file fixture scans in <10min on CI default hardware (NFR-007 / Q-7)
    - [ ] Coverage on `crates/matric-jobs/src/directory_scan_handler.rs` ≥85%
  - Dependencies: `Blocked-by: WS-4-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`; `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-005, NFR-006, NFR-007)
  - Files: `crates/matric-jobs/tests/`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-4-PARENT]`

##### TEST-WS-6 — `test(matric-jobs): derived artifact companion-location dispatch and cleanup tests`
- **Type**: test | **WS**: 6 | **Labels**: `priority: P0`, `type: test`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: WS-6 test bundle.
  - Acceptance criteria:
    - [ ] All WS-6 test cases in `test-plan-construction.md` implemented
    - [ ] Integration test: ingest image into Referenced archive → assert thumbnail at companion path, NOT source dir
    - [ ] Drop test: companion dir removed; source dir untouched (TI-EXTSTORAGE-10 redundancy)
    - [ ] Managed archive behaviour unchanged (regression test)
  - Dependencies: `Blocked-by: WS-6-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
  - Files: `crates/matric-jobs/tests/`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-6-PARENT]`

##### TEST-WS-7 — `test(matric-api): integration tests for 4 new endpoints + write-gate enforcement`
- **Type**: test | **WS**: 7 | **Labels**: `priority: P0`, `type: test`, `scope: matric-api`, `phase: construction`
- **Body**:
  - Context: WS-7 test bundle. Most of the API regression surface for #736 lives here.
  - Acceptance criteria:
    - [ ] All WS-7 test cases in `test-plan-construction.md` implemented (covers UCs 001, 004, 005, 008, 009)
    - [ ] End-to-end test: create Referenced archive → trigger rescan → poll scan-status to completion → search returns ingested content
    - [ ] Write-gate test enumerates all mutating routes (overlap with TI-EXTSTORAGE-7 / WS-9-C — acceptable redundancy)
    - [ ] Coverage on `crates/matric-api/src/middleware/archive_routing.rs` ≥85%
  - Dependencies: `Blocked-by: WS-7-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
  - Files: `crates/matric-api/tests/`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-7-PARENT]`

##### TEST-WS-8 — `test(mcp-server): MCP tool surface tests including backward compat`
- **Type**: test | **WS**: 8 | **Labels**: `priority: P1`, `type: test`, `scope: mcp-server`, `phase: construction`
- **Body**:
  - Context: WS-8 test bundle.
  - Acceptance criteria:
    - [ ] All WS-8 test cases in `test-plan-construction.md` implemented (covers UC-EXTSTORAGE-007)
    - [ ] Backward-compat: pre-existing `manage_archives` invocations without `storage_mode` continue to work (default Managed)
    - [ ] MCP Inspector schema validation passes for both updated and new tools
  - Dependencies: `Blocked-by: WS-8-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
  - Files: `mcp-server/test/`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-8-PARENT]`

##### TEST-WS-9 — `test(security): TI-EXTSTORAGE-* regression suite gate (100% pass requirement)`
- **Type**: test | **WS**: 9 | **Labels**: `priority: P0`, `type: test`, `scope: security`, `phase: construction`
- **Body**:
  - Context: Per `test-strategy.md`, WS-9 has **100% pass gate** on all TI-EXTSTORAGE-* cases — any single failure blocks #736 closure.
  - Acceptance criteria:
    - [ ] All 10 TI-EXTSTORAGE-* tests (implemented across WS-9-A/B/C/D) run on CI for every PR
    - [ ] CI workflow fails the build on any single TI-EXTSTORAGE failure (no `continue-on-error`, no skip — per `dev-pipeline-safety.md`)
    - [ ] Suite tagged `security-critical` so it's never accidentally excluded by future test-runner filters
  - Dependencies: `Blocked-by: WS-9-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`; `@.aiwg/working/issue-planner-storage/testing/test-strategy.md`
  - Files: `.gitea/workflows/`, `crates/matric-api/tests/security_referenced.rs`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-9-PARENT]`

##### TEST-WS-10 — `test(docs): doc-sync validation pass for all updated documentation`
- **Type**: test | **WS**: 10 | **Labels**: `priority: P1`, `type: test`, `scope: docs`, `phase: construction`
- **Body**:
  - Context: WS-10 test bundle (small — docs validation).
  - Acceptance criteria:
    - [ ] `doc-sync` skill passes on `CLAUDE.md`, deployment docs, multi-memory guide, operator guide, OpenAPI spec
    - [ ] Manual walkthrough: external operator follows `docs/content/referenced-storage.md` setup section on a clean Docker bundle and successfully creates a Referenced archive
  - Dependencies: `Blocked-by: WS-10-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
  - Files: validation only (no code changes)
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-10-PARENT]`

---

### 2.5 Cross-cutting issues (4)

##### CROSS-1 — `chore(deploy): feature-flag wiring for FORTEMI_EXTERNAL_STORAGE_ENABLED (default OFF)`
- **Type**: chore | **WS**: cross-cutting | **Labels**: `priority: P0`, `type: chore`, `scope: matric-api`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Per `deployment/deployment-plan.md`, the entire feature ships behind `FORTEMI_EXTERNAL_STORAGE_ENABLED` (default OFF). Soft rollback = flip flag back to OFF without data loss (Managed archives unaffected).
  - Acceptance criteria:
    - [ ] Flag respected in API: when OFF, all 4 new endpoints return 404 `feature_disabled`
    - [ ] Flag respected in MCP: when OFF, `rescan_archive` not registered; `manage_archives` rejects `storage_mode='referenced'`
    - [ ] Flag respected in worker: `DirectoryScanHandler` does not process queued `DirectoryScan` jobs (they remain in queue for next enable)
    - [ ] `.env.example` documents the flag and its rollback semantics
    - [ ] Integration test: flip flag OFF → existing Managed archives unaffected; flip flag back ON → queued jobs resume cleanly
  - Dependencies: `Blocked-by: WS-7-PARENT, WS-8-PARENT, WS-4-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/deployment/deployment-plan.md`
  - Files: `crates/matric-api/src/config.rs`, `crates/matric-jobs/src/`, `mcp-server/`, `.env.example`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-7-PARENT, WS-8-PARENT, WS-4-PARENT]`

##### CROSS-2 — `chore(observability): wire 8 new Prometheus metrics + 5 new alerts (including SourcePathUnreachable critical)`
- **Type**: chore | **WS**: cross-cutting | **Labels**: `priority: P0`, `type: chore`, `scope: observability`, `phase: construction`
- **Body**:
  - Context: Per `deployment/operational-readiness-checklist.md` and NFR-010, 8 new metrics and 5 alerts. Critical alert `SourcePathUnreachable` ties into runbook WS-10-F.
  - Acceptance criteria:
    - [ ] Metrics implemented: `fortemi_scan_jobs_total{archive,status}`, `fortemi_scan_files_processed_total{archive}`, `fortemi_scan_files_quarantined_total{archive,rule_name}`, `fortemi_scan_duration_seconds{archive,quantile}`, `fortemi_referenced_archive_count`, `fortemi_referenced_blob_count{archive}`, `fortemi_derived_storage_bytes{archive}`, `fortemi_source_path_reachable{archive}` (0/1 gauge)
    - [ ] Alerts implemented: `SourcePathUnreachable` (critical, fires when gauge = 0 for >5min), `ScanStuck` (warning, >2x typical duration), `HighQuarantineRate` (warning, >10 quarantines per scan), `DerivedStorageGrowth` (info, >100MB/day), `ScanErrorRate` (warning, >5% scans failing)
    - [ ] **Metric labels contain NO file paths and NO secret content** (`token-security.md` compliance)
    - [ ] Existing metrics endpoint (`/metrics`) surfaces new metrics
  - Dependencies: `Blocked-by: WS-4-PARENT, WS-7-PARENT`; `Blocks: #736, WS-10-F`
  - References: `@.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md`; `@.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md` (NFR-008, NFR-010)
  - Files: `crates/matric-api/src/metrics.rs` (or equivalent), monitoring config
- Dep: `BLOCKS: [#736, WS-10-F]`; `BLOCKED-BY: [WS-4-PARENT, WS-7-PARENT]`

##### CROSS-3 — `chore(ops): scan-worker SIGTERM graceful shutdown and BLAKE3-dedup resume verification`
- **Type**: chore | **WS**: cross-cutting | **Labels**: `priority: P0`, `type: chore`, `scope: matric-jobs`, `phase: construction`
- **Body**:
  - Context: Per `operational-readiness-checklist.md` and `disposable-processes.md`. The scan worker must be disposable: SIGTERM mid-scan checkpoints to `jobs` table; restart resumes from the last unhashed file via content-hash dedup.
  - Acceptance criteria:
    - [ ] SIGTERM handler registered in scan worker entrypoint
    - [ ] On SIGTERM: stop accepting new files, finish in-flight file hashing, write progress to `jobs.progress` JSONB, exit cleanly within 25s (under typical orchestrator 30s SIGKILL window)
    - [ ] Resume test: kill -TERM mid-scan, restart, verify scan completes and final row counts identical to uninterrupted run
    - [ ] No data loss: any file whose hash was computed but blob not inserted is re-hashed on resume (hash dedup ensures no duplicate blobs)
  - Dependencies: `Blocked-by: WS-4-PARENT`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md`; `.claude/rules/disposable-processes.md`
  - Files: `crates/matric-jobs/src/directory_scan_handler.rs`, scan worker bin entrypoint
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-4-PARENT]`

##### CROSS-4 — `chore(deploy): production rollout runbook + soft-rollback procedure`
- **Type**: chore | **WS**: cross-cutting | **Labels**: `priority: P0`, `type: chore`, `scope: docs`, `phase: transition`
- **Body**:
  - Context: Per `deployment-plan.md`. Single rollout playbook covering: migration apply → feature-flag enable on staging → smoke tests → production migration → production enable → 24h monitoring → soft-rollback procedure if needed.
  - Acceptance criteria:
    - [ ] Runbook covers each step with copy-pasteable commands (per `ops-documentation.md`)
    - [ ] Soft-rollback documented: `FORTEMI_EXTERNAL_STORAGE_ENABLED=false` + restart; Referenced archives become inaccessible but data preserved; Managed archives unaffected
    - [ ] Hard-rollback (migration revert) documented as last-resort with explicit Phase 5 operator authorization required
    - [ ] Includes verification commands (curl /health, scan-status check, metrics sanity)
  - Dependencies: `Blocked-by: WS-10-PARENT, CROSS-1, CROSS-2`; `Blocks: #736`
  - References: `@.aiwg/working/issue-planner-storage/deployment/deployment-plan.md`; `@.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md`
  - Files: `docs/ops/referenced-storage-rollout.md`
- Dep: `BLOCKS: [#736]`; `BLOCKED-BY: [WS-10-PARENT, CROSS-1, CROSS-2]`

---

## 3. Dependency Graph

```mermaid
graph TD
    EPIC["#736 — Epic"]

    subgraph "Workstream parents"
        WS1[WS-1: Storage backend]
        WS2[WS-2: Schema + registry]
        WS3[WS-3: Walker + secret-scan]
        WS4[WS-4: Scan-and-ingest]
        WS5[WS-5: Live updates DEFERRED]
        WS6[WS-6: Derived companion]
        WS7[WS-7: API surface]
        WS8[WS-8: MCP surface]
        WS9[WS-9: Security tests]
        WS10[WS-10: Docs + deploy]
    end

    subgraph "Cross-cutting"
        C1[CROSS-1: Feature flag]
        C2[CROSS-2: Metrics + alerts]
        C3[CROSS-3: SIGTERM/resume]
        C4[CROSS-4: Rollout runbook]
    end

    WS1 --> WS2
    WS1 --> WS4
    WS1 --> WS9
    WS2 --> WS4
    WS2 --> WS6
    WS2 --> WS7
    WS2 --> WS8
    WS3 --> WS4
    WS3 --> WS9
    WS4 --> WS7
    WS4 --> WS9
    WS4 --> C1
    WS4 --> C2
    WS4 --> C3
    WS6 --> WS4
    WS6 --> WS9
    WS7 --> WS8
    WS7 --> WS9
    WS7 --> C1
    WS7 --> C2
    WS8 --> C1

    WS1 --> EPIC
    WS2 --> EPIC
    WS3 --> EPIC
    WS4 --> EPIC
    WS6 --> EPIC
    WS7 --> EPIC
    WS8 --> EPIC
    WS9 --> EPIC
    WS10 --> EPIC
    C1 --> EPIC
    C2 --> EPIC
    C3 --> EPIC
    C4 --> EPIC

    WS10 --> C4

    %% WS-5 deferred, not blocking epic
    WS5 -.->|deferred to v2| EPIC

    classDef critical fill:#fdd,stroke:#900,stroke-width:3px
    classDef deferred fill:#eee,stroke:#999,stroke-dasharray: 5 5
    class WS1,WS2,WS3,WS4,WS7,WS9 critical
    class WS5 deferred
```

### Critical path (highlighted red above)

The hottest sequential chain is:

**WS-1 → WS-2 → WS-4 → WS-7 → WS-9 → #736**

This is the minimum sequential chain. Everything else (WS-3, WS-6, WS-8, WS-10, CROSS-*) parallelizes off this spine. The 3-7 agent sweet spot per the no-time-estimates rule applies: dispatch WS-1 first solo (foundation), then fan out WS-2 + WS-3 in parallel, then WS-4 + WS-6 (both depend on WS-2), then WS-7 (depends on WS-4 + WS-6), then WS-8 + WS-9 + CROSS-* in parallel (all depend on WS-7), then WS-10 + CROSS-4 (depend on the rest).

---

## 4. Filing Order (for Phase 6 — deferred until §6 gate)

Gitea's "Blocked-by" field requires the blocking issue to already exist when the dependency is declared. Phase 6 filer must observe this strict order:

### Wave 1 — Workstream parents (10 issues, no inter-parent dependencies need not be set yet; they reference each other but filing order respects the topological order below)

File in this order (each parent's `Blocked-by:` field is populated as it is filed, so earlier parents must already exist):

1. WS-1-PARENT (no deps)
2. WS-3-PARENT (no deps)
3. WS-5-PARENT (no deps — deferred)
4. WS-10-PARENT (no structural deps; references future child issues but those go in wave 4)
5. WS-2-PARENT (Blocked-by: WS-1-PARENT)
6. WS-6-PARENT (Blocked-by: WS-2-PARENT)
7. WS-4-PARENT (Blocked-by: WS-1, WS-2, WS-3)
8. WS-7-PARENT (Blocked-by: WS-2, WS-4, WS-6)
9. WS-8-PARENT (Blocked-by: WS-7)
10. WS-9-PARENT (Blocked-by: WS-1, WS-2, WS-3, WS-4, WS-6, WS-7)

### Wave 2 — Atomic impl issues (42)

File per-workstream, in the topological order within each WS (use the `BLOCKS`/`BLOCKED-BY` fields in §2.3 to determine order within the workstream):

- WS-1: A → B → C, D (C and D after B)
- WS-2: A → B → C, D, E (latter three after B)
- WS-3: A → B → C, D, E (latter three after B)
- WS-4: A → B → C, D, E, F (latter four after B)
- WS-6: A → B → C
- WS-7: E (no deps inside) → A → B → C, D, F (D needs E too; F needs WS-4-D from cross-WS)
- WS-8: A → B → C
- WS-9: A, B, C, D (parallel)
- WS-10: E (no deps inside) → A, B, C, D, F (D depends on WS-9-PARENT)

### Wave 3 — Test tracker issues (9)

File after all parents and atomics for each workstream are filed:

TEST-WS-1, TEST-WS-2, TEST-WS-3, TEST-WS-4, TEST-WS-6, TEST-WS-7, TEST-WS-8, TEST-WS-9, TEST-WS-10

### Wave 4 — Documentation issues (6)

The 6 docs issues are part of WS-10 (WS-10-A through WS-10-F). They are filed as part of WS-10 atomic impl (Wave 2) — listed here for completeness.

### Wave 5 — Cross-cutting issues (4)

File last because they reference all other workstreams:

CROSS-1, CROSS-2, CROSS-3, CROSS-4

### Wave 6 — Epic update + labels

After all 70 new issues are filed:

1. Update fortemi/fortemi#736 body to include the dependency tree (Mermaid graph from §3) and link each WS-N-PARENT
2. Update #736 labels if needed (add `epic` if not present)
3. Add `epic` label to all 10 workstream parents

### Verification step after filing

After Wave 6:

```bash
# Sanity-check the dependency tree using Gitea MCP
mcp__gitea__issue_read --owner fortemi --repo fortemi --issue 736
# Confirm 10 workstream parents appear as Blocked-by
# Confirm each parent's body lists its atomic children
```

---

## 5. Estimated Total Issue Count (recap)

Per `no-time-estimates.md`: no time estimates. Counts only.

| Category | Count |
|---|---|
| Workstream parents | 10 |
| Atomic impl | 42 |
| Test trackers | 9 |
| Docs (subset of WS-10) | 6 |
| Cross-cutting | 4 |
| **Total proposed** | **71** |

This sits in the upper range of the requested 60-80 target. The high end reflects the genuine surface area: 5 ADRs, 9 active workstreams, 10 distinct TI-EXTSTORAGE security tests, 4 new API endpoints, 2 new MCP tools, 8 new metrics + 5 new alerts, and 5 new env vars. Decomposing further would create issues too small to meet the "atomic, single-deliverable" bar per `agent-friendly-code.md`. Bundling further would violate `subagent-scoping.md` Rule 1 (single-responsibility per issue).

---

## 6. Approval Gate (Phase 5 — operator review required BEFORE Phase 6 filing)

**No issues will be filed against `fortemi/fortemi` until the operator resolves the following.** These are the 8 open questions from `synthesis.md` §6 + the 8 open architecture questions from the SAD. The recommended defaults from synthesis are listed; operator must either accept all defaults or specify deviations.

### 6.1 Synthesis open questions (Q-1 through Q-8)

| Q | Question | Recommended default | Operator decision |
|---|---|---|---|
| Q-1 | Live update detection in v1? | **A** — defer entirely to v2 (WS-5 placeholder) | [ ] Accept default  [ ] Other: ______ |
| Q-2 | Per-blob storage mode as user-facing concept? | **A** — system-only, not user-facing | [ ] Accept default  [ ] Other: ______ |
| Q-3 | Secret-scan opt-out for performance? | **A** — no opt-out in v1 | [ ] Accept default  [ ] Other: ______ |
| Q-4 | MCP surface — extend existing or new family? | **A** — extend `manage_archives` + add `rescan_archive` | [ ] Accept default  [ ] Other: ______ |
| Q-5 | Path allowlist for Referenced source paths? | **C** — mandatory in multi-tenant, optional otherwise | [ ] Accept default  [ ] Other: ______ |
| Q-6 | Multi-archive directory overlap? | **A** — allowed with warning | [ ] Accept default  [ ] Other: ______ |
| Q-7 | Initial scan performance target? | **B** — <10 min for 10k-file repo on default CPU embedding | [ ] Accept default  [ ] Other: ______ |
| Q-8 | Failure mode for source unavailability? | **A** — lenient (warn-on-read, 503-on-write) | [ ] Accept default  [ ] Other: ______ |

### 6.2 SAD open architecture questions

The SAD (`architecture/software-architecture-doc.md`) lists 8 open architecture questions. Phase 6 filer must verify these are answered before filing. If the operator wants to alter any default in §6.1, several backlog issues may need scope adjustment (e.g., changing Q-1 from defer to "polling-only v1.5" would re-activate WS-5 and require ~4-6 new atomic issues).

### 6.3 Filing precondition

**Phase 6 may proceed only when**:
- [ ] All 8 Q's in §6.1 are decided
- [ ] All 8 SAD open questions are decided
- [ ] Operator explicitly approves the WS-5 deferral
- [ ] Operator explicitly approves the secret-scan mandatory-on default (Q-3)
- [ ] Operator confirms `FORTEMI_MULTI_TENANT=true` deployments will be the standard production target (relevant to Q-5)

### 6.4 Scope-change triggers

If operator changes any default, the following issues need re-scoping before filing:

| Change | Affected issues |
|---|---|
| Q-1: A → B (polling at 60s) | WS-5-PARENT activates with 4-6 new atomic children; CROSS-2 gains polling metrics |
| Q-1: A → C (full hybrid) | Major scope expansion; do not file v1 backlog — restart Phase 4 |
| Q-3: A → B (opt-out allowed) | WS-3-C, WS-3-D, WS-7-A gain `scan_config.skip_secret_scan` plumbing |
| Q-5: C → A (always-on) | WS-7-E becomes mandatory rather than conditional |
| Q-7: B → A (<2 min target) | NFR-007 tightens; WS-4-PARENT acceptance criteria tightens; may require GPU-always-on deployment doc changes |
| Q-8: A → B (strict) | WS-7-A returns 503 on all reads when source unreachable; UC-EXTSTORAGE-008 rewrites; CROSS-2 alerts change priority |

---

## References

- `@.aiwg/working/issue-planner-storage/synthesis.md` — Phase 2 synthesis (sole input)
- `@.aiwg/working/issue-planner-storage/inception/inception-summary.md` — Inception artifacts (Phase 3)
- `@.aiwg/working/issue-planner-storage/architecture/architecture-summary.md` — Architecture artifacts (Phase 3)
- `@.aiwg/working/issue-planner-storage/requirements/requirements-summary.md` — Requirements artifacts (Phase 3)
- `@.aiwg/working/issue-planner-storage/testing/testing-and-deployment-summary.md` — Testing + deployment artifacts (Phase 3)
- `@.claude/rules/no-time-estimates.md` — Why no time estimates appear here
- `@.claude/rules/agent-friendly-code.md` — Atomic, single-deliverable issue sizing
- `@.claude/rules/subagent-scoping.md` — Rule 1: single-responsibility per issue
- `@CLAUDE.md` — Fortemi project conventions (CalVer, conventional commits, Gitea on git.integrolabs.net)

---

**End of original draft backlog.** All 71 original issues are DRAFT. See §7 below for Phase 5 operator decisions and resulting scope deltas.

---

## 7. Phase 5 Operator Decisions (2026-05-21)

All 8 operator open questions resolved. Final answers below; scope deltas applied to the filing manifest (§8).

| Q | Decision | Delta vs synthesis default |
|---|---|---|
| Q-1 | **Option D** — detection-driven hybrid (RecommendedWatcher on native mounts, PollWatcher fallback on NFS/Docker-macOS/WSL/unknown). Operator override via `FORTEMI_WATCHER_MODE`. | WS-5 re-activates from `v2-deferred` to **v1 in-scope**. +6 new atomic impl issues, +1 test tracker, +1 docs issue. UC-EXTSTORAGE-005 returns from `v2-deferred` to v1. Backed by HIGH-quality notify-rs vendor docs. See `@.aiwg/working/issue-planner-storage/research-q1-live-updates.md`. |
| Q-2 | A — archive-level storage mode only (no per-blob user-facing mode) | No delta |
| Q-3 | A — secret scan mandatory in v1, no opt-out | No delta |
| Q-4 | A — extend `manage_archives` + add `rescan_archive` MCP tool | No delta |
| Q-5 | C — path allowlist required in multi-tenant, optional in single-user | No delta (this was already the conditional recommendation) |
| Q-6 | A — multi-archive directory overlap allowed with warning | No delta |
| Q-7 | B — initial scan SLA <10 min on default CPU embedding (Ollama nomic-embed-text) | No delta |
| Q-8 | A — lenient mount-failure mode (serve cached search + reject writes/rescans with 503) | No delta |

### 7.1 New WS-5 sub-issues (Option D)

These six atomic impl issues replace the WS-5 deferred placeholder:

| ID | Title | Description |
|---|---|---|
| WS-5-A | `feat(matric-core): mount-kind detection via /proc/self/mountinfo + platform equivalents` | Detect `native_*`, `nfs|cifs|smb|sshfs|fuse`, `docker_bind_macos_arm`, `wsl2_drvfs`, `tmpfs|overlay`, `unknown`. Returns enum to backend selector. |
| WS-5-B | `feat(matric-jobs): unified watcher event channel + backend selector` | Single `WatcherEvent` enum + channel; selector picks `RecommendedWatcher` or `PollWatcher` per mount kind. |
| WS-5-C | `feat(matric-jobs): PollWatcher pipeline integration` | notify-rs `PollWatcher` with `compare_contents`, configurable interval, content-hash dedup pass before ingest. |
| WS-5-D | `feat(matric-jobs): RecommendedWatcher pipeline integration` | notify-rs `RecommendedWatcher` wiring, event normalization to unified channel, debounce window. |
| WS-5-E | `feat(matric-api): FORTEMI_WATCHER_MODE operator override (auto|poll|notify|off)` | Env var honored at archive create + on per-archive restart. Documented in deployment-environment-template. |
| WS-5-F | `feat(observability): emit watcher_kind gauge + watcher_event_rate counter per archive` | Prometheus metrics for which backend each archive is using and event throughput. |

### 7.2 Updated test + docs

- **WS-5-TEST**: tracker issue covering 8 new test cases (mount-kind detection per platform, polling-fallback behavior, override env var, observability surface, Docker bind-mount soak, NFS soak, override-to-off-yields-no-watcher, watcher restart on mount disappear/reappear)
- **WS-10-DOC-EXT-005**: documentation for live-update behavior — how to choose mode, troubleshoot, and tune polling interval

### 7.3 Updated totals

| Category | Original | Delta | Final |
|---|---|---|---|
| Workstream parents | 10 | 0 (WS-5 was already a parent) | 10 |
| Atomic implementation | 42 | +6 (WS-5-A..F) | 48 |
| Test trackers | 9 | +1 (WS-5-TEST) | 10 |
| Docs | 6 | +1 (WS-10-DOC-EXT-005) | 7 |
| Cross-cutting | 4 | 0 | 4 |
| **Total** | **71** | **+8** | **79** |

### 7.4 Critical path (unchanged spine, WS-5 parallelizes)

WS-1 (trait extension) → WS-2 (ReferencedBackend) → WS-4 (scan+secret-prescan) → WS-7 (companion location) → WS-9 (degraded-mount handling) → close #736

**WS-5 (live watching, Option D) parallelizes off WS-2** — it needs `ReferencedBackend` in place to attach watchers to, but does not block WS-4/WS-7/WS-9.

---

## 8. Phase 6 Filing Manifest

**Status**: Ready for operator go/no-go.

**Filing order** (unchanged from §4, with WS-5 added):

1. File 10 workstream parents — including reactivated WS-5
2. File 48 atomic impl issues (WS-1 through WS-10 children) — including 6 new WS-5-A..F
3. File 10 test tracker issues (one per active WS) — including new WS-5-TEST
4. File 7 docs issues — including new WS-10-DOC-EXT-005
5. File 4 cross-cutting issues
6. Update fortemi/fortemi#736 epic body with dependency tree
7. Label workstream parents with `type: epic`

**Total**: 79 new Gitea issues + 1 update to #736.

**Target repo**: `fortemi/fortemi` on `git.integrolabs.net` (per memory: Gitea is authority).

**Label scheme**: Fortemi's existing `priority: P0/P1/P2/P3 + type: feat|test|docs|chore|epic + scope: matric-db|matric-api|matric-jobs|matric-core|mcp-server|security|observability|docs|deps + phase: construction|transition|deferred`. WS-5 issues get `scope: matric-jobs` and `scope: matric-core` (detection lives in core).

**Dependencies**: filing order respects Gitea's hard requirement that Blocked-by targets exist before dependent issues.

**Pre-flight check before Phase 6**:
- [ ] Operator reviews this addendum (§7) and confirms Option D scope addition
- [ ] Operator confirms 79-issue filing volume is acceptable (Gitea API limits, project board capacity)
- [ ] Operator confirms filing should target `fortemi/fortemi` (not a staging repo)
- [ ] Operator confirms `delivery.mode` from `.aiwg/aiwg.config` (default `pr-required`) is right for this work

If all four confirmed → Phase 6 fires. If any blocker → revise here first.

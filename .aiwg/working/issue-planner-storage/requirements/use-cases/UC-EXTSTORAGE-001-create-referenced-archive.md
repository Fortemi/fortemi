# UC-EXTSTORAGE-001: Create Referenced Archive Pointing at Local Directory

**Workstream**: WS-2 (Archive Schema and Registry), WS-7 (API Surface)
**Source**: synthesis §4 WS-2/WS-7, §3 Decision 1, §6 Q-5
**Status**: Draft
**Priority**: HIGH (foundational — all other use cases depend on this)

## Actor

**Primary**: Fortemi Admin (authenticated operator with archive-create capability)
**Secondary**: ArchiveRepository, ReferencedBackend, multi-tenant boundary enforcer

## Goal

Create a new Fortemi archive whose source content lives at an operator-owned local filesystem path, without copying any file bytes into Fortemi's managed blob store. Fortemi owns only the index (chunks, embeddings, metadata); the user owns the source data.

## Preconditions

- Admin is authenticated with valid Bearer token (per `REQUIRE_AUTH=true`, ADR-094)
- Target source path exists on the API host's filesystem
- Target source path is readable by the API process UID
- In multi-tenant deployments (`FORTEMI_MULTI_TENANT=true`): source path is under one of the allowlisted roots in `FORTEMI_REFERENCED_STORAGE_ROOTS` (per synthesis Decision 8 / Q-5)
- Archive name does not collide with an existing archive
- PostgreSQL `archive_registry` table is reachable

## Main Success Scenario

1. Admin POSTs to `/api/v1/archives/referenced` with payload: `{name: "company-docs", source_path: "/srv/data/company-docs", scan_config: {}}`
2. API canonicalizes `source_path` via `Path::canonicalize()` (resolves symlinks, removes `..`)
3. API verifies canonical path is under at least one entry in `FORTEMI_REFERENCED_STORAGE_ROOTS` (when multi-tenant); otherwise allows any path
4. API verifies path exists and is a directory (not a file, not a socket, not a device)
5. API verifies API process has read permission on the directory
6. API begins transaction: inserts row into `archive_registry` with `storage_mode='referenced'`, `source_path=<canonical>`, `scan_status='idle'`, `scan_config=<jsonb>`
7. API calls `clone_archive_schema()` to create per-archive PostgreSQL schema (same as managed archives — schema isolation is preserved per ADR-090-style)
8. API queues a `DirectoryScan` background job for the new archive (WS-4)
9. API returns HTTP 201 with `{archive_id, name, storage_mode: "referenced", source_path, scan_job_id}`
10. Admin receives `scan_job_id` and can poll `/api/v1/archives/company-docs/scan-status`

## Alternative Flows

### AF-1: Single-tenant deployment, no allowlist

- At step 3: `FORTEMI_MULTI_TENANT=false` and `FORTEMI_REFERENCED_STORAGE_ROOTS` is unset
- Skip allowlist check; accept any readable path
- Continue to step 4

### AF-2: Source path is a subdirectory of existing Referenced archive's source_path

- At step 6: another archive already references `/srv/data/` and new request references `/srv/data/company-docs`
- API logs warning to response: `{warnings: ["source_path overlaps with archive 'all-data'; embedding compute will be duplicated"]}`
- Continue to step 7 (per Decision Q-6: allow-with-warning)

### AF-3: Empty scan_config (use defaults)

- At step 1: `scan_config: {}` is empty
- API applies defaults from Decision 7: standard ignore list, secret denylist enabled, file-size cap 10MB
- Continue normally

## Exception Flows

### EF-1: source_path outside allowlist (multi-tenant)

- At step 3: canonical path not under any allowlisted root
- API returns HTTP 400 `{error: "source_path_not_allowed", message: "Path /etc/passwd is not under any configured FORTEMI_REFERENCED_STORAGE_ROOTS entry"}`
- No archive row created, no schema cloned

### EF-2: source_path does not exist

- At step 4: `Path::exists()` returns false
- API returns HTTP 400 `{error: "source_path_not_found", message: "Path /srv/data/missing does not exist on the API host"}`

### EF-3: source_path is not a directory

- At step 4: path is a regular file, symlink to file, FIFO, or device
- API returns HTTP 400 `{error: "source_path_not_directory", message: "Path /srv/data/file.txt is not a directory (kind: regular_file)"}`

### EF-4: API process lacks read permission

- At step 5: `read_dir()` returns `EACCES`
- API returns HTTP 403 `{error: "source_path_unreadable", message: "API process (uid 1000) cannot read /srv/data/private; check filesystem permissions"}`

### EF-5: Archive name collision

- At step 6: INSERT violates UNIQUE constraint on `archive_registry.name`
- API rolls back transaction, returns HTTP 409 `{error: "archive_exists", message: "Archive 'company-docs' already exists"}`

## Postconditions

- New row exists in `public.archive_registry` with `storage_mode='referenced'`
- Per-archive PostgreSQL schema exists (e.g., `archive_company_docs`)
- Companion derived storage directory created at `{FORTEMI_DERIVED_STORAGE_PATH}/{archive_id}/` (per Decision 3)
- A `DirectoryScan` job is enqueued for the new archive
- `archive_registry.scan_status = 'scanning'` after worker picks up the job
- Source path bytes are NOT copied, modified, or symlinked into Fortemi's managed storage

## Acceptance Criteria

- [ ] AC-1: Admin can create a Referenced archive with valid source_path in <500ms (API response time, excluding scan job)
- [ ] AC-2: Created archive appears in `GET /api/v1/archives` listing with `storage_mode: "referenced"`
- [ ] AC-3: Path traversal attempt (`source_path: "../../../etc"`) is rejected with HTTP 400 in single-tenant; HTTP 400 in multi-tenant (allowlist violation also catches it)
- [ ] AC-4: Symlink to outside allowlist (when multi-tenant) is rejected after canonicalization
- [ ] AC-5: Source directory is unchanged after archive creation: `find $source_path -newer /tmp/before_create -print` returns no results
- [ ] AC-6: After `scan_status` transitions to `idle`, `GET /api/v1/archives/company-docs/notes` returns chunks indexed from the source directory
- [ ] AC-7: Dropping the archive via `DELETE /api/v1/archives/company-docs` removes the PG schema and the companion `{derived_root}/{archive_id}/` directory but leaves source_path untouched (verifiable via `stat` mtime/inode)
- [ ] AC-8: Concurrent create-with-same-name requests result in exactly one success and one HTTP 409 (no partial state)

## Non-Functional Requirements

Applies (see `nfr-external-storage.md`):

- NFR-EXTSTORAGE-001 (path canonicalization at create-time)
- NFR-EXTSTORAGE-002 (no source-directory writes invariant)
- NFR-EXTSTORAGE-003 (tenant scoping in multi-tenant deployments)
- NFR-EXTSTORAGE-004 (archive creation latency target)
- NFR-EXTSTORAGE-008 (structured JSON logging for create event)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 1, §3 Decision 8, §4 WS-2, §4 WS-7, §6 Q-5
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-091 (planned): Archive-level storage mode declaration
- ADR-094 (existing): Fail-closed authentication

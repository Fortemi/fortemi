# UC-EXTSTORAGE-004: Operator Triggers Manual Rescan of Referenced Archive

**Workstream**: WS-7 (API Surface), WS-4 (Scan Pipeline)
**Source**: synthesis §4 WS-7, §3 Decision 4 (v1 is explicit-reindex-only), §6 Q-1
**Status**: Draft
**Priority**: HIGH (this is the v1 substitute for live filesystem watching)

## Actor

**Primary**: Fortemi Admin (operator who knows the source directory changed)
**Secondary**: DirectoryScanHandler, job queue

## Goal

Re-walk an existing Referenced archive's source directory to pick up file additions, modifications, and deletions, then return a job_id the operator can poll. This is the explicit "I added new files, please reindex" operation that substitutes for live filesystem watching in v1.

## Preconditions

- A Referenced archive exists with `storage_mode='referenced'`
- `archive_registry.scan_status = 'idle'` (no other scan currently running for this archive)
- Source directory is reachable (mount is up, permissions intact)
- Admin is authenticated

## Main Success Scenario

1. Admin POSTs to `/api/v1/archives/{name}/rescan` with optional payload `{full: false}` (incremental is default)
2. API verifies archive exists and is Referenced (`storage_mode='referenced'`)
3. API verifies `scan_status='idle'`; if `scanning`, returns 409 (per EF-2)
4. API enqueues `DirectoryScan` job with `archive_id` and `mode: 'incremental'` (or `full` if requested)
5. API updates `archive_registry.scan_status = 'queued'`, returns HTTP 202 with `{job_id, status_url: "/api/v1/jobs/{job_id}", archive_status_url: "/api/v1/archives/{name}/scan-status"}`
6. Background worker picks up the job; runs the same scan-and-ingest pipeline as UC-EXTSTORAGE-003 (idempotent: dedup by content_hash)
7. After completion, `archive_registry.scan_status = 'idle'`, `last_scan_at = NOW()`, `last_scan_summary` populated
8. Admin polls `/api/v1/archives/{name}/scan-status` (UC-EXTSTORAGE-005) until status returns to `idle`

## Alternative Flows

### AF-1: Full re-scan with quarantine re-validation

- At step 1: payload `{full: true}`
- At step 6: scan-and-ingest runs against ALL files (no dedup-hit short-circuit on secret-scan); re-validates quarantine list
- Use case: operator updated secret-denylist patterns and wants to re-check already-skipped files

### AF-2: Source directory has new files

- At step 6: walker discovers 50 new files; existing files dedup-hit
- New files enter the blob/attachment/note tables and queue Extraction jobs
- `last_scan_summary = {files_ingested: 50, files_deduped: 1000, files_quarantined: 2}`

### AF-3: Source directory has deleted files

- At step 6: walker no longer yields files that previously existed
- v1 behavior: stale notes remain in archive (no cascading delete) — documented limitation
- v2 work: synthesis §5 R-9 mentions rename detection; full delete reconciliation is parallel work

## Exception Flows

### EF-1: Archive does not exist

- At step 2: archive name not found
- API returns HTTP 404 `{error: "archive_not_found"}`

### EF-2: Scan already running

- At step 3: `scan_status='scanning'`
- API returns HTTP 409 `{error: "scan_in_progress", current_job_id, started_at}`
- Admin must wait or cancel the running job (cancel-job is out of scope for v1)

### EF-3: Archive is Managed, not Referenced

- At step 2: `storage_mode='managed'`
- API returns HTTP 400 `{error: "rescan_not_applicable", message: "Archive 'docs' is Managed mode; rescan only applies to Referenced archives"}`

### EF-4: Source directory unreachable

- At step 4 (scan job pickup): worker tries to access `source_path`, gets `ENOENT` or `EACCES`
- Worker updates `scan_status='error'`, `scan_error="source_path unreachable: <details>"`
- Admin sees error via `/scan-status` and must resolve (remount, fix permissions, then retry)

## Postconditions

- A new `DirectoryScan` job ID exists; visible via existing job-status API
- `archive_registry.scan_status` transitions: `idle → queued → scanning → idle | error`
- On success: new files indexed, dedup-hit files unchanged, quarantine list updated
- Source directory unchanged

## Acceptance Criteria

- [ ] AC-1: `POST /rescan` on an idle Referenced archive returns HTTP 202 with a valid job_id within 200ms
- [ ] AC-2: Job_id is queryable via `GET /api/v1/jobs/{job_id}` and shows progression `queued → running → completed`
- [ ] AC-3: After completion, `last_scan_at` is updated to the completion timestamp
- [ ] AC-4: Adding a new file to source_path between scans causes it to appear in `notes` table after the next rescan
- [ ] AC-5: Re-running rescan immediately after a successful rescan is a no-op (zero new INSERTs, completes faster than first run)
- [ ] AC-6: `POST /rescan` on a scan-in-progress archive returns HTTP 409 with current job_id
- [ ] AC-7: `POST /rescan` with `{full: true}` re-validates all files against the current secret denylist (verifiable: drop a new secret pattern in config, full-rescan, observe newly-quarantined files)

## Non-Functional Requirements

Applies:

- NFR-EXTSTORAGE-005 (scan throughput)
- NFR-EXTSTORAGE-007 (scan duration target)
- NFR-EXTSTORAGE-009 (rescan API surfaces job_id for polling)
- NFR-EXTSTORAGE-010 (scan metrics)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 4, §4 WS-7, §6 Q-1
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-094-storage (planned): Update detection model (defer live watching to v2)

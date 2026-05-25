# UC-EXTSTORAGE-005: Operator Views Referenced Archive Scan Status

**Workstream**: WS-7 (API Surface)
**Source**: synthesis §4 WS-7
**Status**: Draft
**Priority**: MEDIUM (observability/UX)

## Actor

**Primary**: Fortemi Admin (polling for scan completion or troubleshooting)
**Secondary**: ArchiveRepository

## Goal

Retrieve the current scan state, last-scan summary, and any error condition for a Referenced archive, so the operator knows whether to wait, retry, or investigate.

## Preconditions

- Referenced archive exists
- Admin authenticated

## Main Success Scenario

1. Admin GETs `/api/v1/archives/{name}/scan-status`
2. API verifies archive exists and `storage_mode='referenced'`
3. API reads `archive_registry` row: `scan_status`, `last_scan_at`, `last_scan_summary`, `scan_error`, `source_path`, current `scan_job_id` (if scanning)
4. API returns HTTP 200 with payload:

```json
{
  "archive_name": "company-docs",
  "storage_mode": "referenced",
  "source_path": "/srv/data/company-docs",
  "source_path_accessible": true,
  "scan_status": "idle",
  "last_scan_at": "2026-05-21T14:33:00Z",
  "last_scan_summary": {
    "files_scanned": 1052,
    "files_ingested": 50,
    "files_deduped": 1000,
    "files_quarantined": 2,
    "duration_ms": 184530
  },
  "current_scan_job_id": null,
  "scan_error": null
}
```

## Alternative Flows

### AF-1: Scan currently running

- At step 3: `scan_status = 'scanning'`, `current_scan_job_id` populated
- Payload includes job_id and `started_at`; `last_scan_summary` reflects PREVIOUS completed scan
- Operator can poll job-status API for live progress

### AF-2: Scan error from previous run

- At step 3: `scan_status = 'error'`, `scan_error = "source_path unreachable"`
- Payload surfaces error prominently; `source_path_accessible: false`
- Admin uses this to decide remediation (remount, fix permissions)

### AF-3: Never scanned (just created)

- At step 3: `last_scan_at = NULL`, `last_scan_summary = NULL`, `scan_status = 'queued' | 'scanning'`
- Payload shows nulls for last-scan fields; current_scan_job_id is populated

## Exception Flows

### EF-1: Archive not found

- HTTP 404 `{error: "archive_not_found"}`

### EF-2: Archive is Managed mode

- HTTP 400 `{error: "scan_status_not_applicable", message: "Archive 'docs' is Managed; no scan status to report"}`

## Postconditions

- No state mutation (read-only)
- Response includes liveness check of source_path (cheap stat call, see AC-3)

## Acceptance Criteria

- [ ] AC-1: Endpoint returns full status payload in <100ms for an archive with normal scan history
- [ ] AC-2: `scan_status` value is one of: `idle`, `queued`, `scanning`, `error`
- [ ] AC-3: `source_path_accessible` field reflects current readability of the directory (performs `stat` call; if path is unreachable, returns false within 1 second — does not block on full NFS timeout)
- [ ] AC-4: `last_scan_summary` matches the actual job-completion record (files_scanned, files_ingested, files_quarantined, duration_ms)
- [ ] AC-5: When `scan_status = 'error'`, `scan_error` field contains a human-readable description of the failure
- [ ] AC-6: Endpoint is read-only; calling it 1000 times in a row does not change any archive state or trigger any background work

## Non-Functional Requirements

Applies:

- NFR-EXTSTORAGE-009 (status endpoint surfaces scan health)
- NFR-EXTSTORAGE-010 (observability)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §4 WS-7
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md

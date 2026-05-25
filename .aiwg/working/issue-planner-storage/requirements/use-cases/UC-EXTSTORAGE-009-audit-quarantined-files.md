# UC-EXTSTORAGE-009: Operator Audits Quarantined Files (Secret-Scan Skips)

**Workstream**: WS-7 (API Surface)
**Source**: synthesis §4 WS-7, §3 Decision 5
**Status**: Draft
**Priority**: MEDIUM (security audit / debugging)

## Actor

**Primary**: Fortemi Admin or Security Auditor
**Secondary**: ArchiveRepository, quarantine table

## Goal

Retrieve the list of files that the secret-scan layer skipped during the most recent (or full history of) scans of a Referenced archive, so the operator can confirm correct behavior, investigate false positives, or audit security-related decisions.

## Preconditions

- Referenced archive exists
- At least one scan has run (`last_scan_at IS NOT NULL`)
- Admin is authenticated

## Main Success Scenario

1. Admin GETs `/api/v1/archives/{name}/quarantined-files?limit=50&offset=0`
2. API verifies archive exists and is Referenced
3. API SELECTs from `archive_<id>.quarantined_files` ORDER BY `scanned_at DESC` LIMIT 50 OFFSET 0
4. API returns HTTP 200:

```json
{
  "archive_name": "company-docs",
  "total_quarantined": 7,
  "limit": 50,
  "offset": 0,
  "files": [
    {
      "path": "/srv/data/company-docs/legacy/old.pem",
      "reason": "path_denylist",
      "pattern": "*.pem",
      "scanned_at": "2026-05-21T14:33:12Z"
    },
    {
      "path": "/srv/data/company-docs/notes/sample-creds.txt",
      "reason": "content_denylist",
      "pattern": "AWS access key (AKIA[0-9A-Z]{16})",
      "line": 42,
      "scanned_at": "2026-05-21T14:33:12Z"
    }
  ]
}
```

## Alternative Flows

### AF-1: Filter by reason

- Admin adds `?reason=content_denylist` query parameter
- API adds `WHERE reason = $1` to query
- Returns only content-based quarantines (useful: distinguishes "I named a file .pem" from "this contains an actual secret")

### AF-2: Filter by date range

- Admin adds `?since=2026-05-21T00:00:00Z`
- API filters by `scanned_at >= $since`

### AF-3: Pagination beyond first page

- Admin requests `offset=50`
- API returns next 50; `total_quarantined` reflects full count

## Exception Flows

### EF-1: Archive not found

- HTTP 404 `{error: "archive_not_found"}`

### EF-2: Archive is Managed

- HTTP 400 `{error: "quarantine_not_applicable", message: "Managed archives have no secret-scan layer"}`

### EF-3: Never scanned

- HTTP 200 with `{total_quarantined: 0, files: []}` (not an error)

## Postconditions

- No state mutation (read-only)
- Operator has list of files skipped, with reasons, for audit

## Acceptance Criteria

- [ ] AC-1: Endpoint returns quarantine list with `reason`, `pattern`, `path`, `scanned_at` for each entry
- [ ] AC-2: For path-denylist matches: `reason='path_denylist'`, `pattern` shows which pattern (e.g., `*.pem`)
- [ ] AC-3: For content-denylist matches: `reason='content_denylist'`, `pattern` shows which regex matched, `line` shows line number
- [ ] AC-4: Pagination works correctly: `offset=50&limit=50` returns the second page
- [ ] AC-5: `total_quarantined` reflects the full count regardless of pagination
- [ ] AC-6: After a rescan that re-quarantines the same files, entries are updated (not duplicated) — verifiable: count remains stable across no-op rescans
- [ ] AC-7: Quarantine entries are removed when source file no longer exists or no longer matches denylist (only on `full=true` rescan per Decision 5)

## Non-Functional Requirements

Applies:

- NFR-EXTSTORAGE-001 (security: quarantine evidence is auditable)
- NFR-EXTSTORAGE-008 (structured logging)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 5, §4 WS-7
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-095 (planned): Secret detection at ingest

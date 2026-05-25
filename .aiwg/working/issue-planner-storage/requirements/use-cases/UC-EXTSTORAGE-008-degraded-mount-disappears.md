# UC-EXTSTORAGE-008: Degraded Read Behavior When Source Mount Disappears

**Workstream**: WS-7 (API), WS-9 (Security Tests)
**Source**: synthesis §3 Decision 8, §6 Q-8, §5 R-5
**Status**: Draft
**Priority**: HIGH (reliability invariant — must fail gracefully)

## Actor

**Primary**: API search request handler (serving a user query)
**Secondary**: ReferencedBackend, integrity-sweep job, operator

## Goal

When a Referenced archive's source directory becomes unreachable mid-operation (NFS timeout, USB unmount, deleted mount point), the system serves cached search results with a clear warning flag rather than hard-failing every query. Writes (rescans) refuse cleanly with 503. This preserves user-facing usefulness during transient mount issues.

## Preconditions

- A Referenced archive exists and was previously scanned (chunks/embeddings present in pgvector)
- The source mount has just become unreachable (after the scan, before the current request)
- A search request arrives at the API targeting this archive

## Main Success Scenario (Lenient Read Path)

1. User issues `GET /api/v1/search?query=foo&archive=company-docs`
2. Search handler executes pgvector query against `archive_company_docs.chunks` — succeeds (chunks live in DB, not on the missing mount)
3. Handler assembles result set with chunk text and metadata
4. For each result, handler checks: does the result need to stream original file content? (e.g., user clicked "download original")
5. If only chunk content is needed (semantic search result preview): return results normally
6. If full-file content is needed: handler attempts `ReferencedBackend::read($source_path)`
7. If read fails: handler returns the result WITH a warning flag `{warnings: [{type: "source_unavailable", archive: "company-docs", message: "Original source files temporarily unreachable; chunk content is cached"}]}`
8. User receives degraded but useful response (search works; download fails gracefully)

## Alternative Flows

### AF-1: Source comes back online

- Mount is restored (admin remounted NFS, plugged USB back in)
- Next request: read succeeds normally; no warning emitted
- Integrity sweep job (scheduled separately) detects health restoration, logs event

### AF-2: Scheduled integrity sweep detects offline state

- Background sweep job runs periodically (e.g., every 5 minutes)
- For each Referenced archive, sweep calls `Path::exists()` on `source_path`
- If unreachable: updates `archive_registry.last_source_check_at`, `source_path_accessible=false`, emits metric `referenced_archive_source_offline{archive=company-docs}`
- Operator alerting (existing monitoring) catches the metric

## Write Path (Fail Closed)

### WP-1: Rescan attempt while source is unreachable

1. Admin POSTs `/api/v1/archives/company-docs/rescan`
2. DirectoryScanHandler picks up job, tries to walk source path
3. Walker returns `ENOENT` or `EACCES` immediately
4. Handler updates `scan_status='error'`, `scan_error="source_path unreachable: /srv/mnt/usb"`
5. API returns 503 to subsequent requests on this endpoint until source is back
6. UC-EXTSTORAGE-005 surfaces the error state for the operator

### WP-2: New attachment upload to Referenced archive

- This is not a valid operation on Referenced archives (per WS-7 middleware: write endpoints return 403)
- No mount-disappearance handling needed; the operation is rejected earlier in the middleware

## Exception Flows

### EF-1: pgvector query itself fails

- At step 2: DB unreachable (separate failure mode)
- Handler returns HTTP 503 (standard DB-down behavior; not Referenced-specific)

### EF-2: Partial mount (some files reachable, some not)

- NFS partially times out; `Path::exists($source_path)` succeeds but individual file reads fail
- Per-result behavior: chunks served normally; individual file streaming returns `{warning: "file_unavailable"}`
- This is acceptable per Decision 8's lenient-on-reads policy

### EF-3: Operator opts into strict mode

- Future v1.5 work per Q-8: `scan_config.strict_consistency: true`
- At step 7: handler returns HTTP 503 instead of warning-flag
- Out of scope for v1 default; documented as opt-in alternative

## Postconditions

- Cached search results remain queryable through source-path outages
- Operator is notified via metrics emitted by integrity sweep
- Writes refuse safely (no partial-write corruption)
- Source path: still unchanged when it returns

## Acceptance Criteria

- [ ] AC-1: After unmounting source path, `GET /api/v1/search?archive=company-docs&query=...` returns chunks WITH a `warnings` array containing `type: source_unavailable`
- [ ] AC-2: After unmounting source path, `POST /api/v1/archives/company-docs/rescan` queues job; job completes with `scan_status='error'`, `scan_error` populated
- [ ] AC-3: Original-file download endpoint (e.g., `GET /api/v1/attachments/{id}/download`) returns HTTP 503 with body `{error: "source_unavailable"}` when source is offline (not 404, not 200 with empty body)
- [ ] AC-4: After remount, next `GET /search` returns no warnings; original-file download succeeds
- [ ] AC-5: Integrity sweep job (testable via manual trigger `POST /api/v1/admin/integrity-sweep`) updates `source_path_accessible` field within 60 seconds of mount state change
- [ ] AC-6: Metric `referenced_archive_source_offline` emits with `archive=<name>` label when offline (verifiable via `/metrics` endpoint)
- [ ] AC-7: API process does NOT crash, hang indefinitely, or leak file descriptors when source is unreachable (verifiable via load test during simulated mount loss)

## Non-Functional Requirements

Applies:

- NFR-EXTSTORAGE-012 (degraded-mode reliability)
- NFR-EXTSTORAGE-010 (observability: source-accessibility metric)

## References

- @.aiwg/working/issue-planner-storage/synthesis.md §3 Decision 8, §6 Q-8, §5 R-5
- @.aiwg/working/issue-planner-storage/requirements/nfr-external-storage.md
- ADR-097 (planned): Failure modes for Referenced archive source-path unavailability

# Non-Functional Requirements: External Storage Backend + Scan-and-Ingest

**Module**: External Storage (Referenced mode)
**Source**: synthesis §3 (load-bearing decisions), §5 (risk register)
**Scope**: All workstreams WS-1 through WS-10
**Status**: Draft for Elaboration gate

## Reasoning

1. **Problem Analysis**: Referenced storage shifts the trust boundary — Fortemi's process now reads user-owned files in place. NFRs must encode the absolute invariants that protect the user (no source writes, no secret leakage, tenant isolation) and the operational expectations that make the feature usable (performance, observability, graceful degradation).
2. **Constraint Identification**: Multi-tenant deployments per ADR-094 (fail-closed auth) and existing per-archive PostgreSQL schema isolation; v1 defers live watching, so on-demand rescan must be performant; the scan-and-ingest pipeline rides existing extraction infrastructure (no breaking changes).
3. **Alternative Consideration**: Considered weaker security NFRs (e.g., opt-in secret scan) but rejected per synthesis §6 Q-3; considered tighter performance NFRs (sub-minute scans) but rejected per Q-7 in favor of realistic <10min targets matching nomic-embed-text throughput on commodity GPU.
4. **Decision Rationale**: Each NFR below is measurable, verifiable, and cites the synthesis decision or risk it operationalizes.
5. **Risk Assessment**: The biggest gap is content-based secret detection beyond the 64KB prefix (NFR-EXTSTORAGE-001 accepts this limitation per Decision 5); the second is rename detection deferred to v2 (NFR-EXTSTORAGE-011 documents the v1 stale-note behavior).

---

## Security NFRs

### NFR-EXTSTORAGE-001: Secret Detection Mandatory at Ingest

**Category**: Security
**Description**: Every file passing through the scan-and-ingest pipeline (UC-EXTSTORAGE-002, UC-EXTSTORAGE-003) MUST be checked against both path-based denylist and content-based regex patterns (Decision 7) before any chunk extraction or embedding occurs.
**Measurable Criterion**:
- Files matching path denylist (`.env*`, `*.pem`, `*.key`, `id_rsa*`, `.ssh/`, `.gnupg/`, `.aws/credentials`, `.kube/config`, `secrets.*`, `credentials.json`) are 100% rejected before content is read
- Files matching content denylist (PEM PRIVATE KEY regex, AWS access key `(AKIA|ASIA)[0-9A-Z]{16}`, GitHub PAT `ghp_[a-zA-Z0-9]{36}`, JWT `eyJ[a-zA-Z0-9_-]{10,}\.{2,}`) are 100% rejected
- Zero secret-bearing fixture files produce chunks in `archive_<id>.chunks` after a full scan (red-team test)
**Verification Method**: Drop a fixture file containing a known AWS access key into a test source directory; trigger scan; SELECT from chunks; assert zero rows reference the file's content_hash
**Risk Addressed**: synthesis §5 R-1 (secret leakage)

### NFR-EXTSTORAGE-002: No Source-Directory Writes (Invariant)

**Category**: Security / Data Integrity
**Description**: Fortemi MUST NEVER write to, modify, delete, rename, or chmod any file under a Referenced archive's `source_path`. This is enforced by `ReferencedBackend::write()` returning `Err(NotSupported)` (WS-1) and is verified by checksum-diff testing.
**Measurable Criterion**:
- After every scan, extraction, or operation on a Referenced archive, the source directory's recursive checksum is byte-identical to its pre-operation state
- Test: `find $source -type f -exec sha256sum {} \; | sort` produces identical output before and after operations
- `ReferencedBackend::write()` and `ReferencedBackend::delete()` return errors (not silent no-ops) — verified by unit test
**Verification Method**: Automated test in WS-9 test suite computes pre/post checksums across all use case flows
**Risk Addressed**: synthesis §5 R-1, R-2 (data tampering)

### NFR-EXTSTORAGE-003: Path Canonicalization at Create Time

**Category**: Security
**Description**: `POST /api/v1/archives/referenced` MUST canonicalize `source_path` via `Path::canonicalize()` (resolves `..` and symlinks) before any validation, allowlist check, or persistence.
**Measurable Criterion**:
- Submitting `source_path: "/srv/data/../../etc"` resolves to `/etc` before allowlist check
- Symlink targets are followed during canonicalization; the resolved target is what gets stored and validated
- Path traversal attempts via `..`, `./`, or symlinks cannot bypass the allowlist
**Verification Method**: WS-9 path-traversal test suite includes 10+ adversarial inputs; all must be rejected or canonicalized correctly
**Risk Addressed**: synthesis §5 R-2 (multi-tenant boundary breach)

### NFR-EXTSTORAGE-004: Tenant Scoping in Multi-Tenant Deployments

**Category**: Security
**Description**: When `FORTEMI_MULTI_TENANT=true`, all Referenced archives MUST have `source_path` under one of the entries in `FORTEMI_REFERENCED_STORAGE_ROOTS` (colon-separated). Per-tenant root prefixes (e.g., `/srv/fortemi/<tenant>/`) prevent Tenant A from referencing Tenant B's storage.
**Measurable Criterion**:
- Creating a Referenced archive with `source_path` outside all allowlisted roots returns HTTP 400 in multi-tenant deployments
- Setting `FORTEMI_MULTI_TENANT=true` with `FORTEMI_REFERENCED_STORAGE_ROOTS` unset OR empty causes server startup to fail (or refuse Referenced-archive creation) — multi-tenant without allowlist is not a valid configuration
- Cross-tenant test (WS-9): Tenant A's archive at `/srv/fortemi/A/code` cannot be read or listed by an authenticated request scoped to Tenant B
**Verification Method**: WS-9 multi-tenant boundary test suite
**Risk Addressed**: synthesis §5 R-2

---

## Performance NFRs

### NFR-EXTSTORAGE-005: Scan Throughput Target

**Category**: Performance
**Description**: The `ScanWalker` (UC-EXTSTORAGE-002) MUST achieve at least 500 files/second sustained throughput on a warmed filesystem cache for files averaging <100KB, on a 4-core API host with local SSD.
**Measurable Criterion**:
- Benchmark: walk a 10,000-file fixture (avg 50KB/file) on warmed cache; total walk + secret-scan completes in ≤20 seconds (≥500 files/sec)
- On cold cache: degraded throughput is acceptable; document expected ratio in operator docs
**Verification Method**: Benchmark suite committed in WS-3 work; CI gate at this threshold
**Risk Addressed**: synthesis §5 R-3 (performance death on monorepo ingest)

### NFR-EXTSTORAGE-006: Embedding Throughput Dependent on Backend

**Category**: Performance
**Description**: Embedding throughput for ingested chunks is bounded by the configured inference backend (Ollama/OpenAI/llama.cpp). Fortemi MUST NOT serialize embedding work behind the walker; chunks should be queued for embedding and processed in parallel with continued walking.
**Measurable Criterion**:
- For nomic-embed-text on Ollama with RTX 3060 12GB: sustained ≥300 chunks/sec (per synthesis §6 Q-7 hardware reference)
- Walker should not block on embedder; queue depth grows but walker reaches end-of-tree in expected time per NFR-EXTSTORAGE-005
**Verification Method**: Load test combines NFR-005 walker with NFR-007 end-to-end timing
**Risk Addressed**: synthesis §5 R-3

### NFR-EXTSTORAGE-007: Initial Scan Duration Target

**Category**: Performance
**Description**: Per synthesis §6 Q-7 recommendation B: initial scan-and-ingest of a 10,000-file Referenced source directory should complete in ≤10 minutes on a 4-core / 12GB-GPU host with default Ollama configuration.
**Measurable Criterion**:
- Test fixture: 10k files averaging ~5KB each, mix of code and text
- End-to-end (POST /rescan → scan_status='idle' with last_scan_summary populated): ≤10 minutes (600 seconds) wall-clock
- Headroom for monorepos: acceptable to exceed target for 100k+ file directories; document scaling
**Verification Method**: Benchmark suite in WS-9; performance CI gate
**Risk Addressed**: synthesis §5 R-3, sets operator expectations per Q-7

---

## Reliability NFRs

### NFR-EXTSTORAGE-008: Structured JSON Logging

**Category**: Observability / Operability
**Description**: All scan, ingest, secret-scan, and source-availability events MUST be logged as structured JSON to stdout per `.claude/rules/logs-as-event-streams.md`. Log entries include archive_name, source_path, event_type, timestamp (ISO-8601 UTC), correlation_id (scan_job_id).
**Measurable Criterion**:
- Every quarantine event produces a log entry with fields: `{event: "quarantine", archive, path, reason, pattern, scan_job_id, timestamp}`
- Every scan completion produces: `{event: "scan_complete", archive, files_scanned, files_ingested, files_quarantined, duration_ms, scan_job_id}`
- No PII or secret content appears in logs (path is logged; file contents are not)
**Verification Method**: Sample 100 log lines during integration test; validate JSON schema and content rules
**Risk Addressed**: operational visibility (no specific R-row; foundational)

### NFR-EXTSTORAGE-009: Operator Force-Rescan API

**Category**: Operability
**Description**: Operators MUST have an authenticated API endpoint to force a rescan of any Referenced archive (UC-EXTSTORAGE-004), returning a job_id for polling and a status endpoint (UC-EXTSTORAGE-005) to monitor progress.
**Measurable Criterion**:
- `POST /api/v1/archives/{name}/rescan` returns HTTP 202 with `{job_id, status_url, archive_status_url}` in ≤200ms
- `GET /api/v1/archives/{name}/scan-status` returns full status payload in ≤100ms
- Both endpoints require valid Bearer token per ADR-094
**Verification Method**: API integration tests in WS-7

### NFR-EXTSTORAGE-010: Per-Archive Scan Metrics

**Category**: Observability
**Description**: Each Referenced archive MUST expose at minimum these metrics via the existing `/metrics` endpoint:
- `referenced_archive_files_scanned_total{archive=<name>}` (counter)
- `referenced_archive_files_quarantined_total{archive=<name>,reason=<...>}` (counter)
- `referenced_archive_scan_duration_seconds{archive=<name>}` (histogram)
- `referenced_archive_last_scan_timestamp{archive=<name>}` (gauge, unix seconds)
- `referenced_archive_source_offline{archive=<name>}` (gauge, 0|1)
**Measurable Criterion**:
- All five metrics present in `/metrics` output after a scan completes
- Metric labels are correctly populated per archive name
**Verification Method**: Scrape /metrics during test, assert presence and shape
**Risk Addressed**: synthesis §5 R-5, R-8 (visibility into health and disk usage)

### NFR-EXTSTORAGE-011: Backward Compatibility with Managed Archives

**Category**: Compatibility
**Description**: All existing Managed-mode archives MUST continue to function identically after the Referenced storage migration lands. Schema migration adds nullable columns only; existing data is unaffected.
**Measurable Criterion**:
- After migration applies, all pre-existing archives have `storage_mode='managed'` (default value)
- All existing API endpoints, search queries, MCP tool invocations, and attachment uploads work without modification
- Existing test suite passes 100% after migration (no regressions)
- `manage_archives` MCP tool without `storage_mode` param defaults to Managed (UC-EXTSTORAGE-007 AF-1)
**Verification Method**: Run full pre-migration test suite against post-migration deployment

### NFR-EXTSTORAGE-012: Degraded-Mode Reliability

**Category**: Reliability
**Description**: Per synthesis §3 Decision 8: when a Referenced archive's source path becomes unreachable, the system MUST serve cached search results with a warning flag (fail-open on reads) and refuse writes/rescans with HTTP 503 (fail-closed on writes). API process MUST NOT crash, hang, or leak file descriptors.
**Measurable Criterion**:
- Unmount-during-search test (WS-9): search returns results with `warnings: [{type: "source_unavailable"}]`
- Unmount-during-rescan test: rescan job moves to `scan_status='error'` with informative `scan_error`
- Load test during simulated mount loss: API process memory, FD count, and CPU remain stable for ≥10 minutes
- After remount, next search returns no warnings (auto-recovery, no operator action needed)
**Verification Method**: WS-9 mount-disappearance test suite + sustained load test
**Risk Addressed**: synthesis §5 R-5

---

## Acceptance Summary

This NFR module is considered baselined when:

- [ ] All 12 NFRs above have a corresponding test in the WS-9 security/reliability test suite
- [ ] Performance benchmarks (NFR-005, NFR-006, NFR-007) run as CI gates
- [ ] Metrics (NFR-010) are scraped by the existing monitoring stack
- [ ] All use cases UC-EXTSTORAGE-001 through UC-EXTSTORAGE-009 reference at least one applicable NFR

## References

- @.aiwg/working/issue-planner-storage/synthesis.md (all sections)
- @.aiwg/working/issue-planner-storage/requirements/use-cases/ (all UCs)
- @.claude/rules/logs-as-event-streams.md (structured logging)
- @CLAUDE.md (existing auth + multi-tenant configuration)
- ADR-094 (fail-closed authentication baseline)
- ADR-091 through ADR-097 (planned, from synthesis §3)

# Deployment Plan: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Phase 3 — SDLC Corpus Generation
**Date**: 2026-05-21
**Sole input**: `@.aiwg/working/issue-planner-storage/synthesis.md`

This document captures the deployment changes, configuration surface, migration strategy, rollout/rollback, and operational runbook stubs required to ship the Referenced storage epic to the existing Fortemi Docker bundle deployment.

---

## 1. Deployment Model Changes

### 1.1 New Bind-Mount Requirements

Referenced archives require the Fortemi API container (and the matric-jobs worker, if separated) to be able to **read** the user-owned source directory and **read+write** the companion derived-artifact directory. Both must be mounted into the container.

**Required mounts** (added to `docker-compose.bundle.yml`):

```yaml
services:
  fortemi-api:
    volumes:
      # Existing
      - fortemi-data:/data
      - ./.env:/app/.env:ro

      # New for Referenced storage
      - ${FORTEMI_REFERENCED_SOURCE_ROOTS}:/srv/fortemi/referenced:ro
      - fortemi-derived:/data/derived

  fortemi-jobs:  # (or whichever container hosts the scan worker)
    volumes:
      - ${FORTEMI_REFERENCED_SOURCE_ROOTS}:/srv/fortemi/referenced:ro
      - fortemi-derived:/data/derived

volumes:
  fortemi-derived:
    driver: local
```

Notes:
- The `:ro` flag on the referenced source mount is enforced at the Docker level as a defense-in-depth measure layered on top of the `ReferencedBackend::write` no-op (synthesis §3 Decision 2). Even a backend bug cannot write to source.
- `fortemi-derived` is a named volume managed by Docker so derived artifacts persist across container restarts but are owned by the bundle.
- Operators with multiple source roots (multi-tenant deployments) mount a parent directory containing per-tenant subdirectories, then set `FORTEMI_REFERENCED_STORAGE_ROOTS` (synthesis Q-5) to point at the in-container path equivalents.

### 1.2 Platform-Specific Considerations

**Linux native Docker** (the primary deployment target):
- Bind mounts work natively.
- uid/gid mapping: container runs as a defined non-root user. Source directory must be readable by that uid. Document the container's uid in the operator guide.

**macOS Docker Desktop**:
- Bind mounts work but file watching does NOT (relevant for WS-5, which is deferred). For v1 (explicit-reindex-only per synthesis Decision 4) this is not a blocker.
- Performance: bind-mount I/O on macOS is slower than on Linux. Document expected scan time degradation for macOS deployments in the operator guide.
- Recommend macOS users mount only directories under their home dir (Docker Desktop has explicit file-sharing settings).

**Windows native Docker**:
- WSL2 backend handles bind mounts via the Linux subsystem.
- Source path must be WSL2-accessible (under `/mnt/c/...` or in the WSL2 filesystem).

**Windows Docker Desktop (Hyper-V backend)**:
- Slow; not recommended for Referenced storage with large repos.

### 1.3 uid/gid Mapping

The Docker bundle entrypoint should:
1. At startup, check the uid of the mounted source directory.
2. If the container user's uid doesn't match, log a clear warning indicating the operator may have read-permission issues.
3. Do NOT chown the source directory — that would violate the source-preservation invariant.

A new env var `FORTEMI_CONTAINER_UID` (default: 1000) lets operators align the container uid with their host filesystem ownership.

---

## 2. Configuration Changes

### 2.1 New Environment Variables

| Variable | Default | Required? | Description |
|---|---|---|---|
| `FORTEMI_EXTERNAL_STORAGE_ENABLED` | `false` | No | Feature flag. When `false`, the API rejects `POST /api/v1/archives/referenced` with HTTP 501. Default-off for v1 rollout safety. Operator must explicitly opt in. |
| `FORTEMI_REFERENCED_STORAGE_ROOTS` | (empty) | No (Yes for `FORTEMI_MULTI_TENANT=true`) | Colon-separated allowlist of path prefixes Referenced archives may reference. Empty = any path (single-user). Per synthesis Q-5 recommendation C. |
| `FORTEMI_DERIVED_STORAGE_PATH` | `${FILE_STORAGE_PATH}/derived/` | No | Companion managed location for derived artifacts of Referenced archives (synthesis §3 Decision 3). |
| `FORTEMI_SCAN_WORKER_THREADS` | `min(4, num_cpus)` | No | Parallel walker thread count for scan-and-ingest (synthesis WS-3). Capped at 4 by default to prevent starvation of other GPU/CPU workloads. |
| `FORTEMI_SCAN_FILE_SIZE_CAP_MB` | `10` | No | Files larger than this are skipped by default (synthesis §3 Decision 7 ignore list). Configurable for operators with large legitimate files. |
| `FORTEMI_SCAN_STRICT_CONSISTENCY` | `false` | No | When `true`, source-path unavailability causes read requests to return 503 (synthesis §3 Decision 8 option B / Q-8 strict mode). Default `false` = lenient mode. |

### 2.2 Updated `.env.example`

The bundle's `.env.example` must add the new variables with inline documentation referencing this deployment plan and the operator guide (`docs/referenced-storage.md`).

### 2.3 No Changes to Existing Variables

`FILE_STORAGE_PATH`, `MAX_MEMORIES`, `REQUIRE_AUTH`, `FORTEMI_MULTI_TENANT` semantics unchanged. The new variables compose with the existing ones; in particular `FORTEMI_MULTI_TENANT=true` forces `FORTEMI_REFERENCED_STORAGE_ROOTS` to be non-empty (startup error if combined with `FORTEMI_EXTERNAL_STORAGE_ENABLED=true` and empty allowlist).

---

## 3. Migration Plan

### 3.1 Schema Migration

A single new migration adds five nullable columns to `archive_registry`:

```sql
-- migrations/<timestamp>_referenced_storage.sql

ALTER TABLE archive_registry
  ADD COLUMN storage_mode TEXT NOT NULL DEFAULT 'managed'
    CHECK (storage_mode IN ('managed', 'referenced')),
  ADD COLUMN source_path TEXT,
  ADD COLUMN scan_config JSONB,
  ADD COLUMN last_scan_at TIMESTAMPTZ,
  ADD COLUMN scan_status TEXT NOT NULL DEFAULT 'idle'
    CHECK (scan_status IN ('idle', 'scanning', 'error'));

-- Constraint: Referenced archives must have a source_path
ALTER TABLE archive_registry
  ADD CONSTRAINT chk_referenced_has_source_path
    CHECK (storage_mode = 'managed' OR source_path IS NOT NULL);
```

**Properties**:
- Backward-compatible: existing rows default to `storage_mode='managed'` and `scan_status='idle'`. Behavior unchanged for all current archives.
- Zero-downtime: pure column additions, no rewrites, no locking that affects existing queries.
- Forward-compatible: future rollback could drop the new columns without data loss (existing archives never populated them).

### 3.2 Opt-In to Referenced Mode

Operators opt in per-archive at creation time. There is no in-place conversion of existing Managed archives to Referenced. If an operator wants to convert (rare), they drop the Managed archive and create a new Referenced one — synthesis §7 non-goal 9 explicitly forbids `source_path` migration.

### 3.3 Migration Sequence on Existing Deployment

1. Operator pulls new bundle release.
2. Operator updates `.env`:
   - Sets `FORTEMI_EXTERNAL_STORAGE_ENABLED=false` (initial deployment leaves the feature flagged off).
   - Optionally adds `FORTEMI_REFERENCED_STORAGE_ROOTS` for later use.
3. Operator runs `docker compose -f docker-compose.bundle.yml up -d`.
4. Bundle entrypoint runs the new migration. All existing archives remain Managed. No behavior change for users.
5. Operator validates: `curl http://localhost:3000/health` returns 200.
6. Operator soaks for an operator-defined period in Managed-only mode to confirm the upgrade path is clean.
7. Operator flips `FORTEMI_EXTERNAL_STORAGE_ENABLED=true` and restarts the bundle (per CLAUDE.md `.env` change procedure).
8. Operator creates first Referenced archive via API.

---

## 4. Rollout Strategy

### 4.1 Phased Rollout (recommended)

1. **Phase R-1 (Internal)**: Deploy to a non-production Fortemi instance. Validate against `with-mixed-media` and `small-repo` fixtures from the test plan. Verify scan completes, search returns results, drop preserves source.
2. **Phase R-2 (Single-user / desktop sidecar)**: Operator opt-in via `FORTEMI_EXTERNAL_STORAGE_ENABLED=true` with `FORTEMI_REFERENCED_STORAGE_ROOTS` empty (any path). One operator-managed Referenced archive against a local code repo. Soak period at operator's discretion.
3. **Phase R-3 (Multi-tenant)**: Combined with `FORTEMI_MULTI_TENANT=true`. Requires `FORTEMI_REFERENCED_STORAGE_ROOTS` set with tenant-scoped roots. All TI-EXTSTORAGE-* tests must be green in CI prior to this phase.

### 4.2 Feature Flag Behavior

When `FORTEMI_EXTERNAL_STORAGE_ENABLED=false`:
- `POST /api/v1/archives/referenced` returns HTTP 501 with body `{"error": "Referenced storage not enabled. Set FORTEMI_EXTERNAL_STORAGE_ENABLED=true to enable."}`
- `POST /api/v1/archives/{name}/rescan` for any existing Referenced archive returns HTTP 501 with the same message.
- `GET /api/v1/search` for Referenced archives still works (read path is preserved so flipping the flag off doesn't break in-place indexes).
- MCP `manage_archives` with `storage_mode='referenced'` returns the same 501 propagated to the agent.

This asymmetry — read works, write doesn't — is the rollback mode (see §5).

---

## 5. Rollback Plan

### 5.1 Soft Rollback (recommended for any production issue short of data corruption)

Flip `FORTEMI_EXTERNAL_STORAGE_ENABLED=false` and restart. No new Referenced archives can be created. Existing Referenced archives become read-only (rescan refused, but search continues). Operator can investigate the issue without data loss.

```bash
# In .env
FORTEMI_EXTERNAL_STORAGE_ENABLED=false

# Restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### 5.2 Hard Rollback (data-corruption emergency only)

Drop all Referenced archives via API (each `DELETE /api/v1/archives/{name}` removes only the schema + companion derived dir, never the source). Then optionally roll back the migration (drop the new columns). Source data on the filesystem is untouched throughout, guaranteed by synthesis §2.3 constraint 6 (`drop_archive_schema` safe for Referenced).

```bash
# Step 1: List Referenced archives
curl -H "Authorization: Bearer ${TOKEN}" http://localhost:3000/api/v1/archives | \
  jq '.[] | select(.storage_mode == "referenced") | .name'

# Step 2: Drop each
for archive in $(curl ... | jq ...); do
  curl -X DELETE -H "Authorization: Bearer ${TOKEN}" \
    http://localhost:3000/api/v1/archives/${archive}
done

# Step 3: Verify source dirs untouched (sample check)
ls -la /srv/fortemi/referenced/...  # contents and mtimes unchanged

# Step 4 (optional, only if columns are causing problems): drop columns
# Apply a rollback migration via the migrations system
```

### 5.3 What Rollback Does NOT Do

- Does NOT delete or modify the user's source directories. Ever.
- Does NOT clear existing scan-status or quarantine-log records (these are managed via per-archive schemas, dropped only when the archive is dropped).
- Does NOT remove the new `.env` variables — operator removes those manually if desired.

---

## 6. Operational Runbook Stubs

Full runbooks live in `docs/runbooks/` (created during construction phase). Stubs below:

### 6.1 Adding a Referenced Archive (Operator)

```text
Procedure: Add a Referenced archive
Purpose: Index a local directory in place without copying files into Fortemi
Prereq: FORTEMI_EXTERNAL_STORAGE_ENABLED=true, source path readable by container uid

1. Confirm source path is mounted into the API container:
   docker compose -f docker-compose.bundle.yml exec fortemi-api ls -la /srv/fortemi/referenced/
   Expected: contents of host source directory visible.

2. If multi-tenant: confirm path is under FORTEMI_REFERENCED_STORAGE_ROOTS.

3. Create the archive via API:
   curl -X POST -H "Authorization: Bearer ${TOKEN}" \
     -H "Content-Type: application/json" \
     -d '{"name":"my-code","source_path":"/srv/fortemi/referenced/my-code"}' \
     http://localhost:3000/api/v1/archives/referenced
   Expected: HTTP 202, response includes job_id.

4. Poll scan-status:
   curl -H "Authorization: Bearer ${TOKEN}" \
     http://localhost:3000/api/v1/archives/my-code/scan-status
   Expected: status transitions idle → scanning → idle. For a 1k-file repo this
   completes in <2 minutes on default CPU embedding.

5. Verify search works:
   curl -X POST -H "Authorization: Bearer ${TOKEN}" \
     -d '{"query":"function foo","archive":"my-code"}' \
     http://localhost:3000/api/v1/search
   Expected: results returned with archive attribution.

6. Inspect quarantine log:
   curl -H "Authorization: Bearer ${TOKEN}" \
     http://localhost:3000/api/v1/archives/my-code/quarantined-files
   Expected: list of skipped files (if any), with reasons.
```

### 6.2 Triggering a Manual Rescan

```text
Procedure: Rescan a Referenced archive
Purpose: Pick up new/changed/deleted files since last scan
Prereq: Archive exists and is Referenced

curl -X POST -H "Authorization: Bearer ${TOKEN}" \
  http://localhost:3000/api/v1/archives/my-code/rescan
Expected: HTTP 202 with job_id; subsequent scan-status poll shows scanning then idle.

If scan-status returns 'error', inspect /var/log/fortemi/jobs.log for the cause.
Most common: source path unreachable (mount disappeared) — see 6.4.
```

### 6.3 Investigating a Quarantine Event

```text
Procedure: Investigate why a file was quarantined
Purpose: Audit secret-scan decisions; recover false positives

1. List quarantined files for the archive:
   curl -H "Authorization: Bearer ${TOKEN}" \
     http://localhost:3000/api/v1/archives/my-code/quarantined-files | jq .

2. Each entry has: path, reason, detected_at.
   Reasons map to synthesis §3 Decision 7 categories:
     - path_denylist:<pattern_name>
     - content_match:pem_private_key_header
     - content_match:aws_access_key
     - content_match:github_pat
     - content_match:jwt_pattern
     - symlink_out_of_root
     - permission_denied
     - file_too_large

3. If quarantine is a FALSE POSITIVE:
   v1 has no per-file override. Operator must either:
     a) Move the file out of the source dir for the next rescan.
     b) Rename the file to avoid the path denylist (if reason is path_denylist).
     c) Adjust scan_config.additional_ignores OR petition for v2 false-positive override.

4. If quarantine is a TRUE POSITIVE:
   - The file is correctly skipped. No action needed in Fortemi.
   - Investigate why the file is in a code repo (likely a secret that shouldn't
     be there in the first place — file an issue with the source repo owner).
```

### 6.4 Handling a "Mount Disappeared" Alert

```text
Procedure: Recover from source-path unavailability
Purpose: Restore scan capability after the underlying mount drops

1. Confirm mount state from inside the container:
   docker compose -f docker-compose.bundle.yml exec fortemi-api ls /srv/fortemi/referenced/
   If listing fails: mount is gone.

2. Re-mount the source on the host:
   mount /mnt/nfs/source-path  # or equivalent for the operator's setup

3. Restart the bundle to re-read the bind mount:
   docker compose -f docker-compose.bundle.yml restart fortemi-api fortemi-jobs

4. Verify the archive's scan-status transitions back to 'idle' on next rescan:
   curl -X POST .../api/v1/archives/my-code/rescan
   curl ... .../api/v1/archives/my-code/scan-status

5. If the underlying mount is permanently lost:
   Decision: keep stale index OR delete archive.
   - Stale index: reads still work (lenient mode, synthesis Decision 8). Document the staleness.
   - Delete: DELETE /api/v1/archives/my-code — source files never touched, derived dir cleaned up.
```

---

## 7. Monitoring Additions

### 7.1 New Metrics

Exposed via the existing Fortemi metrics endpoint (Prometheus format or equivalent — match the existing convention in matric-api):

| Metric Name | Type | Labels | Description |
|---|---|---|---|
| `fortemi_scan_jobs_total` | counter | `archive_name`, `outcome` (`success`/`error`) | Total scan jobs initiated |
| `fortemi_scan_job_duration_seconds` | histogram | `archive_name` | Wall-clock scan duration |
| `fortemi_scan_files_processed_total` | counter | `archive_name` | Files walked per scan |
| `fortemi_scan_files_quarantined_total` | counter | `archive_name`, `reason` | Files skipped by secret scan or denylist |
| `fortemi_scan_bytes_processed_total` | counter | `archive_name` | Bytes hashed during scan |
| `fortemi_scan_failure_rate` | gauge | (none) | Rolling 1h percentage of scan jobs ending in error |
| `fortemi_referenced_archives_total` | gauge | (none) | Count of archives with `storage_mode='referenced'` |
| `fortemi_source_path_unreachable_total` | counter | `archive_name` | Count of read attempts that hit unreachable source (Decision 8 lenient mode) |

### 7.2 New Alert Thresholds

| Alert | Condition | Severity |
|---|---|---|
| `ScanFailureRateHigh` | `fortemi_scan_failure_rate > 0.05` for 1h (5% failure rate) | warning |
| `ScanJobStuck` | scan job in `scanning` state for >30 minutes | warning |
| `SourcePathUnreachable` | `fortemi_source_path_unreachable_total` increases by >10/min | critical |
| `QuarantineRateSurge` | `fortemi_scan_files_quarantined_total` rate suddenly increases >5x baseline | info (could indicate new secret-containing repo OR a regex false-positive update) |
| `DerivedStorageDiskUsage` | derived-storage volume >80% full | warning |

These alerts integrate with whatever monitoring stack the operator is running. The exact wire-up depends on Fortemi's existing metrics export pattern (which the construction phase will inspect during WS-7 implementation).

---

## 8. References

- @.aiwg/working/issue-planner-storage/synthesis.md — §3 Decisions (esp. 1, 2, 3, 8), §6 Open Questions Q-5/Q-7/Q-8, §7 non-goals
- @.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md — 12-factor compliance, health checks, disposability
- @.aiwg/working/issue-planner-storage/testing/test-strategy.md — fixture/CI coupling
- @CLAUDE.md — Docker Bundle commands, Hardware Profiles, MAX_MEMORIES scaling
- @.claude/rules/config-in-environment.md — env-var-first configuration
- @.claude/rules/disposable-processes.md — graceful shutdown for scan worker
- @.claude/rules/stateless-processes.md — scan worker state lives in PostgreSQL
- @.claude/rules/it-service-health.md — health check requirements
- @.claude/rules/it-dr-validation.md — DR procedure validation (existing Fortemi DR plan extended for Referenced)

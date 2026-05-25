# Operational Readiness Checklist: Referenced Storage Mode

**Issue**: fortemi/fortemi#736
**Phase**: Phase 3 — SDLC Corpus Generation
**Date**: 2026-05-21
**Sole input**: `@.aiwg/working/issue-planner-storage/synthesis.md`

This checklist gates production deployment of the Referenced storage epic per Fortemi's project-wide operational standards (12-factor compliance, health checks, disposability, stateless processes, structured logs, environment-configured). Each item is a hard requirement before the GATE-C2T (Construction → Transition) gate per `.claude/rules/hitl-gates.md`.

The checklist is organized by component since this epic introduces new code paths in three places: the API (WS-7), the scan-and-ingest worker (WS-4), and the MCP server (WS-8). Each component is held to the same operational bar.

---

## 1. Health Checks

Per `.claude/rules/it-service-health.md`: every deployed component must have liveness, readiness, and deep-health checks; expected healthy responses documented; checks must be automatable and side-effect-free.

### 1.1 fortemi-api Container (existing + new endpoints)

- [ ] **Liveness**: `GET /health` returns HTTP 200 if the process is running. (Existing — no change.)
- [ ] **Readiness**: `GET /api/v1/health/ready` returns HTTP 200 only when the API can accept requests. New requirement: must include a check that the configured `FORTEMI_REFERENCED_SOURCE_ROOTS` mount(s) are accessible if `FORTEMI_EXTERNAL_STORAGE_ENABLED=true`. If not accessible, return 503.
- [ ] **Deep health**: `GET /api/v1/health/deep` includes a check of:
  - PostgreSQL connectivity (existing)
  - Redis connectivity (existing)
  - The companion derived-storage path is writable (new)
  - Each path in `FORTEMI_REFERENCED_STORAGE_ROOTS` is readable (new)
  - Returns aggregated 200 if all checks pass; 503 with detail body otherwise.

### 1.2 Scan-and-Ingest Worker

- [ ] **Liveness**: Worker process responds to a liveness signal (existing job-system convention; if matric-jobs uses a heartbeat in PostgreSQL, the heartbeat must continue updating).
- [ ] **Readiness**: Worker is ready when it can dequeue and process a job from the `DirectoryScan` queue. Verified by submitting a no-op scan job at startup (against an empty fixture directory or a dedicated readiness test path) and confirming it completes.
- [ ] **Deep health**: Worker reports last-completed-job timestamp; alert fires if >24h with no job completion when jobs are queued.

### 1.3 MCP Server

- [ ] **Liveness**: `GET /health` on port 3001 returns 200 (existing).
- [ ] **Readiness**: MCP server reports ready when it can list tools (i.e., tool registration completed). New tools `manage_archives` (extended) and `rescan_archive` must appear in the listing.
- [ ] **Tool schema validation**: MCP Inspector run as part of CI confirms all tool JSON schemas (including new ones) are valid.

---

## 2. Disposability / Graceful Shutdown

Per `.claude/rules/disposable-processes.md`: processes must shut down gracefully on SIGTERM, finishing in-flight work within a grace window.

### 2.1 fortemi-api

- [ ] SIGTERM handler stops accepting new HTTP requests.
- [ ] In-flight HTTP requests complete within a 30-second grace window before SIGKILL.
- [ ] Connection pools (PG, Redis) drain cleanly.

(All existing for current Fortemi API — verify no regression with the new endpoints. The new endpoints follow the same axum-based pattern as existing routes, so this should hold without code changes.)

### 2.2 Scan-and-Ingest Worker (new)

- [ ] SIGTERM handler:
  - Stops dequeuing new `DirectoryScan` jobs.
  - For an in-flight scan: completes the current file's hash + insert, then checkpoints progress (updates `archive_registry.scan_status='scanning'` with a `last_processed_path` field stored in `scan_config`).
  - Exits within 30 seconds.
- [ ] On worker restart, scan resumes from the checkpointed path:
  - Re-walks the directory (cheap; the BLAKE3 dedup at the blob layer makes re-hashing existing files idempotent).
  - Resumes inserting blobs/chunks for files not yet processed.
  - Synthesis §2.1 finding 4 (BLAKE3 dedup) makes this naturally safe.
- [ ] **Worst case verified**: SIGKILL mid-scan leaves the archive in a consistent state because:
  - Each file's `(blob INSERT, attachment INSERT, extraction job enqueue)` happens in a single transaction per file.
  - Partial transactions roll back; the next scan picks up cleanly.
  - `scan_status` remains 'scanning'; an admin can flip to 'error' if the worker is permanently lost.

### 2.3 MCP Server

- [ ] SIGTERM handler stops accepting new tool calls, completes in-flight calls within grace window. (Existing for Node MCP server pattern — verify no regression.)

---

## 3. Stateless Processes

Per `.claude/rules/stateless-processes.md`: process state lives in backing services (PostgreSQL); local disk used only for declared volume mounts and tmpfs.

### 3.1 Scan Worker State

- [ ] **Job state**: lives in PostgreSQL via the existing `jobs` table — no in-memory job queue.
- [ ] **Scan progress**: stored in `archive_registry.scan_status` and `archive_registry.scan_config` (e.g., last-processed path for resumption).
- [ ] **No local-disk writes outside declared mounts**:
  - Reads source files from `/srv/fortemi/referenced/` (read-only bind mount).
  - Writes derived artifacts to `/data/derived/` (declared `fortemi-derived` volume).
  - Tmpfiles (during streaming hash for very large files) use `/tmp` only and are cleaned up post-hash.
- [ ] **No module-level state**: scan-and-ingest is pure-function-style — every job carries the archive context, no globals.

### 3.2 No In-Memory Caches That Survive Restart

- [ ] BLAKE3 hashes are NOT cached in-process across scans. Each scan re-hashes (cheap, idempotent at blob layer). This eliminates the need for cache invalidation logic.
- [ ] Walker output is NOT cached. Each scan re-walks. The `ignore` crate's directory cache is per-walker-instance, discarded when the scan completes.

---

## 4. Logs as Event Streams

Per `.claude/rules/logs-as-event-streams.md`: structured JSON logs to stdout, with correlation IDs.

### 4.1 New Log Events Introduced by This Epic

All new log entries use the existing Fortemi structured logging convention (whatever JSON shape matric-api already emits — likely `tracing` crate with JSON formatter). Required fields per event:

- `timestamp`, `level`, `message`, `trace_id` (existing required fields, no change)
- `archive_name` (string) — for any scan/ingest event
- `scan_job_id` (UUID) — for scan-and-ingest events
- `operation` (string) — one of: `scan_started`, `scan_completed`, `scan_failed`, `file_quarantined`, `file_ingested`, `source_unreachable`, `derived_artifact_written`, `archive_dropped`

### 4.2 Logging Requirements

- [ ] All scan-job lifecycle transitions emit a log event with correlation to the originating API request's `trace_id`.
- [ ] Quarantine events include the file path AND the reason (denylist pattern matched, content match type) but DO NOT include the file's content (synthesis §3 Decision 5 — never log secret material itself).
- [ ] Source-unreachable events include the archive name and the underlying error (mount errno) — never the full path if it would expose tenant data.
- [ ] Log level for routine events: `info`. For quarantine: `warn`. For source-unreachable: `error`. For drop: `info`.
- [ ] Worker logs to stdout, NOT to a file. Container runtime aggregates.

---

## 5. Config in Environment

Per `.claude/rules/config-in-environment.md`: env-configured; `.env.example` documents every variable; validation at startup.

### 5.1 Env-Var Documentation

- [ ] `.env.example` documents every new variable from Deployment Plan §2.1 with:
  - Default value
  - Whether required
  - Cross-reference to the operator guide
- [ ] No hardcoded path defaults in source code beyond the documented `${FILE_STORAGE_PATH}/derived/` companion location.

### 5.2 Startup Validation

- [ ] If `FORTEMI_EXTERNAL_STORAGE_ENABLED=true` AND `FORTEMI_MULTI_TENANT=true` AND `FORTEMI_REFERENCED_STORAGE_ROOTS` is empty/unset: refuse to start with a loud error. (Per synthesis Q-5 recommendation C.)
- [ ] If `FORTEMI_EXTERNAL_STORAGE_ENABLED=true` AND `FORTEMI_DERIVED_STORAGE_PATH` is unwritable: refuse to start.
- [ ] If `FORTEMI_REFERENCED_STORAGE_ROOTS` is set, each path is canonicalized and verified to exist at startup; refuse to start if any path is missing.
- [ ] If `FORTEMI_SCAN_WORKER_THREADS` is set to a value >32 or <1: refuse to start with a clear error.
- [ ] Validation errors fail fast; do NOT proceed with degraded mode.

### 5.3 No Per-Environment Branches in Business Logic

- [ ] Scan-and-ingest logic does NOT branch on `APP_ENV` or similar. All env-specific behavior is configured via the documented env vars above.

---

## 6. Twelve-Factor Compliance Checklist

| Factor | Status | Notes |
|---|---|---|
| I. Codebase | n/a-existing | One codebase tracked in git; this epic adds files only. |
| II. Dependencies | [ ] | New Rust dep `ignore = "0.4"` added to workspace (synthesis WS-3); `Cargo.lock` updated. |
| III. Config | [ ] | All new behavior gated on env vars per §5 above; no hardcoded values. |
| IV. Backing services | [ ] | Source filesystem treated as an attached resource via bind mount; companion derived volume is named volume; PostgreSQL and Redis unchanged. |
| V. Build, release, run | n/a-existing | Docker bundle's build-release-run separation unchanged. |
| VI. Stateless processes | [ ] | Scan worker is stateless per §3 above. |
| VII. Port binding | n/a-existing | No new ports introduced. API stays 3000, MCP stays 3001. |
| VIII. Concurrency | [ ] | Scan worker scales horizontally — multiple worker instances can dequeue different scan jobs concurrently (no per-archive locking conflict because jobs are scoped by archive_name and the worker writes only to that archive's schema). |
| IX. Disposability | [ ] | Per §2 above. |
| X. Dev/prod parity | [ ] | Local dev uses the same bundle and mounts the same way (volume name pointing at a local dir). |
| XI. Logs as event streams | [ ] | Per §4 above. |
| XII. Admin processes | [ ] | Rescan, drop-archive, quarantine-inspection are all API operations — no separate admin binary needed. |

---

## 7. Pre-Production Gate Checklist

All boxes below must be checked before this epic transitions through GATE-C2T (Construction → Transition) per `.claude/rules/hitl-gates.md`.

- [ ] Test strategy (§test-strategy.md) implemented; aggregate coverage of new code ≥85%
- [ ] All TI-EXTSTORAGE-1 through TI-EXTSTORAGE-10 tests passing in CI
- [ ] Performance smoke benchmark (BENCH-1) baseline recorded; <10 minute target for 10k-file scan verified on `matric-builder` runner
- [ ] Deployment plan §1-7 implemented in `docker-compose.bundle.yml` and `installer/scripts/`
- [ ] `.env.example` updated with all new variables and inline documentation
- [ ] Health-check endpoints §1.1, §1.2, §1.3 implemented and returning correct responses
- [ ] SIGTERM graceful shutdown verified for scan worker (§2.2) — manual test with mid-scan SIGTERM, restart, confirm clean state
- [ ] Operator guide `docs/referenced-storage.md` written and reviewed
- [ ] Runbook stubs (§deployment-plan.md §6.1-6.4) fleshed out into full runbooks in `docs/runbooks/`
- [ ] Metrics (§deployment-plan.md §7.1) wired into existing Fortemi metrics export
- [ ] Alerts (§deployment-plan.md §7.2) configured in the operator's monitoring stack
- [ ] Rollback procedure tested end-to-end (§deployment-plan.md §5.1 and §5.2) on the non-production instance
- [ ] Source-preservation invariant manually verified: drop a Referenced archive, confirm source dir contents and mtimes unchanged
- [ ] Secret-scan default denylist (synthesis §3 Decision 7) reviewed by Security Architect agent or human reviewer
- [ ] Path-traversal protections (TI-EXTSTORAGE-2, TI-EXTSTORAGE-7) verified by manual red-team session, not just automated tests
- [ ] Bundle entrypoint validates new env vars per §5.2 at startup; refuses bad configurations
- [ ] doc-sync skill passes against all new docs (no broken @-mentions)
- [ ] CHANGELOG entry added per Fortemi release procedure
- [ ] Migration tested on a backed-up production PG snapshot (no data loss, no downtime)
- [ ] Feature flag defaults to OFF (`FORTEMI_EXTERNAL_STORAGE_ENABLED=false`) in `.env.example`

---

## 8. References

- @.aiwg/working/issue-planner-storage/synthesis.md — §3 Decisions, §6 Open Questions, §7 non-goals
- @.aiwg/working/issue-planner-storage/deployment/deployment-plan.md — env vars, mounts, runbooks, metrics
- @.aiwg/working/issue-planner-storage/testing/test-strategy.md — coverage and CI gates
- @.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md — TI-EXTSTORAGE-*
- @CLAUDE.md — Docker Bundle, Authentication, Multi-Memory Architecture
- @.claude/rules/it-service-health.md — health check requirements
- @.claude/rules/disposable-processes.md — SIGTERM, graceful shutdown
- @.claude/rules/stateless-processes.md — state in PostgreSQL, not local
- @.claude/rules/logs-as-event-streams.md — structured JSON logs to stdout
- @.claude/rules/config-in-environment.md — env-var-first config, startup validation
- @.claude/rules/hitl-gates.md — GATE-C2T criteria
- @.claude/rules/it-dr-validation.md — DR test discipline

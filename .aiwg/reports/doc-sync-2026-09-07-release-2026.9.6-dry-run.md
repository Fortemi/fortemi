# Doc-sync dry run: release/2026.9.6

- Timestamp: 2026-09-07T00:00:00-04:00
- Worktree: `/tmp/fortemi-release-2026.9.6`
- Head: `ab24adca237e27aa117979de2a966e491eeb6d99`
- AIWG artifact root: `/tmp/fortemi-release-2026.9.6/.aiwg`
- Release config: `.aiwg/release.config`
- Selected AIWG skill: `doc-sync` (`aiwg:skill:89d1e9d44a6d896e`)
- Selected AIWG flow: `release-doc-sync` (`aiwg:flow:b7fad3d5008601ca`)
- Direction: code-to-docs
- Scope: startup, MCP credential readiness, automatic pre-migration recovery, and February-to-current migration gate documentation only.

## Code delta reviewed

Compared `d4220924..ab24adca`, the scoped implementation changed:

- `docker/bundle-entrypoint.sh`: pre-migration recovery helper delegation, postgres-owned backup/staging setup, API readiness timeout/progress loop, bounded curl calls, MCP credential validation/registration failure handling, and PID1 cleanup behavior.
- `scripts/pre-migration-recovery.py`: snapshot-bound recovery point reuse, metadata binding, exclusive lock, memory-backed staging with fallback, retention after final verification, and fail-closed invalidation.
- `scripts/backup.sh`: explicit private scratch requirement, pg_dump exported snapshot support, verification before retention, deferred retention mode, and optional recovery metadata publication.
- `crates/matric-db` tests and fixture scripts: February-to-current migration restore/resume coverage.

## Dry-run findings

1. `docs/ops/feb-to-current-upgrade-runbook.md` already documents the reviewed recovery helper design, including exported snapshots, bounded streaming fingerprinting, materialized-view state, sequence limitations, no persistent database read-only setting, and retention after final verification. Only minor wording review is needed.
2. `README.md` documents extended API readiness and Windows Docker Desktop guidance, but it does not yet summarize automatic pre-migration recovery point behavior, memory/staging headroom, retention-after-validation, or the exact container-only Windows limit.
3. `docs/content/configuration.md` lists `FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS`, but it lacks the related bundle backup/recovery settings: `BACKUP_DEST`, `BACKUP_TEMP_DIR`, `BACKUP_TEMP_TRUSTED_ENCRYPTED`, `PRE_MIGRATION_BACKUP_RETAIN`, `PRE_MIGRATION_BACKUP_ACK_NO_BACKUP`, and the bundle-only limitations.
4. `docs/content/mcp-deployment.md` already matches the MCP credential readiness code for API-unavailable preservation and timeout/progress behavior. It should cross-link long first-boot migrations to the pre-migration recovery/runbook guidance and state that registration is skipped until `/health` succeeds.
5. No version, changelog, announcement, roadmap, or product parity documentation is in scope for this doc-sync pass.

## Planned doc edits after dry run

- Add a concise README subsection under Docker Bundle startup describing automatic pre-migration recovery points, reuse max-age semantics, no database-wide read-only mode, staging memory/fallback behavior, and Windows Docker Desktop/WSL2 limits.
- Add a configuration subsection for bundle pre-migration recovery variables and platform constraints.
- Add a short MCP deployment cross-reference for long first-boot migrations and credential preservation.
- Preserve the upgrade runbook’s existing fail-closed details and avoid broader backup/parity claims.

# Doc-sync report: release/2026.9.6

- Timestamp: 2026-09-07T00:00:00-04:00
- Worktree: `/tmp/fortemi-release-2026.9.6`
- Head: `ab24adca237e27aa117979de2a966e491eeb6d99`
- AIWG artifact root: `/tmp/fortemi-release-2026.9.6/.aiwg`
- Release config: `.aiwg/release.config`
- Selected AIWG skill: `doc-sync` (`aiwg:skill:89d1e9d44a6d896e`)
- Selected AIWG flow: `release-doc-sync` (`aiwg:flow:b7fad3d5008601ca`)
- Direction: code-to-docs
- Dry-run receipt: `.aiwg/reports/doc-sync-2026-09-07-release-2026.9.6-dry-run.md`

## Scope

This pass reconciled documentation for changed startup, recovery, MCP readiness, and February-to-current migration behavior only. Version bumps, changelog entries, release announcements, roadmap text, and shard artifact mutation were intentionally left for the parent release workflow.

## Documentation changes

- `README.md`: added Docker bundle pre-migration recovery behavior under Quick Start, including verified reuse bindings, default max-age semantics, no persistent database-wide read-only mode, staging memory/fallback guidance, retention-after-validation, and Windows Docker Desktop/WSL2 limits.
- `docs/content/configuration.md`: added a Docker bundle pre-migration recovery configuration section covering `BACKUP_DEST`, `BACKUP_TEMP_DIR`, `BACKUP_TEMP_TRUSTED_ENCRYPTED`, `PRE_MIGRATION_BACKUP_RETAIN`, `PRE_MIGRATION_BACKUP_ACK_NO_BACKUP`, and `FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS`.
- `docs/content/mcp-deployment.md`: linked extended first-boot MCP readiness behavior to the pre-migration recovery/runbook path and clarified that credential validation/registration waits for `/health`.
- `docs/ops/feb-to-current-upgrade-runbook.md`: kept the existing fail-closed snapshot/reuse design and added the Linux-container/Windows Docker Desktop scope limit.

## Code-to-docs consistency checks

- The docs describe PostgreSQL exported read-only snapshots used by the helper, not a persistent database-wide read-only setting. This matches `scripts/pre-migration-recovery.py` and `docker/bundle-entrypoint.sh`.
- The docs preserve fail-closed claims for metadata, artifact, migration manifest, schema/data/large-object fingerprint, materialized-view state, and observed sequence movement, without claiming quiescence or complete prevention of sequence writes.
- The docs state the reuse max-age default as `86400` seconds and `0` as “disable reuse,” matching `FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS` handling.
- The docs state that retention for pre-migration recovery artifacts occurs only after final verification and metadata publication, matching the helper/backup script flow.
- The Windows language is limited to Docker Desktop/WSL2 container behavior and does not claim the Windows-native HotM/MSI path runs the bundle shell/Python recovery helpers.

## Validation

```text
$ git diff --check
pass

$ python3 scripts/ci/verify-docs-shard-freshness.py
documentation shard freshness check passed: server=2026.9.5, bytes=2587104, sha256=3c7ee370b52cc1f5a5e48c9d322b0f6fef342b18bc02b195358a0965a8802b41

$ node scripts/ci/docs-contract.cjs --profile hosted_strict
docs-contract profile=hosted_strict mode=advisory findings=0 known=0 new=0 baseline=scripts/ci/docs-contract.baseline.json stale_baseline=0 classifications=none
```

## Documentation shard handoff

No shard files were mutated in this doc-sync pass. The canonical CI-backed regeneration command is:

```bash
scripts/ci/rebuild-shard-in-ci.sh <api-image-tag>
```

Prerequisites from `scripts/ci/rebuild-shard-in-ci.sh`:

- Run from the repository root with Docker available.
- `<api-image-tag>` must be a locally loaded freshly built API image for the same source revision, normally `${ env.IMAGE }:${ env.VERSION }` in the publish workflow.
- The script builds `matric-testdb:shard-rebuild` from `build/Dockerfile.testdb`, starts an isolated Postgres + API stack, waits for `/health`, runs `scripts/rebuild-docs-shard.sh <API_URL>`, and writes `docker/seed-data/fortemi-docs.shard.receipt.json` via `scripts/ci/write-docs-shard-receipt.py`.
- `scripts/rebuild-docs-shard.sh` imports tracked `docs/**/*.md`, `.aiwg/**/*.md`, `CHANGELOG.md`, and `README.md` only, then writes `docker/seed-data/fortemi-docs.shard`.

The freshness verification command is:

```bash
python3 scripts/ci/verify-docs-shard-freshness.py
```

That check verifies the committed shard SHA-256/byte length against the receipt and ensures the shard/receipt server version matches the workspace package version. After parent applies final version/changelog/announcement edits, regenerate the shard with the command above and rerun the freshness check before bundle publication.

## Current modified files

```text
README.md
docs/content/configuration.md
docs/content/mcp-deployment.md
docs/ops/feb-to-current-upgrade-runbook.md
```

## Remaining items

- Parent release workflow should apply final version/changelog/announcement edits.
- Parent release workflow should regenerate `docker/seed-data/fortemi-docs.shard` and receipt after all final docs are complete.

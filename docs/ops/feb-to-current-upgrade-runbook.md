# 2026.2.x to Current Bundle Upgrade Runbook

This runbook is the release gate for the February 2026 GHCR backlog upgrade.
External bundle users may jump from `2026.2.x` to the current image in one
startup, so treat the first boot as a schema migration window.

## Preconditions

- Disk headroom: at least 2x the PostgreSQL data directory plus WAL growth.
- Maintenance window: start with 30 minutes per 100k notes, then adjust from a
  staging run using representative attachment and embedding volume.
- Backups: keep the automatic pre-migration dump under
  `/var/backups/matric-memory/` and copy it off-host before continuing.
- Timeouts: use conservative PostgreSQL settings for the migration window:
  `lock_timeout=30s` and `statement_timeout=30min` are the default guidance for
  large self-hosted systems. Increase `statement_timeout` only after staging
  shows a specific migration needs it.

## Automatic Pre-Migration Backup

On bundle startup, `docker/bundle-entrypoint.sh` first applies the known
schema-only restore compatibility repair for the legacy Feb 3 SKOS/pgvector
trigger, then checks for both a non-empty database and pending migrations. When
both are true, it runs `/app/scripts/backup.sh` before checksum repair or
`sqlx::migrate!()` can run data/schema migrations.

The backup name is:

```text
pre-migration-<from>-<to>-<timestamp>.sql.gz
```

The entrypoint fails closed if the dump cannot be written or verified. Override
only when an operator has an external recovery point:

```bash
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=true
```

The backup script verifies non-zero size, parses the custom-format dump with
`pg_restore --list`, and logs a SHA-256 checksum.

The release fixture also restores the generated dump into a fresh database and
checks the seeded `note_original` count before running current migrations.

## Seeded Large-Data Gate

Before reopening GHCR publishing, run a staging upgrade from a `2026.2.x` data
directory seeded with representative production scale:

- If the release target includes literal Feb 3 users, include an exact
  `v2026.2.0` baseline gate. The first 100k-note fixture starts at
  `20260215000000`; it does not prove the `v2026.2.0` migration checksum state.
  See `docs/ops/feb-3-baseline-migration-audit-2026-07-12.md`.
- Minimum seed: 100k notes, representative revisions, chunks, embeddings, jobs,
  archives, inbound sources, and webhook/outbox rows.
- Capture per-migration wall-clock time from PostgreSQL logs or CI telemetry.
- Capture lock waits and WAL growth during the migration window.
- Re-run the entrypoint after a deliberate mid-batch failure and confirm sqlx
  resumes from the last successful migration.

Generate the static migration-risk inventory first:

```bash
python3 scripts/ci/analyze-post-feb-migrations.py > migration-risk-report.json
```

Attach the report to the release checklist so reviewers can compare the seeded
run's longest locks/WAL growth against the actual post-February migrations.
The current evidence file is
`docs/ops/feb-to-current-upgrade-evidence-2026-07-11.md`.

As of the restore-safe trigger forward migration, the current post-February
inventory is 43 migrations, 9 data/backfill-like statements, 36 index builds,
19 table-altering statements, and 3 migrations touching large-table names.

Release checklist assertion:

```text
2026.2.x seeded upgrade completed:
- seed profile:
- source migration:
- target migration:
- migration-risk-report:
- backup path + sha256:
- longest migration:
- peak WAL growth:
- restore drill result:
```

The scripted gate is:

```bash
FORTEMI_RUN_LARGE_MIGRATION_GATE=true \
FORTEMI_MIN_SEEDED_NOTES=100000 \
DATABASE_URL=<staged-2026.2.x-database-url> \
cargo test -p matric-db --features migrations \
  --test feb_to_current_migration_gate -- --ignored --nocapture
```

The test refuses undersized seeds, runs the current migration set, then runs
migrations a second time to prove idempotent resume behavior after the first
successful pass.

For a self-contained local fixture using the repository's PostgreSQL 18 test
image:

```bash
FORTEMI_SEED_NOTES=100000 \
scripts/ci/run-feb-to-current-fixture.sh
```

To reproduce the literal Feb 3 release baseline:

```bash
FORTEMI_FEB_BASELINE_TAG=v2026.2.0 \
FORTEMI_SEED_NOTES=100000 \
scripts/ci/run-feb-to-current-fixture.sh
```

The fixture applies migrations from either the selected release tag or through
the selected numeric baseline, records matching sqlx checksums, seeds
`note_original`/`note_revised_current`, creates and verifies a pre-migration dump
with `scripts/backup.sh`, restores that dump into a fresh database, runs the
ignored gate, and prints seed count, backup checksum, restore-drill row count,
target migration, duration, WAL bytes, longest migration from
`_sqlx_migrations.execution_time`, and sampled ungranted-lock count.

If a migration holds write-blocking locks longer than the maintenance window
allows on the seeded dataset, do not publish GHCR. Batch the backfill or document
a maintenance-window-only path before release.

## Restore From Pre-Migration Backup

1. Stop the bundle container.
2. Move the failed PostgreSQL data directory aside.
3. Initialize a fresh PostgreSQL 18 data directory with the same database name,
   user, and extensions.
4. Restore the dump:

   ```bash
   gzip -dc /var/backups/matric-memory/pre-migration-<from>-<to>-<timestamp>.sql.gz \
     | pg_restore --clean --if-exists --no-owner --dbname "$POSTGRES_DB"
   ```

5. Start the old image or restart the current image after fixing the failure.
   sqlx migration tracking resumes from the restored `_sqlx_migrations` table.

## Fresh Installs

Fresh bundle volumes create an empty database, skip the pre-migration backup
gate, and proceed through normal first-run migrations.

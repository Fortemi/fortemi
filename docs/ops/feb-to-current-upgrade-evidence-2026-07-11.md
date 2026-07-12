# Feb-to-Current Upgrade Gate Evidence - 2026-07-11

## Fixture

- Command: `FORTEMI_SEED_NOTES=100000 FORTEMI_TESTDB_CONTAINER=fortemi-feb-upgrade-100k-measured FORTEMI_TESTDB_PORT=55435 scripts/ci/run-feb-to-current-fixture.sh`
- Test image: `matric-testdb:local` from `build/Dockerfile.testdb`
- Baseline migration: `20260215000000`
- Target migration: `20260614140000`
- Seeded rows: `100000` in `note_original`
- Pre-migration backup: verified by `scripts/backup.sh` before current migrations ran

## Result

```text
2026.2.x seeded upgrade gate passed: before=20260215000000 after=20260614140000 notes=100000 elapsed_seconds=4
feb-to-current fixture completed
seed_notes=100000
baseline=20260215000000
target=20260614140000
pre_migration_backup=pre-migration-20260711T210328Z-20260215000000-fixture.sql.gz
pre_migration_backup_sha256=08dbe1c7a6ce75c4b237ef6bbf6fc43015becdbbb3efc6f3bd72297539265010
duration_seconds=4
wal_start_lsn=0/3A0EC000
wal_end_lsn=0/3FC6E870
wal_bytes=95955056
longest_migration=20260215200000	fts qualify config names	4096126257
max_ungranted_locks_sampled=0
```

The migration gate also ran `db.migrate()` a second time and passed, proving
the upgraded database is idempotent/resumable at the final migration head.

## Migration Risk Inventory

Generated with `python3 scripts/ci/analyze-post-feb-migrations.py`:

```text
post_feb_migrations=43
data_backfills=9
index_builds=36
table_alters=19
large_table_touches=3
```

## Interpretation

- The scripted 100k-note fixture completed the original 42-migration jump successfully.
- The pre-migration dump was created and verified before the migration gate ran.
- No ungranted lock waits were observed by the fixture sampler.
- The longest sqlx-recorded migration was `20260215200000_fts_qualify_config_names.sql`.
- WAL generated during the migration gate was `95955056` bytes.

## Scope Limitation

This evidence starts from migration `20260215000000`. It does not by itself
prove the literal Feb 3 `v2026.2.0` baseline. A follow-up audit on 2026-07-12
found and repaired earlier migration checksum drift plus duplicate-version
renumbering, added one forward restore-safety migration, then verified an exact
`v2026.2.0` 100k-note fixture through the current 43-migration post-February
inventory. See `docs/ops/feb-3-baseline-migration-audit-2026-07-12.md`.

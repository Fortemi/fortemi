#!/usr/bin/env python3
"""Static guard for the bundle pre-migration backup gate."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ENTRYPOINT = ROOT / "docker" / "bundle-entrypoint.sh"
BACKUP = ROOT / "scripts" / "backup.sh"
DOCKERFILE = ROOT / "Dockerfile.bundle"
RUNBOOK = ROOT / "docs" / "ops" / "feb-to-current-upgrade-runbook.md"
MIGRATION_GATE_TEST = ROOT / "crates" / "matric-db" / "tests" / "feb_to_current_migration_gate.rs"
SMOKE_TEST = ROOT / "scripts" / "ci" / "smoke-pre-migration-backup-gate.sh"
REUSE_SMOKE_TEST = ROOT / "scripts" / "ci" / "smoke-pre-migration-recovery-reuse.sh"
REALPG_SMOKE_TEST = ROOT / "scripts" / "ci" / "smoke-pre-migration-recovery-realpg.sh"
RECOVERY_HELPER = ROOT / "scripts" / "pre-migration-recovery.py"
MIGRATION_RISK = ROOT / "scripts" / "ci" / "analyze-post-feb-migrations.py"
FIXTURE_RUNNER = ROOT / "scripts" / "ci" / "run-feb-to-current-fixture.sh"
DB_LIB = ROOT / "crates" / "matric-db" / "src" / "lib.rs"
RESTORE_SAFE_TRIGGER_MIGRATION = ROOT / "migrations" / "20260614150000_restore_safe_skos_embedding_trigger.sql"
CI_BUILDER = ROOT / ".gitea" / "workflows" / "ci-builder.yaml"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"FAIL: {message}")


def main() -> None:
    entrypoint = ENTRYPOINT.read_text()
    backup = BACKUP.read_text()
    dockerfile = DOCKERFILE.read_text()
    runbook = RUNBOOK.read_text()
    migration_gate_test = MIGRATION_GATE_TEST.read_text()
    smoke_test = SMOKE_TEST.read_text()
    reuse_smoke_test = REUSE_SMOKE_TEST.read_text()
    realpg_smoke_test = REALPG_SMOKE_TEST.read_text()
    recovery_helper = RECOVERY_HELPER.read_text()
    migration_risk = MIGRATION_RISK.read_text()
    fixture_runner = FIXTURE_RUNNER.read_text()
    db_lib = DB_LIB.read_text()
    restore_safe_trigger_migration = RESTORE_SAFE_TRIGGER_MIGRATION.read_text()
    ci_builder = CI_BUILDER.read_text()

    backup_call = entrypoint.index("ensure_pre_migration_backup")
    checksum_repair = entrypoint.index("Checking migration checksum repair")
    require(
        backup_call < checksum_repair,
        "pre-migration backup must run before checksum repair",
    )
    require(
        "repair_legacy_restore_compatibility" in entrypoint
        and "OLD.embedding::text IS DISTINCT FROM NEW.embedding::text" in entrypoint
        and entrypoint.index("repair_legacy_restore_compatibility")
        < entrypoint.index("ensure_pre_migration_backup"),
        "bundle entrypoint must run legacy restore compatibility repair before backup",
    )
    require(
        "PRE_MIGRATION_BACKUP_ACK_NO_BACKUP" in entrypoint,
        "explicit no-backup acknowledgement env var is missing",
    )
    require(
        "verified pre-migration recovery point unavailable; aborting" in entrypoint
        and "verified pre-migration backup failed; aborting" in recovery_helper,
        "backup failure must abort startup before migrations",
    )
    require(
        'env.pop("PGPASSWORD", None)' in recovery_helper
        and 'env.pop("PGPASSFILE", None)' in recovery_helper
        and 'env["PGHOST"] = "/var/run/postgresql"' in recovery_helper
        and 'env["PGUSER"] = "postgres"' in recovery_helper
        and 'install -d -m 0700 -o postgres -g postgres "$BACKUP_DEST"' in entrypoint
        and 'install -d -m 0700 -o postgres -g postgres "${BACKUP_TEMP_DIR:-/dev/shm/fortemi-pre-migration-backup}"' in entrypoint
        and any(line == '    if ! runuser -u postgres -- env -u PGPASSWORD -u PGPASSFILE \\' for line in entrypoint.splitlines())
        and 'PGUSER=postgres' in entrypoint
        and 'PGHOST=/var/run/postgresql' in entrypoint
        and any(line == '        POSTGRES_DB="$POSTGRES_DB" \\' for line in entrypoint.splitlines())
        and '"PGHOST": env["PGHOST"]' in recovery_helper
        and '"PGPORT": env["PGPORT"]' in recovery_helper
        and '"PGDATABASE": env["PGDATABASE"]' in recovery_helper
        and '"PGHOST": os.environ.get("PGHOST"' not in recovery_helper
        and '"PGDATABASE": os.environ.get("POSTGRES_DB", os.environ.get("PGDATABASE"' not in recovery_helper,
        "bundle pre-migration backup must use postgres-owned backup/temp dirs and local postgres peer auth without a database secret or hostile ambient PG override",
    )
    require(
        '[[ "$PGHOST" == /* ]]' in backup
        and '[[ "$(id -un)" == "$PGUSER" ]]' in backup,
        "passwordless backup auth must be limited to a matching OS/DB user over a Unix socket",
    )
    require(
        "set -eo pipefail" in entrypoint or "set -euo pipefail" in entrypoint,
        "entrypoint must enable pipefail so backup failures are not hidden by tee",
    )
    require(
        "Pre-migration backup skipped: database has no user data" in entrypoint,
        "fresh/empty databases must skip the backup gate",
    )
    require(
        '"BACKUP_CLEANUP_PATTERN": "pre-migration-*.sql*"' in recovery_helper
        and "fcntl.flock" in recovery_helper
        and "choose_staging_dir" in recovery_helper
        and "pg_database_size" in recovery_helper
        and "DISK staging" in recovery_helper
        and 'BACKUP_RECOVERY_META_ENABLED": "false"' in recovery_helper
        and 'BACKUP_RETENTION_DEFERRED": "true"' in recovery_helper
        and "cleanup_old_recovery_points" in recovery_helper,
        "pre-migration helper must serialize retention, preserve large-DB staging fallback, and prevent premature backup.sh metadata",
    )
    require(
        "pg_restore --list" in backup and "sha256sum" in backup,
        "backup verification must parse the dump and log a checksum",
    )
    require(
        backup.index('verify_backup "$final_filename"', backup.index('main()')) < backup.index('cleanup_old_backups', backup.index('main()'))
        and 'BACKUP_RETENTION_DEFERRED' in backup,
        "backup.sh must verify before retention and allow helper-owned deferred retention",
    )
    require(
        "publish_recovery_metadata" in backup
        and "BACKUP_RECOVERY_META_ENABLED" in backup
        and ".recovery.meta" in backup
        and "mv -f \"$metadata_tmp\" \"$metadata_path\"" in backup,
        "backup.sh must atomically publish verified recovery metadata beside the artifact",
    )
    require(
        "pre-migration-*.sql*" in backup,
        "backup cleanup must remove pre-migration temp files",
    )
    require(
        "PRE_MIGRATION_RECOVERY_HELPER_PATH" in entrypoint
        and "verified pre-migration recovery point unavailable; aborting" in entrypoint
        and "default_transaction_read_only" not in entrypoint
        and "pg_terminate_backend" not in entrypoint,
        "entrypoint must delegate recovery reuse/create to the snapshot helper without persistent read-only settings or client termination",
    )
    require(
        "pg_export_snapshot" in recovery_helper
        and "BACKUP_PG_DUMP_SNAPSHOT" in recovery_helper
        and "--snapshot=" in recovery_helper
        and "--schema-only" in recovery_helper
        and "COPY (" in recovery_helper
        and "xmin::text" in recovery_helper
        and "to_jsonb(t)::text" in recovery_helper
        and 'COLLATE "C"' in recovery_helper
        and "pg_sequences" in recovery_helper
        and "is_called" in recovery_helper
        and "pg_largeobject" in recovery_helper
        and "LOCK TABLE" in recovery_helper
        and "relkind IN ('r', 'm')" in recovery_helper
        and "relispopulated" in recovery_helper
        and "unpopulated-materialized-view" in recovery_helper
        and "validate_final_state" in recovery_helper
        and "final_exporter, final_snapshot = export_snapshot(db)" in recovery_helper
        and "remove_created_artifact" in recovery_helper
        and "final_exporter: subprocess.Popen[str] | None = None" in recovery_helper
        and "if final_exporter is not None" in recovery_helper
        and "database contents changed during recovery backup creation" in recovery_helper
        and "lock_timeout" in recovery_helper
        and "sequence moved during recovery fingerprint" in recovery_helper
        and "FORTEMI_RECOVERY_SEQUENCE_TEST_HOOK" not in recovery_helper
        and "shell=True" not in recovery_helper
        and "string_agg" not in recovery_helper,
        "snapshot helper must use bounded snapshot COPY fingerprints with xmin, sequences, large objects, deterministic ordering, and bounded DDL locks",
    )
    require(
        "artifact checksum changed" in recovery_helper
        and "database identity changed" in recovery_helper
        and "migration manifest changed" in recovery_helper
        and "state fingerprint changed" in recovery_helper
        and "migration source changed" in recovery_helper,
        "snapshot helper must log non-secret recovery-point invalidation reasons",
    )
    require(
        "COPY scripts/backup.sh /app/scripts/backup.sh" in dockerfile
        and "COPY scripts/pre-migration-recovery.py /app/scripts/pre-migration-recovery.py" in dockerfile,
        "bundle image must include backup.sh and pre-migration recovery helper",
    )
    require(
        "2026.2.x" in runbook
        and "restore" in runbook.lower()
        and "PRE_MIGRATION_BACKUP_ACK_NO_BACKUP" in runbook,
        "upgrade runbook must document 2026.2.x path, restore, and override",
    )
    require(
        "FORTEMI_RUN_LARGE_MIGRATION_GATE" in migration_gate_test
        and "100_000" in migration_gate_test
        and "db.migrate()" in migration_gate_test,
        "seeded 2026.2.x migration gate test is missing required assertions",
    )
    require(
        "simulated recovery helper failure" in smoke_test
        and "backup failure did not abort" in smoke_test
        and "Pre-migration backup skipped: database has no user data" in smoke_test
        and "pre-migration recovery helper is not executable" in smoke_test,
        "pre-migration backup smoke test must cover helper delegation, fail-closed, missing-helper, and empty-db paths",
    )
    require(
        "publish_recovery_metadata" in reuse_smoke_test
        and "reusing verified pre-migration backup" in reuse_smoke_test
        and "verified pre-migration recovery point unavailable" in reuse_smoke_test,
        "pre-migration recovery reuse smoke test must cover sidecar metadata and helper delegation",
    )
    lint_block = ci_builder[ci_builder.index("  lint:"):ci_builder.index("  knowledge-shard-matrix:")]
    build_block = ci_builder[ci_builder.index("  build:"):ci_builder.index("  test-container:")]
    require(
        "smoke-pre-migration-recovery-realpg.sh" not in lint_block
        and "docker build -f build/Dockerfile.testdb -t matric-testdb:local ." in build_block
        and build_block.index("docker build -f build/Dockerfile.testdb -t matric-testdb:local .") < build_block.index("smoke-pre-migration-recovery-realpg.sh"),
        "real PostgreSQL recovery smoke must run after reproducible testdb image provisioning, not in lint",
    )

    require(
        "pg_restore --list" in realpg_smoke_test
        and "publish_recovery_metadata" in recovery_helper
        and "docker image inspect" in realpg_smoke_test
        and "docker network create" in realpg_smoke_test
        and "FORTEMI_REALPG_IN_CLIENT=true" in realpg_smoke_test
        and "FORTEMI_REALPG_DB_HOST" in realpg_smoke_test
        and "matric-testdb:local" in realpg_smoke_test
        and "eligible reuse" in realpg_smoke_test
        and "reuse disabled by zero age" in realpg_smoke_test
        and "no-op update invalidation" in realpg_smoke_test
        and "schema sequence invalidation" in realpg_smoke_test
        and "materialized view content invalidation" in realpg_smoke_test
        and "WITH NO DATA" in realpg_smoke_test
        and "unpopulated_materialized_view_transition=true" in realpg_smoke_test
        and "REFRESH MATERIALIZED VIEW" in realpg_smoke_test
        and "partial migration invalidation" in realpg_smoke_test
        and "migration manifest invalidation" in realpg_smoke_test
        and "restore/database replacement invalidation" in realpg_smoke_test
        and "corrupt artifact invalidation" in realpg_smoke_test
        and "concurrent sequence movement" in realpg_smoke_test
        and "failed backup left unverified dump artifact" in realpg_smoke_test
        and "failed_artifact_cleanup=true" in realpg_smoke_test
        and "failed replacement deleted older recovery point" in realpg_smoke_test
        and "failed_replacement_preserved_old_recovery_point=true" in realpg_smoke_test
        and "materialized_view_invalidation=true" in realpg_smoke_test
        and "Creating database dump" in realpg_smoke_test
        and "assert_restore_count" in realpg_smoke_test
        and "SKIP:" not in realpg_smoke_test,
        "real PostgreSQL smoke test must exercise entrypoint reuse without new backup, invalidation, sequence race fail-closed, and artifact restore without green skips",
    )
    require(
        "BASELINE = 20260215000000" in migration_risk
        and "data_backfills" in migration_risk
        and "index_builds" in migration_risk
        and "large_table_touches" in migration_risk,
        "post-February migration risk inventory is missing required categories",
    )
    require(
        "BASELINE_VERSION=\"${FORTEMI_FEB_BASELINE_VERSION:-20260215000000}\"" in fixture_runner
        and "BASELINE_TAG=\"${FORTEMI_FEB_BASELINE_TAG:-}\"" in fixture_runner
        and "hashlib.sha384" in fixture_runner
        and "FORTEMI_SEED_NOTES must be an integer of at least 100000" in fixture_runner
        and "scripts/backup.sh -d local" in fixture_runner
        and "pg_restore --exit-on-error --no-owner" in fixture_runner
        and "restore_drill_counts" in fixture_runner
        and "validate_fixture_invariants" in fixture_runner
        and "SEEDED_TABLE_COUNTS" in fixture_runner
        and "FORTEMI_SEEDED_TABLE_COUNTS" in fixture_runner
        and "source_invariants" in fixture_runner
        and "restore_invariants" in fixture_runner
        and "max_ungranted_locks_sampled" in fixture_runner
        and "OLD.embedding::text IS DISTINCT FROM NEW.embedding::text" in fixture_runner
        and "FORTEMI_RUN_LARGE_MIGRATION_GATE=true" in fixture_runner,
        "Feb-to-current fixture runner must apply numeric/tag baselines, enforce seed floor, restore-test backup, stamp sqlx checksums, validate invariants, and run the seeded gate",
    )
    require(
        "repair_legacy_migration_history" in db_lib
        and "split_applied" in db_lib
        and "20260202100000" in db_lib
        and "20260205000000" in db_lib
        and "20260215000000" in db_lib
        and "2bdad6ec8fffbe68cde85e0e749ac510ef319b694aa15dee71bcae3ad13b3db2f8b317f7ef2b393ea27e432b5f33872c" in db_lib
        and "sqlx::migrate!" in db_lib,
        "legacy migration history repair must run before sqlx migration validation",
    )
    require(
        "trg_reembed_on_skos_concept_update" in restore_safe_trigger_migration
        and "OLD.embedding::text IS DISTINCT FROM NEW.embedding::text" in restore_safe_trigger_migration,
        "restore-safe SKOS trigger fix must be a forward migration, not an edit to shipped SQL",
    )
    print("pre-migration backup gate verified")


if __name__ == "__main__":
    main()

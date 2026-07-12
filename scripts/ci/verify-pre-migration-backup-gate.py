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
MIGRATION_RISK = ROOT / "scripts" / "ci" / "analyze-post-feb-migrations.py"
FIXTURE_RUNNER = ROOT / "scripts" / "ci" / "run-feb-to-current-fixture.sh"
DB_LIB = ROOT / "crates" / "matric-db" / "src" / "lib.rs"
RESTORE_SAFE_TRIGGER_MIGRATION = ROOT / "migrations" / "20260614150000_restore_safe_skos_embedding_trigger.sql"


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
    migration_risk = MIGRATION_RISK.read_text()
    fixture_runner = FIXTURE_RUNNER.read_text()
    db_lib = DB_LIB.read_text()
    restore_safe_trigger_migration = RESTORE_SAFE_TRIGGER_MIGRATION.read_text()

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
        "verified pre-migration backup failed; aborting" in entrypoint,
        "backup failure must abort startup before migrations",
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
        "BACKUP_CLEANUP_PATTERN='pre-migration-*.sql*'" in entrypoint,
        "pre-migration backup retention pattern is missing",
    )
    require(
        "pg_restore --list" in backup and "sha256sum" in backup,
        "backup verification must parse the dump and log a checksum",
    )
    require(
        "pre-migration-*.sql*" in backup,
        "backup cleanup must remove pre-migration temp files",
    )
    require(
        "COPY scripts/backup.sh /app/scripts/backup.sh" in dockerfile,
        "bundle image must include scripts/backup.sh",
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
        "simulated backup failure" in smoke_test
        and "backup failure did not abort" in smoke_test
        and "Pre-migration backup skipped: database has no user data" in smoke_test,
        "pre-migration backup smoke test must cover fail-closed and empty-db paths",
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
        and "scripts/backup.sh -d local" in fixture_runner
        and "pg_restore --exit-on-error --no-owner" in fixture_runner
        and "restore_drill_note_original_count" in fixture_runner
        and "OLD.embedding::text IS DISTINCT FROM NEW.embedding::text" in fixture_runner
        and "FORTEMI_RUN_LARGE_MIGRATION_GATE=true" in fixture_runner,
        "Feb-to-current fixture runner must apply numeric/tag baselines, restore-test backup, stamp sqlx checksums, and run the seeded gate",
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

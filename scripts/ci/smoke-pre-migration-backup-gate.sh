#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FUNCTIONS_FILE="$TMP_DIR/pre-migration-functions.sh"
awk '
  /^current_migration_version\(\)/ { capture = 1 }
  /^repair_legacy_restore_compatibility$/ { capture = 0 }
  capture { print }
' "$ROOT/docker/bundle-entrypoint.sh" > "$FUNCTIONS_FILE"

# shellcheck source=/dev/null
source "$FUNCTIONS_FILE"

if ! grep -q "d.classid = 'pg_class'::regclass" "$FUNCTIONS_FILE" \
    || ! grep -q "d.deptype = 'e'" "$FUNCTIONS_FILE"; then
    echo "FAIL: user-data detection does not exclude extension-owned relations" >&2
    exit 1
fi

latest_available_migration_version() { echo "20260614140000"; }
current_migration_version() { echo "20260215000000"; }
pending_migrations_exist() { true; }
database_has_user_data() { true; }

install() {
    local path="${*: -1}"
    case "$path" in
        "$BACKUP_DEST"|"$BACKUP_TEMP_DIR") ;;
        *)
            echo "FAIL: unexpected install target: $path" >&2
            return 96
            ;;
    esac
    mkdir -p "$path"
    chmod 700 "$path"
}

runuser() {
    if [[ "$1" != "-u" || "$2" != "postgres" || "$3" != "--" ]]; then
        echo "FAIL: unexpected runuser invocation" >&2
        return 97
    fi
    shift 3
    "$@"
}

POSTGRES_USER=matric
POSTGRES_PASSWORD=test-password
POSTGRES_DB=matric
PGUSER=hostile_user
PGPASSWORD=should_be_stripped
PGPASSFILE=/tmp/should-be-stripped
PGHOST=hostile.example.invalid
PGPORT=6543
PGDATABASE=hostile_db
export PGUSER PGPASSWORD PGPASSFILE PGHOST PGPORT PGDATABASE
BACKUP_DEST="$TMP_DIR/backups"
BACKUP_SCRIPT_PATH="$TMP_DIR/unused-backup.sh"
PRE_MIGRATION_BACKUP_RETAIN=7
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=false
BACKUP_COMPRESS=gzip
BACKUP_TEMP_DIR="$TMP_DIR/scratch"
LOG_FILE="$TMP_DIR/backup.log"
PRE_MIGRATION_RECOVERY_HELPER_PATH="$TMP_DIR/failing-helper.sh"

cat > "$PRE_MIGRATION_RECOVERY_HELPER_PATH" <<'SH2'
#!/usr/bin/env bash
if [[ "$PGUSER" != "postgres" || "$PGHOST" != "/var/run/postgresql" || "$PGPORT" != "5432" || "$PGDATABASE" != "matric" ]]; then
    echo "pre-migration recovery helper did not select postgres peer authentication" >&2
    exit 90
fi
if [[ -n "${PGPASSWORD:-}" || -n "${PGPASSFILE:-}" ]]; then
    echo "pre-migration recovery helper leaked a database secret" >&2
    exit 91
fi
echo "simulated recovery helper failure" >&2
exit 42
SH2
chmod +x "$PRE_MIGRATION_RECOVERY_HELPER_PATH"

if ( ensure_pre_migration_backup ) >"$TMP_DIR/fail.out" 2>"$TMP_DIR/fail.err"; then
    echo "FAIL: backup failure did not abort the gate" >&2
    exit 1
fi
if ! grep -q "verified pre-migration recovery point unavailable; aborting" "$TMP_DIR/fail.err"; then
    echo "FAIL: backup failure did not emit fail-closed diagnostic" >&2
    exit 1
fi

PRE_MIGRATION_RECOVERY_HELPER_PATH="$TMP_DIR/success-helper.sh"
cat > "$PRE_MIGRATION_RECOVERY_HELPER_PATH" <<'SH2'
#!/usr/bin/env bash
if [[ "$PGUSER" != "postgres" || "$PGHOST" != "/var/run/postgresql" || "$PGPORT" != "5432" || "$PGDATABASE" != "matric" ]]; then
    echo "pre-migration recovery helper did not select postgres peer authentication" >&2
    exit 90
fi
if [[ -n "${PGPASSWORD:-}" || -n "${PGPASSFILE:-}" ]]; then
    echo "pre-migration recovery helper leaked a database secret" >&2
    exit 91
fi
if [[ "$1" != "20260215000000" || "$2" != "20260614140000" ]]; then
    echo "unexpected migration versions: $*" >&2
    exit 43
fi
echo ">>> Pre-migration backup ready: /tmp/pre-migration-20260215000000-20260614140000.sql.gz"
SH2
chmod +x "$PRE_MIGRATION_RECOVERY_HELPER_PATH"

ensure_pre_migration_backup >"$TMP_DIR/success.out" 2>"$TMP_DIR/success.err"
if ! grep -q "Pre-migration backup ready:" "$TMP_DIR/success.out"; then
    echo "FAIL: successful recovery helper output did not report ready path" >&2
    exit 1
fi

database_has_user_data() { false; }
PRE_MIGRATION_RECOVERY_HELPER_PATH="$TMP_DIR/should-not-run.sh"
cat > "$PRE_MIGRATION_RECOVERY_HELPER_PATH" <<'SH2'
#!/usr/bin/env bash
echo "backup should not run for empty databases" >&2
exit 99
SH2
chmod +x "$PRE_MIGRATION_RECOVERY_HELPER_PATH"

ensure_pre_migration_backup >"$TMP_DIR/empty.out" 2>"$TMP_DIR/empty.err"
if ! grep -q "Pre-migration backup skipped: database has no user data" "$TMP_DIR/empty.out"; then
    echo "FAIL: empty database skip was not reported" >&2
    exit 1
fi

PRE_MIGRATION_RECOVERY_HELPER_PATH="$TMP_DIR/missing-helper.sh"
database_has_user_data() { true; }

install() {
    local path="${*: -1}"
    case "$path" in
        "$BACKUP_DEST"|"$BACKUP_TEMP_DIR") ;;
        *)
            echo "FAIL: unexpected install target: $path" >&2
            return 96
            ;;
    esac
    mkdir -p "$path"
    chmod 700 "$path"
}

runuser() {
    if [[ "$1" != "-u" || "$2" != "postgres" || "$3" != "--" ]]; then
        echo "FAIL: unexpected runuser invocation" >&2
        return 97
    fi
    shift 3
    "$@"
}
if ( ensure_pre_migration_backup ) >"$TMP_DIR/missing.out" 2>"$TMP_DIR/missing.err"; then
    echo "FAIL: missing recovery helper did not abort" >&2
    exit 1
fi
if ! grep -q "pre-migration recovery helper is not executable" "$TMP_DIR/missing.err"; then
    echo "FAIL: missing recovery helper diagnostic was not emitted" >&2
    exit 1
fi

echo "pre-migration backup gate smoke test passed"

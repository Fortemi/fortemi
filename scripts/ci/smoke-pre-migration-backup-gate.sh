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

if ! grep -q 'runuser -u postgres -- env -u PGPASSWORD -u PGPASSFILE' "$FUNCTIONS_FILE" \
    || ! grep -q 'PGHOST=/var/run/postgresql' "$FUNCTIONS_FILE"; then
    echo "FAIL: pre-migration backup does not use local postgres peer authentication" >&2
    exit 1
fi

# The entrypoint runs as root in the bundle. Keep this unit smoke test
# unprivileged while preserving the administrative-user and environment
# assertions made by ensure_pre_migration_backup.
install() {
    local path="${*: -1}"
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

latest_available_migration_version() {
    echo "20260614140000"
}

current_migration_version() {
    echo "20260215000000"
}

pending_migrations_exist() {
    true
}

database_has_user_data() {
    true
}

POSTGRES_USER=matric
POSTGRES_PASSWORD=test-password
POSTGRES_DB=matric
BACKUP_DEST="$TMP_DIR/backups"
BACKUP_SCRIPT_PATH="$TMP_DIR/failing-backup.sh"
PRE_MIGRATION_BACKUP_RETAIN=7
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=false
BACKUP_COMPRESS=gzip
BACKUP_TEMP_DIR="$TMP_DIR/scratch"
LOG_FILE="$TMP_DIR/backup.log"

cat > "$BACKUP_SCRIPT_PATH" <<'SH'
#!/usr/bin/env bash
if [[ "$PGUSER" != "postgres" || "$PGHOST" != "/var/run/postgresql" ]]; then
    echo "pre-migration backup did not select postgres peer authentication" >&2
    exit 90
fi
if [[ -n "${PGPASSWORD:-}" || -n "${PGPASSFILE:-}" ]]; then
    echo "pre-migration backup leaked a database secret to the peer-auth subprocess" >&2
    exit 91
fi
mkdir -p "$BACKUP_DEST"
touch "$BACKUP_DEST/${BACKUP_BASENAME}.sql.gz"
echo "simulated backup failure"
exit 42
SH
chmod +x "$BACKUP_SCRIPT_PATH"

if ( ensure_pre_migration_backup ) >"$TMP_DIR/fail.out" 2>"$TMP_DIR/fail.err"; then
    echo "FAIL: backup failure did not abort the gate" >&2
    exit 1
fi

if ! grep -q "verified pre-migration backup failed; aborting" "$TMP_DIR/fail.err"; then
    echo "FAIL: backup failure did not emit fail-closed diagnostic" >&2
    exit 1
fi

if find "$BACKUP_DEST" -maxdepth 1 -type f -name 'pre-migration-*.sql*' | grep -q .; then
    echo "FAIL: failed backup left an unverified dump artifact" >&2
    exit 1
fi

cat > "$BACKUP_SCRIPT_PATH" <<'SH'
#!/usr/bin/env bash
echo "pre-migration-20260215000000-20260614140000-20260711T000000Z.sql.gz"
exit 0
SH
chmod +x "$BACKUP_SCRIPT_PATH"

ensure_pre_migration_backup >"$TMP_DIR/success.out" 2>"$TMP_DIR/success.err"

if ! grep -q "Pre-migration backup ready:" "$TMP_DIR/success.out"; then
    echo "FAIL: successful backup did not report ready path" >&2
    exit 1
fi

database_has_user_data() {
    false
}

cat > "$BACKUP_SCRIPT_PATH" <<'SH'
#!/usr/bin/env bash
echo "backup should not run for empty databases" >&2
exit 99
SH
chmod +x "$BACKUP_SCRIPT_PATH"

ensure_pre_migration_backup >"$TMP_DIR/empty.out" 2>"$TMP_DIR/empty.err"

if ! grep -q "Pre-migration backup skipped: database has no user data" "$TMP_DIR/empty.out"; then
    echo "FAIL: empty database skip was not reported" >&2
    exit 1
fi

echo "pre-migration backup gate smoke test passed"

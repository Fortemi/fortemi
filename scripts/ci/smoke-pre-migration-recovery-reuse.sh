#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

BACKUP_LIB="$TMP_DIR/backup-lib.sh"
awk '
  /^# Run main$/ { capture = 0 }
  capture { print }
  /^set -euo pipefail$/ { capture = 1 }
' "$ROOT/scripts/backup.sh" > "$BACKUP_LIB"

# shellcheck source=/dev/null
source "$BACKUP_LIB"

BACKUP_DEST="$TMP_DIR/backups"
BACKUP_TEMP_DIR="$TMP_DIR/scratch"
mkdir -p "$BACKUP_DEST" "$BACKUP_TEMP_DIR"

BACKUP_RECOVERY_META_ENABLED=true
BACKUP_RECOVERY_DB_IDENTITY_SHA256=db-a
BACKUP_RECOVERY_FROM_VERSION=from-a
BACKUP_RECOVERY_TO_VERSION=to-a
BACKUP_RECOVERY_MIGRATION_MANIFEST_SHA256=manifest-a
BACKUP_RECOVERY_STATE_SHA256=state-a
BACKUP_RECOVERY_REUSE_MAX_AGE_SECONDS=86400

printf 'artifact one' > "$BACKUP_DEST/sample.sql.gz"
publish_recovery_metadata "sample.sql.gz"

if [[ ! -f "$BACKUP_DEST/sample.sql.gz.recovery.meta" ]]; then
    echo "FAIL: backup.sh did not publish recovery metadata" >&2
    exit 1
fi
for required in '^verified=true$' '^artifact_sha256=' '^artifact_size_bytes=' '^db_identity_sha256=db-a$' '^state_sha256=state-a$'; do
    if ! grep -q "$required" "$BACKUP_DEST/sample.sql.gz.recovery.meta"; then
        echo "FAIL: recovery metadata missing $required" >&2
        exit 1
    fi
done

FUNCTIONS_FILE="$TMP_DIR/pre-migration-functions.sh"
awk '
  /^current_migration_version\(\)/ { capture = 1 }
  /^repair_legacy_restore_compatibility$/ { capture = 0 }
  capture { print }
' "$ROOT/docker/bundle-entrypoint.sh" > "$FUNCTIONS_FILE"

# The entrypoint function creates postgres-owned backup directories and
# delegates through runuser in production. This smoke test runs in a non-root
# lint runner, so assert the production identity contract before executing the
# filesystem and helper operations as the current user. Real postgres-owned
# permissions remain covered by the container startup/recovery gates.
INSTALL_STUB_LOG="$TMP_DIR/install.calls"
RUNUSER_STUB_LOG="$TMP_DIR/runuser.calls"
install() {
    local saw_mode=false
    local saw_owner=false
    local saw_group=false
    local args=()
    printf 'install %s\n' "$*" >> "$INSTALL_STUB_LOG"
    while (($#)); do
        case "$1" in
            -d) args+=("$1"); shift ;;
            -m)
                if [[ "${2:-}" != "0700" ]]; then
                    echo "FAIL: smoke expected install -m 0700, got -m ${2:-}" >&2
                    exit 1
                fi
                saw_mode=true
                args+=("$1" "$2")
                shift 2
                ;;
            -o)
                if [[ "${2:-}" != "postgres" ]]; then
                    echo "FAIL: smoke expected install -o postgres, got -o ${2:-}" >&2
                    exit 1
                fi
                saw_owner=true
                shift 2
                ;;
            -g)
                if [[ "${2:-}" != "postgres" ]]; then
                    echo "FAIL: smoke expected install -g postgres, got -g ${2:-}" >&2
                    exit 1
                fi
                saw_group=true
                shift 2
                ;;
            *) args+=("$1"); shift ;;
        esac
    done
    if [[ "$saw_mode" != true || "$saw_owner" != true || "$saw_group" != true ]]; then
        echo "FAIL: smoke expected install to request -m 0700 -o postgres -g postgres" >&2
        exit 1
    fi
    command install "${args[@]}"
}

runuser() {
    printf 'runuser %s\n' "$*" >> "$RUNUSER_STUB_LOG"
    if [[ "${1:-}" != "-u" || "${2:-}" != "postgres" || "${3:-}" != "--" ]]; then
        echo "FAIL: smoke expected runuser -u postgres --, got: $*" >&2
        exit 1
    fi
    shift 3
    "$@"
}

# shellcheck source=/dev/null
source "$FUNCTIONS_FILE"

database_has_user_data() { true; }
pending_migrations_exist() { true; }
latest_available_migration_version() { echo to-a; }
current_migration_version() { echo from-a; }

POSTGRES_DB=matric
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=false
PRE_MIGRATION_BACKUP_RETAIN=7
BACKUP_SCRIPT_PATH="$ROOT/scripts/backup.sh"
PRE_MIGRATION_RECOVERY_HELPER_PATH="$TMP_DIR/helper.sh"
cat > "$PRE_MIGRATION_RECOVERY_HELPER_PATH" <<'SH2'
#!/usr/bin/env bash
if [[ "$1" != "from-a" || "$2" != "to-a" ]]; then
    echo "wrong helper args: $*" >&2
    exit 3
fi
echo ">>> Pre-migration backup ready: /tmp/pre-migration-from-a-to-a.sql.gz (reusing verified pre-migration backup)"
SH2
chmod +x "$PRE_MIGRATION_RECOVERY_HELPER_PATH"
ensure_pre_migration_backup > "$TMP_DIR/reuse.out" 2> "$TMP_DIR/reuse.err"
if ! grep -q "reusing verified pre-migration backup" "$TMP_DIR/reuse.out"; then
    echo "FAIL: entrypoint did not delegate recovery reuse to helper" >&2
    exit 1
fi
if [[ "$(grep -c -- '-m 0700 -o postgres -g postgres' "$INSTALL_STUB_LOG")" -ne 2 ]]; then
    echo "FAIL: entrypoint did not request postgres-owned 0700 backup directories" >&2
    exit 1
fi
if [[ "$(grep -c '^runuser -u postgres -- env ' "$RUNUSER_STUB_LOG")" -ne 1 ]]; then
    echo "FAIL: entrypoint did not delegate helper through runuser -u postgres -- env" >&2
    exit 1
fi

cat > "$PRE_MIGRATION_RECOVERY_HELPER_PATH" <<'SH2'
#!/usr/bin/env bash
echo "simulated helper failure" >&2
exit 44
SH2
chmod +x "$PRE_MIGRATION_RECOVERY_HELPER_PATH"
if ( ensure_pre_migration_backup ) > "$TMP_DIR/fail.out" 2> "$TMP_DIR/fail.err"; then
    echo "FAIL: helper failure did not abort" >&2
    exit 1
fi
if ! grep -q "verified pre-migration recovery point unavailable" "$TMP_DIR/fail.err"; then
    echo "FAIL: helper failure did not emit fail-closed diagnostic" >&2
    exit 1
fi

echo "pre-migration recovery reuse smoke test passed"

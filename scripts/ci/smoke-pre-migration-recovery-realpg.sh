#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
POSTGRES_IMAGE="${FORTEMI_REALPG_IMAGE:-matric-testdb:local}"
CLIENT_IMAGE="${FORTEMI_REALPG_CLIENT_IMAGE:-fortemi-pre-migration-recovery-client:local}"

if [[ "${FORTEMI_REALPG_IN_CLIENT:-false}" != "true" && "${FORTEMI_REALPG_USE_HOST_CLIENTS:-false}" != "true" ]]; then
    if ! command -v docker >/dev/null 2>&1; then
        echo "FAIL: docker is required to orchestrate the real PostgreSQL recovery reuse smoke test" >&2
        exit 1
    fi
    if [[ "${FORTEMI_REALPG_BUILD_TESTDB:-false}" == "true" ]]; then
        docker build -f build/Dockerfile.testdb -t "$POSTGRES_IMAGE" .
    fi
    if ! docker image inspect "$POSTGRES_IMAGE" >/dev/null 2>&1; then
        echo "FAIL: required local PostgreSQL image $POSTGRES_IMAGE is not available" >&2
        exit 1
    fi

    run_id="${GITHUB_RUN_ID:-$$}-$(date +%s)"
    network="fortemi-issue1133-net-${run_id}"
    db_container="fortemi-issue1133-pg-${run_id}"
    cleanup_outer() {
        docker rm -f "$db_container" >/dev/null 2>&1 || true
        docker network rm "$network" >/dev/null 2>&1 || true
    }
    trap cleanup_outer EXIT

    docker network create "$network" >/dev/null
    docker run -d --rm --name "$db_container" --network "$network" \
        -e POSTGRES_PASSWORD=pass \
        -e POSTGRES_USER=postgres \
        -e POSTGRES_DB=matric \
        "$POSTGRES_IMAGE" >/dev/null

    docker build -t "$CLIENT_IMAGE" -f - . <<DOCKERFILE
FROM $POSTGRES_IMAGE
USER root
RUN apt-get update && apt-get install -y --no-install-recommends python3 gzip ca-certificates coreutils findutils && rm -rf /var/lib/apt/lists/*
ENTRYPOINT ["/bin/bash"]
DOCKERFILE

    docker run --rm --network "$network" \
        -v "$ROOT:/work" \
        -w /work \
        -e FORTEMI_REALPG_IN_CLIENT=true \
        -e FORTEMI_REALPG_DB_HOST="$db_container" \
        -e FORTEMI_REALPG_DB_PORT=5432 \
        -e FORTEMI_REALPG_IMAGE="$POSTGRES_IMAGE" \
        "$CLIENT_IMAGE" scripts/ci/smoke-pre-migration-recovery-realpg.sh
    exit 0
fi

for required_cmd in psql pg_dump pg_restore createdb dropdb gzip sha256sum python3; do
    if ! command -v "$required_cmd" >/dev/null 2>&1; then
        echo "FAIL: $required_cmd is required inside the PostgreSQL client environment" >&2
        exit 1
    fi
done

TEST_ROOT="$ROOT/.aiwg/working/issue-1133-realpg-$(date +%s)-$$"
CONTAINER="${FORTEMI_REALPG_DB_HOST:-fortemi-issue1133-pg-$$}"
DB_HOST="${FORTEMI_REALPG_DB_HOST:-127.0.0.1}"
PORT="${FORTEMI_REALPG_DB_PORT:-}"
cleanup() {
    if [[ "${FORTEMI_REALPG_IN_CLIENT:-false}" != "true" ]]; then
        docker stop "$CONTAINER" >/dev/null 2>&1 || true
    fi
    rm -rf "$TEST_ROOT"
}
container_logs() {
    if command -v docker >/dev/null 2>&1; then
        docker logs "$CONTAINER" 2>/dev/null || true
    fi
}
trap cleanup EXIT

mkdir -p "$TEST_ROOT/backup" "$TEST_ROOT/scratch" "$TEST_ROOT/migrations"
printf '%s\n' '-- migration a' > "$TEST_ROOT/migrations/20260215000000_a.sql"

if [[ "${FORTEMI_REALPG_IN_CLIENT:-false}" != "true" ]]; then
    if ! command -v docker >/dev/null 2>&1; then
        echo "FAIL: docker is required for host-client real PostgreSQL mode" >&2
        exit 1
    fi
    if ! docker image inspect "$POSTGRES_IMAGE" >/dev/null 2>&1; then
        echo "FAIL: required local PostgreSQL image $POSTGRES_IMAGE is not available" >&2
        exit 1
    fi
    docker run -d --rm --name "$CONTAINER" \
        -e POSTGRES_PASSWORD=pass \
        -e POSTGRES_USER=postgres \
        -e POSTGRES_DB=matric \
        -p 127.0.0.1::5432 \
        "$POSTGRES_IMAGE" >/dev/null
    PORT="$(docker port "$CONTAINER" 5432/tcp | sed 's/.*://')"
fi
PORT="${PORT:-5432}"

for _ in $(seq 1 60); do
    if env PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" \
        psql -d matric -At -v ON_ERROR_STOP=1 -c 'SELECT 1' >/dev/null 2>&1; then
        sleep 1
        if env PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" \
            psql -d matric -At -v ON_ERROR_STOP=1 -c 'SELECT 1' >/dev/null 2>&1; then
            break
        fi
    fi
    sleep 1
done
if ! env PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" \
    psql -d matric -At -v ON_ERROR_STOP=1 -c 'SELECT 1' >/dev/null 2>&1; then
    echo "FAIL: PostgreSQL container did not become ready" >&2
    container_logs >&2
    exit 1
fi

pg_env() {
    env PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" "$@"
}
run_sql() {
    pg_env psql -d matric -v ON_ERROR_STOP=1 "$@"
}
run_admin_sql() {
    pg_env psql -d postgres -v ON_ERROR_STOP=1 "$@"
}
backup_count() {
    find "$TEST_ROOT/backup" -maxdepth 1 -type f -name 'pre-migration-*.sql.gz' | wc -l
}
latest_artifact() {
    find "$TEST_ROOT/backup" -maxdepth 1 -type f -name 'pre-migration-*.sql.gz' | sort | tail -n 1
}
assert_backup_count() {
    local expected="$1"
    local label="$2"
    local actual
    actual="$(backup_count | tr -d ' ')"
    if [[ "$actual" != "$expected" ]]; then
        echo "FAIL: $label expected $expected backup artifacts, found $actual" >&2
        exit 1
    fi
}
assert_output_contains() {
    local file="$1"
    local pattern="$2"
    if ! grep -q "$pattern" "$file"; then
        echo "FAIL: expected '$pattern' in $file" >&2
        cat "$file" >&2
        exit 1
    fi
}
assert_restore_count() {
    local artifact="$1"
    local expected="$2"
    local restore_db="restore_${RANDOM}_$$"
    pg_env createdb "$restore_db"
    gzip -dc "$artifact" > "$TEST_ROOT/restore.dump"
    pg_env pg_restore --exit-on-error --no-owner -d "$restore_db" "$TEST_ROOT/restore.dump"
    local actual
    actual="$(pg_env psql -d "$restore_db" -At -v ON_ERROR_STOP=1 -c 'SELECT count(*) FROM tenant.note_original')"
    pg_env dropdb "$restore_db"
    if [[ "$actual" != "$expected" ]]; then
        echo "FAIL: restored artifact had $actual tenant.note_original rows, expected $expected" >&2
        exit 1
    fi
}

run_sql >/dev/null <<'SQL'
CREATE SCHEMA tenant;
CREATE TABLE tenant.note_original (id integer primary key, content text);
CREATE SEQUENCE tenant.note_seq START 100;
INSERT INTO tenant.note_original
SELECT g, repeat('issue 1133 real backup ', 200)
FROM generate_series(1, 50) AS g;
CREATE MATERIALIZED VIEW tenant.note_summary AS
SELECT left(content, 12) AS prefix, count(*) AS total
FROM tenant.note_original
GROUP BY left(content, 12);
CREATE MATERIALIZED VIEW tenant.note_summary_empty AS
SELECT content
FROM tenant.note_original
WHERE false
WITH NO DATA;
SQL

FUNCTIONS_FILE="$TEST_ROOT/pre-migration-functions.sh"
awk '
  /^current_migration_version\(\)/ { capture = 1 }
  /^repair_legacy_restore_compatibility$/ { capture = 0 }
  capture { print }
' "$ROOT/docker/bundle-entrypoint.sh" > "$FUNCTIONS_FILE"

# shellcheck source=/dev/null
source "$FUNCTIONS_FILE"

install() {
    local path="${*: -1}"
    mkdir -p "$path"
    chmod 700 "$path"
}

runuser() {
    if [[ "$1" != "-u" || "$2" != "postgres" || "$3" != "--" ]]; then
        echo "FAIL: unexpected runuser invocation: $*" >&2
        return 97
    fi
    shift 3
    if [[ "${1:-}" == "env" ]]; then
        shift
        local kept=()
        while (($#)); do
            if [[ "$1" == "-u" ]]; then
                shift 2
                continue
            fi
            case "$1" in
                PGUSER=*|PGPASSWORD=*|PGPASSFILE=*|PGHOST=*|PGPORT=*|PGDATABASE=*)
                    shift
                    ;;
                *=*)
                    kept+=("$1")
                    shift
                    ;;
                *)
                    break
                    ;;
            esac
        done
        command env "${kept[@]}" PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" PGDATABASE="$POSTGRES_DB" "$@"
        return $?
    fi
    command env PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" "$@"
}

su() {
    if [[ "$1" != "postgres" || "$2" != "-c" ]]; then
        echo "FAIL: unexpected su invocation: $*" >&2
        return 98
    fi
    command env PGUSER=postgres PGPASSWORD=pass PGHOST="$DB_HOST" PGPORT="$PORT" bash -c "$3"
}

database_has_user_data() { true; }
pending_migrations_exist() { true; }
latest_available_migration_version() { echo "${TEST_TO_VERSION:-to-real}"; }
current_migration_version() { echo "${TEST_FROM_VERSION:-from-real}"; }
pre_migration_migration_manifest_hash() { printf '%s\n' "${TEST_MANIFEST:-manifest-real}"; }

POSTGRES_DB=matric
POSTGRES_USER=postgres
POSTGRES_PASSWORD=pass
BACKUP_DEST="$TEST_ROOT/backup"
BACKUP_TEMP_DIR="$TEST_ROOT/scratch"
BACKUP_TEMP_TRUSTED_ENCRYPTED=true
BACKUP_SCRIPT_PATH="$ROOT/scripts/backup.sh"
PRE_MIGRATION_RECOVERY_HELPER_PATH="$ROOT/scripts/pre-migration-recovery.py"
FORTEMI_PRE_MIGRATION_RECOVERY_ALLOW_PASSWORD=true
FORTEMI_MIGRATIONS_DIR="$TEST_ROOT/migrations"
BACKUP_RETAIN=7
PRE_MIGRATION_BACKUP_RETAIN=7
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=false
BACKUP_COMPRESS=gzip
LOG_FILE="$TEST_ROOT/backup.log"
FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS=86400
export POSTGRES_DB POSTGRES_USER POSTGRES_PASSWORD BACKUP_DEST BACKUP_TEMP_DIR BACKUP_TEMP_TRUSTED_ENCRYPTED
export BACKUP_SCRIPT_PATH PRE_MIGRATION_RECOVERY_HELPER_PATH FORTEMI_PRE_MIGRATION_RECOVERY_ALLOW_PASSWORD
export FORTEMI_MIGRATIONS_DIR BACKUP_RETAIN PRE_MIGRATION_BACKUP_RETAIN BACKUP_COMPRESS LOG_FILE
export FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS PGUSER PGPASSWORD PGHOST PGPORT PGDATABASE
PGUSER=postgres
PGPASSWORD=pass
PGHOST="$DB_HOST"
PGPORT="$PORT"
PGDATABASE=matric
export PGUSER PGPASSWORD PGHOST PGPORT PGDATABASE

ensure_pre_migration_backup > "$TEST_ROOT/create.out" 2> "$TEST_ROOT/create.err"
assert_backup_count 1 "initial backup"
first_artifact="$(latest_artifact)"
first_meta="$first_artifact.recovery.meta"
test -s "$first_artifact"
test -f "$first_meta"
gzip -dc "$first_artifact" > "$TEST_ROOT/first.dump"
pg_restore --list "$TEST_ROOT/first.dump" >/dev/null
grep -q '^verified=true$' "$first_meta"
grep -q '^artifact_sha256=' "$first_meta"
assert_restore_count "$first_artifact" 50

ensure_pre_migration_backup > "$TEST_ROOT/reuse.out" 2> "$TEST_ROOT/reuse.err"
assert_backup_count 1 "eligible reuse"
assert_output_contains "$TEST_ROOT/reuse.out" "reusing verified pre-migration backup"
if grep -q "Creating database dump" "$TEST_ROOT/reuse.out"; then
    echo "FAIL: eligible reuse ran a new full pg_dump backup" >&2
    exit 1
fi

FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS=0 ensure_pre_migration_backup > "$TEST_ROOT/reuse-disabled.out" 2> "$TEST_ROOT/reuse-disabled.err"
assert_backup_count 2 "reuse disabled by zero age"
assert_output_contains "$TEST_ROOT/reuse-disabled.out" "reuse disabled"
FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS=86400

run_sql >/dev/null <<'SQL'
UPDATE tenant.note_original SET content = content WHERE id = 1;
SQL
ensure_pre_migration_backup > "$TEST_ROOT/noop-update.out" 2> "$TEST_ROOT/noop-update.err"
assert_backup_count 3 "no-op update invalidation"
assert_output_contains "$TEST_ROOT/noop-update.out" "state fingerprint changed"

run_sql >/dev/null <<'SQL'
CREATE SEQUENCE tenant.sequence_after_backup START 500;
SELECT nextval('tenant.sequence_after_backup');
SQL
ensure_pre_migration_backup > "$TEST_ROOT/sequence.out" 2> "$TEST_ROOT/sequence.err"
assert_backup_count 4 "schema sequence invalidation"
assert_output_contains "$TEST_ROOT/sequence.out" "state fingerprint changed"

run_sql >/dev/null <<'SQL'
UPDATE tenant.note_original SET content = 'matview refresh mutation' WHERE id = 2;
REFRESH MATERIALIZED VIEW tenant.note_summary;
REFRESH MATERIALIZED VIEW tenant.note_summary_empty;
SQL
ensure_pre_migration_backup > "$TEST_ROOT/materialized-view.out" 2> "$TEST_ROOT/materialized-view.err"
assert_backup_count 5 "materialized view content invalidation"
assert_output_contains "$TEST_ROOT/materialized-view.out" "state fingerprint changed"

TEST_FROM_VERSION=from-partial ensure_pre_migration_backup > "$TEST_ROOT/partial.out" 2> "$TEST_ROOT/partial.err"
assert_backup_count 6 "partial migration invalidation"
assert_output_contains "$TEST_ROOT/partial.out" "migration source changed"
unset TEST_FROM_VERSION

printf '%s\n' '-- migration b' > "$TEST_ROOT/migrations/20260614140000_b.sql"
ensure_pre_migration_backup > "$TEST_ROOT/manifest.out" 2> "$TEST_ROOT/manifest.err"
assert_backup_count 7 "migration manifest invalidation"
assert_output_contains "$TEST_ROOT/manifest.out" "migration manifest changed"

pg_env dropdb --force matric
pg_env createdb matric
gzip -dc "$first_artifact" > "$TEST_ROOT/replaced.dump"
pg_env pg_restore --exit-on-error --no-owner -d matric "$TEST_ROOT/replaced.dump"
ensure_pre_migration_backup > "$TEST_ROOT/restore-replace.out" 2> "$TEST_ROOT/restore-replace.err"
assert_backup_count 8 "restore/database replacement invalidation"
assert_output_contains "$TEST_ROOT/restore-replace.out" "database identity changed"

write_mode="$(run_sql -At -c 'SHOW default_transaction_read_only')"
if [[ "$write_mode" != "off" ]]; then
    echo "FAIL: default_transaction_read_only was left at $write_mode" >&2
    exit 1
fi

while IFS= read -r artifact; do
    printf 'corrupted' > "$artifact"
done < <(find "$TEST_ROOT/backup" -maxdepth 1 -type f -name 'pre-migration-*.sql.gz' | sort)
ensure_pre_migration_backup > "$TEST_ROOT/corrupt.out" 2> "$TEST_ROOT/corrupt.err"
assert_backup_count 9 "corrupt artifact invalidation"
assert_output_contains "$TEST_ROOT/corrupt.out" "artifact checksum changed"

(
    for _ in $(seq 1 400); do
        psql -d matric -At -c "SELECT nextval('tenant.note_seq')" >/dev/null 2>&1 || true
        sleep 0.01
    done
) &
sequence_writer_pid=$!
if "$ROOT/scripts/pre-migration-recovery.py" from-real to-real > "$TEST_ROOT/sequence-race.out" 2> "$TEST_ROOT/sequence-race.err"; then
    wait "$sequence_writer_pid" || true
    echo "FAIL: concurrent sequence movement did not fail closed" >&2
    exit 1
fi
wait "$sequence_writer_pid" || true
assert_output_contains "$TEST_ROOT/sequence-race.err" "sequence moved during recovery"

FAIL_DEST="$TEST_ROOT/fail-backup"
mkdir -p "$FAIL_DEST"
old_retained="$FAIL_DEST/pre-migration-old-retained.sql.gz"
old_retained_meta="$old_retained.recovery.meta"
printf 'older verified recovery point' > "$old_retained"
printf 'verified=true\n' > "$old_retained_meta"
touch -d '14 days ago' "$old_retained" "$old_retained_meta"
FAILING_BACKUP_SCRIPT="$TEST_ROOT/failing-backup.sh"
cat > "$FAILING_BACKUP_SCRIPT" <<'SH2'
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$BACKUP_DEST"
printf 'partial failed dump' > "$BACKUP_DEST/${BACKUP_BASENAME}.sql.gz"
echo "simulated backup failure"
exit 42
SH2
chmod +x "$FAILING_BACKUP_SCRIPT"
if (
    BACKUP_DEST="$FAIL_DEST"
    BACKUP_SCRIPT_PATH="$FAILING_BACKUP_SCRIPT"
    FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS=0
    export BACKUP_DEST BACKUP_SCRIPT_PATH FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS
    ensure_pre_migration_backup
) > "$TEST_ROOT/failed-cleanup.out" 2> "$TEST_ROOT/failed-cleanup.err"; then
    echo "FAIL: simulated backup failure did not fail closed" >&2
    exit 1
fi
assert_output_contains "$TEST_ROOT/failed-cleanup.out" "simulated backup failure"
if find "$FAIL_DEST" -maxdepth 1 -type f -name 'pre-migration-*.sql*' ! -name 'pre-migration-old-retained.sql.gz' ! -name 'pre-migration-old-retained.sql.gz.recovery.meta' | grep -q .; then
    echo "FAIL: failed backup left unverified dump artifact" >&2
    find "$FAIL_DEST" -maxdepth 1 -type f -print >&2
    exit 1
fi
if [[ ! -f "$old_retained" || ! -f "$old_retained_meta" ]]; then
    echo "FAIL: failed replacement deleted older recovery point before helper validation passed" >&2
    find "$FAIL_DEST" -maxdepth 1 -type f -print >&2
    exit 1
fi
if find "$FAIL_DEST" -maxdepth 1 -type f -name '*.recovery.meta' ! -name 'pre-migration-old-retained.sql.gz.recovery.meta' | grep -q .; then
    echo "FAIL: failed backup published recovery metadata" >&2
    find "$FAIL_DEST" -maxdepth 1 -type f -print >&2
    exit 1
fi

RECEIPT="$ROOT/.aiwg/working/issue-1133-realpg-receipt-latest.txt"
mkdir -p "$(dirname "$RECEIPT")"
latest_artifact_path="$(latest_artifact)"
latest_meta="$latest_artifact_path.recovery.meta"
artifact_count="$(backup_count | tr -d ' ')"
metadata_count="$(find "$TEST_ROOT/backup" -maxdepth 1 -type f -name '*.recovery.meta' | wc -l | tr -d ' ')"
db_identity_sha256="$(awk -F= '$1 == "db_identity_sha256" { print $2 }' "$latest_meta")"
state_sha256="$(awk -F= '$1 == "state_sha256" { print $2 }' "$latest_meta")"
artifact_sha256="$(awk -F= '$1 == "artifact_sha256" { print $2 }' "$latest_meta")"
cat > "$RECEIPT" <<EOF
postgres_image=$POSTGRES_IMAGE
container=$CONTAINER
test_root=$TEST_ROOT
test_root_cleaned_on_success=true
artifact_count=$artifact_count
metadata_count=$metadata_count
latest_artifact=$(basename "$latest_artifact_path")
latest_db_identity_sha256=$db_identity_sha256
latest_state_sha256=$state_sha256
latest_artifact_sha256=$artifact_sha256
restore_verified_rows=50
reuse_without_new_backup=true
sequence_race_fail_closed=true
failed_artifact_cleanup=true
failed_replacement_preserved_old_recovery_point=true
materialized_view_invalidation=true
unpopulated_materialized_view_transition=true
EOF

echo "pre-migration recovery real PostgreSQL smoke test passed"

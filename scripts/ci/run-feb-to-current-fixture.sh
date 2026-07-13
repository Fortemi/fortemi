#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BASELINE_VERSION="${FORTEMI_FEB_BASELINE_VERSION:-20260215000000}"
BASELINE_TAG="${FORTEMI_FEB_BASELINE_TAG:-}"
BASELINE_LABEL="${BASELINE_TAG:-$BASELINE_VERSION}"
SEED_NOTES="${FORTEMI_SEED_NOTES:-100000}"
DB_IMAGE="${FORTEMI_TESTDB_IMAGE:-matric-testdb:local}"
CONTAINER_NAME="${FORTEMI_TESTDB_CONTAINER:-fortemi-feb-upgrade-$RANDOM}"
DB_PASSWORD="${POSTGRES_PASSWORD:-matric}"
DB_NAME="${POSTGRES_DB:-matric}"
DB_USER="${POSTGRES_USER:-matric}"
HOST_PORT="${FORTEMI_TESTDB_PORT:-55432}"
KEEP_CONTAINER="${FORTEMI_KEEP_TESTDB:-false}"
BACKUP_DEST_DIR=""

cleanup() {
    if [[ "$KEEP_CONTAINER" != "true" ]]; then
        docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
    if [[ -n "$BACKUP_DEST_DIR" ]]; then
        rm -rf "$BACKUP_DEST_DIR"
    fi
}
trap cleanup EXIT

cd "$ROOT"

if ! docker image inspect "$DB_IMAGE" >/dev/null 2>&1; then
    docker build -f build/Dockerfile.testdb -t "$DB_IMAGE" .
fi

docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d \
    --name "$CONTAINER_NAME" \
    -e POSTGRES_USER="$DB_USER" \
    -e POSTGRES_PASSWORD="$DB_PASSWORD" \
    -e POSTGRES_DB="$DB_NAME" \
    -p "127.0.0.1:${HOST_PORT}:5432" \
    "$DB_IMAGE" >/dev/null

ready_samples=0
for _ in $(seq 1 90); do
    if docker exec "$CONTAINER_NAME" pg_isready -U "$DB_USER" -d "$DB_NAME" >/dev/null 2>&1; then
        ready_samples="$((ready_samples + 1))"
        if [[ "$ready_samples" -ge 3 ]]; then
            break
        fi
    else
        ready_samples=0
    fi
    sleep 1
done

if [[ "$ready_samples" -lt 3 ]]; then
    docker logs "$CONTAINER_NAME" >&2 || true
    echo "FAIL: PostgreSQL fixture did not become ready" >&2
    exit 1
fi

PSQL=(docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" psql -v ON_ERROR_STOP=1 -U "$DB_USER" -d "$DB_NAME")

echo "Applying baseline migrations from ${BASELINE_LABEL}"
if [[ -n "$BASELINE_TAG" ]]; then
    mapfile -t baseline_migrations < <(git ls-tree -r --name-only "$BASELINE_TAG" migrations | sort)
else
    mapfile -t baseline_migrations < <(
        find migrations -maxdepth 1 -type f -name '*.sql' -printf '%f\n' \
            | sort \
            | awk -v baseline="$BASELINE_VERSION" -F_ '$1 <= baseline {print "migrations/" $0}'
    )
fi

for migration_path in "${baseline_migrations[@]}"; do
    migration="${migration_path#migrations/}"
    version="${migration%%_*}"
    migration_sql="$(mktemp)"
    if [[ -n "$BASELINE_TAG" ]]; then
        git show "${BASELINE_TAG}:${migration_path}" >"$migration_sql"
    else
        cp "$migration_path" "$migration_sql"
    fi

    "${PSQL[@]}" < "$migration_sql" >/dev/null

    checksum="$(python3 - "$migration_sql" <<'PY'
import hashlib
import pathlib
import sys
print(hashlib.sha384(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)"
    description="${migration#${version}_}"
    description="${description%.sql}"
    "${PSQL[@]}" >/dev/null <<SQL
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL
);
INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
VALUES (${version}, '${description//\'/\'\'}', true, decode('${checksum}', 'hex'), 0)
ON CONFLICT (version) DO UPDATE
SET checksum = EXCLUDED.checksum,
    success = EXCLUDED.success;
SQL
    rm -f "$migration_sql"
done

baseline_sql_version="$("${PSQL[@]}" -At -c "SELECT COALESCE(max(version), 0) FROM _sqlx_migrations WHERE success = true")"

echo "Seeding ${SEED_NOTES} baseline notes"
"${PSQL[@]}" >/dev/null <<SQL
ALTER TABLE note DISABLE TRIGGER ALL;
INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, metadata, title)
SELECT uuidv7(),
       'markdown',
       'feb-upgrade-fixture',
       now(),
       now(),
       jsonb_build_object('fixture', 'feb-to-current', 'ordinal', gs),
       'Fixture note ' || gs
FROM generate_series(1, ${SEED_NOTES}) AS gs;
ALTER TABLE note ENABLE TRIGGER ALL;

INSERT INTO note_original (id, note_id, content, hash, user_created_at, user_last_edited_at, version_number)
SELECT uuidv7(),
       id,
       repeat('Large migration fixture content ' || id::text || ' ', 4),
       md5(id::text),
       created_at_utc,
       updated_at_utc,
       1
FROM note
WHERE source = 'feb-upgrade-fixture';

INSERT INTO note_revised_current (note_id, content, ai_metadata)
SELECT id,
       'Revised fixture content for ' || id::text,
       '{"fixture":"feb-to-current"}'::jsonb
FROM note
WHERE source = 'feb-upgrade-fixture';
SQL

"${PSQL[@]}" >/dev/null <<'SQL'
DO $$
BEGIN
  IF to_regclass('public.skos_concept') IS NULL
     OR to_regprocedure('public.queue_reembed_for_skos_changes()') IS NULL THEN
    RETURN;
  END IF;

  IF EXISTS (
      SELECT 1
      FROM pg_trigger
      WHERE tgname = 'trg_reembed_on_skos_concept_update'
        AND tgrelid = 'public.skos_concept'::regclass
        AND NOT tgisinternal
        AND pg_get_triggerdef(oid) LIKE '%embedding IS DISTINCT FROM%'
  ) THEN
    DROP TRIGGER trg_reembed_on_skos_concept_update ON public.skos_concept;
    CREATE TRIGGER trg_reembed_on_skos_concept_update
    AFTER UPDATE ON public.skos_concept
    FOR EACH ROW
    WHEN (OLD.embedding::text IS DISTINCT FROM NEW.embedding::text)
    EXECUTE FUNCTION public.queue_reembed_for_skos_changes();
  END IF;
END $$;
SQL

database_scheme="postgres"
DATABASE_URL="${database_scheme}://${DB_USER}:${DB_PASSWORD}@127.0.0.1:${HOST_PORT}/${DB_NAME}"
BACKUP_DEST_DIR="$(mktemp -d)"
backup_basename="pre-migration-$(date -u '+%Y%m%dT%H%M%SZ')-${BASELINE_LABEL//[^A-Za-z0-9_.-]/_}-fixture"
backup_output="$(
    BACKUP_DEST="$BACKUP_DEST_DIR" \
    BACKUP_BASENAME="$backup_basename" \
    BACKUP_CLEANUP_PATTERN='pre-migration-*.sql*' \
    BACKUP_RETAIN=1 \
    BACKUP_TEMP_DIR="/dev/shm/fortemi-fixture-backup-$$" \
    BACKUP_TEMP_TRUSTED_ENCRYPTED=true \
    BACKUP_COMPRESS=gzip \
    PGUSER="$DB_USER" \
    PGPASSWORD="$DB_PASSWORD" \
    PGHOST=127.0.0.1 \
    PGPORT="$HOST_PORT" \
    PGDATABASE="$DB_NAME" \
    LOG_FILE= \
    scripts/backup.sh -d local
)"
backup_file="$(printf '%s\n' "$backup_output" | tail -n 1)"
backup_path="$BACKUP_DEST_DIR/$backup_file"
backup_sha256="$(sha256sum "$backup_path" | awk '{print $1}')"
restore_db="restore_${RANDOM}"
docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
    createdb -U "$DB_USER" "$restore_db"
if [[ "$backup_path" == *.gz ]]; then
    gzip -dc "$backup_path" | docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        pg_restore --exit-on-error --no-owner -U "$DB_USER" -d "$restore_db"
else
    docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        pg_restore --exit-on-error --no-owner -U "$DB_USER" -d "$restore_db" <"$backup_path"
fi
restore_note_count="$(
    docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        psql -U "$DB_USER" -d "$restore_db" -At -c "SELECT count(*) FROM note_original"
)"
docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
    dropdb -U "$DB_USER" "$restore_db"

before_wal_lsn="$("${PSQL[@]}" -At -c "SELECT pg_current_wal_lsn()")"
started_at="$(date +%s)"
lock_sample_file="$(mktemp)"

(
    while true; do
        "${PSQL[@]}" -At -c "SELECT count(*) FROM pg_locks WHERE NOT granted" 2>/dev/null || true
        sleep 0.2
    done
) >"$lock_sample_file" &
lock_sampler_pid="$!"

echo "Running current migration gate against ${SEED_NOTES} seeded notes"
set +e
FORTEMI_RUN_LARGE_MIGRATION_GATE=true \
    FORTEMI_MIN_SEEDED_NOTES="$SEED_NOTES" \
    DATABASE_URL="$DATABASE_URL" \
    cargo test -p matric-db --features migrations \
        --test feb_to_current_migration_gate -- --ignored --nocapture
test_status="$?"
set -e

kill "$lock_sampler_pid" >/dev/null 2>&1 || true
wait "$lock_sampler_pid" >/dev/null 2>&1 || true

if [[ "$test_status" -ne 0 ]]; then
    exit "$test_status"
fi

finished_at="$(date +%s)"
duration="$((finished_at - started_at))"

applied_version="$("${PSQL[@]}" -At -c "SELECT max(version) FROM _sqlx_migrations WHERE success = true")"
after_wal_lsn="$("${PSQL[@]}" -At -c "SELECT pg_current_wal_lsn()")"
wal_bytes="$("${PSQL[@]}" -At -c "SELECT pg_wal_lsn_diff('${after_wal_lsn}'::pg_lsn, '${before_wal_lsn}'::pg_lsn)::bigint")"
note_count="$("${PSQL[@]}" -At -c "SELECT count(*) FROM note_original")"
longest_migration="$("${PSQL[@]}" -At -F $'\t' -c "SELECT version, description, execution_time FROM _sqlx_migrations WHERE version > ${baseline_sql_version} AND success = true ORDER BY execution_time DESC LIMIT 1")"
max_ungranted_locks="$(awk 'BEGIN{max=0} /^[0-9]+$/ {if ($1 > max) max = $1} END{print max}' "$lock_sample_file")"
rm -f "$lock_sample_file"

cat <<EOF
feb-to-current fixture completed
seed_notes=${note_count}
baseline=${BASELINE_LABEL}
baseline_sql_version=${baseline_sql_version}
target=${applied_version}
pre_migration_backup=${backup_file}
pre_migration_backup_sha256=${backup_sha256}
restore_drill_note_original_count=${restore_note_count}
duration_seconds=${duration}
wal_start_lsn=${before_wal_lsn}
wal_end_lsn=${after_wal_lsn}
wal_bytes=${wal_bytes}
longest_migration=${longest_migration}
max_ungranted_locks_sampled=${max_ungranted_locks}
container=${CONTAINER_NAME}
EOF

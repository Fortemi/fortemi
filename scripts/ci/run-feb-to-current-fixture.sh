#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BASELINE_VERSION="${FORTEMI_FEB_BASELINE_VERSION:-20260215000000}"
BASELINE_TAG="${FORTEMI_FEB_BASELINE_TAG:-}"
BASELINE_LABEL="${BASELINE_TAG:-$BASELINE_VERSION}"
SEED_NOTES="${FORTEMI_SEED_NOTES:-100000}"
ALLOW_SMALL_FIXTURE="${FORTEMI_ALLOW_SMALL_FEB_FIXTURE:-false}"
DB_IMAGE="${FORTEMI_TESTDB_IMAGE:-matric-testdb:local}"
CONTAINER_NAME="${FORTEMI_TESTDB_CONTAINER:-fortemi-feb-upgrade-$RANDOM}"
DB_PASSWORD="${POSTGRES_PASSWORD:-matric}"
DB_NAME="${POSTGRES_DB:-matric}"
DB_USER="${POSTGRES_USER:-matric}"
HOST_PORT="${FORTEMI_TESTDB_PORT:-55432}"
KEEP_CONTAINER="${FORTEMI_KEEP_TESTDB:-false}"
BACKUP_DEST_DIR=""
CREATED_CONTAINER_ID=""
BASELINE_MISSING_PROFILE="inbound_source=baseline-missing,event_outbox=baseline-missing,incoming_webhook_receiver=baseline-missing"
declare -a SEEDED_TABLE_COUNTS=()

if [[ ! "$SEED_NOTES" =~ ^[1-9][0-9]{0,8}$ ]]; then
    echo "FAIL: FORTEMI_SEED_NOTES must be an integer of at least 100000" >&2
    exit 2
fi
if [[ "$ALLOW_SMALL_FIXTURE" == "true" ]]; then
    if ((SEED_NOTES < 1000)); then
        echo "FAIL: small February fixtures require at least 1000 seeded notes" >&2
        exit 2
    fi
else
    if ((SEED_NOTES < 100000)); then
        echo "FAIL: FORTEMI_SEED_NOTES must be an integer of at least 100000" >&2
        exit 2
    fi
fi

REPRESENTATIVE_ROWS="$((SEED_NOTES / 100))"
if ((REPRESENTATIVE_ROWS < 1000)); then
    REPRESENTATIVE_ROWS=1000
fi
if ((REPRESENTATIVE_ROWS > 5000)); then
    REPRESENTATIVE_ROWS=5000
fi

cleanup() {
    if [[ "$KEEP_CONTAINER" != "true" && -n "$CREATED_CONTAINER_ID" ]]; then
        docker rm -f "$CREATED_CONTAINER_ID" >/dev/null 2>&1 || true
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

CREATED_CONTAINER_ID="$(docker run -d \
    --name "$CONTAINER_NAME" \
    -e POSTGRES_USER="$DB_USER" \
    -e POSTGRES_PASSWORD="$DB_PASSWORD" \
    -e POSTGRES_DB="$DB_NAME" \
    -p "127.0.0.1:${HOST_PORT}:5432" \
    "$DB_IMAGE")"

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

psql_scalar() {
    "${PSQL[@]}" -At -c "$1"
}

relation_exists() {
    [[ "$(psql_scalar "SELECT to_regclass('$1') IS NOT NULL")" == "t" ]]
}

record_seeded_count() {
    local relation="$1"
    local count
    if relation_exists "$relation"; then
        count="$(psql_scalar "SELECT count(*) FROM ${relation}")"
        SEEDED_TABLE_COUNTS+=("${relation}=${count}")
    fi
}

restore_table_count() {
    local database="$1"
    local relation="$2"
    docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        psql -U "$DB_USER" -d "$database" -At -c "SELECT count(*) FROM ${relation}"
}

validate_fixture_invariants() {
    local database="$1"
    docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        psql -v ON_ERROR_STOP=1 -U "$DB_USER" -d "$database" -At <<'SQL'
WITH chunk_parents AS (
    SELECT id, chunk_metadata->'chunk_sequence' AS chunk_sequence
    FROM public.note
    WHERE source = 'feb-upgrade-fixture'
      AND chunk_metadata->>'fixture' = 'feb-to-current'
),
missing_chunk_children AS (
    SELECT p.id, child_id
    FROM chunk_parents p
    CROSS JOIN LATERAL jsonb_array_elements_text(p.chunk_sequence) AS child_id
    LEFT JOIN public.note child
      ON child.id::text = child_id
     AND child.source = 'feb-upgrade-fixture-chunk'
     AND child.metadata->>'parent_note_id' = p.id::text
    WHERE child.id IS NULL
),
bad_chunk_children AS (
    SELECT id
    FROM public.note
    WHERE source = 'feb-upgrade-fixture-chunk'
      AND (
          metadata->>'fixture' IS DISTINCT FROM 'feb-to-current'
          OR metadata->>'parent_note_id' IS NULL
          OR metadata->>'chunk_index' NOT IN ('0', '1')
          OR metadata->>'total_chunks' IS DISTINCT FROM '2'
      )
),
bad_attachments AS (
    SELECT b.id
    FROM public.attachment_blob b
    JOIN public.attachment a ON a.blob_id = b.id
    WHERE a.created_by = 'feb-fixture'
      AND b.reference_count <> 1
),
archive_schema_notes AS (
    SELECT
      (SELECT count(*) FROM archive_fixture_research.note) +
      (SELECT count(*) FROM archive_fixture_import.note) AS total_notes
)
SELECT 'chunk_parent_count=' || (SELECT count(*) FROM chunk_parents)
UNION ALL
SELECT 'chunk_sequence_bad_count=' || (
    SELECT count(*)
    FROM chunk_parents
    WHERE jsonb_typeof(chunk_sequence) IS DISTINCT FROM 'array'
       OR jsonb_array_length(chunk_sequence) <> 2
)
UNION ALL
SELECT 'missing_chunk_child_count=' || (SELECT count(*) FROM missing_chunk_children)
UNION ALL
SELECT 'bad_chunk_child_count=' || (SELECT count(*) FROM bad_chunk_children)
UNION ALL
SELECT 'attachment_refcount_bad_count=' || (SELECT count(*) FROM bad_attachments)
UNION ALL
SELECT 'archive_schema_count=' || (
    SELECT count(*)
    FROM pg_namespace
    WHERE nspname IN ('archive_fixture_research', 'archive_fixture_import')
)
UNION ALL
SELECT 'archive_schema_note_count=' || (SELECT total_notes FROM archive_schema_notes)
UNION ALL
SELECT 'gen_uuid_v7_default_count=' || (
    SELECT count(*)
    FROM pg_attrdef d
    JOIN pg_class c ON c.oid = d.adrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname IN ('archive_fixture_research', 'archive_fixture_import')
      AND pg_get_expr(d.adbin, d.adrelid) LIKE '%gen_uuid_v7%'
);
SQL
}

require_invariant() {
    local invariants="$1"
    local key="$2"
    local expected="$3"
    local actual
    actual="$(printf '%s\n' "$invariants" | awk -F= -v key="$key" '$1 == key {print $2}')"
    if [[ "$actual" != "$expected" ]]; then
        echo "FAIL: fixture invariant ${key} expected ${expected}, got ${actual:-missing}" >&2
        printf '%s\n' "$invariants" >&2
        exit 1
    fi
}

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

echo "Seeding ${REPRESENTATIVE_ROWS} representative baseline rows for revisions, chunks, embeddings, jobs, archives, and attachments"
"${PSQL[@]}" >/dev/null <<SQL
CREATE TEMP TABLE feb_fixture_sample_notes AS
SELECT id, row_number() OVER (ORDER BY created_at_utc, id) AS ordinal
FROM note
WHERE source = 'feb-upgrade-fixture'
ORDER BY created_at_utc, id
LIMIT ${REPRESENTATIVE_ROWS};

INSERT INTO note_revision (id, note_id, revision_number, content, type, summary, rationale, created_at_utc, model)
SELECT uuidv7(),
       id,
       1,
       'Representative AI revision for fixture note ' || ordinal,
       'ai_enhancement',
       'Fixture revision ' || ordinal,
       'Production-scale February upgrade fixture',
       now(),
       'fixture-model'
FROM feb_fixture_sample_notes;

DO \$\$
BEGIN
  IF EXISTS (
      SELECT 1 FROM information_schema.columns
      WHERE table_schema = 'public' AND table_name = 'note' AND column_name = 'chunk_metadata'
  ) THEN
    CREATE TEMP TABLE feb_fixture_chunk_notes AS
    SELECT s.id AS parent_id,
           uuidv7() AS chunk_note_id,
           chunk_index,
           s.ordinal
    FROM feb_fixture_sample_notes s
    CROSS JOIN generate_series(0, 1) AS chunk_index;

    INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, metadata, title)
    SELECT chunk_note_id,
           'markdown',
           'feb-upgrade-fixture-chunk',
           now(),
           now(),
           jsonb_build_object(
               'fixture', 'feb-to-current',
               'parent_note_id', parent_id,
               'chunk_index', chunk_index,
               'total_chunks', 2
           ),
           'Fixture note ' || ordinal || ' chunk ' || chunk_index
    FROM feb_fixture_chunk_notes;

    INSERT INTO note_original (id, note_id, content, hash, user_created_at, user_last_edited_at, version_number)
    SELECT uuidv7(),
           chunk_note_id,
           'Chunk ' || chunk_index || ' content for fixture note ' || parent_id::text,
           md5(chunk_note_id::text),
           now(),
           now(),
           1
    FROM feb_fixture_chunk_notes;

    INSERT INTO note_revised_current (note_id, content, ai_metadata)
    SELECT chunk_note_id,
           'Revised chunk ' || chunk_index || ' content for fixture note ' || parent_id::text,
           jsonb_build_object('fixture', 'feb-to-current', 'chunk_index', chunk_index)
    FROM feb_fixture_chunk_notes;

    UPDATE note n
    SET chunk_metadata = jsonb_build_object(
        'fixture', 'feb-to-current',
        'total_chunks', 2,
        'chunking_strategy', 'fixed',
        'chunk_sequence', (
            SELECT jsonb_agg(c.chunk_note_id::text ORDER BY c.chunk_index)
            FROM feb_fixture_chunk_notes c
            WHERE c.parent_id = n.id
        )
    )
    FROM feb_fixture_sample_notes s
    WHERE n.id = s.id;
  END IF;
END \$\$;

INSERT INTO embedding (note_id, chunk_index, text, vector, model)
SELECT id,
       0,
       'Representative embedding chunk for fixture note ' || ordinal,
       ('[' || rtrim(repeat('0,', 768), ',') || ']')::vector,
       'fixture-embedding-768'
FROM feb_fixture_sample_notes;

INSERT INTO job_queue (note_id, job_type, status, priority, payload, result, actual_duration_ms, progress_percent, created_at, completed_at)
SELECT id,
       'embedding'::job_type,
       CASE WHEN ordinal % 3 = 0 THEN 'completed'::job_status ELSE 'pending'::job_status END,
       5,
       jsonb_build_object('fixture', 'feb-to-current', 'ordinal', ordinal),
       CASE WHEN ordinal % 3 = 0 THEN '{"fixture":"completed"}'::jsonb ELSE NULL END,
       CASE WHEN ordinal % 3 = 0 THEN 25 ELSE NULL END,
       CASE WHEN ordinal % 3 = 0 THEN 100 ELSE 0 END,
       now(),
       CASE WHEN ordinal % 3 = 0 THEN now() ELSE NULL END
FROM feb_fixture_sample_notes;

INSERT INTO archive_registry (name, schema_name, description, note_count, size_bytes, is_default)
VALUES ('public', 'public', 'Default public archive namespace for the February fixture', ${SEED_NOTES}, 0, true)
ON CONFLICT (name) DO NOTHING;

DO \$\$
DECLARE
    archive_rec RECORD;
    table_rec RECORD;
    shared_tables TEXT[] := ARRAY[
        '_sqlx_migrations',
        'api_key',
        'archive_registry',
        'document_type',
        'embedding_config',
        'file_upload_audit',
        'job_history',
        'job_queue',
        'oauth_authorization_code',
        'oauth_client',
        'oauth_token',
        'user_config',
        'user_metadata_label'
    ];
BEGIN
  FOR archive_rec IN
      SELECT *
      FROM (
          VALUES
              ('fixture-research'::text, 'archive_fixture_research'::text, 'Representative research archive namespace'::text),
              ('fixture-import'::text, 'archive_fixture_import'::text, 'Representative imported archive namespace'::text)
      ) AS archives(name, schema_name, description)
  LOOP
    EXECUTE format('CREATE SCHEMA IF NOT EXISTS %I', archive_rec.schema_name);

    FOR table_rec IN
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = 'public'
          AND tablename <> ALL(shared_tables)
        ORDER BY tablename
    LOOP
      EXECUTE format(
          'CREATE TABLE IF NOT EXISTS %I.%I (LIKE public.%I INCLUDING ALL)',
          archive_rec.schema_name,
          table_rec.tablename,
          table_rec.tablename
      );
    END LOOP;

    FOR table_rec IN
        SELECT c.relname, a.attname
        FROM pg_attrdef d
        JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum
        JOIN pg_class c ON c.oid = d.adrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = archive_rec.schema_name
          AND pg_get_expr(d.adbin, d.adrelid) LIKE '%gen_uuid_v7%'
    LOOP
      EXECUTE format(
          'ALTER TABLE %I.%I ALTER COLUMN %I SET DEFAULT uuidv7()',
          archive_rec.schema_name,
          table_rec.relname,
          table_rec.attname
      );
    END LOOP;

    INSERT INTO archive_registry (name, schema_name, description, note_count, size_bytes, is_default)
    VALUES (archive_rec.name, archive_rec.schema_name, archive_rec.description, ${REPRESENTATIVE_ROWS}, 0, false)
    ON CONFLICT (name) DO NOTHING;

    EXECUTE format(
        'INSERT INTO %I.note (id, format, source, created_at_utc, updated_at_utc, metadata, title)
         SELECT id, format, source, created_at_utc, updated_at_utc, metadata, title
         FROM feb_fixture_sample_notes s JOIN public.note n USING (id)',
        archive_rec.schema_name
    );
    EXECUTE format(
        'INSERT INTO %I.note_original (id, note_id, content, hash, user_created_at, user_last_edited_at, version_number)
         SELECT id, note_id, content, hash, user_created_at, user_last_edited_at, version_number
         FROM public.note_original
         WHERE note_id IN (SELECT id FROM feb_fixture_sample_notes)',
        archive_rec.schema_name
    );
    EXECUTE format(
        'INSERT INTO %I.note_revised_current (note_id, content, ai_metadata)
         SELECT note_id, content, ai_metadata
         FROM public.note_revised_current
         WHERE note_id IN (SELECT id FROM feb_fixture_sample_notes)',
        archive_rec.schema_name
    );
  END LOOP;
END \$\$;

DO \$\$
BEGIN
  IF to_regclass('public.attachment_blob') IS NOT NULL
     AND to_regclass('public.attachment') IS NOT NULL
     AND to_regclass('public.attachment_embedding') IS NOT NULL THEN
    INSERT INTO attachment_blob (content_hash, content_type, size_bytes, storage_type, data)
    SELECT 'fixture-blob-' || md5(id::text),
           'text/plain',
           length(('fixture attachment ' || id::text)::bytea),
           'database',
           ('fixture attachment ' || id::text)::bytea
    FROM feb_fixture_sample_notes;

    INSERT INTO attachment (note_id, blob_id, filename, original_filename, status, extracted_text, extracted_metadata, created_by)
    SELECT s.id,
           b.id,
           'fixture-' || s.ordinal || '.txt',
           'fixture-' || s.ordinal || '.txt',
           'completed'::attachment_status,
           'Representative attachment text for fixture note ' || s.ordinal,
           jsonb_build_object('fixture', 'feb-to-current', 'ordinal', s.ordinal),
           'feb-fixture'
    FROM feb_fixture_sample_notes s
    JOIN attachment_blob b ON b.content_hash = 'fixture-blob-' || md5(s.id::text);

    INSERT INTO attachment_embedding (attachment_id, embedding_set_id, chunk_index, text, vector, model, embedding_type)
    SELECT a.id,
           NULL,
           0,
           'Representative attachment embedding for ' || a.filename,
           ('[' || rtrim(repeat('0,', 768), ',') || ']')::vector,
           'fixture-embedding-768',
           'text'
    FROM attachment a
    WHERE a.created_by = 'feb-fixture';
  END IF;
END \$\$;
SQL

record_seeded_count public.note_original
record_seeded_count public.note_revised_current
record_seeded_count public.note_revision
record_seeded_count public.embedding
record_seeded_count public.job_queue
record_seeded_count public.archive_registry
record_seeded_count public.attachment_blob
record_seeded_count public.attachment
record_seeded_count public.attachment_embedding
record_seeded_count archive_fixture_research.note
record_seeded_count archive_fixture_research.note_original
record_seeded_count archive_fixture_research.note_revised_current
record_seeded_count archive_fixture_import.note
record_seeded_count archive_fixture_import.note_original
record_seeded_count archive_fixture_import.note_revised_current
FORTEMI_SEEDED_TABLE_COUNTS="$(IFS=,; echo "${SEEDED_TABLE_COUNTS[*]}")"
echo "seed_profile=${FORTEMI_SEEDED_TABLE_COUNTS}"
echo "baseline_missing_profile=${BASELINE_MISSING_PROFILE}"

source_invariants="$(validate_fixture_invariants "$DB_NAME")"
require_invariant "$source_invariants" chunk_parent_count "$REPRESENTATIVE_ROWS"
require_invariant "$source_invariants" chunk_sequence_bad_count 0
require_invariant "$source_invariants" missing_chunk_child_count 0
require_invariant "$source_invariants" bad_chunk_child_count 0
require_invariant "$source_invariants" attachment_refcount_bad_count 0
require_invariant "$source_invariants" archive_schema_count 2
require_invariant "$source_invariants" archive_schema_note_count "$((REPRESENTATIVE_ROWS * 2))"
require_invariant "$source_invariants" gen_uuid_v7_default_count 0
echo "source_invariants=$(printf '%s' "$source_invariants" | paste -sd ',' -)"

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
restore_drill_counts=""
for table_count in "${SEEDED_TABLE_COUNTS[@]}"; do
    table="${table_count%%=*}"
    source_count="${table_count#*=}"
    restored_count="$(restore_table_count "$restore_db" "$table")"
    if [[ "$restored_count" != "$source_count" ]]; then
        echo "FAIL: restore drill changed ${table} count (source=${source_count}, restored=${restored_count})" >&2
        exit 1
    fi
    if [[ -n "$restore_drill_counts" ]]; then
        restore_drill_counts+=","
    fi
    restore_drill_counts+="${table}=${restored_count}"
done
restore_invariants="$(validate_fixture_invariants "$restore_db")"
if [[ "$restore_invariants" != "$source_invariants" ]]; then
    echo "FAIL: restore drill changed fixture invariants" >&2
    echo "source=${source_invariants}" >&2
    echo "restored=${restore_invariants}" >&2
    exit 1
fi
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
    FORTEMI_ALLOW_SMALL_FEB_FIXTURE="$ALLOW_SMALL_FIXTURE" \
    FORTEMI_MIN_SEEDED_NOTES="$SEED_NOTES" \
    FORTEMI_SEEDED_TABLE_COUNTS="$FORTEMI_SEEDED_TABLE_COUNTS" \
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
seed_profile=${FORTEMI_SEEDED_TABLE_COUNTS}
baseline_missing_profile=${BASELINE_MISSING_PROFILE}
restore_drill_counts=${restore_drill_counts}
fixture_invariants=$(printf '%s' "$source_invariants" | paste -sd ',' -)
duration_seconds=${duration}
wal_start_lsn=${before_wal_lsn}
wal_end_lsn=${after_wal_lsn}
wal_bytes=${wal_bytes}
longest_migration=${longest_migration}
max_ungranted_locks_sampled=${max_ungranted_locks}
container=${CONTAINER_NAME}
EOF

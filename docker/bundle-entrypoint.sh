#!/bin/bash
set -eo pipefail

# bundle-entrypoint.sh - Initialize and run PostgreSQL + matric-api + MCP server
#
# This script:
# 1. Initializes PostgreSQL if data directory is empty
# 2. Starts PostgreSQL
# 3. Waits for PostgreSQL to be ready
# 4. Creates database and enables pgvector extension
# 5. Starts matric-api (runs migrations on startup)
# 6. Validates/auto-registers MCP OAuth credentials
# 7. Starts MCP server with valid credentials

echo "=== Matric Memory Bundle Startup ==="
echo "Version: ${MATRIC_VERSION:-unknown}"

# PostgreSQL data directory
PGDATA="${PGDATA:-/var/lib/postgresql/data}"
POSTGRES_USER="${POSTGRES_USER:-matric}"
POSTGRES_DB="${POSTGRES_DB:-matric}"
export POSTGRES_USER POSTGRES_DB

# Whole-database backup operations need every tenant row while the application
# role intentionally remains NOBYPASSRLS. This marker is set only by the
# all-in-one bundle entrypoint and selects a fixed local peer-authenticated
# postgres subprocess; native deployments retain their configured libpq path.
FORTEMI_BUNDLE_LOCAL_BACKUP_ADMIN=true
export FORTEMI_BUNDLE_LOCAL_BACKUP_ADMIN
BACKUP_DEST="${BACKUP_DEST:-/var/backups/matric-memory}"
BACKUP_SCRIPT_PATH="${BACKUP_SCRIPT_PATH:-/app/scripts/backup.sh}"
PRE_MIGRATION_BACKUP_RETAIN="${PRE_MIGRATION_BACKUP_RETAIN:-7}"
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP="${PRE_MIGRATION_BACKUP_ACK_NO_BACKUP:-false}"

require_postgres_password() {
    local normalized
    if [ -z "${POSTGRES_PASSWORD:-}" ]; then
        echo "ERROR: POSTGRES_PASSWORD is required for every Docker bundle profile." >&2
        echo "Run scripts/init-bundle-env.sh before starting the bundle." >&2
        exit 1
    fi
    if [[ "$POSTGRES_PASSWORD" == *$'\n'* || "$POSTGRES_PASSWORD" == *$'\r'* ]]; then
        echo "ERROR: POSTGRES_PASSWORD must be a single-line value." >&2
        exit 1
    fi
    normalized="${POSTGRES_PASSWORD,,}"
    case "$normalized" in
        matric|fortemi-local-dev|password|changeme|\
        "<postgres_password>"|"<operator_supplied_database_password>")
            echo "ERROR: POSTGRES_PASSWORD uses a known reusable or placeholder value." >&2
            echo "Run scripts/init-bundle-env.sh to generate a per-install secret." >&2
            exit 1
            ;;
    esac
    export POSTGRES_PASSWORD
}

require_postgres_password

if [ -z "${DATABASE_URL:-}" ]; then
    DATABASE_URL="$(printf '%s%s:%s@localhost:5432/%s' 'postgres://' "$POSTGRES_USER" "$POSTGRES_PASSWORD" "$POSTGRES_DB")"
    export DATABASE_URL
fi

# Ensure PGDATA directory exists and is owned by postgres
# (Required for fresh volumes where the mount point may be owned by root)
mkdir -p "$PGDATA"
chown postgres:postgres "$PGDATA"
chmod 700 "$PGDATA"

# Check if this is a fresh install (empty data directory)
if [ -z "$(ls -A "$PGDATA" 2>/dev/null)" ]; then
    echo ">>> Initializing PostgreSQL data directory..."

    # Initialize PostgreSQL as postgres user (SCRAM-SHA-256 for pg18+)
    su postgres -c "initdb -D $PGDATA --auth-host=scram-sha-256 --auth-local=trust"

    # Configure PostgreSQL to listen on localhost only (internal)
    echo "listen_addresses = 'localhost'" >> "$PGDATA/postgresql.conf"
    echo "max_connections = 100" >> "$PGDATA/postgresql.conf"
    echo "password_encryption = 'scram-sha-256'" >> "$PGDATA/postgresql.conf"

    # Allow local connections (SCRAM-SHA-256 for network, trust for local socket)
    echo "local all all trust" > "$PGDATA/pg_hba.conf"
    echo "host all all 127.0.0.1/32 scram-sha-256" >> "$PGDATA/pg_hba.conf"
    echo "host all all ::1/128 scram-sha-256" >> "$PGDATA/pg_hba.conf"

    FRESH_INSTALL=true
else
    echo ">>> Using existing PostgreSQL data directory"
    FRESH_INSTALL=false
fi

# Start PostgreSQL
echo ">>> Starting PostgreSQL..."
mkdir -p /var/log/postgresql
chown postgres:postgres /var/log/postgresql
su postgres -c "pg_ctl -D $PGDATA -l /var/log/postgresql/postgresql.log start"

# Wait for PostgreSQL to be ready
echo ">>> Waiting for PostgreSQL to be ready..."
for i in {1..30}; do
    if su postgres -c "pg_isready -q"; then
        echo "PostgreSQL is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "ERROR: PostgreSQL failed to start"
        cat /var/log/postgresql/postgresql.log 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

# On fresh install, create user and database
if [ "$FRESH_INSTALL" = true ]; then
    echo ">>> Creating database and user..."

    # Create user and database
    su postgres -c "psql -c \"CREATE USER ${POSTGRES_USER} WITH PASSWORD '${POSTGRES_PASSWORD}' CREATEDB;\""
    su postgres -c "psql -c \"CREATE DATABASE ${POSTGRES_DB} OWNER ${POSTGRES_USER};\""

    # Enable required extensions (must be done as superuser)
    echo ">>> Enabling extensions..."
    su postgres -c "psql -d ${POSTGRES_DB} -c 'CREATE EXTENSION IF NOT EXISTS vector;'"
    su postgres -c "psql -d ${POSTGRES_DB} -c 'CREATE EXTENSION IF NOT EXISTS postgis;'"
fi

# Ensure required extensions exist (idempotent, must run as superuser before migrations)
echo ">>> Ensuring PostgreSQL extensions..."
su postgres -c "psql -d ${POSTGRES_DB} -c 'CREATE EXTENSION IF NOT EXISTS vector;'" 2>/dev/null || true
su postgres -c "psql -d ${POSTGRES_DB} -c 'CREATE EXTENSION IF NOT EXISTS postgis;'" 2>/dev/null || true

current_migration_version() {
    if ! database_has_sqlx_migrations_table; then
        echo "none"
        return 0
    fi

    su postgres -c "psql -d ${POSTGRES_DB} -At -v ON_ERROR_STOP=1" <<'SQL'
SELECT COALESCE(MAX(version)::text, 'none')
FROM public._sqlx_migrations
WHERE success = true;
SQL
}

database_has_sqlx_migrations_table() {
    [ "$(su postgres -c "psql -d ${POSTGRES_DB} -At -v ON_ERROR_STOP=1 -c \"SELECT to_regclass('public._sqlx_migrations') IS NOT NULL\"")" = "t" ]
}

database_has_user_data() {
    local tables
    tables="$(su postgres -c "psql -d ${POSTGRES_DB} -At -v ON_ERROR_STOP=1" <<'SQL'
SELECT format('%I.%I', n.nspname, c.relname)
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public'
  AND c.relkind IN ('r', 'p')
  AND c.relname <> '_sqlx_migrations'
  AND NOT EXISTS (
      SELECT 1
      FROM pg_depend d
      WHERE d.classid = 'pg_class'::regclass
        AND d.objid = c.oid
        AND d.deptype = 'e'
  );
SQL
)"

    while IFS= read -r table_ref; do
        if [ -z "$table_ref" ]; then
            continue
        fi
        if [ "$(su postgres -c "psql -d ${POSTGRES_DB} -At -v ON_ERROR_STOP=1 -c \"SELECT EXISTS (SELECT 1 FROM ${table_ref} LIMIT 1)\"")" = "t" ]; then
            return 0
        fi
    done <<< "$tables"

    return 1
}

latest_available_migration_version() {
    find /app/migrations -maxdepth 1 -type f -name '*.sql' -printf '%f\n' \
        | sed -E 's/^([0-9]+)_.*/\1/' \
        | sort -n \
        | tail -1
}

pending_migrations_exist() {
    local to_version="$1"
    if ! database_has_sqlx_migrations_table; then
        [ -n "$to_version" ]
        return
    fi

    local applied_count
    applied_count=$(su postgres -c "psql -d ${POSTGRES_DB} -At -v ON_ERROR_STOP=1" <<'SQL'
SELECT COUNT(*) FROM public._sqlx_migrations WHERE success = true;
SQL
)
    local available_count
    available_count=$(find /app/migrations -maxdepth 1 -type f -name '*.sql' | wc -l)
    [ "${applied_count:-0}" -lt "${available_count:-0}" ]
}

repair_legacy_restore_compatibility() {
    echo ">>> Checking legacy restore compatibility repairs..."
    su postgres -c "psql -d ${POSTGRES_DB} -v ON_ERROR_STOP=1" <<'SQL'
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
}

# Align the application role's password with POSTGRES_PASSWORD. Existing data
# directories may have been initialized under an older compose default
# (pre-2026.7 bundles hardcoded a different password), in which case every TCP
# client — the pre-migration backup, the API, MCP — fails scram auth and the
# container restart-loops. Local socket access is trust, so the alignment
# itself always works. (#1048)
align_role_password() {
    if env PGPASSWORD="$POSTGRES_PASSWORD" psql -U "$POSTGRES_USER" -h localhost -p 5432 -d "$POSTGRES_DB" -At -c 'SELECT 1' >/dev/null 2>&1; then
        return 0
    fi

    echo "!!! WARNING: role '${POSTGRES_USER}' does not accept the configured POSTGRES_PASSWORD"
    echo "!!! (data directory was initialized under a different password default);"
    echo "!!! aligning the role password with the current environment."

    local escaped sql
    escaped=$(printf '%s' "$POSTGRES_PASSWORD" | sed "s/'/''/g")
    sql=$(printf 'ALTER ROLE "%s" WITH PASSWORD '"'"'%s'"'"';' "$POSTGRES_USER" "$escaped")
    printf '%s\n' "$sql" | su postgres -c "psql -d ${POSTGRES_DB} -v ON_ERROR_STOP=1" >/dev/null

    if ! env PGPASSWORD="$POSTGRES_PASSWORD" psql -U "$POSTGRES_USER" -h localhost -p 5432 -d "$POSTGRES_DB" -At -c 'SELECT 1' >/dev/null 2>&1; then
        echo "ERROR: could not align role password for '${POSTGRES_USER}'; TCP auth still failing" >&2
        exit 1
    fi
    echo ">>> Role password aligned; TCP auth verified"
}

ensure_pre_migration_backup() {
    local to_version
    to_version="$(latest_available_migration_version)"

    if ! database_has_user_data; then
        echo ">>> Pre-migration backup skipped: database has no user data"
        return 0
    fi

    if ! pending_migrations_exist "$to_version"; then
        echo ">>> Pre-migration backup skipped: no pending migrations"
        return 0
    fi

    if [ "$PRE_MIGRATION_BACKUP_ACK_NO_BACKUP" = "true" ]; then
        echo "!!! WARNING: PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=true; running migrations without automatic recovery point"
        return 0
    fi

    local from_version
    local helper_path
    from_version="$(current_migration_version)"
    helper_path="${PRE_MIGRATION_RECOVERY_HELPER_PATH:-/app/scripts/pre-migration-recovery.py}"

    if [ ! -x "$helper_path" ]; then
        echo "ERROR: pre-migration recovery helper is not executable: $helper_path" >&2
        echo "Set PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=true only after accepting rollback risk." >&2
        exit 1
    fi

    install -d -m 0700 -o postgres -g postgres "$BACKUP_DEST"
    install -d -m 0700 -o postgres -g postgres "${BACKUP_TEMP_DIR:-/dev/shm/fortemi-pre-migration-backup}"

    if ! runuser -u postgres -- env -u PGPASSWORD -u PGPASSFILE \
        POSTGRES_DB="$POSTGRES_DB" \
        BACKUP_DEST="$BACKUP_DEST" \
        BACKUP_SCRIPT_PATH="$BACKUP_SCRIPT_PATH" \
        PRE_MIGRATION_BACKUP_RETAIN="$PRE_MIGRATION_BACKUP_RETAIN" \
        BACKUP_TEMP_DIR="${BACKUP_TEMP_DIR:-/dev/shm/fortemi-pre-migration-backup}" \
        BACKUP_TEMP_TRUSTED_ENCRYPTED="${BACKUP_TEMP_TRUSTED_ENCRYPTED:-false}" \
        BACKUP_COMPRESS="${BACKUP_COMPRESS:-gzip}" \
        FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS="${FORTEMI_PRE_MIGRATION_REUSE_MAX_AGE_SECONDS:-86400}" \
        FORTEMI_MIGRATIONS_DIR="${FORTEMI_MIGRATIONS_DIR:-/app/migrations}" \
        PGUSER=postgres \
        PGHOST=/var/run/postgresql \
        PGPORT=5432 \
        PGDATABASE="$POSTGRES_DB" \
        LOG_FILE="${LOG_FILE:-/var/log/fortemi/backup.log}" \
        "$helper_path" "$from_version" "$to_version"; then
        echo "ERROR: verified pre-migration recovery point unavailable; aborting before checksum repair and migrations" >&2
        echo "Set PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=true only for constrained environments with an external recovery point." >&2
        exit 1
    fi
}

repair_legacy_restore_compatibility
align_role_password
ensure_pre_migration_backup

# One-time repair for deployments that applied the briefly modified
# 20260215000000 migration from 10d2601f before the file was restored.
# sqlx stores SHA-384 bytes in _sqlx_migrations.checksum and validates them
# before pending migrations can run.
echo ">>> Checking migration checksum repair..."
su postgres -c "psql -d ${POSTGRES_DB} -v ON_ERROR_STOP=1" <<'SQL'
DO $$
BEGIN
  IF to_regclass('public._sqlx_migrations') IS NULL THEN
    RETURN;
  END IF;

  UPDATE public._sqlx_migrations
     SET checksum = decode('c4a8d7097ce200e9bd39d7bd70882403119c1181bbfa5999335d48ebd087e9703587297347bbef014974cb1699f07772', 'hex')
   WHERE version = 20260215000000
     AND success = true
     AND checksum = decode('2bdad6ec8fffbe68cde85e0e749ac510ef319b694aa15dee71bcae3ad13b3db2f8b317f7ef2b393ea27e432b5f33872c', 'hex');
END $$;
SQL

# NOTE: Database schema migrations are handled automatically by the API on startup
# via sqlx::migrate!() with _sqlx_migrations tracking table.
# This ensures migrations run exactly once, in order, with proper error handling.
echo ">>> Database migrations will be applied by API on startup"

# Create required directories for file storage and backups
echo ">>> Creating storage directories..."
mkdir -p /var/lib/matric/files
mkdir -p "$BACKUP_DEST"
echo "  File storage: /var/lib/matric/files"
echo "  Backup storage: $BACKUP_DEST"

# --- Testable bundle runtime helpers (sourced by startup regression tests) ---
bundle_positive_int_or_default() {
    local name="$1"
    local default_value="$2"
    local allow_zero="$3"
    local value="${!name:-}"

    if [ -z "$value" ]; then
        printf '%s\n' "$default_value"
        return 0
    fi

    case "$value" in
        *[!0-9]*)
            echo "WARNING: $name must be an integer number of seconds; using ${default_value}" >&2
            printf '%s\n' "$default_value"
            return 0
            ;;
    esac

    # Bound arithmetic and normalize decimal input (08 must not become octal).
    if [ "${#value}" -gt 7 ]; then
        echo "WARNING: $name is too large; using ${default_value}" >&2
        printf '%s\n' "$default_value"
        return 0
    fi
    value=$((10#$value))
    if [ "$allow_zero" != "true" ] && [ "$value" -eq 0 ]; then
        echo "WARNING: $name must be greater than zero; using ${default_value}" >&2
        printf '%s\n' "$default_value"
        return 0
    fi

    printf '%s\n' "$value"
}

wait_for_api_ready() {
    local api_port="${PORT:-3000}"
    local timeout_seconds progress_seconds poll_seconds elapsed next_progress started

    timeout_seconds="$(bundle_positive_int_or_default API_STARTUP_TIMEOUT_SECONDS 7200 true)"
    progress_seconds="$(bundle_positive_int_or_default API_STARTUP_PROGRESS_SECONDS 30 false)"
    poll_seconds="$(bundle_positive_int_or_default API_STARTUP_POLL_SECONDS 1 false)"
    elapsed=0
    started=$SECONDS
    next_progress="$progress_seconds"
    API_READY=false

    echo ">>> Waiting for API to be healthy..."
    if [ "$timeout_seconds" -eq 0 ]; then
        echo "  API startup wait timeout: disabled; waiting until healthy while PID ${API_PID} remains alive"
    else
        echo "  API startup wait timeout: ${timeout_seconds}s"
    fi

    while true; do
        elapsed=$((SECONDS - started))
        if ! kill -0 "$API_PID" 2>/dev/null; then
            echo "ERROR: API process died during startup" >&2
            exit 1
        fi
        if curl --connect-timeout 1 --max-time 2 -sf "http://localhost:${api_port}/health" >/dev/null 2>&1; then
            echo "  API is healthy after ${elapsed}s"
            API_READY=true
            return 0
        fi

        if ! kill -0 "$API_PID" 2>/dev/null; then
            echo "ERROR: API process died during startup" >&2
            cat /var/log/matric/api.log 2>/dev/null | tail -50 || true
            exit 1
        fi

        if [ "$timeout_seconds" -gt 0 ] && [ "$elapsed" -ge "$timeout_seconds" ]; then
            echo "ERROR: API health check timed out after ${timeout_seconds}s; MCP credential setup was not attempted" >&2
            echo "Increase API_STARTUP_TIMEOUT_SECONDS for first upgrades with long migrations, or set it to 0 to wait indefinitely." >&2
            cat /var/log/matric/api.log 2>/dev/null | tail -50 || true
            exit 1
        fi

        if [ "$elapsed" -ge "$next_progress" ]; then
            echo "  Still waiting for API after ${elapsed}s; migrations may still be running"
            next_progress=$((next_progress + progress_seconds))
        fi

        sleep "$poll_seconds"
        elapsed=$((SECONDS - started))
    done
}

parse_mcp_register_response() {
    python3 -c '
import json
import sys

try:
    payload = json.load(sys.stdin)
except Exception:
    sys.exit(1)

if not isinstance(payload, dict):
    sys.exit(1)

client_id = payload.get("client_id")
client_secret = payload.get("client_secret")
if not isinstance(client_id, str) or not client_id:
    sys.exit(1)
if not isinstance(client_secret, str) or not client_secret:
    sys.exit(1)

if any(c in client_id + client_secret for c in "\r\n\x00"):
    sys.exit(1)
print(client_id)
print(client_secret)
'
}

persist_mcp_credentials() {
    local temporary
    temporary=$(mktemp "${MCP_CREDS_FILE}.XXXXXX") || return 1
    if ! (umask 077; declare -p MCP_CLIENT_ID MCP_CLIENT_SECRET > "$temporary") ||
       ! chmod 600 "$temporary" || ! mv -f "$temporary" "$MCP_CREDS_FILE"; then
        rm -f "$temporary"
        return 1
    fi
}

validate_mcp_credentials() {
    local http_code curl_status

    MCP_CREDS_VALID=false
    if [ -z "${MCP_CLIENT_ID:-}" ] || [ -z "${MCP_CLIENT_SECRET:-}" ]; then
        echo ">>> No MCP credentials configured"
        return 0
    fi

    echo ">>> Validating MCP credentials (client_id: $MCP_CLIENT_ID)..."
    curl_status=0
    http_code=$(curl --connect-timeout 2 --max-time 10 -s -o /dev/null -w "%{http_code}" -X POST \
        "http://localhost:${PORT:-3000}/oauth/introspect" \
        -u "$MCP_CLIENT_ID:$MCP_CLIENT_SECRET" \
        -d "token=startup_validation_check" 2>/dev/null) || curl_status=$?

    if [ "$curl_status" -ne 0 ] || [ -z "$http_code" ] || [ "$http_code" = "000" ]; then
        MCP_CREDS_VALID=true
        echo "  WARNING: API unavailable during MCP credential validation; preserving existing MCP credentials"
        return 0
    fi

    case "$http_code" in
        200)
            MCP_CREDS_VALID=true
            echo "  MCP credentials valid"
            ;;
        400|401|403)
            echo "  MCP credentials invalid (HTTP $http_code)"
            ;;
        *)
            MCP_CREDS_VALID=true
            echo "  WARNING: MCP credential validation returned HTTP $http_code; preserving existing MCP credentials"
            ;;
    esac
}

register_mcp_client() {
    local register_response register_status parsed_credentials

    echo ">>> Auto-registering MCP OAuth client..."
    register_status=0
    register_response=$(curl --connect-timeout 2 --max-time 10 -fsS -X POST "http://localhost:${PORT:-3000}/oauth/register" \
        -H "Content-Type: application/json" \
        -d '{"client_name":"MCP Server (auto-registered)","grant_types":["client_credentials"],"scope":"mcp read write"}' 2>/dev/null) || register_status=$?

    if [ "$register_status" -ne 0 ] || [ -z "$register_response" ]; then
        echo "  WARNING: MCP client auto-registration failed"
        echo "  Registration request failed or returned an empty response"
        echo "  MCP server will start but token introspection will fail"
        echo "  Fix: manually register via POST /oauth/register"
        return 0
    fi

    if ! parsed_credentials="$(printf '%s' "$register_response" | parse_mcp_register_response 2>/dev/null)"; then
        echo "  WARNING: MCP client auto-registration failed"
        echo "  Registration response omitted because it may contain credentials"
        echo "  MCP server will start but token introspection will fail"
        echo "  Fix: manually register via POST /oauth/register"
        return 0
    fi

    MCP_CLIENT_ID="$(printf '%s\n' "$parsed_credentials" | sed -n '1p')"
    MCP_CLIENT_SECRET="$(printf '%s\n' "$parsed_credentials" | sed -n '2p')"
    export MCP_CLIENT_ID MCP_CLIENT_SECRET

    if ! persist_mcp_credentials; then
        echo "  WARNING: MCP credentials could not be persisted; API remains running" >&2
        echo "  Repair credential storage before restarting; current credentials remain in memory" >&2
        return 0
    fi

    echo "  Registered MCP client: $MCP_CLIENT_ID"
    echo "  Credentials persisted to $MCP_CREDS_FILE"
    echo ""
    echo "  ================================================================"
    echo "  MCP credentials registered and persisted (mode 600):"
    echo "    $MCP_CREDS_FILE"
    echo "  Client ID: $MCP_CLIENT_ID"
    echo "  Secret: (masked - never logged; read from the creds file if"
    echo "          you need it, e.g. for .env after a volume wipe)"
    echo "  ================================================================"
    echo ""
}
# --- End testable bundle runtime helpers ---

# --- Start API first (MCP needs the API for credential validation) ---
echo ">>> Starting Matric API..."
mkdir -p /var/log/matric
echo "  Listening on: ${HOST:-0.0.0.0}:${PORT:-3000}"

# Trap to clean up background processes on exit
cleanup() {
    echo "Shutting down..."
    kill ${MCP_PID:-} 2>/dev/null || true
    kill ${SEED_PID:-} 2>/dev/null || true
    kill ${RENDERER_PID:-} 2>/dev/null || true
    kill ${API_PID:-} 2>/dev/null || true
    su postgres -c "pg_ctl -D $PGDATA stop -m fast" 2>/dev/null || true
    exit 0
}
trap cleanup SIGTERM SIGINT

# --- Start Open3D 3D Renderer (for GLB/GLTF/OBJ/STL extraction) ---
echo ">>> Starting Open3D 3D Renderer..."
mkdir -p /var/log/matric
RENDERER_PORT="${RENDERER_PORT:-8080}"
RENDERER_ENABLED="${RENDERER_ENABLED:-${OPEN3D_ENABLED:-true}}"

# EGL headless rendering environment
export XDG_RUNTIME_DIR=/tmp

RENDERER_AVAILABLE=false
case "$(printf '%s' "$RENDERER_ENABLED" | tr '[:upper:]' '[:lower:]')" in
    0|false|no|off)
        echo "  Open3D renderer disabled by RENDERER_ENABLED/OPEN3D_ENABLED"
        ;;
    *)
        # Try GPU first (EGL device). CPU software rendering is opt-in because
        # Open3D's Filament backend can segfault on some non-GPU hosts.
        if [ -e /dev/dri ] || [ -e /dev/nvidia0 ]; then
            echo "  GPU detected — using EGL device rendering"
            export EGL_PLATFORM=device
        elif [ "${OPEN3D_CPU_RENDERING:-}" = "true" ] || [ "${OPEN3D_CPU_RENDERING:-}" = "1" ]; then
            echo "  No GPU detected — using requested CPU software rendering"
            export OPEN3D_CPU_RENDERING=true
            # Mesa llvmpipe needs GL version override for Open3D's Filament backend
            export MESA_GL_VERSION_OVERRIDE=4.5
            export LIBGL_ALWAYS_SOFTWARE=1
        else
            echo "  No GPU detected — skipping Open3D probe; set OPEN3D_CPU_RENDERING=true to try CPU rendering"
        fi

        if [ -e /dev/dri ] || [ -e /dev/nvidia0 ] || [ "${OPEN3D_CPU_RENDERING:-}" = "true" ]; then
            # Probe: test if Open3D can initialize before starting the full renderer.
            # Open3D 0.19.0's Filament backend may segfault during EGL init if no GPU
            # device is available. Running the probe in a subprocess contains the crash.
            if python3 -c "
import open3d as o3d
r = o3d.visualization.rendering.OffscreenRenderer(64, 64)
del r
print('ok')
" > /dev/null 2>&1; then
                echo "  Open3D probe passed — renderer available"
                RENDERER_AVAILABLE=true
            else
                echo "  Open3D probe failed — renderer unavailable (no GPU or EGL init failed)"
                echo "  3D model extraction will be disabled. To enable, add GPU device reservation"
                echo "  to docker-compose.bundle.yml (see deploy.resources.reservations.devices)"
            fi
        fi
        ;;
esac

RENDERER_READY=false
if [ "$RENDERER_AVAILABLE" = true ]; then
    PORT=$RENDERER_PORT python3 /app/open3d-renderer/server.py > /var/log/matric/renderer.log 2>&1 &
    RENDERER_PID=$!
    echo "  Renderer started (PID: $RENDERER_PID) on port $RENDERER_PORT"

    # Wait for renderer to be ready (health check now includes test render)
    echo "  Waiting for renderer to be ready..."
    for i in {1..20}; do
        if curl -sf http://localhost:$RENDERER_PORT/health >/dev/null 2>&1; then
            RENDERER_READY=true
            break
        fi
        # Check renderer process is still alive
        if ! kill -0 $RENDERER_PID 2>/dev/null; then
            echo "  WARNING: Renderer process died during startup"
            cat /var/log/matric/renderer.log 2>/dev/null | tail -20 || true
            break
        fi
        sleep 1
    done

    if [ "$RENDERER_READY" = true ]; then
        # Validate render quality — health endpoint now includes a test render
        RENDER_STATUS=$(curl -sf http://localhost:$RENDERER_PORT/health | python3 -c "
import sys, json
h = json.load(sys.stdin)
rt = h.get('render_test', {})
print(rt.get('status', 'unknown'))
" 2>/dev/null || echo "unknown")

        if [ "$RENDER_STATUS" = "pass" ]; then
            echo "  Renderer is healthy — test render passed!"
        elif [ "$RENDER_STATUS" = "fail" ]; then
            echo "  WARNING: Renderer is running but test render produces BLANK images"
            echo "  3D model thumbnails will appear grey. Check GPU/software rendering."
            echo "  Render test details:"
            curl -sf http://localhost:$RENDERER_PORT/health | python3 -c "
import sys, json
h = json.load(sys.stdin)
rt = h.get('render_test', {})
for k, v in rt.items():
    print(f'    {k}: {v}')
" 2>/dev/null || true
        else
            echo "  WARNING: Could not validate render quality (status: $RENDER_STATUS)"
        fi
    elif kill -0 $RENDERER_PID 2>/dev/null; then
        echo "  WARNING: Renderer health check timed out after 20s (3D model extraction may not work)"
        cat /var/log/matric/renderer.log 2>/dev/null | tail -20 || true
    fi
else
    RENDERER_PID=""
fi

/app/matric-api &
API_PID=$!

# Wait for API to be healthy before starting MCP server. The API applies sqlx
# migrations on startup, so first upgrades on constrained machines can spend
# substantially longer than ordinary cold starts before listening.
wait_for_api_ready

# --- MCP Credential Management ---
# Credentials are persisted on the pgdata volume so they survive container restarts.
# Only a volume wipe (docker compose down -v) requires re-registration, and that
# is handled automatically here.
#
# Priority: persisted file > env vars > auto-register
# The persisted file always matches the current database state. Env vars from .env
# may be stale after a clean deploy, so persisted credentials take precedence.
MCP_CREDS_FILE="$PGDATA/.fortemi-mcp-credentials"

# Prefer persisted credentials (they match the current DB)
if [ -f "$MCP_CREDS_FILE" ]; then
    echo ">>> Loading MCP credentials from persistent storage..."
    . "$MCP_CREDS_FILE"
    export MCP_CLIENT_ID MCP_CLIENT_SECRET
fi

# Validate existing credentials against the API's introspection endpoint
validate_mcp_credentials

# Auto-register if credentials are missing or invalid
if [ "$MCP_CREDS_VALID" = false ]; then
    register_mcp_client
fi

# --- Seed Support Archive (opt-in, background, non-blocking) ---
# Default off so the Docker bundle mirrors the native build path
# (which never auto-seeds). Operators opt in by setting
# LOAD_SUPPORT_MEMORY=true in .env, or run the seed script manually
# inside the running container at any time:
#   docker compose -f docker-compose.bundle.yml \
#     exec fortemi /app/seed-support-archive.sh
# The seed script is idempotent — re-running is a no-op after the
# first successful seed (flag file on the persistent pgdata volume).
if [ "${LOAD_SUPPORT_MEMORY:-false}" = "true" ] \
   && [ "${DISABLE_SUPPORT_MEMORY:-false}" != "true" ]; then
    echo ">>> Seeding support archive on first boot (LOAD_SUPPORT_MEMORY=true)..."
    MANUAL_INVOCATION=false /app/seed-support-archive.sh &
    SEED_PID=$!
fi

# --- Start MCP Server ---
echo ">>> Starting MCP Server..."
cd /app/mcp-server
MCP_TRANSPORT="${MCP_TRANSPORT:-http}" \
PORT="${MCP_PORT:-3001}" \
MATRIC_API_URL="${MATRIC_API_URL:-http://localhost:3000}" \
MCP_CLIENT_ID="$MCP_CLIENT_ID" \
MCP_CLIENT_SECRET="$MCP_CLIENT_SECRET" \
DEBUG_SESSION_CONTEXT="${DEBUG_SESSION_CONTEXT:-}" \
node index.js > /var/log/matric/mcp-server.log 2>&1 &
MCP_PID=$!
echo "  MCP server started (PID: $MCP_PID)"
echo "  Listening on: 0.0.0.0:${MCP_PORT:-3001}"
cd /app

# Wait for MCP server to be ready
sleep 2
if kill -0 $MCP_PID 2>/dev/null; then
    echo "  MCP server running"
else
    echo "  WARNING: MCP server may have failed to start"
    cat /var/log/matric/mcp-server.log 2>/dev/null || true
fi

echo "========================================"
echo "=== Matric Memory Bundle Ready ==="
echo "  API:      http://0.0.0.0:${PORT:-3000}"
echo "  MCP:      http://0.0.0.0:${MCP_PORT:-3001}"
echo "  Renderer: http://localhost:${RENDERER_PORT:-8080} (Open3D, 3D models)"
echo "  MCP Client ID: ${MCP_CLIENT_ID:-NOT SET}"
echo "========================================"

# Wait for critical processes to exit (API and MCP are required, renderer is optional)
# Only include renderer in wait if it started successfully
WAIT_PIDS="$API_PID $MCP_PID"
if [ "$RENDERER_READY" = true ] && kill -0 $RENDERER_PID 2>/dev/null; then
    WAIT_PIDS="$WAIT_PIDS $RENDERER_PID"
fi

wait -n $WAIT_PIDS

# If we get here, one of the critical processes died
echo "A critical process exited unexpectedly"
cleanup

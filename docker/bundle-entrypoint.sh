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
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-fortemi-local-dev}"
POSTGRES_DB="${POSTGRES_DB:-matric}"
export POSTGRES_USER POSTGRES_PASSWORD POSTGRES_DB
BACKUP_DEST="${BACKUP_DEST:-/var/backups/matric-memory}"
BACKUP_SCRIPT_PATH="${BACKUP_SCRIPT_PATH:-/app/scripts/backup.sh}"
PRE_MIGRATION_BACKUP_RETAIN="${PRE_MIGRATION_BACKUP_RETAIN:-7}"
PRE_MIGRATION_BACKUP_ACK_NO_BACKUP="${PRE_MIGRATION_BACKUP_ACK_NO_BACKUP:-false}"

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
SELECT format('%I.%I', schemaname, tablename)
FROM pg_tables
WHERE schemaname = 'public'
  AND tablename <> '_sqlx_migrations';
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

    if [ ! -x "$BACKUP_SCRIPT_PATH" ]; then
        echo "ERROR: pre-migration backup script is not executable: $BACKUP_SCRIPT_PATH" >&2
        echo "Set PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=true only after accepting rollback risk." >&2
        exit 1
    fi

    local from_version
    local timestamp
    local basename
    local backup_log
    from_version="$(current_migration_version)"
    timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
    basename="pre-migration-${from_version}-${to_version}-${timestamp}"
    backup_log="${BACKUP_DEST}/.${basename}.log"

    echo ">>> Pending migrations on non-empty database: creating verified pre-migration backup"
    mkdir -p "$BACKUP_DEST"

    # Pre-flight: the dump stages in BACKUP_TEMP_DIR before compression, and
    # Docker's default /dev/shm is only 64MB. Refuse to start a dump that
    # cannot fit; fall back to disk staging under BACKUP_DEST with a loud
    # warning rather than restart-looping. (#1049)
    local staging_dir staging_trusted db_size avail_staging avail_disk
    staging_dir="${BACKUP_TEMP_DIR:-/dev/shm/fortemi-pre-migration-backup}"
    staging_trusted="${BACKUP_TEMP_TRUSTED_ENCRYPTED:-false}"
    db_size=$(su postgres -c "psql -d ${POSTGRES_DB} -At -v ON_ERROR_STOP=1 -c 'SELECT pg_database_size(current_database())'" || echo "")
    mkdir -p "$staging_dir" 2>/dev/null || true
    avail_staging=$(df -B1 --output=avail "$staging_dir" 2>/dev/null | tail -1 || echo "")
    if [ -n "$db_size" ] && [ -n "$avail_staging" ] && [ "$avail_staging" -lt "$db_size" ]; then
        echo "!!! WARNING: backup staging dir ${staging_dir} has ${avail_staging} bytes free"
        echo "!!! but the database is ${db_size} bytes; the staged dump may not fit."
        echo "!!! Falling back to DISK staging under ${BACKUP_DEST}/.staging — the dump"
        echo "!!! will touch disk unencrypted while the backup runs."
        echo "!!! To keep RAM-backed staging, raise the container shm size (compose:"
        echo "!!! shm_size on the fortemi service) or point BACKUP_TEMP_DIR at a larger tmpfs."
        staging_dir="${BACKUP_DEST}/.staging"
        staging_trusted=true
        mkdir -p "$staging_dir"
        avail_disk=$(df -B1 --output=avail "$staging_dir" 2>/dev/null | tail -1 || echo "")
        if [ -n "$avail_disk" ] && [ "$avail_disk" -lt "$db_size" ]; then
            echo "ERROR: disk staging under ${BACKUP_DEST} also lacks space (${avail_disk} bytes free, database is ${db_size} bytes)." >&2
            echo "Free up space, mount a larger BACKUP_DEST, or set BACKUP_TEMP_DIR to a location with enough room." >&2
            exit 1
        fi
    fi

    if ! env \
        BACKUP_DEST="$BACKUP_DEST" \
        BACKUP_BASENAME="$basename" \
        BACKUP_CLEANUP_PATTERN='pre-migration-*.sql*' \
        BACKUP_RETAIN="$PRE_MIGRATION_BACKUP_RETAIN" \
        BACKUP_TEMP_DIR="$staging_dir" \
        BACKUP_TEMP_TRUSTED_ENCRYPTED="$staging_trusted" \
        BACKUP_COMPRESS="${BACKUP_COMPRESS:-gzip}" \
        PGUSER="$POSTGRES_USER" \
        PGPASSWORD="$POSTGRES_PASSWORD" \
        PGHOST=localhost \
        PGPORT=5432 \
        PGDATABASE="$POSTGRES_DB" \
        LOG_FILE="${LOG_FILE:-/var/log/fortemi/backup.log}" \
        "$BACKUP_SCRIPT_PATH" -d local | tee "$backup_log"; then
        echo "ERROR: verified pre-migration backup failed; aborting before checksum repair and migrations" >&2
        echo "Set PRE_MIGRATION_BACKUP_ACK_NO_BACKUP=true only for constrained environments with an external recovery point." >&2
        exit 1
    fi

    local final_file
    final_file="$(tail -n 1 "$backup_log")"
    echo ">>> Pre-migration backup ready: ${BACKUP_DEST}/${final_file}"
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

# --- Start API first (MCP needs the API for credential validation) ---
echo ">>> Starting Matric API..."
mkdir -p /var/log/matric
echo "  Listening on: ${HOST:-0.0.0.0}:${PORT:-3000}"

# Trap to clean up background processes on exit
cleanup() {
    echo "Shutting down..."
    kill $MCP_PID 2>/dev/null || true
    kill $RENDERER_PID 2>/dev/null || true
    kill $API_PID 2>/dev/null || true
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

# Wait for API to be healthy before starting MCP server
echo ">>> Waiting for API to be healthy..."
API_READY=false
for i in {1..60}; do
    if curl -sf http://localhost:${PORT:-3000}/health >/dev/null 2>&1; then
        echo "  API is healthy!"
        API_READY=true
        break
    fi
    # Check API process is still alive
    if ! kill -0 $API_PID 2>/dev/null; then
        echo "ERROR: API process died during startup"
        exit 1
    fi
    sleep 1
done

if [ "$API_READY" = false ]; then
    echo "WARNING: API health check timed out after 60s, continuing anyway..."
fi

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
MCP_CREDS_VALID=false
if [ -n "$MCP_CLIENT_ID" ] && [ -n "$MCP_CLIENT_SECRET" ]; then
    echo ">>> Validating MCP credentials (client_id: $MCP_CLIENT_ID)..."
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
        "http://localhost:${PORT:-3000}/oauth/introspect" \
        -u "$MCP_CLIENT_ID:$MCP_CLIENT_SECRET" \
        -d "token=startup_validation_check" 2>/dev/null || echo "000")
    if [ "$HTTP_CODE" = "200" ]; then
        MCP_CREDS_VALID=true
        echo "  MCP credentials valid"
    else
        echo "  MCP credentials invalid (HTTP $HTTP_CODE)"
    fi
else
    echo ">>> No MCP credentials configured"
fi

# Auto-register if credentials are missing or invalid
if [ "$MCP_CREDS_VALID" = false ]; then
    echo ">>> Auto-registering MCP OAuth client..."
    REGISTER_RESPONSE=$(curl -s -X POST "http://localhost:${PORT:-3000}/oauth/register" \
        -H "Content-Type: application/json" \
        -d '{"client_name":"MCP Server (auto-registered)","grant_types":["client_credentials"],"scope":"mcp read write"}' 2>/dev/null || echo "")

    # Parse client_id and client_secret from JSON response (no jq dependency)
    NEW_CLIENT_ID=$(echo "$REGISTER_RESPONSE" | grep -o '"client_id":"[^"]*"' | head -1 | cut -d'"' -f4)
    NEW_CLIENT_SECRET=$(echo "$REGISTER_RESPONSE" | grep -o '"client_secret":"[^"]*"' | head -1 | cut -d'"' -f4)

    if [ -n "$NEW_CLIENT_ID" ] && [ -n "$NEW_CLIENT_SECRET" ]; then
        export MCP_CLIENT_ID="$NEW_CLIENT_ID"
        export MCP_CLIENT_SECRET="$NEW_CLIENT_SECRET"

        # Persist credentials on pgdata volume (survives container restarts)
        cat > "$MCP_CREDS_FILE" <<CREDS
MCP_CLIENT_ID="$MCP_CLIENT_ID"
MCP_CLIENT_SECRET="$MCP_CLIENT_SECRET"
CREDS
        chmod 600 "$MCP_CREDS_FILE"

        echo "  Registered MCP client: $MCP_CLIENT_ID"
        echo "  Credentials persisted to $MCP_CREDS_FILE"
        echo ""
        echo "  ================================================================"
        echo "  NOTE: To persist across volume wipes, update your .env file:"
        echo "    MCP_CLIENT_ID=$MCP_CLIENT_ID"
        echo "    MCP_CLIENT_SECRET=$MCP_CLIENT_SECRET"
        echo "  ================================================================"
        echo ""
    else
        echo "  WARNING: MCP client auto-registration failed"
        echo "  Response: $REGISTER_RESPONSE"
        echo "  MCP server will start but token introspection will fail"
        echo "  Fix: manually register via POST /oauth/register"
    fi
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

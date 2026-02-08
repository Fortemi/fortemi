#!/bin/bash
set -e

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

# Check if this is a fresh install (empty data directory)
if [ -z "$(ls -A "$PGDATA" 2>/dev/null)" ]; then
    echo ">>> Initializing PostgreSQL data directory..."

    # Initialize PostgreSQL as postgres user
    su postgres -c "initdb -D $PGDATA --auth-host=md5 --auth-local=trust"

    # Configure PostgreSQL to listen on localhost only (internal)
    echo "listen_addresses = 'localhost'" >> "$PGDATA/postgresql.conf"
    echo "max_connections = 100" >> "$PGDATA/postgresql.conf"

    # Allow local connections
    echo "local all all trust" > "$PGDATA/pg_hba.conf"
    echo "host all all 127.0.0.1/32 md5" >> "$PGDATA/pg_hba.conf"
    echo "host all all ::1/128 md5" >> "$PGDATA/pg_hba.conf"

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
    su postgres -c "psql -c \"CREATE USER ${POSTGRES_USER:-matric} WITH PASSWORD '${POSTGRES_PASSWORD:-matric}' CREATEDB;\""
    su postgres -c "psql -c \"CREATE DATABASE ${POSTGRES_DB:-matric} OWNER ${POSTGRES_USER:-matric};\""

    # Enable required extensions (must be done as superuser)
    echo ">>> Enabling extensions..."
    su postgres -c "psql -d ${POSTGRES_DB:-matric} -c 'CREATE EXTENSION IF NOT EXISTS vector;'"
    su postgres -c "psql -d ${POSTGRES_DB:-matric} -c 'CREATE EXTENSION IF NOT EXISTS postgis;'"
fi

# Ensure required extensions exist (idempotent, must run as superuser before migrations)
echo ">>> Ensuring PostgreSQL extensions..."
su postgres -c "psql -d ${POSTGRES_DB:-matric} -c 'CREATE EXTENSION IF NOT EXISTS vector;'" 2>/dev/null || true
su postgres -c "psql -d ${POSTGRES_DB:-matric} -c 'CREATE EXTENSION IF NOT EXISTS postgis;'" 2>/dev/null || true

# NOTE: Database schema migrations are handled automatically by the API on startup
# via sqlx::migrate!() with _sqlx_migrations tracking table.
# This ensures migrations run exactly once, in order, with proper error handling.
echo ">>> Database migrations will be applied by API on startup"

# Create required directories for file storage and backups
echo ">>> Creating storage directories..."
mkdir -p /var/lib/matric/files
mkdir -p /var/backups/matric-memory
echo "  File storage: /var/lib/matric/files"
echo "  Backup storage: /var/backups/matric-memory"

# --- Start API first (MCP needs the API for credential validation) ---
echo ">>> Starting Matric API..."
mkdir -p /var/log/matric
echo "  Listening on: ${HOST:-0.0.0.0}:${PORT:-3000}"

# Trap to clean up background processes on exit
cleanup() {
    echo "Shutting down..."
    kill $MCP_PID 2>/dev/null || true
    kill $API_PID 2>/dev/null || true
    su postgres -c "pg_ctl -D $PGDATA stop -m fast" 2>/dev/null || true
    exit 0
}
trap cleanup SIGTERM SIGINT

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

# --- Start MCP Server ---
echo ">>> Starting MCP Server..."
cd /app/mcp-server
MCP_TRANSPORT="${MCP_TRANSPORT:-http}" \
PORT="${MCP_PORT:-3001}" \
MATRIC_API_URL="${MATRIC_API_URL:-http://localhost:3000}" \
MCP_CLIENT_ID="$MCP_CLIENT_ID" \
MCP_CLIENT_SECRET="$MCP_CLIENT_SECRET" \
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
echo "  API: http://0.0.0.0:${PORT:-3000}"
echo "  MCP: http://0.0.0.0:${MCP_PORT:-3001}"
echo "  MCP Client ID: ${MCP_CLIENT_ID:-NOT SET}"
echo "========================================"

# Wait for any process to exit
wait -n $API_PID $MCP_PID

# If we get here, one of the processes died
echo "A process exited unexpectedly"
cleanup

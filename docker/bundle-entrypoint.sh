#!/bin/bash
set -e

# bundle-entrypoint.sh - Initialize and run PostgreSQL + matric-api + MCP server
#
# This script:
# 1. Initializes PostgreSQL if data directory is empty
# 2. Starts PostgreSQL
# 3. Waits for PostgreSQL to be ready
# 4. Creates database and enables pgvector extension
# 5. Runs migrations
# 6. Starts MCP server (background)
# 7. Starts matric-api (foreground)

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

# Start MCP server in background
echo ">>> Starting MCP Server..."
mkdir -p /var/log/matric
cd /app/mcp-server
MCP_TRANSPORT="${MCP_TRANSPORT:-http}" \
PORT="${MCP_PORT:-3001}" \
MATRIC_API_URL="${MATRIC_API_URL:-http://localhost:3000}" \
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

# Start matric-api
echo ">>> Starting Matric API..."
echo "  Listening on: ${HOST:-0.0.0.0}:${PORT:-3000}"
echo "  Database: ${DATABASE_URL}"
echo "========================================"

# Trap to clean up background processes on exit
cleanup() {
    echo "Shutting down..."
    kill $MCP_PID 2>/dev/null || true
    su postgres -c "pg_ctl -D $PGDATA stop -m fast" 2>/dev/null || true
    exit 0
}
trap cleanup SIGTERM SIGINT

# Run matric-api in foreground
/app/matric-api &
API_PID=$!

# Wait for any process to exit
wait -n $API_PID $MCP_PID

# If we get here, one of the processes died
echo "A process exited unexpectedly"
cleanup

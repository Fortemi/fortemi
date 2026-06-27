#!/usr/bin/env bash
# rebuild-shard-in-ci.sh — regenerate docker/seed-data/fortemi-docs.shard
# inside CI by standing up a transient Postgres + API stack, importing the
# current source tree via the existing rebuild script, then tearing down.
#
# Usage:
#   scripts/ci/rebuild-shard-in-ci.sh <api-image-tag>
#
# The api-image-tag must be a locally-loaded Docker image (the freshly-built
# API-only image from the same workflow run). The script:
#   1. Builds the matric-testdb image from build/Dockerfile.testdb
#   2. Creates an isolated Docker network
#   3. Starts Postgres and the API on that network
#   4. Waits for /health (the API auto-runs migrations on startup)
#   5. Runs scripts/rebuild-docs-shard.sh against the API
#   6. Tears down all resources
#
# Exits non-zero on any failure so the wrapping job fails loudly rather
# than silently shipping a stale shard.

set -euo pipefail

API_IMAGE="${1:?Usage: $0 <api-image-tag>}"

NETWORK="shard-rebuild-$$"
DB_NAME="shard-rebuild-db-$$"
API_NAME="shard-rebuild-api-$$"
TESTDB_IMAGE="matric-testdb:shard-rebuild"
DB_USER="${SHARD_REBUILD_DB_USER:-matric}"
DB_PASSWORD="${SHARD_REBUILD_DB_PASSWORD:-fortemi-shard-${RANDOM:-0}-$$}"
DB_DATABASE="${SHARD_REBUILD_DB_NAME:-matric}"

# Unique host port per invocation. Both `publish-release` (Gitea) and
# `publish-github` (ghcr.io) jobs in ci-builder.yaml run in parallel on
# the same matric-builder runner; both call this script. A fixed
# `-p 3000:3000` collides — observed in CI run #1509 ("Bind for
# 0.0.0.0:3000 failed: port is already allocated"). Pick a port from a
# wide ephemeral-ish range based on the PID; collision risk is
# negligible across the two parallel jobs.
HOST_PORT=$((30000 + ($$ % 5000)))
API_URL="http://localhost:${HOST_PORT}"

cleanup() {
    echo ">>> Tearing down shard-rebuild stack..."
    docker rm -f "$API_NAME" "$DB_NAME" 2>/dev/null || true
    docker network rm "$NETWORK" 2>/dev/null || true
}
trap cleanup EXIT

# 1. Build testdb image (cached across runs on the matric-builder runner)
echo ">>> Building $TESTDB_IMAGE..."
docker build -f build/Dockerfile.testdb -t "$TESTDB_IMAGE" .

# 2. Isolated network so API can reach DB by container name
echo ">>> Creating network $NETWORK..."
docker network create "$NETWORK"

# 3. Start Postgres
echo ">>> Starting Postgres ($DB_NAME)..."
docker run -d --name "$DB_NAME" --network "$NETWORK" \
    -e POSTGRES_USER="$DB_USER" \
    -e POSTGRES_PASSWORD="$DB_PASSWORD" \
    -e POSTGRES_DB="$DB_DATABASE" \
    "$TESTDB_IMAGE"

# Wait for Postgres ready
for i in $(seq 1 30); do
    if docker exec "$DB_NAME" pg_isready -U "$DB_USER" -d "$DB_DATABASE" >/dev/null 2>&1; then
        echo ">>> Postgres ready after ${i}s"
        break
    fi
    if [ "$i" = "30" ]; then
        echo "ERROR: Postgres did not become ready within 30s"
        docker logs "$DB_NAME"
        exit 1
    fi
    sleep 1
done

# 4. Start API container
# DISABLE_SUPPORT_MEMORY=true: skip the seed step in entrypoint, since we're
#   the ones generating the seed
# OLLAMA_BASE pointed at an unreachable host: rebuild script imports notes
#   with revision_mode=none, so no inference calls are made
# RATE_LIMIT_ENABLED=false: the rebuild fires ~200 POST /api/v1/notes calls
#   back-to-back; the bundle's default 100 req / 60 s limit (RATE_LIMIT_*)
#   would 429 every request after the first 100. Without this, ~half the
#   import fails silently, the export step then errors, and the bundle build
#   aborts. Observed in CI run #1486 (PR #653 first end-to-end run).
# REQUIRE_AUTH=false + I_UNDERSTAND_NO_AUTH=true (ADR-094, fortemi/fortemi#709):
#   Post-ADR-094 the API defaults to fail-closed and rebuild-docs-shard.sh
#   issues unauthenticated POST /api/v1/archives + POST /api/v1/notes. This
#   is a throwaway shard-build harness; explicitly opt into anonymous mode
#   so the import calls succeed. Observed failing in CI run #1698 with
#   "ERROR: Failed to create archive" (401 from the new fail-closed default).
echo ">>> Starting API ($API_NAME) on host port ${HOST_PORT} from $API_IMAGE..."
docker run -d --name "$API_NAME" --network "$NETWORK" \
    -p "${HOST_PORT}:3000" \
    -e DATABASE_URL="$(printf '%s%s:%s@%s:5432/%s' 'postgres://' "$DB_USER" "$DB_PASSWORD" "$DB_NAME" "$DB_DATABASE")" \
    -e DISABLE_SUPPORT_MEMORY=true \
    -e MATRIC_INFERENCE_DEFAULT=ollama \
    -e OLLAMA_BASE=http://disabled.invalid:11434 \
    -e RATE_LIMIT_ENABLED=false \
    -e REQUIRE_AUTH=false \
    -e I_UNDERSTAND_NO_AUTH=true \
    -e ISSUER_URL="${API_URL}" \
    "$API_IMAGE"

# 5. Wait for /health (API runs sqlx migrations on startup — can take time
#    on a fresh DB with 100+ migrations)
echo ">>> Waiting for API /health at ${API_URL}..."
for i in $(seq 1 60); do
    if curl -fsS "${API_URL}/health" >/dev/null 2>&1; then
        echo ">>> API ready after ${i}s"
        break
    fi
    if [ "$i" = "60" ]; then
        echo "ERROR: API did not become healthy within 120s"
        echo "--- API logs ---"
        docker logs "$API_NAME" | tail -100
        echo "--- DB logs ---"
        docker logs "$DB_NAME" | tail -50
        exit 1
    fi
    sleep 2
done

# 6. Run the rebuild — operates on the host filesystem from the workspace
#    root, writes docker/seed-data/fortemi-docs.shard
echo ">>> Running scripts/rebuild-docs-shard.sh against ${API_URL}..."
scripts/rebuild-docs-shard.sh "${API_URL}"

# 7. Verify the shard was actually written and is non-trivially sized
SHARD="docker/seed-data/fortemi-docs.shard"
if [ ! -f "$SHARD" ]; then
    echo "ERROR: $SHARD was not produced"
    exit 1
fi
SIZE=$(stat -c%s "$SHARD")
if [ "$SIZE" -lt 102400 ]; then
    echo "ERROR: $SHARD suspiciously small (${SIZE} bytes); aborting to avoid shipping a broken shard"
    exit 1
fi
echo ">>> Shard regenerated: $SHARD ($((SIZE / 1024)) KB)"

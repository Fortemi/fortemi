#!/usr/bin/env bash
set -euo pipefail

# Full reset — stop containers, remove volumes, redeploy
# WARNING: This destroys all data. Back up first.
# Params: INSTALL_DIR

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"

cd "${INSTALL_DIR}"

echo "WARNING: This will destroy all Fortémi data (database, uploads, etc.)"
echo "Press Ctrl+C within 5 seconds to abort..."
sleep 5

echo "Stopping containers and removing volumes..."
docker compose -f docker-compose.bundle.yml down -v

echo "Restarting clean..."
docker compose -f docker-compose.bundle.yml up -d

echo "Waiting for API to become healthy..."
RETRIES=30
until curl -sf http://localhost:3000/health >/dev/null 2>&1; do
    RETRIES=$((RETRIES - 1))
    if [ "${RETRIES}" -le 0 ]; then
        echo "ERROR: API did not become healthy after reset."
        exit 1
    fi
    sleep 2
done

echo "Reset complete. Fortémi is running with a clean database."

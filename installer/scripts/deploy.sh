#!/usr/bin/env bash
set -euo pipefail

# Pull and start the Fortémi Docker bundle
# Params: INSTALL_DIR

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"

cd "${INSTALL_DIR}"

echo "Validating rendered bundle exposure policy..."
bash scripts/validate-bundle-exposure.sh .env

echo "Pulling Docker images..."
docker compose -f docker-compose.bundle.yml pull

echo "Starting Fortémi services..."
docker compose -f docker-compose.bundle.yml up -d

echo "Waiting for API to become healthy..."
RETRIES=30
until curl -sf http://localhost:3000/health >/dev/null 2>&1; do
    RETRIES=$((RETRIES - 1))
    if [ "${RETRIES}" -le 0 ]; then
        echo "ERROR: API did not become healthy within 60 seconds."
        echo "Check logs: docker compose -f docker-compose.bundle.yml logs -f"
        exit 1
    fi
    sleep 2
done

echo "Fortémi is running."
echo "  API:  http://localhost:3000"
echo "  MCP:  http://localhost:3001"
echo "  Health: http://localhost:3000/health"

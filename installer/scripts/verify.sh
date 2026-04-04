#!/usr/bin/env bash
set -euo pipefail

# Verify all Fortémi services are healthy
# Params: INSTALL_DIR

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"
ERRORS=0

cd "${INSTALL_DIR}"

echo "=== Fortémi Health Check ==="

# API health
echo -n "API (port 3000): "
if HEALTH=$(curl -sf http://localhost:3000/health 2>&1); then
    echo "OK"
else
    echo "FAIL"
    ERRORS=$((ERRORS + 1))
fi

# MCP server
echo -n "MCP (port 3001): "
if curl -sf http://localhost:3001/ >/dev/null 2>&1; then
    echo "OK"
else
    echo "FAIL (may need OAuth setup)"
    ERRORS=$((ERRORS + 1))
fi

# Docker containers
echo ""
echo "Container status:"
docker compose -f docker-compose.bundle.yml ps --format "table {{.Name}}\t{{.Status}}" 2>/dev/null || \
    docker compose -f docker-compose.bundle.yml ps

echo ""
if [ "${ERRORS}" -eq 0 ]; then
    echo "All checks passed."
else
    echo "${ERRORS} check(s) failed. Review logs:"
    echo "  docker compose -f docker-compose.bundle.yml logs -f"
    exit 1
fi

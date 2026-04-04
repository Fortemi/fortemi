#!/usr/bin/env bash
set -euo pipefail

# Check that required ports are available before deploying
# Ports: 3000 (API), 3001 (MCP)

REQUIRED_PORTS=(3000 3001)
ERRORS=0

echo "Checking port availability..."

for port in "${REQUIRED_PORTS[@]}"; do
    if command -v ss &>/dev/null; then
        # Linux
        if ss -tlnp 2>/dev/null | grep -q ":${port} "; then
            echo "ERROR: Port ${port} is already in use:"
            ss -tlnp | grep ":${port} "
            ERRORS=$((ERRORS + 1))
        else
            echo "  Port ${port}: available"
        fi
    elif command -v lsof &>/dev/null; then
        # macOS
        if lsof -iTCP:${port} -sTCP:LISTEN &>/dev/null; then
            echo "ERROR: Port ${port} is already in use:"
            lsof -iTCP:${port} -sTCP:LISTEN
            ERRORS=$((ERRORS + 1))
        else
            echo "  Port ${port}: available"
        fi
    else
        echo "  Port ${port}: skipped (no ss or lsof)"
    fi
done

if [ "${ERRORS}" -gt 0 ]; then
    echo ""
    echo "${ERRORS} port(s) in use. Stop the conflicting process or change the port mapping."
    exit 1
fi

echo "All ports available."

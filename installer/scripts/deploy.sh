#!/usr/bin/env bash
set -euo pipefail

# Pull and start the Fortémi Docker bundle
# Params: INSTALL_DIR

INSTALL_DIR="${INSTALL_DIR:?INSTALL_DIR is required}"

cd "${INSTALL_DIR}"

# Compose normalizes its project name to lowercase [a-z0-9_-], starting
# with an alphanumeric character. Query daemon labels: the retired service
# is absent from the current Compose model and would otherwise be orphaned.
compose_project="${COMPOSE_PROJECT_NAME:-}"
if [ -z "$compose_project" ]; then
    # Compose also reads COMPOSE_PROJECT_NAME from .env. Use the rendered
    # project name when available, falling back to the installation directory.
    if ! compose_project="$(docker compose -f docker-compose.bundle.yml config --format json |
        python3 -c 'import json, pathlib, sys; print(json.load(sys.stdin).get("name") or pathlib.Path.cwd().name)')"; then
        echo "ERROR: cannot resolve Compose project name; deployment stopped." >&2
        exit 1
    fi
fi
compose_project="$(printf '%s' "$compose_project" |
    LC_ALL=C tr '[:upper:]' '[:lower:]' | LC_ALL=C tr -cd 'a-z0-9_-' | sed 's/^[_-]*//')"
if [ -z "$compose_project" ]; then
    echo "ERROR: cannot determine Compose project name; set COMPOSE_PROJECT_NAME." >&2
    exit 1
fi
if ! autoheal_containers="$(docker ps -a \
    --filter "label=com.docker.compose.project=${compose_project}" \
    --filter 'label=com.docker.compose.service=autoheal' --format '{{.Names}}')"; then
    echo "ERROR: cannot check for retired autoheal containers; deployment stopped." >&2
    exit 1
fi
if [ -n "$autoheal_containers" ]; then
    mapfile -t autoheal_names <<< "$autoheal_containers"
    echo "ERROR: retired autoheal containers remain in Compose project ${compose_project} and may retain Docker socket access." >&2
    echo "Retire them explicitly before retrying deployment. Run:" >&2
    { printf 'docker rm -f'; printf ' %q' "${autoheal_names[@]}"; printf '\n'; } >&2
    echo "See docs/content/operators-guide.md#upgrade-retire-existing-autoheal-containers." >&2
    exit 1
fi

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

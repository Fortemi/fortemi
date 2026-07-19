#!/usr/bin/env bash
# Render and validate the effective Docker bundle exposure policy (#989).

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
env_file="${1:-${FORTEMI_ENV_FILE:-$root/.env}}"
compose_file="${2:-${FORTEMI_COMPOSE_FILE:-$root/docker-compose.bundle.yml}}"

if [[ ! -f "$env_file" ]]; then
  env_file=/dev/null
fi
if ! docker compose version >/dev/null 2>&1; then
  echo "ERROR: Docker Compose v2 is required for bundle exposure validation" >&2
  exit 2
fi

rendered=$(mktemp)
render_error=$(mktemp)
trap 'rm -f "$rendered" "$render_error"' EXIT

if ! active_profiles=$(
  docker compose \
    --env-file "$env_file" \
    -f "$compose_file" \
    config --environment 2>"$render_error" |
    sed -n 's/^COMPOSE_PROFILES=//p'
); then
  echo "ERROR: Docker bundle environment did not render successfully" >&2
  cat "$render_error" >&2
  exit 2
fi

if ! docker compose \
  --env-file "$env_file" \
  -f "$compose_file" \
  config --format json >"$rendered" 2>"$render_error"; then
  echo "ERROR: Docker bundle did not render successfully" >&2
  cat "$render_error" >&2
  exit 2
fi

python3 "$root/scripts/ci/verify-bundle-exposure.py" \
  --active-profiles "$active_profiles" \
  "$rendered"

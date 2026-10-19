#!/usr/bin/env bash
# Install validation-only ownership wrapper; release jobs never call this.
set -euo pipefail
[[ "${CI_RESOURCE_SCOPE:?}" =~ ^[1-9][0-9]*$ ]]
[[ "${CI_RESOURCE_ATTEMPT:?}" =~ ^[1-9][0-9]*$ ]]
tooling="${CI_SAFETY_TOOLS:?}/scripts/ci"
run_root="$(python3 "$tooling/configure-run-root.py")"
real_docker="$(command -v docker || true)"
if [[ -n "$real_docker" ]]; then
  ln -sf "$tooling/owned-docker.py" "$run_root/bin/docker"
  printf 'CI_DOCKER_REAL=%s\n' "$real_docker" >> "$GITHUB_ENV"
  printf '%s\n' "$run_root/bin" >> "$GITHUB_PATH"
fi
printf 'TMPDIR=%s\n' "$run_root/tmp" >> "$GITHUB_ENV"

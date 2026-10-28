#!/usr/bin/env bash
# Install validation-only ownership wrapper; release jobs never call this.
set -euo pipefail
[[ "${CI_RESOURCE_SCOPE:?}" =~ ^[1-9][0-9]*$ ]]
[[ "${CI_RESOURCE_ATTEMPT:?}" =~ ^[1-9][0-9]*$ ]]
tooling="${CI_SAFETY_TOOLS:?}/scripts/ci"
# Bootstrap the run-local target in the setup process only. Later steps receive
# the selected target through GITHUB_ENV, without a competing job env value.
if [[ -n "${FORTEMI_CARGO_RUN_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="$FORTEMI_CARGO_RUN_TARGET_DIR"
fi
run_root="$(python3 "$tooling/configure-run-root.py")"
real_docker="$(command -v docker || true)"
if [[ -n "$real_docker" ]]; then
  ln -sf "$tooling/owned-docker.py" "$run_root/bin/docker"
  printf 'CI_DOCKER_REAL=%s\n' "$real_docker" >> "$GITHUB_ENV"
  printf '%s\n' "$run_root/bin" >> "$GITHUB_PATH"
fi
export TMPDIR="$run_root/tmp"
printf 'TMPDIR=%s\n' "$TMPDIR" >> "$GITHUB_ENV"
# The workflow-revision helper owns cache policy; candidate source cannot select
# a trusted namespace. Docker credentials and install prefixes remain run-local.
real_cargo="$(command -v cargo || true)"
if [[ -n "$real_cargo" && "${FORTEMI_CACHE_CI:-0}" = 1 ]]; then
  printf 'FORTEMI_REAL_CARGO=%s\n' "$real_cargo" >> "$GITHUB_ENV"
  printf 'FORTEMI_CACHE_VARIANT=%s\n' "${FORTEMI_CACHE_VARIANT:-}" >> "$GITHUB_ENV"
  python3 "$tooling/cargo-cache.py" setup
  ln -sf "$tooling/cargo-cache.py" "$run_root/bin/cargo"
  printf '%s\n' "$run_root/bin" >> "$GITHUB_PATH"
fi

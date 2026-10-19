#!/usr/bin/env bash
# Explicit CI teardown: never depend on a successfully installed PATH wrapper.
set -euo pipefail
if [[ -n "${CI_RESOURCE_SCOPE:-}" ]]; then
  helper="${CI_SAFETY_TOOLS:-}/scripts/ci/owned-docker.py"
  if [[ -f "$helper" ]]; then
    python3 "$helper" "$@"
  else
    echo '::warning::Kept Docker resources: workflow-revision removal helper unavailable.'
  fi
else
  # Preserve nonscoped local/release callers' existing cleanup behavior.
  docker "$@"
fi

#!/usr/bin/env bash
# Gitea job concurrency cancels pending main jobs on 1.27.2; use a daemon fence.
set -euo pipefail
[[ "${GITHUB_REF:?}" == refs/heads/main ]]
exec python3 scripts/ci/main-side-effect.py "$@"

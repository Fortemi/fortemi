#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOCK_PATH="scripts/ci/mutsu-build-lock.sh"

for workflow in \
  .gitea/workflows/publish-sidecar.yml \
  .gitea/workflows/suite-platform-contract.yml; do
  path="$ROOT/$workflow"
  grep -qF "$LOCK_PATH" "$path"
  grep -qF -- "--timeout 3600" "$path"
  grep -qF "IdentitiesOnly yes" "$path"
  grep -qF "StrictHostKeyChecking yes" "$path"
  grep -qF "PasswordAuthentication no" "$path"
  if grep -qF "StrictHostKeyChecking no" "$path"; then
    echo "$workflow permits an unverified mutsu host key" >&2
    exit 1
  fi
done

[[ "$(grep -cF -- "--timeout 3600" "$ROOT/.gitea/workflows/publish-sidecar.yml")" -eq 2 ]]
[[ "$(grep -cF -- "--timeout 3600" "$ROOT/.gitea/workflows/suite-platform-contract.yml")" -eq 2 ]]

echo "mutsu workflow serialization checks passed"

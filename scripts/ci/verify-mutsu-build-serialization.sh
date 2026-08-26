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
  if grep -Eq -- '--label "[^"]* [^"]*"' "$path"; then
    echo "$workflow uses a remote lock label that SSH will split" >&2
    exit 1
  fi
  if grep -qF "StrictHostKeyChecking no" "$path"; then
    echo "$workflow permits an unverified mutsu host key" >&2
    exit 1
  fi
done

[[ "$(grep -cF -- "--timeout 3600" "$ROOT/.gitea/workflows/publish-sidecar.yml")" -eq 2 ]]
[[ "$(grep -cF -- "--timeout 3600" "$ROOT/.gitea/workflows/suite-platform-contract.yml")" -eq 2 ]]

for workflow in \
  .gitea/workflows/build-builder.yaml \
  .gitea/workflows/ci-builder.yaml \
  .gitea/workflows/publish-sidecar.yml \
  .gitea/workflows/suite-platform-contract.yml \
  .gitea/workflows/test.yml; do
  path="$ROOT/$workflow"
  grep -qF "group: fortemi-shared-runner" "$path"
  grep -qF "cancel-in-progress: false" "$path"
done

sidecar="$ROOT/.gitea/workflows/publish-sidecar.yml"
awk '
  /^  build-linux-arm64:/ { in_job = 1; next }
  /^  [a-zA-Z0-9_-]+:/ { in_job = 0 }
  in_job && /needs: \[build-linux\]/ { found = 1 }
  END { exit(found ? 0 : 1) }
' "$sidecar"
awk '
  /^  build-macos:/ { in_job = 1; next }
  /^  [a-zA-Z0-9_-]+:/ { in_job = 0 }
  in_job && /needs: \[build-linux-arm64\]/ { found = 1 }
  END { exit(found ? 0 : 1) }
' "$sidecar"

echo "mutsu workflow serialization checks passed"

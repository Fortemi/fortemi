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

# Gitea scopes concurrency groups to one workflow. The capacity-one runner is
# protected across workflow files by a success-gated dispatch chain, not by the
# shared spelling of those groups.
ci_workflow="$ROOT/.gitea/workflows/ci-builder.yaml"
test_workflow="$ROOT/.gitea/workflows/test.yml"
sidecar_workflow="$ROOT/.gitea/workflows/publish-sidecar.yml"
suite_workflow="$ROOT/.gitea/workflows/suite-platform-contract.yml"
docsite_build="$ROOT/.gitea/workflows/docsite-build.yml"
docsite_deploy="$ROOT/.gitea/workflows/docsite-deploy.yml"
builder_workflow="$ROOT/.gitea/workflows/build-builder.yaml"

grep -qF "handoff-tests:" "$ci_workflow"
grep -qF "actions/workflows/test.yml/dispatches" "$ci_workflow"
grep -qF "handoff-sidecar:" "$test_workflow"
grep -qF "actions/workflows/publish-sidecar.yml/dispatches" "$test_workflow"
grep -qF "handoff-suite:" "$sidecar_workflow"
grep -qF "actions/workflows/suite-platform-contract.yml/dispatches" "$sidecar_workflow"
grep -qF "handoff-docsite:" "$suite_workflow"
grep -qF "actions/workflows/docsite-deploy.yml/dispatches" "$suite_workflow"

for manually_chained in "$test_workflow" "$sidecar_workflow" "$suite_workflow" "$docsite_build" "$docsite_deploy"; do
  if grep -qE '^  push:' "$manually_chained"; then
    echo "${manually_chained#"$ROOT/"} bypasses the capacity-one dispatch chain" >&2
    exit 1
  fi
done

if grep -qF "actions/workflows/test.yml/dispatches" "$builder_workflow"; then
  echo "build-builder dispatches Tests in parallel with CI" >&2
  exit 1
fi
# Gitea 1.25 treats an unqualified workflow-dispatch ref as a branch name.
# Preserve refs/tags/v* across every handoff so release runs stay bound to the
# signed tag rather than failing lookup or silently selecting a same-named branch.
for dispatcher in \
  "$builder_workflow" \
  "$ci_workflow" \
  "$test_workflow" \
  "$sidecar_workflow" \
  "$suite_workflow"; do
  # Assert the literal workflow expression.
  # shellcheck disable=SC2016
  grep -qF -- '--arg ref "${GITHUB_REF:?GITHUB_REF is required}"' "$dispatcher"
  if grep -qF -- '--arg ref "${GITHUB_REF_NAME:' "$dispatcher"; then
    echo "${dispatcher#"$ROOT/"} truncates a workflow-dispatch ref" >&2
    exit 1
  fi
done
grep -qF "needs: [coverage, build-testdb]" "$test_workflow"
grep -qF "needs: [slow-tests]" "$test_workflow"
grep -qF "needs: [fast-tests, integration-tests, coverage, slow-tests, validate-intel-overlay]" "$test_workflow"
grep -qF "github.ref == 'refs/heads/main' && github.event_name == 'workflow_dispatch'" "$sidecar_workflow"
grep -qF "needs: [linux-x86_64]" "$suite_workflow"

# Every Titan-backed CI job is an explicit single-capacity chain. The immutable
# Knowledge Shard matrix stays independent because it is routed to a separate
# general runner, then joins the chain at publication.
for dependency in \
  "needs: [runner-capacity]" \
  "needs: [verify-release-ref]" \
  "needs: [build-testdb]" \
  "needs: [lint]" \
  "needs: [audit]" \
  "needs: [deny]" \
  "needs: [auth-consumer]" \
  "needs: [mcp-lockfile-sync]" \
  "needs: [mcp-server-tests]" \
  "needs: build" \
  "needs: [build-image]" \
  "needs: test-container" \
  "needs: [integration-test]" \
  "sync-github-source" \
  "main-validation" \
  "publish-release"; do
  grep -qF "$dependency" "$ci_workflow"
done
grep -qF "if: github.event_name != 'pull_request'" "$ci_workflow"

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

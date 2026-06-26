#!/usr/bin/env bash
set -euo pipefail

root="${1:-.}"
shift || true

node "$root/scripts/ci/docs-contract.cjs" --root="$root" "$@"

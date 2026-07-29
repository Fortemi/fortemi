#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_SCRIPT="$ROOT/scripts/ci/mutsu-build-lock.sh"
TEST_ROOT="$(mktemp -d)"
READY="$TEST_ROOT/ready"

cleanup() {
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

export MUTSU_BUILD_LOCK_ROOT="$TEST_ROOT/locks"
export MUTSU_BUILD_LOCK_POLL_SECONDS=1

"$LOCK_SCRIPT" --label first --timeout 10 -- \
  bash -c 'touch "$1"; sleep 3' _ "$READY" &
holder_pid=$!

for _ in $(seq 1 20); do
  [[ -f "$READY" ]] && break
  sleep 0.1
done
[[ -f "$READY" ]] || {
  echo "lock holder did not start" >&2
  exit 1
}

if "$LOCK_SCRIPT" --label contender --timeout 1 -- true; then
  echo "concurrent lock acquisition unexpectedly succeeded" >&2
  exit 1
fi
wait "$holder_pid"

"$LOCK_SCRIPT" --label successor --timeout 2 -- true

mkdir -p "$MUTSU_BUILD_LOCK_ROOT/mutsu-heavy-build"
printf '%s\n' 999999 >"$MUTSU_BUILD_LOCK_ROOT/mutsu-heavy-build/pid"
printf '%s\n' stale >"$MUTSU_BUILD_LOCK_ROOT/mutsu-heavy-build/token"
printf '%s\n' stale-test >"$MUTSU_BUILD_LOCK_ROOT/mutsu-heavy-build/label"
"$LOCK_SCRIPT" --label stale-recovery --timeout 2 -- true

echo "mutsu build lock tests passed"

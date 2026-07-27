#!/usr/bin/env bash
set -euo pipefail

FORTEMI_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITE_ROOT="${FORTEMI_SUITE_ROOT:-$(cd "$FORTEMI_ROOT/.." && pwd)}"
MODE="${1:-focused}"

run() {
  local label="$1"
  local repo="$2"
  shift 2
  printf '\n==> %s\n' "$label"
  (
    cd "$SUITE_ROOT/$repo"
    "$@"
  )
}

for path in \
  "fortemi/Cargo.toml" \
  "fortemi-react/pnpm-lock.yaml" \
  "HotM/ui/package-lock.json" \
  "aiwg/package-lock.json"; do
  if [[ ! -f "$SUITE_ROOT/$path" ]]; then
    printf 'missing sibling suite file: %s\n' "$path" >&2
    exit 1
  fi
done

if [[ "$MODE" == "--install" ]]; then
  run "Install AIWG dependencies" "aiwg" npm ci
  run "Install HotM UI dependencies" "HotM/ui" npm ci
  run "Install React dependencies" "fortemi-react" pnpm install --frozen-lockfile
  MODE="focused"
fi

if [[ "$MODE" != "focused" ]]; then
  printf 'usage: %s [focused|--install]\n' "${BASH_SOURCE[0]}" >&2
  exit 2
fi

run "Fortemi full-v1 filesystem/server route" "fortemi" \
  cargo test -p matric-api --bin matric-api \
  shard_full_v1_route_round_trip_preserves_every_component_and_required_blob -- --nocapture

run "Fortemi sidecar rollback" "fortemi" \
  cargo test -p matric-api --bin matric-api \
  shard_optional_sidecars_round_trip_and_fail_without_partial_storage -- --nocapture

run "Fortemi filesystem refcounts" "fortemi" \
  cargo test -p matric-db --test file_storage_blob_refcount_test -- --nocapture

run "React/PGlite recovery cells" "fortemi-react" \
  pnpm --filter @fortemi/core exec vitest run \
  src/__tests__/shard/blob-roundtrip.test.ts \
  src/__tests__/shard/full-v1-store.test.ts \
  src/__tests__/aiwg-index-full-shard.test.ts \
  src/__tests__/shard/fortemi-core-v1-consumer-cell.test.ts

run "AIWG released bridge" "aiwg" \
  npx vitest run --config config/vitest.config.js \
  test/unit/artifacts/fortemi-shard-export.test.ts \
  test/unit/sessions/knowledge-shard.test.ts

run "HotM asset clients" "HotM/ui" \
  npm exec vitest run -- \
  src/api/__tests__/knowledgeShard.test.ts \
  src/api/__tests__/backup.test.ts \
  src/api/__tests__/attachments.test.ts \
  src/services/__tests__/uploadStore.test.ts \
  src/services/__tests__/tusUploader.test.ts

printf '\nFocused lifecycle validation passed: 135 selected tests.\n'
printf 'Open: live desktop/browser, restart, concurrency, resumability, and performance scenarios.\n'

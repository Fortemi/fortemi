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

run "Fortemi bounded TUS finalization prefix" "fortemi" \
  cargo test -p matric-api --bin matric-api \
  tus_finalization_inspection_enforces_caps_and_bounds_prefix_memory -- --nocapture

run "Fortemi streamed TUS request rollback" "fortemi" \
  cargo test -p matric-api --bin matric-api \
  tus_request_body_streams_frames_and_rolls_back_overrun_residue -- --nocapture

run "Fortemi bounded verified filesystem copy" "fortemi" \
  cargo test -p matric-db \
  filesystem_write_file_copies_with_bounded_identity_verification -- --nocapture

run "Fortemi sidecar staging/promotion/journal storage crash windows" "fortemi" \
  cargo test -p matric-db shard_ -- --nocapture

run "Fortemi live restart/crash/concurrency" "fortemi" \
  cargo test -p matric-api --bin matric-api al_sys -- --nocapture

run "Fortemi AL-PERF01 receipt scaffold" "fortemi" \
  scripts/ci/verify-asset-lifecycle-perf-receipt.sh target/al-perf01-receipt.json

run "Fortemi process-isolated TUS memory receipt" "fortemi" \
  scripts/ci/verify-tus-bounded-memory-receipt.sh target/al-tus-bounded-memory-receipt.json

run "Fortemi AL-PERF01 receipt bundle verifier" "fortemi" \
  python3 -m unittest tests/test_verify_al_perf01_receipt_bundle.py

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

run "HotM receipt verifiers" "HotM/ui" \
  npm exec vitest run -- \
  scripts/verify-live-asset-metrics.test.js \
  scripts/write-receipt-artifact-manifest.test.js \
  scripts/verify-tauri-command-core-receipt.test.js

run "HotM Tauri local-file command core" "HotM/ui/src-tauri" \
  env TAURI_CONFIG='{"bundle":{"externalBin":[]}}' \
  cargo test local_file_ -- --nocapture

run "HotM Tauri native dialog command boundaries" "HotM/ui/src-tauri" \
  env TAURI_CONFIG='{"bundle":{"externalBin":[]}}' \
  cargo test native_ -- --nocapture

run "HotM live-assets default schema guard" "HotM/ui" \
  npm run test:e2e:live-assets

printf '\nFocused lifecycle validation passed.\n'
printf 'Open: launched native desktop GUI artifacts, mid-syscall/power-loss crash matrix, approved policy budgets/RPO-RTO/max-size, whole asset-lifecycle process RSS proof, and immutable receipt publication.\n'

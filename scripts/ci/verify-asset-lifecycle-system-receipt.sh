#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/target/al-sys04-05-asset-lifecycle-receipt.json}"

if [[ "$OUT" != /* ]]; then
  OUT="$ROOT/$OUT"
fi

cd "$ROOT"

cargo test -p matric-api --bin matric-api al_sys -- \
  --nocapture --test-threads=1
cargo test -p matric-api --bin matric-api \
  shard_optional_sidecars_round_trip_and_fail_without_partial_storage -- \
  --nocapture --test-threads=1
cargo test -p matric-db --lib shard_import_journal_ -- \
  --nocapture --test-threads=1
cargo test -p matric-db --test file_storage_blob_refcount_test -- \
  --nocapture --test-threads=1

python3 scripts/ci/verify-asset-lifecycle-system-receipt.py --write "$OUT"

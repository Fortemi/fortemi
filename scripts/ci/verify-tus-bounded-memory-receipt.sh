#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/target/al-tus-bounded-memory-receipt.json}"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "process-isolated TUS memory receipt requires Linux /proc" >&2
  exit 2
fi

if [[ "$OUT" != /* ]]; then
  OUT="$ROOT/$OUT"
fi

mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"

(
  cd "$ROOT"
  FORTEMI_AL_TUS_MEMORY_RECEIPT_PATH="$OUT" \
    cargo test -p matric-api --bin matric-api \
      al_perf01_process_isolated_streamed_tus_memory_guard -- --nocapture
)

python3 - "$OUT" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
receipt = json.loads(path.read_text(encoding="utf-8"))
failures = []

if receipt.get("schemaVersion") != "fortemi.asset-lifecycle.tus-memory-receipt.v1":
    failures.append("schemaVersion mismatch")
if receipt.get("status") != "local-process-isolated-memory-guard-passed":
    failures.append("status mismatch")
if receipt.get("scope") != "Fortemi server filesystem TUS PATCH and finalization":
    failures.append("scope mismatch")
if receipt.get("profile") != "filesystem-tus-v1":
    failures.append("profile mismatch")

corpora = receipt.get("corpora", {})
expected_corpora = {
    "small": 1_048_576,
    "large": 104_857_600,
}
for name, expected_bytes in expected_corpora.items():
    corpus = corpora.get(name, {})
    if corpus.get("corpusBytes") != expected_bytes:
        failures.append(f"{name} corpus byte count mismatch")
    if corpus.get("requestChunkBytes") != 65_536:
        failures.append(f"{name} request chunk mismatch")
    if corpus.get("finalFileBytes") != expected_bytes:
        failures.append(f"{name} final file byte count mismatch")
    if corpus.get("stagingDiskBytesAfter") != 0:
        failures.append(f"{name} staging residue must be zero")
    expected_hash = corpus.get("expectedContentHash")
    if (
        not isinstance(expected_hash, str)
        or not expected_hash.startswith("blake3:")
        or len(expected_hash) != len("blake3:") + 64
    ):
        failures.append(f"{name} content hash missing or invalid")
    for key in (
        "rssResidentBytesBefore",
        "rssResidentBytesAfter",
        "rssHighWaterBytesBefore",
        "rssHighWaterBytesAfter",
    ):
        if not isinstance(corpus.get(key), int) or corpus[key] <= 0:
            failures.append(f"{name} {key} missing or invalid")
    before = corpus.get("rssHighWaterBytesBefore")
    after = corpus.get("rssHighWaterBytesAfter")
    delta = corpus.get("rssHighWaterDeltaBytes")
    if (
        isinstance(before, int)
        and isinstance(after, int)
        and isinstance(delta, int)
        and delta != max(0, after - before)
    ):
        failures.append(f"{name} RSS high-water delta arithmetic mismatch")
    if not isinstance(corpus.get("uploadMillis"), int) or corpus["uploadMillis"] < 0:
        failures.append(f"{name} upload timing missing or invalid")
    expected_oracles = {
        "databaseHashSizeRefcountPassed": True,
        "finalFileHashSizePassed": True,
        "stagingCleanupPassed": True,
    }
    if corpus.get("oracles") != expected_oracles:
        failures.append(f"{name} storage oracles mismatch")
    repro = corpus.get("reproducibility", {})
    if repro.get("targetOs") != "linux":
        failures.append(f"{name} target OS must be linux")
    for key in ("targetArch", "storageFilesystem", "tusFilesystem"):
        if not isinstance(repro.get(key), str) or not repro[key].strip():
            failures.append(f"{name} reproducibility.{key} missing or invalid")

guard = receipt.get("memoryGuard", {})
expected_guard = {
    "requestChunkBytes": 65_536,
    "tusSafetyPrefixMaxBytes": 8_192,
    "filesystemCopyBufferBytes": 65_536,
    "maxLargeRssHighWaterDeltaBytes": 67_108_864,
    "maxGrowthOverSmallBytes": 33_554_432,
    "approvedPolicy": False,
}
for key, expected in expected_guard.items():
    if guard.get(key) != expected:
        failures.append(f"memoryGuard.{key} mismatch")
small_delta = corpora.get("small", {}).get("rssHighWaterDeltaBytes")
large_delta = corpora.get("large", {}).get("rssHighWaterDeltaBytes")
observed_growth = guard.get("observedGrowthOverSmallBytes")
if isinstance(small_delta, int) and isinstance(large_delta, int):
    if observed_growth != max(0, large_delta - small_delta):
        failures.append("observed growth over small corpus mismatch")
    if large_delta > expected_guard["maxLargeRssHighWaterDeltaBytes"]:
        failures.append("large RSS high-water delta exceeds guard")
    if observed_growth > expected_guard["maxGrowthOverSmallBytes"]:
        failures.append("RSS growth over small corpus exceeds guard")

expected_claims = {
    "processIsolatedTusPathMemoryGuardPassed": True,
    "wholeAssetLifecycleProcessBoundedMemoryPassed": False,
    "approvedPeakRssBudgetPassed": False,
    "nonFilesystemBackendsPassed": False,
    "scannerPathPassed": False,
    "suiteWidePortability": False,
}
if receipt.get("claims") != expected_claims:
    failures.append("claim scope mismatch")

if failures:
    print("process-isolated TUS memory receipt verification failed", file=sys.stderr)
    for failure in failures:
        print(f"- {failure}", file=sys.stderr)
    raise SystemExit(1)

print(f"process-isolated TUS memory receipt verified: {path}")
PY

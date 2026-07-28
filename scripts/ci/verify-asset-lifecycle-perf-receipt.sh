#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/target/al-perf01-receipt.json}"
CORPUS_BYTES="${FORTEMI_AL_PERF_CORPUS_BYTES:-1048576}"
REPETITIONS="${FORTEMI_AL_PERF_REPETITIONS:-1}"

if [[ "$OUT" != /* ]]; then
  OUT="$ROOT/$OUT"
fi

mkdir -p "$(dirname "$OUT")"
rm -f "$OUT" "$OUT".repeat-*.json

if ! [[ "$REPETITIONS" =~ ^[0-9]+$ ]] || [ "$REPETITIONS" -lt 1 ]; then
  echo "FORTEMI_AL_PERF_REPETITIONS must be a positive integer" >&2
  exit 2
fi

run_receipt() {
  local receipt_path="$1"
  (
    cd "$ROOT"
    FORTEMI_AL_PERF_CORPUS_BYTES="$CORPUS_BYTES" \
    FORTEMI_AL_PERF_RECEIPT_PATH="$receipt_path" \
      cargo test -p matric-api --bin matric-api \
        al_perf01_configurable_corpus_records_receipt_and_limit_plus_one_gate -- --nocapture
  )
}

run_receipt "$OUT"

for index in $(seq 2 "$REPETITIONS"); do
  run_receipt "$OUT.repeat-$index.json"
done

python3 - "$OUT" "$CORPUS_BYTES" "$REPETITIONS" <<'PY'
import json
import os
import sys
from pathlib import Path

path = Path(sys.argv[1])
expected_bytes = int(sys.argv[2])
repetitions = int(sys.argv[3])
receipts = [json.loads(path.read_text())]
receipt_paths = [path]
for index in range(2, repetitions + 1):
    repeat_path = Path(f"{path}.repeat-{index}.json")
    receipts.append(json.loads(repeat_path.read_text()))
    receipt_paths.append(repeat_path)

failures = []
receipt = receipts[0]
if receipt.get("schemaVersion") != "fortemi.asset-lifecycle.perf-receipt.v1":
    failures.append("schemaVersion mismatch")
if receipt.get("status") != "local-focused-measurement-passed":
    failures.append("status mismatch")
if receipt.get("profile") != "2.0.0/full-v1":
    failures.append("profile mismatch")
if receipt.get("corpus", {}).get("bytes") != expected_bytes:
    failures.append("corpus byte count mismatch")
if (
    not isinstance(receipt.get("corpus", {}).get("archiveEntryCount"), int)
    or receipt["corpus"]["archiveEntryCount"] <= 0
    or receipt["corpus"]["archiveEntryCount"] > 64
):
    failures.append("archive entry count missing or invalid")
if not receipt.get("limits", {}).get("limitPlusOneRejectedBeforeTusMutation"):
    failures.append("limit-plus-one gate was not recorded as passed")
if receipt.get("limits", {}).get("maxArchiveEntries") != 64:
    failures.append("maximum archive entry limit mismatch")

bounded_io = receipt.get("boundedIo", {})
expected_bounded_io = {
    "scope": "Fortemi server filesystem TUS and 2.0.0/full-v1 sidecars",
    "tusRequestBodyStreaming": True,
    "tusFinalizationWholePayloadBuffered": False,
    "tusSafetyPrefixMaxBytes": 8 * 1024,
    "filesystemCopyBufferBytes": 64 * 1024,
    "fullV1SidecarImportSpooledToDisk": True,
    "fullV1SidecarStreamBufferBytes": 64 * 1024,
    "wholeTestProcessBoundedMemoryPassed": False,
}
if bounded_io != expected_bounded_io:
    failures.append("bounded server TUS/full-v1 sidecar I/O contract mismatch")

metrics = receipt.get("metrics", {})
for key in [
    "uploadMillis",
    "downloadMillis",
    "exportMillis",
    "importMillis",
    "recoveryDownloadMillis",
    "uploadBytesPerSecond",
    "downloadBytesPerSecond",
    "exportArchiveBytesPerSecond",
    "importArchiveBytesPerSecond",
    "archiveBytes",
]:
    if not isinstance(metrics.get(key), int) or metrics[key] < 0:
        failures.append(f"metric {key} missing or invalid")

recovery = receipt.get("recovery", {})
if recovery.get("rpoLostBytesAfterSignedFullV1Export") != 0:
    failures.append("RPO lost-byte oracle must be zero for the focused signed full-v1 recovery")
if recovery.get("rpoDigestMatchesExportedSidecar") is not True:
    failures.append("RPO digest sidecar match must be recorded")
if recovery.get("timedRpoRtoRecorded") is not True:
    failures.append("timed RPO/RTO section was not recorded")
if not isinstance(recovery.get("rtoMillisImportAndVerifyDownload"), int) or recovery["rtoMillisImportAndVerifyDownload"] < 0:
    failures.append("RTO import+verify timing missing or invalid")

repro = receipt.get("reproducibility", {})
if not repro.get("deterministicCorpusAlgorithm"):
    failures.append("deterministic corpus algorithm not recorded")
if repro.get("command") != "scripts/ci/verify-asset-lifecycle-perf-receipt.sh":
    failures.append("receipt command mismatch")
expect_clean_checkout = os.environ.get("FORTEMI_AL_PERF_EXPECT_CLEAN_CHECKOUT") == "1"
if repro.get("cleanCheckoutReproduced") is not expect_clean_checkout:
    failures.append("clean-checkout reproduction claim mismatch")
for key in ["packageVersion", "targetOs", "targetArch", "storageFilesystem", "tusFilesystem"]:
    if not isinstance(repro.get(key), str) or not repro[key].strip():
        failures.append(f"reproducibility.{key} missing or invalid")
if not isinstance(repro.get("worktreeDirty"), bool):
    failures.append("reproducibility worktreeDirty missing or invalid")
elif expect_clean_checkout and repro.get("worktreeDirty") is not False:
    failures.append("clean-checkout reproduction requires a clean git worktree")
if repro.get("gitCommit") is not None and (
    not isinstance(repro.get("gitCommit"), str) or not repro["gitCommit"]
):
    failures.append("reproducibility gitCommit invalid")

expect_approved_budgets = os.environ.get("FORTEMI_AL_PERF_EXPECT_APPROVED_BUDGETS") == "1"
expected_max_corpus_raw = os.environ.get("FORTEMI_AL_PERF_EXPECT_MAX_CORPUS_BYTES")
expected_max_corpus = None
if expected_max_corpus_raw:
    try:
        expected_max_corpus = int(expected_max_corpus_raw)
    except ValueError:
        failures.append("FORTEMI_AL_PERF_EXPECT_MAX_CORPUS_BYTES must be an integer")
    if expected_max_corpus is not None and expected_max_corpus <= 0:
        failures.append("FORTEMI_AL_PERF_EXPECT_MAX_CORPUS_BYTES must be positive")
expected_max_sidecar_count_raw = os.environ.get(
    "FORTEMI_AL_PERF_EXPECT_MAX_SIDECAR_COUNT"
)
expected_max_sidecar_count = None
if expected_max_sidecar_count_raw:
    try:
        expected_max_sidecar_count = int(expected_max_sidecar_count_raw)
    except ValueError:
        failures.append("FORTEMI_AL_PERF_EXPECT_MAX_SIDECAR_COUNT must be an integer")
    if expected_max_sidecar_count is not None and expected_max_sidecar_count <= 0:
        failures.append("FORTEMI_AL_PERF_EXPECT_MAX_SIDECAR_COUNT must be positive")
budgets = receipt.get("budgets", {})
if not isinstance(budgets.get("approvedGateEnabled"), bool):
    failures.append("budget gate enabled flag missing")
if not isinstance(budgets.get("approvedGateComplete"), bool):
    failures.append("budget gate complete flag missing")
if budgets.get("approvedGatePassed") is not expect_approved_budgets:
    failures.append("approved budget gate pass state mismatch")
if expect_approved_budgets and budgets.get("approvedGateEnabled") is not True:
    failures.append("approved budget gate must be enabled")
if expect_approved_budgets and budgets.get("approvedGateComplete") is not True:
    failures.append("approved budget gate must be complete")
for key in [
    "uploadMillis",
    "downloadMillis",
    "exportMillis",
    "importMillis",
    "recoveryRtoMillis",
    "rssHighWaterDeltaBytes",
    "storageAndTusDiskBytesAfter",
]:
    evaluation = budgets.get(key, {})
    if not isinstance(evaluation.get("actual"), int) or evaluation["actual"] < 0:
        failures.append(f"budget actual {key} missing or invalid")
    if not isinstance(evaluation.get("passed"), bool):
        failures.append(f"budget pass flag {key} missing")
    if expect_approved_budgets:
        if not isinstance(evaluation.get("max"), int) or evaluation["max"] <= 0:
            failures.append(f"approved budget max {key} missing or invalid")
        if evaluation.get("passed") is not True:
            failures.append(f"approved budget {key} did not pass")
if recovery.get("approvedRpoRtoBudgetPassed") is not expect_approved_budgets:
    failures.append("approved RPO/RTO budget claim mismatch")

claims = receipt.get("claims", {})
if claims.get("approvedBudgetsPassed") is not expect_approved_budgets:
    failures.append("approvedBudgetsPassed claim mismatch")
if claims.get("rpoRtoPassed") is not expect_approved_budgets:
    failures.append("rpoRtoPassed claim mismatch")
if claims.get("boundedServerTusAndFullV1SidecarIoPassed") is not True:
    failures.append("bounded server TUS/full-v1 sidecar I/O claim missing")
expected_max_corpus_passed = (
    expected_max_corpus is not None and expected_bytes >= expected_max_corpus
)
if receipt.get("limits", {}).get("expectedMaxCorpusBytes") != expected_max_corpus:
    failures.append("expected max corpus byte gate mismatch")
if claims.get("maxCorpusPassed") is not expected_max_corpus_passed:
    failures.append("maxCorpusPassed claim mismatch")
expected_max_count_passed = (
    expected_max_sidecar_count is not None
    and receipt.get("corpus", {}).get("segmentCount") == expected_max_sidecar_count
    and receipt.get("corpus", {}).get("archiveEntryCount") == 64
)
if receipt.get("limits", {}).get("expectedMaxSidecarCount") != expected_max_sidecar_count:
    failures.append("expected max sidecar count gate mismatch")
if claims.get("maxCountCorpusPassed") is not expected_max_count_passed:
    failures.append("maxCountCorpusPassed claim mismatch")
for key in ["hotmBrowserDesktopPassed", "suiteWidePortability"]:
    if claims.get(key) is not False:
        failures.append(f"claim {key} must remain false in focused scaffold")

stable_paths = [
    ("schemaVersion",),
    ("profile",),
    ("corpus", "bytes"),
    ("corpus", "segmentCount"),
    ("corpus", "segmentMaxBytes"),
    ("corpus", "archiveEntryCount"),
    ("corpus", "sha256"),
    ("corpus", "blake3Segments"),
    ("limits", "maxUploadSizeBytes"),
    ("limits", "expectedMaxCorpusBytes"),
    ("limits", "expectedMaxSidecarCount"),
    ("limits", "maxArchiveEntries"),
    ("limits", "limitPlusOneRejectedBeforeTusMutation"),
    ("boundedIo",),
    ("recovery", "rpoLostBytesAfterSignedFullV1Export"),
    ("recovery", "rpoDigestMatchesExportedSidecar"),
    ("recovery", "timedRpoRtoRecorded"),
    ("reproducibility", "deterministicCorpusAlgorithm"),
    ("reproducibility", "command"),
    ("reproducibility", "packageVersion"),
    ("reproducibility", "targetOs"),
    ("reproducibility", "targetArch"),
    ("reproducibility", "storageFilesystem"),
    ("reproducibility", "tusFilesystem"),
    ("claims", "hundredMiBCorpusPassed"),
    ("claims", "maxCorpusPassed"),
    ("claims", "maxCountCorpusPassed"),
    ("claims", "boundedServerTusAndFullV1SidecarIoPassed"),
    ("claims", "hotmBrowserDesktopPassed"),
    ("claims", "suiteWidePortability"),
]

def get_path(obj, parts):
    value = obj
    for part in parts:
        value = value.get(part) if isinstance(value, dict) else None
    return value

if repetitions > 1:
    baseline = receipts[0]
    for repeat_index, repeated in enumerate(receipts[1:], start=2):
        for parts in stable_paths:
            if get_path(repeated, parts) != get_path(baseline, parts):
                failures.append(
                    f"repeatability mismatch in run {repeat_index}: {'.'.join(parts)}"
                )
    if len({get_path(r, ("corpus", "sha256")) for r in receipts}) != 1:
        failures.append("repeatability corpus sha256 mismatch")

if failures:
    print("AL-PERF01 receipt verification failed:", file=sys.stderr)
    for failure in failures:
        print(f"- {failure}", file=sys.stderr)
    sys.exit(1)

verified_paths = ", ".join(str(receipt_path) for receipt_path in receipt_paths)
print(f"AL-PERF01 receipt verified: {verified_paths}")
PY

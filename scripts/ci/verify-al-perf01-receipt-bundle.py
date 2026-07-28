#!/usr/bin/env python3
"""Validate the AL-PERF01 CI artifact bundle before upload."""

from __future__ import annotations

import json
import hashlib
import sys
from pathlib import Path
from typing import Any


DEFAULT_DIR = Path("target/al-perf01-receipts")
DEFAULT_BYTES = 1_048_576
MAX_CORPUS_BYTES = 104_857_600
MAX_COUNT_CORPUS_BYTES = 28 * 1_048_576
MAX_SIDECAR_COUNT = 28
MAX_ARCHIVE_ENTRIES = 64
MANIFEST_NAME = "manifest.json"
MANIFEST_SCHEMA = "fortemi.asset-lifecycle.perf-receipt-bundle-manifest.v1"
NOT_CONFIGURED_SCHEMA = "fortemi.asset-lifecycle.approved-budget-receipt.v1"
PERF_SCHEMA = "fortemi.asset-lifecycle.perf-receipt.v1"
STATISTICAL_NAME = "observed-percentiles.json"
STATISTICAL_SCHEMA = "fortemi.asset-lifecycle.observed-percentiles.v1"
STATISTICAL_SAMPLE_COUNT = 5
TUS_MEMORY_SCHEMA = "fortemi.asset-lifecycle.tus-memory-receipt.v1"
PROFILE = "2.0.0/full-v1"
APPROVED_BUDGET_FIELDS = (
    "uploadMillis",
    "downloadMillis",
    "exportMillis",
    "importMillis",
    "recoveryRtoMillis",
    "rssHighWaterDeltaBytes",
    "storageAndTusDiskBytesAfter",
)
OBSERVATION_FIELDS = (
    ("uploadMillis", ("metrics", "uploadMillis"), "milliseconds", "lower-is-better"),
    ("downloadMillis", ("metrics", "downloadMillis"), "milliseconds", "lower-is-better"),
    ("exportMillis", ("metrics", "exportMillis"), "milliseconds", "lower-is-better"),
    ("importMillis", ("metrics", "importMillis"), "milliseconds", "lower-is-better"),
    (
        "recoveryRtoMillis",
        ("recovery", "rtoMillisImportAndVerifyDownload"),
        "milliseconds",
        "lower-is-better",
    ),
    (
        "uploadBytesPerSecond",
        ("metrics", "uploadBytesPerSecond"),
        "bytes-per-second",
        "higher-is-better",
    ),
    (
        "downloadBytesPerSecond",
        ("metrics", "downloadBytesPerSecond"),
        "bytes-per-second",
        "higher-is-better",
    ),
    (
        "exportArchiveBytesPerSecond",
        ("metrics", "exportArchiveBytesPerSecond"),
        "bytes-per-second",
        "higher-is-better",
    ),
    (
        "importArchiveBytesPerSecond",
        ("metrics", "importArchiveBytesPerSecond"),
        "bytes-per-second",
        "higher-is-better",
    ),
    (
        "rssHighWaterDeltaBytes",
        ("metrics", "rssHighWaterDeltaBytes"),
        "bytes",
        "lower-is-better",
    ),
    (
        "storageAndTusDiskBytesAfter",
        ("metrics", "storageAndTusDiskBytesAfter"),
        "bytes",
        "lower-is-better",
    ),
)


def load_json(path: Path, failures: list[str]) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        failures.append(f"{path.name}: cannot read: {error}")
    except json.JSONDecodeError as error:
        failures.append(f"{path.name}: invalid JSON: {error}")
    return None


def require_file(directory: Path, name: str, failures: list[str]) -> Path | None:
    path = directory / name
    if not path.is_file():
        failures.append(f"missing required receipt {name}")
        return None
    return path


def read_path(value: dict[str, Any], parts: tuple[str, ...]) -> Any:
    current: Any = value
    for part in parts:
        current = current.get(part) if isinstance(current, dict) else None
    return current


def repeatability_names() -> tuple[str, ...]:
    return ("repeatability.json",) + tuple(
        f"repeatability.json.repeat-{index}.json"
        for index in range(2, STATISTICAL_SAMPLE_COUNT + 1)
    )


def nearest_rank(values: list[int], percentile: int) -> int:
    ordered = sorted(values)
    rank = (percentile * len(ordered) + 99) // 100
    return ordered[max(1, rank) - 1]


def build_statistical_receipt(
    directory: Path,
    failures: list[str],
) -> dict[str, Any] | None:
    receipts: list[dict[str, Any]] = []
    source_receipts = []
    for name in repeatability_names():
        path = require_file(directory, name, failures)
        if path is None:
            continue
        receipt = load_json(path, failures)
        if receipt is None:
            continue
        receipts.append(receipt)
        contents = path.read_bytes()
        source_receipts.append(
            {
                "name": name,
                "bytes": len(contents),
                "sha256": hashlib.sha256(contents).hexdigest(),
            }
        )
    if len(receipts) != STATISTICAL_SAMPLE_COUNT:
        return None

    if any(
        receipt.get("reproducibility", {}).get("cleanCheckoutReproduced") is not True
        or receipt.get("reproducibility", {}).get("worktreeDirty") is not False
        for receipt in receipts
    ):
        failures.append(
            f"{STATISTICAL_NAME}: every source receipt must prove a clean checkout"
        )
        return None

    baseline = receipts[0]
    observations: dict[str, Any] = {}
    for name, path, unit, direction in OBSERVATION_FIELDS:
        values = [read_path(receipt, path) for receipt in receipts]
        if any(
            not isinstance(value, int) or isinstance(value, bool) or value < 0
            for value in values
        ):
            failures.append(
                f"{STATISTICAL_NAME}: source metric {'.'.join(path)} missing or invalid"
            )
            continue
        observations[name] = {
            "unit": unit,
            "direction": direction,
            "minimum": min(values),
            "maximum": max(values),
            "p50": nearest_rank(values, 50),
            "p95": nearest_rank(values, 95),
            "p99": nearest_rank(values, 99),
        }
    if len(observations) != len(OBSERVATION_FIELDS):
        return None

    reproducibility = baseline.get("reproducibility", {})
    git_commit = reproducibility.get("gitCommit")
    if (
        not isinstance(git_commit, str)
        or len(git_commit) != 40
        or any(character not in "0123456789abcdef" for character in git_commit)
    ):
        failures.append(f"{STATISTICAL_NAME}: exact git commit is missing or invalid")
        return None

    identity = {
        "issue": "Fortemi/fortemi#1094",
        "profile": PROFILE,
        "corpusBytes": baseline.get("corpus", {}).get("bytes"),
        "corpusSha256": baseline.get("corpus", {}).get("sha256"),
        "gitCommit": git_commit,
        "packageVersion": reproducibility.get("packageVersion"),
        "targetOs": reproducibility.get("targetOs"),
        "targetArch": reproducibility.get("targetArch"),
        "storageFilesystem": reproducibility.get("storageFilesystem"),
        "tusFilesystem": reproducibility.get("tusFilesystem"),
    }
    trend_key = hashlib.sha256(
        json.dumps(identity, separators=(",", ":"), sort_keys=True).encode("utf-8")
    ).hexdigest()
    return {
        "schemaVersion": STATISTICAL_SCHEMA,
        "status": "clean-checkout-observed-percentiles-passed",
        "issue": "Fortemi/fortemi#1094",
        "profile": PROFILE,
        "sampleCount": STATISTICAL_SAMPLE_COUNT,
        "method": {
            "name": "nearest-rank",
            "percentiles": [50, 95, 99],
        },
        "identity": identity,
        "trend": {
            "key": trend_key,
            "artifactRetentionDays": 30,
        },
        "sourceReceipts": source_receipts,
        "observations": observations,
        "claims": {
            "cleanCheckoutSamplesPassed": True,
            "observedP50P95P99Recorded": True,
            "approvedPercentileBudgetsPassed": False,
            "historicalTrendComparisonPassed": False,
            "wholeAssetLifecycleProcessBoundedMemoryPassed": False,
            "suiteWidePortability": False,
        },
    }


def write_statistical_receipt(directory: Path) -> list[str]:
    failures: list[str] = []
    receipt = build_statistical_receipt(directory, failures)
    if receipt is not None and not failures:
        (directory / STATISTICAL_NAME).write_text(
            json.dumps(receipt, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    return failures


def validate_statistical_receipt(directory: Path, failures: list[str]) -> None:
    path = require_file(directory, STATISTICAL_NAME, failures)
    if path is None:
        return
    actual = load_json(path, failures)
    if actual is None:
        return
    expected_failures: list[str] = []
    expected = build_statistical_receipt(directory, expected_failures)
    failures.extend(expected_failures)
    if expected is not None and actual != expected:
        failures.append(
            f"{STATISTICAL_NAME}: content does not match exact source-receipt recomputation"
        )


def require_perf_receipt(
    directory: Path,
    name: str,
    failures: list[str],
    *,
    corpus_bytes: int | None = None,
    expected_max_corpus_bytes: int | None = None,
    max_corpus_passed: bool | None = None,
    expected_max_sidecar_count: int | None = None,
    max_count_corpus_passed: bool | None = None,
    clean_checkout: bool | None = None,
    approved_budgets: bool | None = None,
) -> dict[str, Any] | None:
    path = require_file(directory, name, failures)
    if path is None:
        return None
    receipt = load_json(path, failures)
    if receipt is None:
        return None

    if receipt.get("schemaVersion") != PERF_SCHEMA:
        failures.append(f"{name}: schemaVersion mismatch")
    if receipt.get("status") != "local-focused-measurement-passed":
        failures.append(f"{name}: status mismatch")
    if receipt.get("profile") != PROFILE:
        failures.append(f"{name}: profile mismatch")

    if corpus_bytes is not None and receipt.get("corpus", {}).get("bytes") != corpus_bytes:
        failures.append(f"{name}: corpus byte count mismatch")
    if (
        expected_max_corpus_bytes is not None
        and receipt.get("limits", {}).get("expectedMaxCorpusBytes")
        != expected_max_corpus_bytes
    ):
        failures.append(f"{name}: expected max corpus byte gate mismatch")
    if (
        max_corpus_passed is not None
        and receipt.get("claims", {}).get("maxCorpusPassed") is not max_corpus_passed
    ):
        failures.append(f"{name}: maxCorpusPassed claim mismatch")
    corpus = receipt.get("corpus", {})
    archive_entry_count = corpus.get("archiveEntryCount")
    if (
        not isinstance(archive_entry_count, int)
        or archive_entry_count <= 0
        or archive_entry_count > MAX_ARCHIVE_ENTRIES
    ):
        failures.append(f"{name}: archive entry count missing or invalid")
    limits = receipt.get("limits", {})
    if limits.get("maxArchiveEntries") != MAX_ARCHIVE_ENTRIES:
        failures.append(f"{name}: maximum archive entry limit mismatch")
    if (
        expected_max_sidecar_count is not None
        and limits.get("expectedMaxSidecarCount") != expected_max_sidecar_count
    ):
        failures.append(f"{name}: expected max sidecar count gate mismatch")
    if (
        max_count_corpus_passed is not None
        and receipt.get("claims", {}).get("maxCountCorpusPassed")
        is not max_count_corpus_passed
    ):
        failures.append(f"{name}: maxCountCorpusPassed claim mismatch")
    if max_count_corpus_passed is True and (
        corpus.get("segmentCount") != expected_max_sidecar_count
        or archive_entry_count != MAX_ARCHIVE_ENTRIES
    ):
        failures.append(f"{name}: maximum-count corpus boundary mismatch")
    if (
        clean_checkout is not None
        and receipt.get("reproducibility", {}).get("cleanCheckoutReproduced")
        is not clean_checkout
    ):
        failures.append(f"{name}: clean-checkout claim mismatch")
    repro = receipt.get("reproducibility", {})
    for key in ("packageVersion", "targetOs", "targetArch", "storageFilesystem", "tusFilesystem"):
        value = repro.get(key)
        if not isinstance(value, str) or not value.strip():
            failures.append(f"{name}: reproducibility.{key} missing or invalid")
    if clean_checkout is True and receipt.get("reproducibility", {}).get("worktreeDirty") is not False:
        failures.append(f"{name}: clean-checkout receipt requires worktreeDirty=false")
    if (
        approved_budgets is not None
        and receipt.get("claims", {}).get("approvedBudgetsPassed") is not approved_budgets
    ):
        failures.append(f"{name}: approvedBudgetsPassed claim mismatch")
    if (
        approved_budgets is not None
        and receipt.get("claims", {}).get("rpoRtoPassed") is not approved_budgets
    ):
        failures.append(f"{name}: rpoRtoPassed claim mismatch")
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
    if receipt.get("boundedIo") != expected_bounded_io:
        failures.append(f"{name}: bounded server TUS/full-v1 sidecar I/O contract mismatch")
    if (
        receipt.get("claims", {}).get("boundedServerTusAndFullV1SidecarIoPassed")
        is not True
    ):
        failures.append(f"{name}: bounded server TUS/full-v1 sidecar I/O claim missing")
    if approved_budgets is True:
        validate_approved_budget_fields(name, receipt, failures)
    for claim in ("hotmBrowserDesktopPassed", "suiteWidePortability"):
        if receipt.get("claims", {}).get(claim) is not False:
            failures.append(f"{name}: claim {claim} must remain false")

    return receipt


def validate_approved_budget_fields(
    name: str,
    receipt: dict[str, Any],
    failures: list[str],
) -> None:
    budgets = receipt.get("budgets", {})
    if budgets.get("approvedGateEnabled") is not True:
        failures.append(f"{name}: approved budget gate must be enabled")
    if budgets.get("approvedGateComplete") is not True:
        failures.append(f"{name}: approved budget gate must be complete")
    if budgets.get("approvedGatePassed") is not True:
        failures.append(f"{name}: approved budget gate must pass")
    for field in APPROVED_BUDGET_FIELDS:
        evaluation = budgets.get(field, {})
        if not isinstance(evaluation.get("actual"), int) or evaluation["actual"] < 0:
            failures.append(f"{name}: budget actual {field} missing or invalid")
        if not isinstance(evaluation.get("max"), int) or evaluation["max"] <= 0:
            failures.append(f"{name}: approved budget max {field} missing or invalid")
        if evaluation.get("passed") is not True:
            failures.append(f"{name}: approved budget {field} did not pass")
    if receipt.get("recovery", {}).get("approvedRpoRtoBudgetPassed") is not True:
        failures.append(f"{name}: approved RPO/RTO budget must pass")


def validate_tus_memory_receipt(directory: Path, failures: list[str]) -> None:
    name = "tus-bounded-memory.json"
    path = require_file(directory, name, failures)
    if path is None:
        return
    receipt = load_json(path, failures)
    if receipt is None:
        return
    if receipt.get("schemaVersion") != TUS_MEMORY_SCHEMA:
        failures.append(f"{name}: schemaVersion mismatch")
    if receipt.get("status") != "local-process-isolated-memory-guard-passed":
        failures.append(f"{name}: status mismatch")
    if receipt.get("scope") != "Fortemi server filesystem TUS PATCH and finalization":
        failures.append(f"{name}: scope mismatch")
    if receipt.get("profile") != "filesystem-tus-v1":
        failures.append(f"{name}: profile mismatch")

    corpora = receipt.get("corpora", {})
    for corpus_name, expected_bytes in (
        ("small", DEFAULT_BYTES),
        ("large", MAX_CORPUS_BYTES),
    ):
        corpus = corpora.get(corpus_name, {})
        if corpus.get("corpusBytes") != expected_bytes:
            failures.append(f"{name}: {corpus_name} corpus byte count mismatch")
        if corpus.get("requestChunkBytes") != 64 * 1024:
            failures.append(f"{name}: {corpus_name} request chunk mismatch")
        if corpus.get("finalFileBytes") != expected_bytes:
            failures.append(f"{name}: {corpus_name} final file byte count mismatch")
        if corpus.get("stagingDiskBytesAfter") != 0:
            failures.append(f"{name}: {corpus_name} staging residue must be zero")
        if corpus.get("oracles") != {
            "databaseHashSizeRefcountPassed": True,
            "finalFileHashSizePassed": True,
            "stagingCleanupPassed": True,
        }:
            failures.append(f"{name}: {corpus_name} storage oracles mismatch")
        before = corpus.get("rssHighWaterBytesBefore")
        after = corpus.get("rssHighWaterBytesAfter")
        delta = corpus.get("rssHighWaterDeltaBytes")
        if (
            not isinstance(before, int)
            or before <= 0
            or not isinstance(after, int)
            or after < before
            or not isinstance(delta, int)
            or delta != after - before
        ):
            failures.append(f"{name}: {corpus_name} RSS high-water evidence invalid")
        repro = corpus.get("reproducibility", {})
        if repro.get("targetOs") != "linux":
            failures.append(f"{name}: {corpus_name} target OS must be linux")
        for key in ("targetArch", "storageFilesystem", "tusFilesystem"):
            value = repro.get(key)
            if not isinstance(value, str) or not value.strip():
                failures.append(
                    f"{name}: {corpus_name} reproducibility.{key} missing or invalid"
                )

    guard = receipt.get("memoryGuard", {})
    expected_guard = {
        "requestChunkBytes": 64 * 1024,
        "tusSafetyPrefixMaxBytes": 8 * 1024,
        "filesystemCopyBufferBytes": 64 * 1024,
        "maxLargeRssHighWaterDeltaBytes": 64 * 1024 * 1024,
        "maxGrowthOverSmallBytes": 32 * 1024 * 1024,
        "approvedPolicy": False,
    }
    for key, expected in expected_guard.items():
        if guard.get(key) != expected:
            failures.append(f"{name}: memoryGuard.{key} mismatch")
    small_delta = corpora.get("small", {}).get("rssHighWaterDeltaBytes")
    large_delta = corpora.get("large", {}).get("rssHighWaterDeltaBytes")
    observed_growth = guard.get("observedGrowthOverSmallBytes")
    if isinstance(small_delta, int) and isinstance(large_delta, int):
        if observed_growth != max(0, large_delta - small_delta):
            failures.append(f"{name}: observed growth mismatch")
        if large_delta > expected_guard["maxLargeRssHighWaterDeltaBytes"]:
            failures.append(f"{name}: large RSS high-water delta exceeds guard")
        if observed_growth > expected_guard["maxGrowthOverSmallBytes"]:
            failures.append(f"{name}: RSS growth over small corpus exceeds guard")
    if receipt.get("claims") != {
        "processIsolatedTusPathMemoryGuardPassed": True,
        "wholeAssetLifecycleProcessBoundedMemoryPassed": False,
        "approvedPeakRssBudgetPassed": False,
        "nonFilesystemBackendsPassed": False,
        "scannerPathPassed": False,
        "suiteWidePortability": False,
    }:
        failures.append(f"{name}: claim scope mismatch")


def validate_repeatability(directory: Path, failures: list[str]) -> None:
    receipts = [
        require_perf_receipt(
            directory,
            name,
            failures,
            corpus_bytes=DEFAULT_BYTES,
            max_corpus_passed=False,
            clean_checkout=True,
            approved_budgets=False,
        )
        for name in repeatability_names()
    ]
    if any(receipt is None for receipt in receipts):
        return
    complete_receipts = [receipt for receipt in receipts if receipt is not None]
    first = complete_receipts[0]
    stable_paths = (
        ("schemaVersion",),
        ("profile",),
        ("corpus", "bytes"),
        ("corpus", "segmentCount"),
        ("corpus", "segmentMaxBytes"),
        ("corpus", "archiveEntryCount"),
        ("corpus", "sha256"),
        ("corpus", "blake3Segments"),
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
        ("reproducibility", "gitCommit"),
        ("reproducibility", "cleanCheckoutReproduced"),
        ("reproducibility", "worktreeDirty"),
        ("claims", "hundredMiBCorpusPassed"),
        ("claims", "maxCorpusPassed"),
        ("claims", "maxCountCorpusPassed"),
        ("claims", "boundedServerTusAndFullV1SidecarIoPassed"),
        ("claims", "hotmBrowserDesktopPassed"),
        ("claims", "suiteWidePortability"),
    )
    for index, receipt in enumerate(complete_receipts[1:], start=2):
        for path in stable_paths:
            if read_path(first, path) != read_path(receipt, path):
                failures.append(
                    f"repeatability mismatch in sample {index}: {'.'.join(path)}"
                )


def validate_budget_branch(directory: Path, failures: list[str]) -> None:
    approved = directory / "approved-budget.json"
    not_configured = directory / "approved-budget.not-configured.json"
    if approved.is_file() == not_configured.is_file():
        failures.append("expected exactly one approved-budget.json or approved-budget.not-configured.json")
        return
    if approved.is_file():
        require_perf_receipt(
            directory,
            "approved-budget.json",
            failures,
            corpus_bytes=DEFAULT_BYTES,
            max_corpus_passed=False,
            clean_checkout=False,
            approved_budgets=True,
        )
        return
    receipt = load_json(not_configured, failures)
    if receipt is None:
        return
    if receipt.get("schemaVersion") != NOT_CONFIGURED_SCHEMA:
        failures.append("approved-budget.not-configured.json: schemaVersion mismatch")
    if receipt.get("status") != "not-configured":
        failures.append("approved-budget.not-configured.json: status mismatch")
    reason = receipt.get("reason")
    if not isinstance(reason, str) or "FORTEMI_AL_PERF_MAX_*" not in reason:
        failures.append("approved-budget.not-configured.json: reason must name FORTEMI_AL_PERF_MAX_*")


def allowed_receipt_names() -> set[str]:
    return {
        "default.json",
        *repeatability_names(),
        STATISTICAL_NAME,
        "clean-checkout.json",
        "max-corpus-100mib.json",
        "max-count-28-sidecars.json",
        "tus-bounded-memory.json",
        "approved-budget.json",
        "approved-budget.not-configured.json",
    }


def receipt_file_names(directory: Path) -> list[str]:
    allowed = allowed_receipt_names()
    return sorted(path.name for path in directory.glob("*.json") if path.name in allowed)


def manifest_for_bundle(directory: Path) -> dict[str, Any]:
    files = []
    for name in receipt_file_names(directory):
        path = directory / name
        contents = path.read_bytes()
        files.append(
            {
                "name": name,
                "bytes": len(contents),
                "sha256": hashlib.sha256(contents).hexdigest(),
            }
        )
    return {
        "schemaVersion": MANIFEST_SCHEMA,
        "issue": "Fortemi/fortemi#1094",
        "profile": PROFILE,
        "files": files,
    }


def write_manifest(directory: Path) -> None:
    if not directory.is_dir():
        return
    manifest = manifest_for_bundle(directory)
    (directory / MANIFEST_NAME).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def validate_manifest(directory: Path, failures: list[str]) -> None:
    path = require_file(directory, MANIFEST_NAME, failures)
    if path is None:
        return
    manifest = load_json(path, failures)
    if manifest is None:
        return
    if manifest.get("schemaVersion") != MANIFEST_SCHEMA:
        failures.append("manifest.json: schemaVersion mismatch")
    if manifest.get("issue") != "Fortemi/fortemi#1094":
        failures.append("manifest.json: issue mismatch")
    if manifest.get("profile") != PROFILE:
        failures.append("manifest.json: profile mismatch")
    files = manifest.get("files")
    if not isinstance(files, list):
        failures.append("manifest.json: files must be an array")
        return

    expected = manifest_for_bundle(directory).get("files", [])
    if files != expected:
        failures.append("manifest.json: file list, byte count, or sha256 mismatch")


def validate_bundle(directory: Path) -> list[str]:
    failures: list[str] = []
    if not directory.is_dir():
        return [f"receipt bundle directory does not exist: {directory}"]

    require_perf_receipt(
        directory,
        "default.json",
        failures,
        corpus_bytes=DEFAULT_BYTES,
        max_corpus_passed=False,
        clean_checkout=False,
        approved_budgets=False,
    )
    validate_repeatability(directory, failures)
    validate_statistical_receipt(directory, failures)
    require_perf_receipt(
        directory,
        "clean-checkout.json",
        failures,
        corpus_bytes=DEFAULT_BYTES,
        max_corpus_passed=False,
        clean_checkout=True,
        approved_budgets=False,
    )
    require_perf_receipt(
        directory,
        "max-corpus-100mib.json",
        failures,
        corpus_bytes=MAX_CORPUS_BYTES,
        expected_max_corpus_bytes=MAX_CORPUS_BYTES,
        max_corpus_passed=True,
        clean_checkout=False,
        approved_budgets=False,
    )
    require_perf_receipt(
        directory,
        "max-count-28-sidecars.json",
        failures,
        corpus_bytes=MAX_COUNT_CORPUS_BYTES,
        expected_max_sidecar_count=MAX_SIDECAR_COUNT,
        max_count_corpus_passed=True,
        max_corpus_passed=False,
        clean_checkout=False,
        approved_budgets=False,
    )
    validate_tus_memory_receipt(directory, failures)
    validate_budget_branch(directory, failures)

    validate_manifest(directory, failures)

    allowed = allowed_receipt_names() | {MANIFEST_NAME}
    for path in directory.glob("*.json"):
        if path.name not in allowed:
            failures.append(f"unexpected receipt file {path.name}")

    return failures


def main() -> int:
    args = sys.argv[1:]
    write_manifest_first = False
    if args and args[0] == "--write-manifest":
        write_manifest_first = True
        args = args[1:]
    directory = Path(args[0]) if args else DEFAULT_DIR
    if write_manifest_first:
        statistical_failures = write_statistical_receipt(directory)
        if statistical_failures:
            print("AL-PERF01 statistical receipt generation failed", file=sys.stderr)
            for failure in statistical_failures:
                print(f"- {failure}", file=sys.stderr)
            return 1
        write_manifest(directory)
    failures = validate_bundle(directory)
    if failures:
        print("AL-PERF01 receipt bundle verification failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print(f"AL-PERF01 receipt bundle verified: {directory}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

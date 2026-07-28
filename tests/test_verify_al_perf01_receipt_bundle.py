from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-al-perf01-receipt-bundle.py"


def perf_receipt(
    *,
    corpus_bytes: int = 1_048_576,
    expected_max_corpus_bytes: int | None = None,
    max_corpus_passed: bool = False,
    segment_count: int = 1,
    expected_max_sidecar_count: int | None = None,
    max_count_corpus_passed: bool = False,
    clean_checkout: bool = False,
    worktree_dirty: bool = True,
    approved_budgets: bool = False,
    sample: int = 1,
) -> dict:
    return {
        "schemaVersion": "fortemi.asset-lifecycle.perf-receipt.v1",
        "status": "local-focused-measurement-passed",
        "profile": "2.0.0/full-v1",
        "corpus": {
            "bytes": corpus_bytes,
            "segmentCount": segment_count,
            "segmentMaxBytes": 52_428_800,
            "archiveEntryCount": 64 if max_count_corpus_passed else 37,
            "sha256": "a" * 64,
            "blake3Segments": ["b" * 64],
        },
        "limits": {
            "expectedMaxCorpusBytes": expected_max_corpus_bytes,
            "expectedMaxSidecarCount": expected_max_sidecar_count,
            "maxArchiveEntries": 64,
            "limitPlusOneRejectedBeforeTusMutation": True,
        },
        "boundedIo": {
            "scope": "Fortemi server filesystem TUS and 2.0.0/full-v1 sidecars",
            "tusRequestBodyStreaming": True,
            "tusFinalizationWholePayloadBuffered": False,
            "tusSafetyPrefixMaxBytes": 8 * 1024,
            "filesystemCopyBufferBytes": 64 * 1024,
            "fullV1SidecarImportSpooledToDisk": True,
            "fullV1SidecarStreamBufferBytes": 64 * 1024,
            "wholeTestProcessBoundedMemoryPassed": False,
        },
        "recovery": {
            "rpoLostBytesAfterSignedFullV1Export": 0,
            "rpoDigestMatchesExportedSidecar": True,
            "timedRpoRtoRecorded": True,
            "approvedRpoRtoBudgetPassed": approved_budgets,
            "rtoMillisImportAndVerifyDownload": sample * 10,
        },
        "metrics": {
            "uploadMillis": sample,
            "downloadMillis": sample * 2,
            "exportMillis": sample * 3,
            "importMillis": sample * 4,
            "uploadBytesPerSecond": 10_000_000 + sample,
            "downloadBytesPerSecond": 20_000_000 + sample,
            "exportArchiveBytesPerSecond": 200_000 + sample,
            "importArchiveBytesPerSecond": 200_000 + sample,
            "rssHighWaterDeltaBytes": 10_000_000 + sample,
            "storageAndTusDiskBytesAfter": 2_000_000 + sample,
        },
        "phaseMeasurements": phase_measurements(sample),
        "reproducibility": {
            "cleanCheckoutReproduced": clean_checkout,
            "worktreeDirty": worktree_dirty,
            "deterministicCorpusAlgorithm": "al-perf01 deterministic bytes",
            "command": "scripts/ci/verify-asset-lifecycle-perf-receipt.sh",
            "packageVersion": "0.1.0",
            "targetOs": "linux",
            "targetArch": "x86_64",
            "storageFilesystem": "overlay",
            "tusFilesystem": "overlay",
            "gitCommit": "a" * 40,
        },
        "claims": {
            "hundredMiBCorpusPassed": corpus_bytes >= 104_857_600,
            "maxCorpusPassed": max_corpus_passed,
            "maxCountCorpusPassed": max_count_corpus_passed,
            "approvedBudgetsPassed": approved_budgets,
            "rpoRtoPassed": approved_budgets,
            "boundedServerTusAndFullV1SidecarIoPassed": True,
            "hotmBrowserDesktopPassed": False,
            "suiteWidePortability": False,
        },
        "budgets": budget_block(approved_budgets, corpus_bytes, sample),
    }


def phase_measurements(sample: int) -> dict:
    names = (
        "before",
        "afterUpload",
        "afterSourceDownload",
        "afterSignedExport",
        "afterCleanImport",
        "afterRecoveryDownload",
    )
    return {
        name: {
            "rssHighWaterBytes": 50_000_000 + index * sample,
            "storageDiskBytes": index * 1000,
            "tusStagingDiskBytes": 0,
            "combinedStorageAndTusDiskBytes": index * 1000,
        }
        for index, name in enumerate(names)
    }


def budget_block(approved_budgets: bool, corpus_bytes: int, sample: int) -> dict:
    fields = [
        "uploadMillis",
        "downloadMillis",
        "exportMillis",
        "importMillis",
        "recoveryRtoMillis",
        "rssHighWaterDeltaBytes",
        "storageAndTusDiskBytesAfter",
    ]
    default_limits = {
        "uploadMillis": 1000,
        "downloadMillis": 500,
        "exportMillis": 2000,
        "importMillis": 2000,
        "recoveryRtoMillis": 2500,
        "rssHighWaterDeltaBytes": 67_108_864,
        "storageAndTusDiskBytesAfter": 3_145_728,
    }
    maximum_limits = {
        "uploadMillis": 3000,
        "downloadMillis": 2000,
        "exportMillis": 10000,
        "importMillis": 5000,
        "recoveryRtoMillis": 7000,
        "rssHighWaterDeltaBytes": 805_306_368,
        "storageAndTusDiskBytesAfter": 268_435_456,
    }
    limits = maximum_limits if corpus_bytes == 104_857_600 else default_limits
    actuals = {
        "uploadMillis": sample,
        "downloadMillis": sample * 2,
        "exportMillis": sample * 3,
        "importMillis": sample * 4,
        "recoveryRtoMillis": sample * 10,
        "rssHighWaterDeltaBytes": 10_000_000 + sample,
        "storageAndTusDiskBytesAfter": 2_000_000 + sample,
    }
    return {
        "approvedGateEnabled": approved_budgets,
        "approvedGateComplete": approved_budgets,
        "approvedGatePassed": approved_budgets,
        **{
            field: {
                "actual": actuals[field],
                "max": limits[field] if approved_budgets else None,
                "passed": approved_budgets,
            }
            for field in fields
        },
    }


def tus_memory_receipt(
    *,
    small_delta: int = 2_000_000,
    large_delta: int = 3_000_000,
) -> dict:
    def corpus(corpus_bytes: int, delta: int) -> dict:
        high_water_before = 50_000_000
        return {
            "corpusBytes": corpus_bytes,
            "requestChunkBytes": 65_536,
            "expectedContentHash": "blake3:" + ("a" * 64),
            "uploadMillis": 1,
            "rssResidentBytesBefore": 49_000_000,
            "rssResidentBytesAfter": 49_000_000 + delta,
            "rssHighWaterBytesBefore": high_water_before,
            "rssHighWaterBytesAfter": high_water_before + delta,
            "rssHighWaterDeltaBytes": delta,
            "finalFileBytes": corpus_bytes,
            "stagingDiskBytesAfter": 0,
            "oracles": {
                "databaseHashSizeRefcountPassed": True,
                "finalFileHashSizePassed": True,
                "stagingCleanupPassed": True,
            },
            "reproducibility": {
                "targetOs": "linux",
                "targetArch": "x86_64",
                "storageFilesystem": "tmpfs",
                "tusFilesystem": "tmpfs",
            },
        }

    return {
        "schemaVersion": "fortemi.asset-lifecycle.tus-memory-receipt.v1",
        "status": "local-process-isolated-memory-guard-passed",
        "scope": "Fortemi server filesystem TUS PATCH and finalization",
        "profile": "filesystem-tus-v1",
        "corpora": {
            "small": corpus(1_048_576, small_delta),
            "large": corpus(104_857_600, large_delta),
        },
        "memoryGuard": {
            "requestChunkBytes": 65_536,
            "tusSafetyPrefixMaxBytes": 8_192,
            "filesystemCopyBufferBytes": 65_536,
            "maxLargeRssHighWaterDeltaBytes": 67_108_864,
            "maxGrowthOverSmallBytes": 33_554_432,
            "observedGrowthOverSmallBytes": max(0, large_delta - small_delta),
            "approvedPolicy": True,
            "policyRevision": "1",
        },
        "claims": {
            "processIsolatedTusPathMemoryGuardPassed": True,
            "wholeAssetLifecycleProcessBoundedMemoryPassed": False,
            "approvedPeakRssBudgetPassed": True,
            "nonFilesystemBackendsPassed": False,
            "scannerPathPassed": False,
            "suiteWidePortability": False,
        },
    }


class VerifyAlPerf01ReceiptBundleTests(unittest.TestCase):
    def test_not_configured_budget_branch_fails_after_policy_approval(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=False, manifest=False)

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "approved-budget.not-configured.json is invalid after policy approval",
                result.stderr,
            )

    def test_approved_budget_branch_passes(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)

            result = run_verifier(bundle)

            self.assertEqual(result.returncode, 0, result.stderr)

    def test_approved_budget_missing_max_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads((bundle / "approved-budget.json").read_text(encoding="utf-8"))
            receipt["budgets"]["uploadMillis"]["max"] = None
            write_json(bundle / "approved-budget.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "approved-budget.json: approved budget max uploadMillis missing or invalid",
                result.stderr,
            )

    def test_approved_budget_failed_rto_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads((bundle / "approved-budget.json").read_text(encoding="utf-8"))
            receipt["budgets"]["recoveryRtoMillis"]["passed"] = False
            receipt["recovery"]["approvedRpoRtoBudgetPassed"] = False
            write_json(bundle / "approved-budget.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "approved-budget.json: approved budget recoveryRtoMillis did not pass",
                result.stderr,
            )
            self.assertIn(
                "approved-budget.json: approved RPO/RTO budget must pass",
                result.stderr,
            )

    def test_approved_budget_drift_from_policy_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads(
                (bundle / "approved-budget.json").read_text(encoding="utf-8")
            )
            receipt["budgets"]["uploadMillis"]["max"] = 1001
            write_json(bundle / "approved-budget.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "approved-budget.json: approved budget uploadMillis differs from policy",
                result.stderr,
            )

    def test_missing_clean_checkout_receipt_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            (bundle / "clean-checkout.json").unlink()

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing required receipt clean-checkout.json", result.stderr)

    def test_dirty_clean_checkout_receipt_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            write_json(
                bundle / "clean-checkout.json",
                perf_receipt(clean_checkout=True, worktree_dirty=True),
            )

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "clean-checkout.json: clean-checkout receipt requires worktreeDirty=false",
                result.stderr,
            )

    def test_incomplete_budget_branch_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=False, manifest=False)
            write_json(
                bundle / "approved-budget.json",
                perf_receipt(approved_budgets=True),
            )

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "approved-budget.not-configured.json is invalid after policy approval",
                result.stderr,
            )

    def test_understated_max_corpus_claim_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            write_json(
                bundle / "max-corpus-100mib.json",
                perf_receipt(
                    corpus_bytes=104_857_600,
                    expected_max_corpus_bytes=104_857_600,
                    max_corpus_passed=False,
                ),
            )
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "max-corpus-100mib.json: maxCorpusPassed claim mismatch",
                result.stderr,
            )

    def test_understated_max_count_claim_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = perf_receipt(
                corpus_bytes=28 * 1_048_576,
                segment_count=28,
                expected_max_sidecar_count=28,
                max_count_corpus_passed=False,
            )
            write_json(bundle / "max-count-28-sidecars.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "max-count-28-sidecars.json: maxCountCorpusPassed claim mismatch",
                result.stderr,
            )

    def test_missing_platform_filesystem_context_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads((bundle / "default.json").read_text(encoding="utf-8"))
            receipt["reproducibility"]["storageFilesystem"] = " "
            write_json(bundle / "default.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "default.json: reproducibility.storageFilesystem missing or invalid",
                result.stderr,
            )

    def test_understated_bounded_io_contract_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads((bundle / "default.json").read_text(encoding="utf-8"))
            receipt["boundedIo"]["tusFinalizationWholePayloadBuffered"] = True
            receipt["claims"]["boundedServerTusAndFullV1SidecarIoPassed"] = False
            write_json(bundle / "default.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "default.json: bounded server TUS/full-v1 sidecar I/O contract mismatch",
                result.stderr,
            )
            self.assertIn(
                "default.json: bounded server TUS/full-v1 sidecar I/O claim missing",
                result.stderr,
            )

    def test_incomplete_phase_measurements_fail(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads((bundle / "default.json").read_text(encoding="utf-8"))
            del receipt["phaseMeasurements"]["afterSignedExport"]
            write_json(bundle / "default.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "default.json: phase measurement names mismatch",
                result.stderr,
            )

    def test_missing_tus_memory_receipt_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            (bundle / "tus-bounded-memory.json").unlink()

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "missing required receipt tus-bounded-memory.json",
                result.stderr,
            )

    def test_understated_tus_memory_claim_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = tus_memory_receipt()
            receipt["claims"]["processIsolatedTusPathMemoryGuardPassed"] = False
            write_json(bundle / "tus-bounded-memory.json", receipt)
            run_verifier(bundle, "--write-manifest")

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "tus-bounded-memory.json: claim scope mismatch",
                result.stderr,
            )

    def test_missing_statistical_sample_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            (bundle / "repeatability.json.repeat-5.json").unlink()

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "missing required receipt repeatability.json.repeat-5.json",
                result.stderr,
            )

    def test_statistical_receipt_uses_nearest_rank_percentiles(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)

            receipt = json.loads(
                (bundle / "observed-percentiles.json").read_text(encoding="utf-8")
            )

            self.assertEqual(receipt["sampleCount"], 5)
            self.assertEqual(receipt["method"]["name"], "nearest-rank")
            self.assertEqual(
                receipt["observations"]["uploadMillis"],
                {
                    "direction": "lower-is-better",
                    "maximum": 5,
                    "minimum": 1,
                    "p50": 3,
                    "p95": 5,
                    "p99": 5,
                    "unit": "milliseconds",
                },
            )
            self.assertTrue(receipt["claims"]["approvedPercentileBudgetsPassed"])
            self.assertTrue(receipt["claims"]["historicalTrendComparisonPassed"])
            self.assertEqual(receipt["policy"]["revision"], "1")
            self.assertFalse(receipt["policy"]["comparisonMayRelaxBudgets"])

    def test_tampered_statistical_receipt_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            receipt = json.loads(
                (bundle / "observed-percentiles.json").read_text(encoding="utf-8")
            )
            receipt["observations"]["uploadMillis"]["p50"] = 0
            write_json(bundle / "observed-percentiles.json", receipt)

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "observed-percentiles.json: content does not match exact source-receipt recomputation",
                result.stderr,
            )

    def test_percentile_regression_beyond_policy_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            for name in (
                "repeatability.json",
                "repeatability.json.repeat-2.json",
                "repeatability.json.repeat-3.json",
                "repeatability.json.repeat-4.json",
                "repeatability.json.repeat-5.json",
            ):
                receipt = json.loads((bundle / name).read_text(encoding="utf-8"))
                receipt["metrics"]["uploadMillis"] = 1001
                write_json(bundle / name, receipt)

            result = run_verifier(bundle, "--write-manifest")

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "observed-percentiles.json: approved percentile budgets did not pass",
                result.stderr,
            )

    def test_statistical_generation_rejects_dirty_source_sample(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            sample = json.loads(
                (bundle / "repeatability.json.repeat-3.json").read_text(
                    encoding="utf-8"
                )
            )
            sample["reproducibility"]["cleanCheckoutReproduced"] = False
            sample["reproducibility"]["worktreeDirty"] = True
            write_json(bundle / "repeatability.json.repeat-3.json", sample)

            result = run_verifier(bundle, "--write-manifest")

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "observed-percentiles.json: every source receipt must prove a clean checkout",
                result.stderr,
            )

    def test_missing_manifest_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            (bundle / "manifest.json").unlink()

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing required receipt manifest.json", result.stderr)

    def test_stale_manifest_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True)
            default = json.loads((bundle / "default.json").read_text(encoding="utf-8"))
            default["corpus"]["sha256"] = "c" * 64
            write_json(bundle / "default.json", default)

            result = run_verifier(bundle)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "manifest.json: file list, byte count, or sha256 mismatch",
                result.stderr,
            )

    def test_write_manifest_mode_creates_valid_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp)
            write_valid_bundle(bundle, approved=True, manifest=False)

            result = run_verifier(bundle, "--write-manifest")

            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(
                manifest["schemaVersion"],
                "fortemi.asset-lifecycle.perf-receipt-bundle-manifest.v1",
            )
            self.assertEqual(
                [entry["name"] for entry in manifest["files"]],
                [
                    "approved-budget.json",
                    "clean-checkout.json",
                    "default.json",
                    "max-corpus-100mib.json",
                    "max-count-28-sidecars.json",
                    "observed-percentiles.json",
                    "repeatability.json",
                    "repeatability.json.repeat-2.json",
                    "repeatability.json.repeat-3.json",
                    "repeatability.json.repeat-4.json",
                    "repeatability.json.repeat-5.json",
                    "tus-bounded-memory.json",
                ],
            )


def write_valid_bundle(bundle: Path, *, approved: bool, manifest: bool = True) -> None:
    write_json(bundle / "default.json", perf_receipt())
    for sample in range(1, 6):
        name = (
            "repeatability.json"
            if sample == 1
            else f"repeatability.json.repeat-{sample}.json"
        )
        write_json(
            bundle / name,
            perf_receipt(
                clean_checkout=True,
                worktree_dirty=False,
                sample=sample,
            ),
        )
    write_json(
        bundle / "clean-checkout.json",
        perf_receipt(clean_checkout=True, worktree_dirty=False),
    )
    write_json(
        bundle / "max-corpus-100mib.json",
        perf_receipt(
            corpus_bytes=104_857_600,
            expected_max_corpus_bytes=104_857_600,
            max_corpus_passed=True,
            approved_budgets=True,
        ),
    )
    write_json(
        bundle / "max-count-28-sidecars.json",
        perf_receipt(
            corpus_bytes=28 * 1_048_576,
            segment_count=28,
            expected_max_sidecar_count=28,
            max_count_corpus_passed=True,
        ),
    )
    write_json(bundle / "tus-bounded-memory.json", tus_memory_receipt())
    if approved:
        write_json(bundle / "approved-budget.json", perf_receipt(approved_budgets=True))
    else:
        write_json(
            bundle / "approved-budget.not-configured.json",
            {
                "schemaVersion": "fortemi.asset-lifecycle.approved-budget-receipt.v1",
                "status": "not-configured",
                "reason": "all seven FORTEMI_AL_PERF_MAX_* repository variables are required",
            },
        )
    if manifest:
        result = run_verifier(bundle, "--write-manifest")
        if result.returncode != 0:
            raise AssertionError(result.stderr)


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value), encoding="utf-8")


def run_verifier(bundle: Path, *extra_args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(SCRIPT), *extra_args, str(bundle)],
        check=False,
        capture_output=True,
        text=True,
    )


if __name__ == "__main__":
    unittest.main()

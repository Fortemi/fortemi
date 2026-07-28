#!/usr/bin/env python3
"""Write or verify the scoped AL-SYS04/05 lifecycle CI receipt."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
SCHEMA_VERSION = "fortemi.asset-lifecycle.system-receipt.v1"
PROFILE = "2.0.0/full-v1"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SENSITIVE_PATTERNS = (
    re.compile(r"(?i)authorization\s*:"),
    re.compile(r"(?i)bearer\s+[a-z0-9._~+/=-]+"),
    re.compile(r"(?i)postgres(?:ql)?://"),
    re.compile(r"(?i)(?:password|private[_ -]?key|client[_ -]?secret)\s*[=:]"),
    re.compile(r"(?:/home/|/tmp/|/var/tmp/|[A-Za-z]:\\\\)"),
)
SOURCE_PATHS = (
    "crates/matric-api/src/main.rs",
    "crates/matric-db/src/file_storage.rs",
    "scripts/ci/verify-asset-lifecycle-system-receipt.sh",
    "scripts/ci/verify-asset-lifecycle-system-receipt.py",
)
TESTS = (
    {
        "id": "route-lifecycle",
        "command": (
            "cargo test -p matric-api --bin matric-api al_sys -- "
            "--nocapture --test-threads=1"
        ),
        "covers": [
            "live HTTP/TUS upload and download",
            "post-upload restart",
            "signed full-v1 clean-destination recovery",
            "offset-committed TUS finalization retry",
            "same-byte upload/import/delete concurrency",
        ],
    },
    {
        "id": "sidecar-process-aborts",
        "command": (
            "cargo test -p matric-api --bin matric-api "
            "shard_optional_sidecars_round_trip_and_fail_without_partial_storage "
            "-- --nocapture --test-threads=1"
        ),
        "covers": [
            "partial sidecar-copy process abort",
            "post-staging process abort",
            "journal persistence process aborts",
            "promotion and commit-boundary process aborts",
            "clean retry and committed-state reconciliation",
        ],
    },
    {
        "id": "journal-recovery",
        "command": (
            "cargo test -p matric-db --lib shard_import_journal_ -- "
            "--nocapture --test-threads=1"
        ),
        "covers": [
            "atomic journal rewrite authority",
            "lock-guarded initial-candidate salvage",
            "orphan and ambiguous state fail-closed behavior",
        ],
    },
    {
        "id": "filesystem-refcounts",
        "command": (
            "cargo test -p matric-db --test file_storage_blob_refcount_test -- "
            "--nocapture --test-threads=1"
        ),
        "covers": [
            "content-addressed filesystem deduplication",
            "reference-count preservation",
            "orphan cleanup",
        ],
    },
)
REQUIRED_TRUE_ORACLES = (
    "liveHttpTusRoutesPassed",
    "postCommitRestartAndCleanRecoveryPassed",
    "controlledStagingPromotionTerminationPassed",
    "sameByteConcurrencyRefcountPassed",
    "deterministicByteDigestLengthMetadataOwnershipPassed",
    "destinationIndependentOfSourceRuntimePassed",
    "survivingDownloadByteExactPassed",
    "stagingAndPartialStateCleanupPassed",
    "authModeExplicitPassed",
    "receiptRedactionPassed",
)
REQUIRED_FALSE_CLAIMS = (
    "authenticatedModePassed",
    "midSyscallTerminationPassed",
    "kernelWriteFsyncFailurePassed",
    "powerLossDurabilityPassed",
    "inFlightCommitAcknowledgementResolved",
    "nonUnixDurabilityPassed",
    "platformFilesystemMatrixPassed",
    "suiteWidePortability",
)


def run_git(*args: str) -> str:
    return subprocess.check_output(
        ["git", *args],
        cwd=ROOT,
        text=True,
        stderr=subprocess.DEVNULL,
    ).strip()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def filesystem_type(path: Path) -> str:
    try:
        return subprocess.check_output(
            ["stat", "-f", "-c", "%T", str(path)],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def source_receipts() -> list[dict[str, Any]]:
    return [
        {
            "path": relative,
            "bytes": (ROOT / relative).stat().st_size,
            "sha256": sha256(ROOT / relative),
        }
        for relative in SOURCE_PATHS
    ]


def build_receipt() -> dict[str, Any]:
    commit = run_git("rev-parse", "HEAD")
    expected_commit = os.environ.get("GITHUB_SHA")
    if expected_commit and expected_commit != commit:
        raise ValueError(
            f"GITHUB_SHA {expected_commit!r} does not match checked-out commit {commit!r}"
        )

    worktree_dirty = bool(run_git("status", "--porcelain"))
    expect_clean = os.environ.get("FORTEMI_AL_SYS_EXPECT_CLEAN_CHECKOUT") == "1"
    if expect_clean and worktree_dirty:
        raise ValueError("clean-checkout receipt requested from a dirty worktree")

    run_id = os.environ.get("GITHUB_RUN_ID")
    ci_provider = "gitea-actions" if run_id else "local"
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": (
            "clean-checkout-headless-ci-passed"
            if ci_provider == "gitea-actions" and expect_clean
            else "local-headless-passed"
        ),
        "issue": "Fortemi/fortemi#1093",
        "profile": PROFILE,
        "scope": (
            "Fortemi Linux/PostgreSQL/filesystem AL-SYS04/05 lifecycle, "
            "restart, controlled process-abort, recovery, and concurrency"
        ),
        "execution": {
            "gitCommit": commit,
            "worktreeDirty": worktree_dirty,
            "cleanCheckoutReproduced": expect_clean and not worktree_dirty,
            "headless": True,
            "ci": {
                "provider": ci_provider,
                "runId": run_id,
                "job": os.environ.get("GITHUB_JOB"),
                "artifactName": "al-sys04-05-asset-lifecycle-receipt",
                "artifactRetentionDays": 30,
            },
            "targetOs": platform.system().lower(),
            "targetArch": platform.machine(),
            "workspaceFilesystem": filesystem_type(ROOT),
            "databaseMode": "isolated-postgresql-schema-per-test",
            "storageMode": "content-addressed-filesystem",
            "authMode": "disabled-isolated-ci",
        },
        "sources": source_receipts(),
        "tests": [{**test, "passed": True} for test in TESTS],
        "oracles": {
            "liveHttpTusRoutesPassed": True,
            "postCommitRestartAndCleanRecoveryPassed": True,
            "controlledStagingPromotionTerminationPassed": True,
            "sameByteConcurrencyRefcountPassed": True,
            "deterministicByteDigestLengthMetadataOwnershipPassed": True,
            "destinationIndependentOfSourceRuntimePassed": True,
            "survivingDownloadByteExactPassed": True,
            "stagingAndPartialStateCleanupPassed": True,
            "authModeExplicitPassed": True,
            "receiptRedactionPassed": True,
        },
        "relatedConsumerEvidence": {
            "repository": "Fortemi/HotM",
            "issue": "Fortemi/HotM#283",
            "implementationCommit": "6950658a2de62c5084c74f98454d9bcfcf80fae7",
            "receiptPublicationCommit": "1ca1c755680c9c7de1642e714556d50779a4278b",
            "receiptSha256": (
                "6efae6e12c68c42adb78c099a9f07b5b39492c35164422cbb919fde6c92eef2a"
            ),
            "reexecutedByThisJob": False,
        },
        "redaction": {
            "secretsExcluded": True,
            "payloadBytesExcluded": True,
            "manifestContentsExcluded": True,
            "absoluteLocalPathsExcluded": True,
            "databaseConnectionDetailsExcluded": True,
        },
        "claims": {
            "authenticatedModePassed": False,
            "midSyscallTerminationPassed": False,
            "kernelWriteFsyncFailurePassed": False,
            "powerLossDurabilityPassed": False,
            "inFlightCommitAcknowledgementResolved": False,
            "nonUnixDurabilityPassed": False,
            "platformFilesystemMatrixPassed": False,
            "suiteWidePortability": False,
        },
    }


def load_receipt(path: Path, failures: list[str]) -> dict[str, Any] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        failures.append(f"cannot read receipt: {error}")
        return None
    except json.JSONDecodeError as error:
        failures.append(f"invalid JSON: {error}")
        return None
    if not isinstance(value, dict):
        failures.append("receipt root must be an object")
        return None
    return value


def validate_receipt(receipt: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    if receipt.get("schemaVersion") != SCHEMA_VERSION:
        failures.append("schemaVersion mismatch")
    if receipt.get("profile") != PROFILE:
        failures.append("profile mismatch")
    if receipt.get("issue") != "Fortemi/fortemi#1093":
        failures.append("issue mismatch")

    execution = receipt.get("execution")
    if not isinstance(execution, dict):
        failures.append("execution must be an object")
        execution = {}
    commit = execution.get("gitCommit")
    if not isinstance(commit, str) or not COMMIT_RE.fullmatch(commit):
        failures.append("execution.gitCommit must be an exact lowercase commit")
    elif commit != run_git("rev-parse", "HEAD"):
        failures.append("execution.gitCommit does not match the checked-out commit")
    if execution.get("headless") is not True:
        failures.append("execution.headless must be true")
    if execution.get("authMode") != "disabled-isolated-ci":
        failures.append("execution.authMode must explicitly identify isolated no-auth mode")
    if not isinstance(execution.get("worktreeDirty"), bool):
        failures.append("execution.worktreeDirty must be boolean")
    if not isinstance(execution.get("cleanCheckoutReproduced"), bool):
        failures.append("execution.cleanCheckoutReproduced must be boolean")
    if execution.get("cleanCheckoutReproduced") and execution.get("worktreeDirty"):
        failures.append("clean checkout cannot be dirty")

    ci = execution.get("ci")
    if not isinstance(ci, dict):
        failures.append("execution.ci must be an object")
        ci = {}
    if ci.get("provider") not in {"local", "gitea-actions"}:
        failures.append("execution.ci.provider is invalid")
    if ci.get("provider") == "gitea-actions" and not str(ci.get("runId") or "").isdigit():
        failures.append("Gitea receipt requires a numeric run ID")
    if ci.get("artifactName") != "al-sys04-05-asset-lifecycle-receipt":
        failures.append("artifact name mismatch")
    if ci.get("artifactRetentionDays") != 30:
        failures.append("artifact retention mismatch")

    actual_sources = receipt.get("sources")
    if not isinstance(actual_sources, list):
        failures.append("sources must be an array")
    else:
        expected_sources = source_receipts()
        if actual_sources != expected_sources:
            failures.append("source receipts do not match the checked-out artifacts")

    tests = receipt.get("tests")
    expected_tests = [{**test, "passed": True} for test in TESTS]
    if tests != expected_tests:
        failures.append("test command inventory or pass state mismatch")

    oracles = receipt.get("oracles")
    if not isinstance(oracles, dict):
        failures.append("oracles must be an object")
        oracles = {}
    for key in REQUIRED_TRUE_ORACLES:
        if oracles.get(key) is not True:
            failures.append(f"oracle {key} must be true")

    claims = receipt.get("claims")
    if not isinstance(claims, dict):
        failures.append("claims must be an object")
        claims = {}
    for key in REQUIRED_FALSE_CLAIMS:
        if claims.get(key) is not False:
            failures.append(f"unsupported claim {key} must remain false")

    consumer = receipt.get("relatedConsumerEvidence")
    if not isinstance(consumer, dict):
        failures.append("relatedConsumerEvidence must be an object")
    else:
        for key in ("implementationCommit", "receiptPublicationCommit"):
            value = consumer.get(key)
            if not isinstance(value, str) or not COMMIT_RE.fullmatch(value):
                failures.append(f"relatedConsumerEvidence.{key} is invalid")
        digest = consumer.get("receiptSha256")
        if (
            not isinstance(digest, str)
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            failures.append("relatedConsumerEvidence.receiptSha256 is invalid")
        if consumer.get("reexecutedByThisJob") is not False:
            failures.append("producer job must not claim consumer re-execution")

    redaction = receipt.get("redaction")
    if not isinstance(redaction, dict) or not redaction:
        failures.append("redaction must be a non-empty object")
    elif any(value is not True for value in redaction.values()):
        failures.append("all redaction assertions must be true")

    serialized = json.dumps(receipt, sort_keys=True)
    for pattern in SENSITIVE_PATTERNS:
        if pattern.search(serialized):
            failures.append(f"receipt contains prohibited sensitive pattern {pattern.pattern!r}")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    path = args.receipt if args.receipt.is_absolute() else ROOT / args.receipt

    if args.write:
        try:
            receipt = build_receipt()
        except (OSError, subprocess.CalledProcessError, ValueError) as error:
            print(f"AL-SYS04/05 receipt write failed: {error}", file=sys.stderr)
            return 1
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(receipt, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    failures: list[str] = []
    receipt = load_receipt(path, failures)
    if receipt is not None:
        failures.extend(validate_receipt(receipt))
    if failures:
        for failure in failures:
            print(f"AL-SYS04/05 receipt verification failed: {failure}", file=sys.stderr)
        return 1
    print(f"AL-SYS04/05 receipt verified: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

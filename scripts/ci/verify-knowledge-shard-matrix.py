#!/usr/bin/env python3
"""Validate the fail-closed Knowledge Shard conformance matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
PARTICIPANTS = {"fortemi", "pglite", "recordstore", "aiwg"}
PROFILES = {"core-v1", "record-v1", "full-v1"}
STATES = {"advertised", "required-pending"}
CELL_STATUSES = {"passed", "pending", "failed"}
REQUIRED_CLAIMS = {"compatibility", "portability", "backup", "parity"}
REQUIRED_ASSERTIONS = {
    "cleanDestination",
    "semanticReexport",
    "zeroMutationOnFailure",
    "versionMatrix",
}
ALLOWED_REPOSITORIES = {
    "https://git.integrolabs.net/Fortemi/fortemi.git",
    "https://git.integrolabs.net/Fortemi/fortemi-react.git",
    "https://git.integrolabs.net/roctinam/aiwg.git",
}
REQUIRED_PRODUCER_PROFILES = {
    "fortemi": {"core-v1", "full-v1"},
    "pglite": {"core-v1"},
    "recordstore": {"record-v1"},
    "aiwg": {"core-v1"},
}
REQUIRED_CONSUMER_PROFILES = {
    "fortemi": {"core-v1", "record-v1", "full-v1"},
    "pglite": {"core-v1"},
    "recordstore": {"record-v1"},
    "aiwg": set(),
}


class MatrixError(Exception):
    """Raised when matrix structure or evidence fails closed."""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--matrix",
        type=Path,
        default=Path("contracts/knowledge-shard/conformance/matrix.json"),
    )
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("knowledge-shard-matrix-results.json"),
    )
    parser.add_argument(
        "--verify-remotes",
        action="store_true",
        help="fetch each immutable sibling commit and verify its declared files",
    )
    parser.add_argument(
        "--require-complete",
        action="store_true",
        help="fail when any required cell is not passed",
    )
    return parser.parse_args()


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise MatrixError(f"cannot read JSON {path}: {error}") from error


def require(condition: bool, message: str) -> None:
    if not condition:
        raise MatrixError(message)


def require_keys(
    value: Any,
    keys: set[str],
    label: str,
    allowed: set[str] | None = None,
) -> None:
    require(isinstance(value, dict), f"{label} must be an object")
    missing = keys - set(value)
    require(not missing, f"{label} missing keys: {', '.join(sorted(missing))}")
    if allowed is not None:
        extra = set(value) - allowed
        require(not extra, f"{label} has unknown keys: {', '.join(sorted(extra))}")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def resolve_pointer(value: Any, pointer: str) -> Any:
    require(pointer.startswith("/"), f"JSON expectation path must start with '/': {pointer}")
    current = value
    for raw_part in pointer[1:].split("/"):
        part = raw_part.replace("~1", "/").replace("~0", "~")
        if isinstance(current, list):
            try:
                current = current[int(part)]
            except (ValueError, IndexError) as error:
                raise MatrixError(f"JSON expectation path not found: {pointer}") from error
        elif isinstance(current, dict) and part in current:
            current = current[part]
        else:
            raise MatrixError(f"JSON expectation path not found: {pointer}")
    return current


def verify_expectations(data: bytes, evidence: dict[str, Any], label: str) -> None:
    expectations = evidence.get("expect", {})
    require(isinstance(expectations, dict), f"{label}.expect must be an object")
    if not expectations:
        return
    try:
        value = json.loads(data)
    except json.JSONDecodeError as error:
        raise MatrixError(f"{label} has JSON expectations but is not valid JSON") from error
    for pointer, expected in expectations.items():
        actual = resolve_pointer(value, pointer)
        require(
            actual == expected,
            f"{label} expectation drift at {pointer}: expected {expected!r}, got {actual!r}",
        )


def validate_evidence_shape(evidence: Any, label: str) -> None:
    require_keys(
        evidence,
        {"kind", "path", "sha256"},
        label,
        {"kind", "repository", "commit", "path", "sha256", "expect"},
    )
    require(evidence["kind"] in {"local-file", "git-file"}, f"{label}.kind is invalid")
    require(
        isinstance(evidence["path"], str)
        and evidence["path"]
        and not Path(evidence["path"]).is_absolute()
        and ".." not in Path(evidence["path"]).parts,
        f"{label}.path must be a safe repository-relative path",
    )
    require(
        isinstance(evidence["sha256"], str) and SHA256_RE.fullmatch(evidence["sha256"]),
        f"{label}.sha256 must be lowercase SHA-256",
    )
    if evidence["kind"] == "git-file":
        require_keys(
            evidence,
            {"repository", "commit"},
            label,
            {"kind", "repository", "commit", "path", "sha256", "expect"},
        )
        require(
            evidence["repository"] in ALLOWED_REPOSITORIES,
            f"{label}.repository is not allowlisted",
        )
        require(
            isinstance(evidence["commit"], str) and COMMIT_RE.fullmatch(evidence["commit"]),
            f"{label}.commit must be a full lowercase commit",
        )


def validate_profiles(profiles: Any, label: str) -> set[str]:
    require(isinstance(profiles, list), f"{label} must be an array")
    found: set[str] = set()
    for index, entry in enumerate(profiles):
        entry_label = f"{label}[{index}]"
        require_keys(
            entry,
            {"profile", "state"},
            entry_label,
            {"profile", "state"},
        )
        require(entry["profile"] in PROFILES, f"{entry_label}.profile is invalid")
        require(entry["state"] in STATES, f"{entry_label}.state is invalid")
        require(entry["profile"] not in found, f"{label} repeats {entry['profile']}")
        found.add(entry["profile"])
    return found


def validate_matrix_shape(matrix: Any) -> set[tuple[str, str, str]]:
    require_keys(
        matrix,
        {
            "schemaVersion",
            "matrixId",
            "authority",
            "claimPolicy",
            "coverageRequirements",
            "participants",
            "cells",
        },
        "matrix",
        {
            "$schema",
            "schemaVersion",
            "matrixId",
            "authority",
            "claimPolicy",
            "coverageRequirements",
            "participants",
            "cells",
        },
    )
    require(matrix["schemaVersion"] == 1, "matrix.schemaVersion must be 1")
    require(
        matrix["matrixId"] == "fortemi.knowledge-shard.conformance-matrix.v1",
        "matrix.matrixId is invalid",
    )

    authority = matrix["authority"]
    require_keys(
        authority,
        {
            "repository",
            "commit",
            "contractPath",
            "contractSha256",
            "contractRevision",
            "schemaVersion",
            "schemaBundleSha256",
        },
        "authority",
        {
            "repository",
            "commit",
            "contractPath",
            "contractSha256",
            "contractRevision",
            "schemaVersion",
            "schemaBundleSha256",
        },
    )
    require(authority["repository"] == "Fortemi/fortemi", "authority.repository is invalid")
    require(COMMIT_RE.fullmatch(authority["commit"]) is not None, "authority.commit is invalid")
    require(
        SHA256_RE.fullmatch(authority["contractSha256"]) is not None,
        "authority.contractSha256 is invalid",
    )
    require(
        SHA256_RE.fullmatch(authority["schemaBundleSha256"]) is not None,
        "authority.schemaBundleSha256 is invalid",
    )

    policy = matrix["claimPolicy"]
    require_keys(
        policy,
        {"mode", "blockedClaims"},
        "claimPolicy",
        {"mode", "blockedClaims"},
    )
    require(
        policy["mode"] == "all-required-cells-passed",
        "claimPolicy.mode must fail closed on every required cell",
    )
    require(
        isinstance(policy["blockedClaims"], list)
        and set(policy["blockedClaims"]) == REQUIRED_CLAIMS,
        "claimPolicy.blockedClaims must name compatibility, portability, backup, and parity",
    )

    coverage = matrix["coverageRequirements"]
    require(
        isinstance(coverage, list)
        and coverage
        and len(coverage) == len(set(coverage))
        and all(isinstance(item, str) and item for item in coverage),
        "coverageRequirements must be a non-empty unique string array",
    )

    participants = matrix["participants"]
    require(isinstance(participants, list), "participants must be an array")
    participant_ids: set[str] = set()
    producer_profiles: dict[str, set[str]] = {}
    consumer_profiles: dict[str, set[str]] = {}
    for index, participant in enumerate(participants):
        label = f"participants[{index}]"
        require_keys(
            participant,
            {
                "id",
                "repository",
                "commit",
                "producerProfiles",
                "consumerProfiles",
                "trackingIssues",
                "immutableInputs",
            },
            label,
            {
                "id",
                "repository",
                "commit",
                "package",
                "producerProfiles",
                "consumerProfiles",
                "trackingIssues",
                "immutableInputs",
            },
        )
        participant_id = participant["id"]
        require(participant_id in PARTICIPANTS, f"{label}.id is invalid")
        require(participant_id not in participant_ids, f"duplicate participant: {participant_id}")
        require(
            participant["repository"] in ALLOWED_REPOSITORIES,
            f"{label}.repository is not allowlisted",
        )
        require(COMMIT_RE.fullmatch(participant["commit"]) is not None, f"{label}.commit is invalid")
        package = participant.get("package")
        if package is not None:
            require_keys(
                package,
                {"name", "version", "published"},
                f"{label}.package",
                {"name", "version", "published"},
            )
            require(
                isinstance(package["name"], str) and package["name"],
                f"{label}.package.name is invalid",
            )
            require(
                isinstance(package["version"], str) and package["version"],
                f"{label}.package.version is invalid",
            )
            require(
                isinstance(package["published"], bool),
                f"{label}.package.published must be boolean",
            )
        require(
            isinstance(participant["trackingIssues"], list) and participant["trackingIssues"],
            f"{label}.trackingIssues must not be empty",
        )
        require(
            isinstance(participant["immutableInputs"], list),
            f"{label}.immutableInputs must be an array",
        )
        for evidence_index, evidence in enumerate(participant["immutableInputs"]):
            validate_evidence_shape(evidence, f"{label}.immutableInputs[{evidence_index}]")
        participant_ids.add(participant_id)
        producer_profiles[participant_id] = validate_profiles(
            participant["producerProfiles"], f"{label}.producerProfiles"
        )
        consumer_profiles[participant_id] = validate_profiles(
            participant["consumerProfiles"], f"{label}.consumerProfiles"
        )
    require(
        participant_ids == PARTICIPANTS,
        "participants must contain Fortemi, PGlite, RecordStore, and AIWG",
    )
    require(
        producer_profiles == REQUIRED_PRODUCER_PROFILES,
        "producer profile topology does not cover the required Fortemi matrix",
    )
    require(
        consumer_profiles == REQUIRED_CONSUMER_PROFILES,
        "consumer profile topology does not cover every advertised consumer",
    )

    expected_cells = {
        (producer, consumer, profile)
        for producer, produced in producer_profiles.items()
        for consumer, consumed in consumer_profiles.items()
        for profile in produced & consumed
    }
    cells = matrix["cells"]
    require(isinstance(cells, list), "cells must be an array")
    actual_cells: set[tuple[str, str, str]] = set()
    ids: set[str] = set()
    coverage_set = set(coverage)
    for index, cell in enumerate(cells):
        label = f"cells[{index}]"
        require_keys(
            cell,
            {
                "id",
                "producer",
                "consumer",
                "profile",
                "status",
                "coverage",
                "evidence",
                "trackingIssues",
                "blockingReason",
            },
            label,
            {
                "id",
                "producer",
                "consumer",
                "profile",
                "status",
                "coverage",
                "assertions",
                "evidence",
                "trackingIssues",
                "blockingReason",
            },
        )
        require(isinstance(cell["id"], str) and cell["id"], f"{label}.id is invalid")
        require(cell["id"] not in ids, f"duplicate cell id: {cell['id']}")
        ids.add(cell["id"])
        key = (cell["producer"], cell["consumer"], cell["profile"])
        require(key not in actual_cells, f"duplicate cell: {key}")
        actual_cells.add(key)
        require(cell["status"] in CELL_STATUSES, f"{label}.status is invalid")
        require(
            isinstance(cell["coverage"], list)
            and len(cell["coverage"]) == len(set(cell["coverage"]))
            and set(cell["coverage"]) <= coverage_set,
            f"{label}.coverage contains duplicates or unknown requirements",
        )
        require(isinstance(cell["evidence"], list), f"{label}.evidence must be an array")
        for evidence_index, evidence in enumerate(cell["evidence"]):
            validate_evidence_shape(evidence, f"{label}.evidence[{evidence_index}]")
        require(
            isinstance(cell["trackingIssues"], list) and cell["trackingIssues"],
            f"{label}.trackingIssues must not be empty",
        )
        if cell["status"] == "passed":
            require(
                set(cell["coverage"]) == coverage_set,
                f"{label} cannot pass without every coverage requirement",
            )
            assertions = cell.get("assertions")
            require(
                isinstance(assertions, dict)
                and set(assertions) == REQUIRED_ASSERTIONS
                and all(assertions.values()),
                f"{label} cannot pass without all fail-closed assertions",
            )
            require(cell["evidence"], f"{label} cannot pass without immutable evidence")
            require(cell["blockingReason"] is None, f"{label} passed but remains blocked")
        else:
            require(
                isinstance(cell["blockingReason"], str) and cell["blockingReason"].strip(),
                f"{label} must explain why it is not passed",
            )

    missing = expected_cells - actual_cells
    extra = actual_cells - expected_cells
    require(not missing, f"matrix missing required cells: {sorted(missing)}")
    require(not extra, f"matrix contains unsupported cells: {sorted(extra)}")
    return expected_cells


def run_git(args: list[str], cwd: Path) -> str:
    environment = os.environ.copy()
    environment.update(
        {
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
        }
    )
    result = subprocess.run(
        ["git", *args],
        cwd=cwd,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise MatrixError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def checkout_remote(repository: str, commit: str, destination: Path) -> None:
    require(repository in ALLOWED_REPOSITORIES, f"remote repository is not allowlisted: {repository}")
    require(COMMIT_RE.fullmatch(commit) is not None, f"remote commit is invalid: {commit}")
    destination.mkdir()
    run_git(["init", "--quiet"], destination)
    run_git(["remote", "add", "origin", repository], destination)
    run_git(["fetch", "--quiet", "--depth=1", "origin", commit], destination)
    run_git(["checkout", "--quiet", "--detach", "FETCH_HEAD"], destination)
    actual = run_git(["rev-parse", "HEAD"], destination)
    require(actual == commit, f"remote checkout drift: expected {commit}, got {actual}")
    require(not run_git(["status", "--porcelain"], destination), "remote checkout is not clean")


def verify_evidence_bytes(data: bytes, evidence: dict[str, Any], label: str) -> None:
    actual = sha256_bytes(data)
    require(
        actual == evidence["sha256"],
        f"{label} SHA-256 drift: expected {evidence['sha256']}, got {actual}",
    )
    verify_expectations(data, evidence, label)


def read_evidence_file(root: Path, relative_path: str, label: str) -> bytes:
    try:
        resolved_root = root.resolve(strict=True)
        candidate = root / relative_path
        require(not candidate.is_symlink(), f"{label} evidence path must not be a symlink")
        resolved = candidate.resolve(strict=True)
        require(
            resolved_root == resolved or resolved_root in resolved.parents,
            f"{label} evidence path escapes its checkout",
        )
        require(resolved.is_file(), f"{label} evidence path is not a regular file")
        return resolved.read_bytes()
    except OSError as error:
        raise MatrixError(f"{label} cannot read {root / relative_path}: {error}") from error


def collect_evidence(matrix: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    collected: list[tuple[str, dict[str, Any]]] = []
    for participant in matrix["participants"]:
        for index, evidence in enumerate(participant["immutableInputs"]):
            collected.append((f"participant:{participant['id']}:{index}", evidence))
    for cell in matrix["cells"]:
        for index, evidence in enumerate(cell["evidence"]):
            collected.append((f"cell:{cell['id']}:{index}", evidence))
    return collected


def verify_evidence(
    root: Path, matrix: dict[str, Any], verify_remotes: bool
) -> tuple[int, int, int]:
    evidence_items = collect_evidence(matrix)
    local_count = 0
    remote_count = 0
    checkout_count = 0
    for label, evidence in evidence_items:
        if evidence["kind"] != "local-file":
            continue
        data = read_evidence_file(root, evidence["path"], label)
        verify_evidence_bytes(data, evidence, label)
        local_count += 1

    if not verify_remotes:
        return local_count, remote_count, checkout_count

    remote_groups: dict[tuple[str, str], list[tuple[str, dict[str, Any]]]] = {}
    for participant in matrix["participants"]:
        if participant["id"] != "fortemi":
            key = (participant["repository"], participant["commit"])
            remote_groups.setdefault(key, [])
    for label, evidence in evidence_items:
        if evidence["kind"] == "git-file":
            key = (evidence["repository"], evidence["commit"])
            remote_groups.setdefault(key, []).append((label, evidence))

    with tempfile.TemporaryDirectory(prefix="fortemi-shard-matrix-") as temp:
        temp_root = Path(temp)
        for group_index, ((repository, commit), entries) in enumerate(
            sorted(remote_groups.items())
        ):
            checkout = temp_root / f"checkout-{group_index}"
            checkout_remote(repository, commit, checkout)
            checkout_count += 1
            for label, evidence in entries:
                data = read_evidence_file(checkout, evidence["path"], label)
                verify_evidence_bytes(data, evidence, label)
                remote_count += 1
    return local_count, remote_count, checkout_count


def verify_authority(root: Path, matrix: dict[str, Any]) -> None:
    authority = matrix["authority"]
    path = root / authority["contractPath"]
    try:
        data = path.read_bytes()
    except OSError as error:
        raise MatrixError(f"cannot read authority contract {path}: {error}") from error
    actual = sha256_bytes(data)
    require(
        actual == authority["contractSha256"],
        f"authority contract drift: expected {authority['contractSha256']}, got {actual}",
    )
    try:
        contract = json.loads(data)
    except json.JSONDecodeError as error:
        raise MatrixError("authority contract is not valid JSON") from error
    require(
        contract.get("contractRevision") == authority["contractRevision"],
        "authority contract revision drift",
    )
    require(
        contract.get("knowledgeShard", {}).get("schemaVersion") == authority["schemaVersion"],
        "authority schema version drift",
    )
    require(
        contract.get("schemaBundle", {}).get("sha256") == authority["schemaBundleSha256"],
        "authority schema bundle drift",
    )


def result_document(
    matrix: dict[str, Any],
    local_count: int,
    remote_count: int,
    checkout_count: int,
    remotes_requested: bool,
) -> dict[str, Any]:
    statuses = {status: 0 for status in sorted(CELL_STATUSES)}
    cells = []
    for cell in matrix["cells"]:
        statuses[cell["status"]] += 1
        cells.append(
            {
                "id": cell["id"],
                "producer": cell["producer"],
                "consumer": cell["consumer"],
                "profile": cell["profile"],
                "status": cell["status"],
                "coverage": cell["coverage"],
                "blockingReason": cell["blockingReason"],
            }
        )
    capabilities_ready = all(
        profile["state"] == "advertised"
        for participant in matrix["participants"]
        for profile in participant["producerProfiles"] + participant["consumerProfiles"]
    )
    complete = statuses["passed"] == len(matrix["cells"]) and capabilities_ready
    return {
        "schemaVersion": 1,
        "matrixId": matrix["matrixId"],
        "authority": matrix["authority"],
        "inventoryValid": True,
        "immutableInputs": {
            "localFilesVerified": local_count,
            "remoteFilesVerified": remote_count,
            "remoteCheckoutsVerified": checkout_count,
            "remoteVerificationRequested": remotes_requested,
        },
        "summary": {"requiredCells": len(matrix["cells"]), **statuses},
        "capabilitiesReady": capabilities_ready,
        "claimsAllowed": complete,
        "blockedClaims": [] if complete else matrix["claimPolicy"]["blockedClaims"],
        "cells": cells,
    }


def write_result(path: Path, result: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    matrix_path = args.matrix if args.matrix.is_absolute() else root / args.matrix
    output_path = args.output if args.output.is_absolute() else root / args.output
    try:
        matrix = read_json(matrix_path)
        validate_matrix_shape(matrix)
        verify_authority(root, matrix)
        local_count, remote_count, checkout_count = verify_evidence(
            root, matrix, args.verify_remotes
        )
        result = result_document(
            matrix,
            local_count,
            remote_count,
            checkout_count,
            args.verify_remotes,
        )
        write_result(output_path, result)
    except MatrixError as error:
        print(f"Knowledge Shard matrix verification failed: {error}", file=sys.stderr)
        return 2

    summary = result["summary"]
    print(
        "Knowledge Shard matrix inventory verified: "
        f"{summary['passed']} passed, {summary['pending']} pending, "
        f"{summary['failed']} failed; claimsAllowed={str(result['claimsAllowed']).lower()}"
    )
    if summary["failed"]:
        print("Knowledge Shard matrix contains failed cells.", file=sys.stderr)
        return 1
    if args.require_complete and not result["claimsAllowed"]:
        print(
            "Knowledge Shard release claims are blocked until every required cell passes.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

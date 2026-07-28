#!/usr/bin/env python3
"""Fail-closed verifier and receipt writer for ADR-104 platform conformance."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = ROOT / "contracts/suite-conformance/platform-matrix.json"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
REQUIRED_PARTICIPANTS = ("authority", "fortemi_react", "hotm")
PROHIBITED_TRUE_CLAIMS = (
    "universal_portability",
    "complete_backup",
    "one_universal_schema",
    "launched_gui",
)


class VerificationError(ValueError):
    pass


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationError(f"cannot read JSON {path}: {error}") from error
    if not isinstance(value, dict):
        raise VerificationError(f"{path} must contain a JSON object")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def exact_keys(value: dict[str, Any], expected: set[str], path: str) -> None:
    actual = set(value)
    if actual != expected:
        raise VerificationError(
            f"{path} keys mismatch: expected {sorted(expected)}, got {sorted(actual)}"
        )


def require_commit(value: Any, path: str) -> str:
    if not isinstance(value, str) or COMMIT_RE.fullmatch(value) is None:
        raise VerificationError(f"{path} must be an exact lowercase 40-hex commit")
    return value


def require_sha256(value: Any, path: str) -> str:
    if not isinstance(value, str) or SHA256_RE.fullmatch(value) is None:
        raise VerificationError(f"{path} must be a lowercase SHA-256")
    return value


def validate_manifest(manifest: dict[str, Any]) -> None:
    exact_keys(
        manifest,
        {
            "schema_version",
            "matrix_id",
            "issue",
            "authority",
            "participants",
            "required_platforms",
            "required_gates",
            "deferred_platforms",
            "claim_boundary",
        },
        "manifest",
    )
    if manifest["schema_version"] != 1:
        raise VerificationError("manifest.schema_version must be 1")
    if not isinstance(manifest["matrix_id"], str) or not manifest["matrix_id"].startswith(
        "fortemi.suite-conformance."
    ):
        raise VerificationError("manifest.matrix_id is invalid")
    if manifest["issue"] != "Fortemi/fortemi#1095":
        raise VerificationError("manifest.issue must be Fortemi/fortemi#1095")

    participants = manifest["participants"]
    if not isinstance(participants, dict):
        raise VerificationError("manifest.participants must be an object")
    exact_keys(participants, {"fortemi_react", "hotm"}, "manifest.participants")
    authority = manifest["authority"]
    if not isinstance(authority, dict):
        raise VerificationError("manifest authority must be an object")
    exact_keys(
        authority,
        {
            "repository",
            "schema_commit",
            "runtime_commit",
            "contract_path",
            "contract_sha256",
            "contract_revision",
            "schema_bundle_sha256",
            "profile",
        },
        "manifest.authority",
    )
    require_commit(authority.get("schema_commit"), "manifest.authority.schema_commit")
    require_commit(authority.get("runtime_commit"), "manifest.authority.runtime_commit")
    require_sha256(
        authority.get("contract_sha256"), "manifest.authority.contract_sha256"
    )
    require_sha256(
        authority.get("schema_bundle_sha256"),
        "manifest.authority.schema_bundle_sha256",
    )
    if authority.get("repository") != "Fortemi/fortemi":
        raise VerificationError("manifest.authority.repository must be Fortemi/fortemi")
    if authority.get("contract_path") != "contracts/knowledge-shard/2.0.0/contract.json":
        raise VerificationError("manifest authority contract path mismatch")
    if authority.get("contract_revision") != "21":
        raise VerificationError("manifest authority contract revision must be 21")
    if authority.get("profile") != "2.0.0/full-v1":
        raise VerificationError("manifest authority profile must be 2.0.0/full-v1")

    for name, participant in (
        ("fortemi_react", participants["fortemi_react"]),
        ("hotm", participants["hotm"]),
    ):
        if not isinstance(participant, dict):
            raise VerificationError(f"manifest {name} participant must be an object")
        require_commit(participant.get("commit"), f"manifest.{name}.commit")
        if not isinstance(participant.get("repository"), str) or not participant["repository"]:
            raise VerificationError(f"manifest.{name}.repository is required")

    platforms = manifest["required_platforms"]
    expected_platforms = {
        ("linux-x86_64", "linux", "x86_64"),
        ("macos-arm64", "macos", "arm64"),
    }
    if not isinstance(platforms, list):
        raise VerificationError("manifest.required_platforms must be an array")
    actual_platforms: set[tuple[Any, Any, Any]] = set()
    for platform in platforms:
        if not isinstance(platform, dict):
            raise VerificationError("each required platform must be an object")
        exact_keys(platform, {"id", "os", "architecture", "runner"}, "required platform")
        actual_platforms.add(
            (platform.get("id"), platform.get("os"), platform.get("architecture"))
        )
    if actual_platforms != expected_platforms:
        raise VerificationError(
            "required platforms must be exactly linux-x86_64 and macos-arm64"
        )

    gates = manifest["required_gates"]
    if (
        not isinstance(gates, list)
        or not gates
        or any(not isinstance(gate, str) or not gate for gate in gates)
        or len(gates) != len(set(gates))
    ):
        raise VerificationError("manifest.required_gates must be a unique non-empty string array")
    deferred = manifest["deferred_platforms"]
    if not isinstance(deferred, list) or not deferred:
        raise VerificationError("manifest.deferred_platforms must not be empty")

    boundary = manifest["claim_boundary"]
    expected_boundary = {
        "supported_platforms_only": True,
        "universal_portability": False,
        "complete_backup": False,
        "one_universal_schema": False,
        "launched_gui": False,
    }
    if boundary != expected_boundary:
        raise VerificationError("manifest.claim_boundary must preserve ADR-104 claim limits")


def manifest_participant_commits(manifest: dict[str, Any]) -> dict[str, str]:
    return {
        "authority_schema": manifest["authority"]["schema_commit"],
        "authority_runtime": manifest["authority"]["runtime_commit"],
        "fortemi_react": manifest["participants"]["fortemi_react"]["commit"],
        "hotm": manifest["participants"]["hotm"]["commit"],
    }


def required_platform(manifest: dict[str, Any], platform_id: str) -> dict[str, str]:
    platform = next(
        (
            item
            for item in manifest["required_platforms"]
            if item["id"] == platform_id
        ),
        None,
    )
    if platform is None:
        raise VerificationError(f"unsupported platform id: {platform_id}")
    return platform


def authority_gates(manifest: dict[str, Any]) -> dict[str, bool]:
    gates = {
        gate: True
        for gate in manifest["required_gates"]
        if gate.startswith("authority.")
    }
    if not gates:
        raise VerificationError("manifest must declare at least one authority gate")
    return gates


def validate_authority_receipt(
    receipt: dict[str, Any], manifest: dict[str, Any]
) -> None:
    exact_keys(
        receipt,
        {
            "schema_version",
            "issue",
            "status",
            "platform",
            "database",
            "identity",
            "required_gates",
            "claims",
        },
        "authority receipt",
    )
    if receipt["schema_version"] != 1:
        raise VerificationError("authority receipt schema_version must be 1")
    if receipt["issue"] != manifest["issue"] or receipt["status"] != "passed":
        raise VerificationError("authority receipt issue/status mismatch")
    platform = receipt["platform"]
    exact_keys(
        platform,
        {"id", "os", "architecture", "filesystem"},
        "authority receipt platform",
    )
    expected_platform = required_platform(manifest, platform["id"])
    if (platform["os"], platform["architecture"]) != (
        expected_platform["os"],
        expected_platform["architecture"],
    ):
        raise VerificationError("authority receipt platform identity mismatch")
    if not isinstance(platform["filesystem"], str) or not platform["filesystem"]:
        raise VerificationError("authority receipt filesystem is required")

    database = receipt["database"]
    exact_keys(
        database,
        {"engine", "provisioning", "architecture", "version", "extensions"},
        "authority receipt database",
    )
    if database["engine"] != "PostgreSQL":
        raise VerificationError("authority receipt database engine mismatch")
    if database["provisioning"] not in {"managed-docker", "external"}:
        raise VerificationError("authority receipt database provisioning mismatch")
    for key in ("architecture", "version"):
        if not isinstance(database[key], str) or not database[key]:
            raise VerificationError(f"authority receipt database {key} is required")
    if (
        not isinstance(database["extensions"], list)
        or not {"postgis", "vector"}.issubset(database["extensions"])
    ):
        raise VerificationError(
            "authority receipt database extensions must include postgis and vector"
        )

    identity = receipt["identity"]
    exact_keys(
        identity,
        {
            "schema_commit",
            "runtime_commit",
            "contract_sha256",
            "contract_revision",
            "schema_bundle_sha256",
            "openapi_sha256",
            "asyncapi_sha256",
        },
        "authority receipt identity",
    )
    authority = manifest["authority"]
    expected_identity = {
        "schema_commit": authority["schema_commit"],
        "runtime_commit": authority["runtime_commit"],
        "contract_sha256": authority["contract_sha256"],
        "contract_revision": authority["contract_revision"],
        "schema_bundle_sha256": authority["schema_bundle_sha256"],
    }
    if {key: identity.get(key) for key in expected_identity} != expected_identity:
        raise VerificationError("authority receipt contract identity drift")
    require_sha256(identity["openapi_sha256"], "authority receipt openapi_sha256")
    require_sha256(identity["asyncapi_sha256"], "authority receipt asyncapi_sha256")
    if receipt["required_gates"] != authority_gates(manifest):
        raise VerificationError("authority receipt gate coverage mismatch")
    if receipt["claims"] != manifest["claim_boundary"]:
        raise VerificationError("authority receipt claim boundary drift")


def git_value(checkout: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(checkout), *args],
            text=True,
            stderr=subprocess.STDOUT,
        ).strip()
    except subprocess.CalledProcessError as error:
        raise VerificationError(
            f"git command failed in {checkout}: {error.output.strip()}"
        ) from error


def write_authority_receipt(args: argparse.Namespace, manifest: dict[str, Any]) -> None:
    checkout = args.runtime_checkout.resolve()
    authority = manifest["authority"]
    if git_value(checkout, "rev-parse", "HEAD") != authority["runtime_commit"]:
        raise VerificationError("authority runtime checkout commit drift")
    if git_value(checkout, "status", "--porcelain"):
        raise VerificationError("authority runtime checkout must be clean")
    contract_path = args.schema_contract.resolve()
    if sha256_file(contract_path) != authority["contract_sha256"]:
        raise VerificationError("schema authority contract bytes drift")
    contract = load_json(contract_path)
    if contract.get("contractRevision") != authority["contract_revision"]:
        raise VerificationError("authority contract revision drift")
    if contract.get("schemaBundle", {}).get("sha256") != authority["schema_bundle_sha256"]:
        raise VerificationError("authority schema bundle drift")

    platform = required_platform(manifest, args.platform_id)
    receipt = {
        "schema_version": 1,
        "issue": manifest["issue"],
        "status": "passed",
        "platform": {
            "id": platform["id"],
            "os": platform["os"],
            "architecture": platform["architecture"],
            "filesystem": args.filesystem,
        },
        "database": {
            "engine": "PostgreSQL",
            "provisioning": args.database_provisioning,
            "architecture": args.database_architecture,
            "version": args.database_version,
            "extensions": sorted(
                item for item in args.database_extensions.split(",") if item
            ),
        },
        "identity": {
            "schema_commit": authority["schema_commit"],
            "runtime_commit": authority["runtime_commit"],
            "contract_sha256": authority["contract_sha256"],
            "contract_revision": authority["contract_revision"],
            "schema_bundle_sha256": authority["schema_bundle_sha256"],
            "openapi_sha256": sha256_file(
                checkout / "contracts/openapi/openapi.yaml"
            ),
            "asyncapi_sha256": sha256_file(
                checkout / "contracts/asyncapi/asyncapi.yaml"
            ),
        },
        "required_gates": authority_gates(manifest),
        "claims": manifest["claim_boundary"],
    }
    validate_authority_receipt(receipt, manifest)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")


def validate_react_receipt(
    receipt: dict[str, Any], manifest: dict[str, Any], platform_id: str
) -> None:
    if receipt.get("schemaVersion") != "fortemi.platform-contract-receipt.v1":
        raise VerificationError("React receipt schemaVersion mismatch")
    if receipt.get("status") != "passed":
        raise VerificationError("React receipt status must be passed")
    if receipt.get("repository", {}).get("commit") != manifest["participants"][
        "fortemi_react"
    ]["commit"]:
        raise VerificationError("React receipt commit drift")
    expected_platform = {
        "linux-x86_64": "linux/x86_64",
        "macos-arm64": "darwin/arm64",
    }[platform_id]
    if receipt.get("platform", {}).get("id") != expected_platform:
        raise VerificationError("React receipt platform drift")

    authority = manifest["authority"]
    bindings = receipt.get("authority")
    if not isinstance(bindings, list):
        raise VerificationError("React receipt authority bindings are missing")
    binding = next(
        (
            item
            for item in bindings
            if isinstance(item, dict) and item.get("schemaVersion") == "2.0.0"
        ),
        None,
    )
    expected_binding = {
        "commit": authority["schema_commit"],
        "contractRevision": authority["contract_revision"],
        "contractSha256": authority["contract_sha256"],
        "schemaBundleSha256": authority["schema_bundle_sha256"],
    }
    if binding is None or {
        key: binding.get(key) for key in expected_binding
    } != expected_binding:
        raise VerificationError("React receipt schema-2 authority binding drift")
    claims = receipt.get("claims", {})
    if (
        claims.get("suiteWide") is not False
        or claims.get("completeBackup") is not False
        or claims.get("universalPlatformPortability") is not False
    ):
        raise VerificationError("React receipt exceeds the suite claim boundary")


def validate_hotm_receipt(
    receipt: dict[str, Any], manifest: dict[str, Any], platform_id: str
) -> None:
    if receipt.get("schemaVersion") != "hotm.live-asset-ci-receipt.v1":
        raise VerificationError("HotM receipt schemaVersion mismatch")
    if receipt.get("issue") != "Fortemi/HotM#284" or receipt.get("status") != "passed":
        raise VerificationError("HotM receipt issue/status mismatch")
    identity = receipt.get("identity", {})
    if identity.get("hotmCommit") != manifest["participants"]["hotm"]["commit"]:
        raise VerificationError("HotM receipt commit drift")
    if identity.get("fortemiCommit") != manifest["authority"]["runtime_commit"]:
        raise VerificationError("HotM receipt authority runtime drift")
    expected_platform = {
        "linux-x86_64": ("linux", "x86_64"),
        "macos-arm64": ("darwin", "arm64"),
    }[platform_id]
    execution = receipt.get("execution", {})
    if (execution.get("os"), execution.get("arch")) != expected_platform:
        raise VerificationError("HotM receipt platform drift")
    claims = receipt.get("claims", {})
    if (
        claims.get("launchedDesktopGui") is not False
        or claims.get("interactiveNativeDialogs") is not False
        or claims.get("suiteWidePortability") is not False
    ):
        raise VerificationError("HotM receipt exceeds the suite claim boundary")


def validate_platform_receipt(
    receipt: dict[str, Any], manifest: dict[str, Any]
) -> tuple[str, str]:
    expected_keys = {
        "schema_version",
        "matrix_id",
        "issue",
        "status",
        "platform",
        "identity",
        "required_gates",
        "child_receipts",
        "claims",
    }
    exact_keys(receipt, expected_keys, "platform receipt")
    if receipt["schema_version"] != 1:
        raise VerificationError("platform receipt schema_version must be 1")
    if receipt["matrix_id"] != manifest["matrix_id"]:
        raise VerificationError("platform receipt matrix_id drift")
    if receipt["issue"] != manifest["issue"]:
        raise VerificationError("platform receipt issue drift")
    if receipt["status"] != "passed":
        raise VerificationError("platform receipt status must be passed")

    platform = receipt["platform"]
    if not isinstance(platform, dict):
        raise VerificationError("platform receipt platform must be an object")
    exact_keys(
        platform,
        {"id", "os", "architecture", "filesystem"},
        "platform receipt platform",
    )
    allowed = {
        item["id"]: (item["os"], item["architecture"])
        for item in manifest["required_platforms"]
    }
    platform_id = platform.get("id")
    if platform_id not in allowed:
        raise VerificationError(f"unsupported platform receipt: {platform_id}")
    if (platform.get("os"), platform.get("architecture")) != allowed[platform_id]:
        raise VerificationError(f"platform identity mismatch for {platform_id}")
    if not isinstance(platform.get("filesystem"), str) or not platform["filesystem"]:
        raise VerificationError("platform filesystem is required")

    identity = receipt["identity"]
    if identity != manifest_participant_commits(manifest):
        raise VerificationError("platform participant identity drift")

    gates = receipt["required_gates"]
    expected_gates = {gate: True for gate in manifest["required_gates"]}
    if gates != expected_gates:
        raise VerificationError("platform required gate coverage mismatch")

    children = receipt["child_receipts"]
    if not isinstance(children, dict):
        raise VerificationError("platform child_receipts must be an object")
    exact_keys(children, set(REQUIRED_PARTICIPANTS), "platform child_receipts")
    for name in REQUIRED_PARTICIPANTS:
        require_sha256(children[name], f"platform child_receipts.{name}")

    if receipt["claims"] != manifest["claim_boundary"]:
        raise VerificationError("platform claim boundary drift")
    for claim in PROHIBITED_TRUE_CLAIMS:
        if receipt["claims"].get(claim) is not False:
            raise VerificationError(f"prohibited platform claim is true: {claim}")

    canonical = json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode()
    return str(platform_id), hashlib.sha256(canonical).hexdigest()


def write_platform_receipt(args: argparse.Namespace, manifest: dict[str, Any]) -> None:
    platform = required_platform(manifest, args.platform_id)
    child_paths = {
        "authority": args.authority_receipt,
        "fortemi_react": args.react_receipt,
        "hotm": args.hotm_receipt,
    }
    for name, path in child_paths.items():
        if not path.is_file():
            raise VerificationError(f"{name} child receipt does not exist: {path}")
    validate_authority_receipt(load_json(args.authority_receipt), manifest)
    validate_react_receipt(
        load_json(args.react_receipt), manifest, args.platform_id
    )
    validate_hotm_receipt(load_json(args.hotm_receipt), manifest, args.platform_id)

    receipt = {
        "schema_version": 1,
        "matrix_id": manifest["matrix_id"],
        "issue": manifest["issue"],
        "status": "passed",
        "platform": {
            "id": platform["id"],
            "os": platform["os"],
            "architecture": platform["architecture"],
            "filesystem": args.filesystem,
        },
        "identity": manifest_participant_commits(manifest),
        "required_gates": {gate: True for gate in manifest["required_gates"]},
        "child_receipts": {
            name: sha256_file(path) for name, path in child_paths.items()
        },
        "claims": manifest["claim_boundary"],
    }
    validate_platform_receipt(receipt, manifest)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")


def aggregate_receipts(
    receipt_paths: list[Path], manifest: dict[str, Any], output: Path
) -> None:
    if len(receipt_paths) != len(manifest["required_platforms"]):
        raise VerificationError("one receipt per required platform is required")
    receipts: dict[str, str] = {}
    for path in receipt_paths:
        platform_id, canonical_sha256 = validate_platform_receipt(load_json(path), manifest)
        if platform_id in receipts:
            raise VerificationError(f"duplicate platform receipt: {platform_id}")
        receipts[platform_id] = canonical_sha256
    expected = {item["id"] for item in manifest["required_platforms"]}
    if set(receipts) != expected:
        raise VerificationError(
            f"platform receipt set mismatch: expected {sorted(expected)}, got {sorted(receipts)}"
        )

    aggregate = {
        "schema_version": 1,
        "matrix_id": manifest["matrix_id"],
        "issue": manifest["issue"],
        "status": "passed",
        "identity": manifest_participant_commits(manifest),
        "platform_receipts": dict(sorted(receipts.items())),
        "claims": manifest["claim_boundary"],
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(aggregate, indent=2) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("manifest")

    authority = subparsers.add_parser("authority")
    authority.add_argument(
        "--platform-id", required=True, choices=("linux-x86_64", "macos-arm64")
    )
    authority.add_argument("--filesystem", required=True)
    authority.add_argument(
        "--database-provisioning",
        required=True,
        choices=("managed-docker", "external"),
    )
    authority.add_argument("--database-architecture", required=True)
    authority.add_argument("--database-version", required=True)
    authority.add_argument("--database-extensions", required=True)
    authority.add_argument("--runtime-checkout", type=Path, required=True)
    authority.add_argument("--schema-contract", type=Path, required=True)
    authority.add_argument("--output", type=Path, required=True)

    platform = subparsers.add_parser("platform")
    platform.add_argument(
        "--platform-id", required=True, choices=("linux-x86_64", "macos-arm64")
    )
    platform.add_argument("--filesystem", required=True)
    platform.add_argument("--authority-receipt", type=Path, required=True)
    platform.add_argument("--react-receipt", type=Path, required=True)
    platform.add_argument("--hotm-receipt", type=Path, required=True)
    platform.add_argument("--output", type=Path, required=True)

    aggregate = subparsers.add_parser("aggregate")
    aggregate.add_argument(
        "--platform-receipt", type=Path, action="append", required=True
    )
    aggregate.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        manifest = load_json(args.manifest)
        validate_manifest(manifest)
        if args.command == "manifest":
            pass
        elif args.command == "authority":
            write_authority_receipt(args, manifest)
        elif args.command == "platform":
            write_platform_receipt(args, manifest)
        elif args.command == "aggregate":
            aggregate_receipts(args.platform_receipt, manifest, args.output)
        else:
            raise AssertionError(args.command)
    except VerificationError as error:
        print(f"suite-platform-matrix: {error}", file=sys.stderr)
        return 1
    print(f"suite-platform-matrix: {args.command} passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

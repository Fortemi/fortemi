#!/usr/bin/env python3
"""Verify the committed documentation shard matches its server baseline."""

from __future__ import annotations

import hashlib
import io
import json
import re
import sys
import tarfile
from pathlib import Path


SHARD = Path("docker/seed-data/fortemi-docs.shard")
RECEIPT = Path("docker/seed-data/fortemi-docs.shard.receipt.json")
CARGO = Path("Cargo.toml")


def workspace_version() -> str:
    in_workspace_package = False
    for line in CARGO.read_text().splitlines():
        stripped = line.strip()
        if stripped == "[workspace.package]":
            in_workspace_package = True
            continue
        if in_workspace_package and stripped.startswith("["):
            break
        if in_workspace_package and stripped.startswith("version = "):
            return stripped.split('"', 2)[1]
    raise ValueError("workspace package version not found")


def manifest_server_version(manifest: dict[str, object]) -> object:
    producer = manifest.get("producer")
    if isinstance(producer, dict) and producer.get("version"):
        return producer["version"]
    return manifest.get("matric_version")


def artifact_failures(receipt: dict[str, object], expected_version: str) -> list[str]:
    artifact = receipt.get("server_artifact")
    if "server_artifact" not in receipt:
        if "runtime_health" in receipt:
            return ["image receipt must not contain native runtime identity"]
        if not str(receipt.get("server_image", "")).endswith(f":{expected_version}"):
            return ["receipt server image is not pinned to the workspace version"]
        return []
    if not isinstance(artifact, dict) or artifact.get("kind") != "native-binary":
        return ["receipt server artifact kind is unsupported"]
    failures = []
    if set(artifact) != {"kind", "name", "sha256", "version", "revision"}:
        failures.append("native receipt artifact fields are incomplete or unsupported")
    runtime = receipt.get("runtime_health")
    if (not isinstance(runtime, dict)
            or set(runtime) != {"version", "git_sha"}
            or runtime.get("version") != expected_version
            or runtime.get("git_sha") != artifact.get("revision")):
        failures.append("native receipt observed runtime identity does not match artifact")
    if "server_image" in receipt:
        failures.append("native receipt must not claim a server image")
    if receipt.get("generator") != "scripts/rebuild-docs-shard.sh":
        failures.append("native receipt generator is not the documentation importer")
    if artifact.get("name") != "matric-api":
        failures.append("native receipt must identify matric-api")
    if not re.fullmatch(r"[0-9a-f]{64}", str(artifact.get("sha256", ""))):
        failures.append("native receipt binary SHA-256 is invalid")
    if artifact.get("version") != expected_version:
        failures.append("native receipt binary version does not match workspace")
    revision = artifact.get("revision")
    if (not re.fullmatch(r"[0-9a-f]{40}", str(revision or ""))
            or revision != receipt.get("server_commit")
            or revision != receipt.get("source_commit")):
        failures.append("native receipt binary revision does not match source")
    return failures


def main() -> int:
    failures: list[str] = []
    shard_bytes = SHARD.read_bytes()
    receipt = json.loads(RECEIPT.read_text())

    digest = hashlib.sha256(shard_bytes).hexdigest()
    if receipt.get("sha256") != digest:
        failures.append("receipt SHA-256 does not match committed shard")
    if receipt.get("byte_length") != len(shard_bytes):
        failures.append("receipt byte length does not match committed shard")

    with tarfile.open(fileobj=io.BytesIO(shard_bytes), mode="r:gz") as archive:
        manifest_file = archive.extractfile("manifest.json")
        if manifest_file is None:
            failures.append("manifest.json missing from committed shard")
            manifest = {}
        else:
            manifest = json.load(manifest_file)

    expected_version = workspace_version()
    shard_server_version = manifest_server_version(manifest)
    if shard_server_version != expected_version:
        failures.append(
            "shard manifest version "
            f"{shard_server_version!r} does not match workspace {expected_version!r}"
        )
    if receipt.get("server_version") != expected_version:
        failures.append(
            "receipt server version "
            f"{receipt.get('server_version')!r} does not match workspace {expected_version!r}"
        )
    if receipt.get("manifest_version") != manifest.get("version"):
        failures.append("receipt manifest version does not match shard manifest")
    failures.extend(artifact_failures(receipt, expected_version))
    producer = manifest.get("producer")
    if isinstance(producer, dict) and producer.get("revision") != receipt.get("server_commit"):
        failures.append("receipt server commit does not match shard producer revision")

    if failures:
        print("documentation shard freshness check failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(
        "documentation shard freshness check passed: "
        f"server={expected_version}, bytes={len(shard_bytes)}, sha256={digest}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

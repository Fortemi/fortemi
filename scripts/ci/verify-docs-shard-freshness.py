#!/usr/bin/env python3
"""Verify the committed documentation shard matches its server baseline."""

from __future__ import annotations

import hashlib
import io
import json
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
    if not str(receipt.get("server_image", "")).endswith(f":{expected_version}"):
        failures.append("receipt server image is not pinned to the workspace version")

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

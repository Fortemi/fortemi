#!/usr/bin/env python3
"""Write a provenance receipt for a generated Fortemi documentation shard."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import subprocess
import tarfile
from pathlib import Path


DEFAULT_SHARD = Path("docker/seed-data/fortemi-docs.shard")
DEFAULT_RECEIPT = Path("docker/seed-data/fortemi-docs.shard.receipt.json")


def read_manifest(shard_bytes: bytes) -> dict[str, object]:
    with tarfile.open(fileobj=io.BytesIO(shard_bytes), mode="r:gz") as archive:
        manifest_file = archive.extractfile("manifest.json")
        if manifest_file is None:
            raise ValueError("manifest.json missing from documentation shard")
        manifest = json.load(manifest_file)
    if not isinstance(manifest, dict):
        raise ValueError("documentation shard manifest must be a JSON object")
    return manifest


def current_revision() -> str:
    return subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True
    ).strip()


def build_receipt(
    shard: Path, server_image: str, source_commit: str
) -> dict[str, object]:
    shard_bytes = shard.read_bytes()
    manifest = read_manifest(shard_bytes)
    producer = manifest.get("producer")
    if not isinstance(producer, dict):
        raise ValueError("documentation shard manifest has no canonical producer")

    server_commit = producer.get("revision")
    server_version = producer.get("version")
    manifest_version = manifest.get("version")
    generated_at = manifest.get("created_at")
    required = {
        "producer.revision": server_commit,
        "producer.version": server_version,
        "version": manifest_version,
        "created_at": generated_at,
    }
    missing = [name for name, value in required.items() if not value]
    if missing:
        raise ValueError(
            "documentation shard manifest is missing " + ", ".join(missing)
        )
    if server_commit != source_commit:
        raise ValueError(
            "documentation shard producer revision "
            f"{server_commit!r} does not match source commit {source_commit!r}"
        )

    return {
        "artifact": str(shard),
        "byte_length": len(shard_bytes),
        "generated_at": generated_at,
        "generator": "scripts/ci/rebuild-shard-in-ci.sh",
        "manifest_version": manifest_version,
        "server_commit": server_commit,
        "server_image": server_image,
        "server_version": server_version,
        "sha256": hashlib.sha256(shard_bytes).hexdigest(),
        "source_commit": source_commit,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--server-image", required=True)
    parser.add_argument("--shard", type=Path, default=DEFAULT_SHARD)
    parser.add_argument("--receipt", type=Path, default=DEFAULT_RECEIPT)
    parser.add_argument("--source-commit", default=None)
    args = parser.parse_args()

    receipt = build_receipt(
        args.shard,
        args.server_image,
        args.source_commit or current_revision(),
    )
    args.receipt.write_text(
        json.dumps(receipt, indent=2) + "\n",
        encoding="utf-8",
    )
    print(
        "documentation shard receipt written: "
        f"server={receipt['server_version']}, "
        f"commit={receipt['server_commit']}, "
        f"sha256={receipt['sha256']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

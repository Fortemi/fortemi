#!/usr/bin/env python3
"""Generate the digest-pinned Knowledge Shard 2.0 authority descriptor."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT_ROOT = ROOT / "contracts/knowledge-shard/2.0.0"
OUTPUT = CONTRACT_ROOT / "contract.json"
FIXTURE = ROOT / "tests/fixtures/shards/presence-semantics-v2.0.json"
PREVIOUS = ROOT / "contracts/knowledge-shard/contract.json"


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    bundle_files = sorted(
        [
            *CONTRACT_ROOT.glob("*-v1/*.schema.json"),
            CONTRACT_ROOT / "contract.schema.json",
            CONTRACT_ROOT / "field-semantics.schema.json",
            CONTRACT_ROOT / "field-semantics.json",
        ],
        key=relative,
    )
    aggregate = hashlib.sha256()
    file_hashes: dict[str, str] = {}
    for path in bundle_files:
        aggregate.update(path.read_bytes())
        file_hashes[relative(path)] = sha256(path)

    previous = json.loads(PREVIOUS.read_text())
    inventory = json.loads((CONTRACT_ROOT / "field-semantics.json").read_text())
    document = {
        "$schema": "https://schemas.fortemi.dev/knowledge-shard/2.0.0/contract.schema.json",
        "schemaVersion": 1,
        "contractRevision": "20",
        "status": "specified-implementation-pending",
        "authority": {
            "repository": "Fortemi/fortemi",
            "adr": "docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md",
        },
        "knowledgeShard": {
            "format": "matric-shard",
            "schemaVersion": "2.0.0",
            "minimumReaderVersion": "2.0.0",
            "profileIdentity": "(manifest.version, manifest.profile)",
            "wireRepresentation": "direct-json-key-presence",
        },
        "profiles": {
            "core-v1": {"schemaRoot": "contracts/knowledge-shard/2.0.0/core-v1", "advertised": False},
            "record-v1": {"schemaRoot": "contracts/knowledge-shard/2.0.0/record-v1", "advertised": False},
            "full-v1": {"schemaRoot": "contracts/knowledge-shard/2.0.0/full-v1", "advertised": False},
        },
        "presenceSemantics": {
            "inventory": relative(CONTRACT_ROOT / "field-semantics.json"),
            "inventorySchema": relative(CONTRACT_ROOT / "field-semantics.schema.json"),
            "fieldCount": len(inventory["fields"]),
            "states": ["absent", "null", "empty", "value", "unsupported"],
        },
        "schemaBundle": {
            "aggregateAlgorithm": "sha256(raw file bytes concatenated in lexicographic path order)",
            "sha256": aggregate.hexdigest(),
            "files": file_hashes,
        },
        "canonicalCorpus": {
            "path": relative(FIXTURE),
            "sha256": sha256(FIXTURE),
            "verifier": "scripts/ci/verify-knowledge-shard-presence.py",
        },
        "compatibility": {
            "upMigration": "1.2.0 -> 2.0.0 is registered only by a runtime implementation receipt; legacy defaults must be reported.",
            "downgrade": "No implicit downgrade. full-v1 rejects any lossy projection; reduced profiles require deterministic field losses.",
            "unknownTuple": "Reject before archive staging, database writes, or blob mutation.",
            "rollback": "Disable production/default selection but retain released readers and stored presence metadata.",
        },
        "previousAuthority": {
            "contractPath": "contracts/knowledge-shard/contract.json",
            "contractRevision": previous["contractRevision"],
            "schemaVersion": previous["knowledgeShard"]["schemaVersion"],
            "schemaBundleSha256": previous["schemaBundle"]["sha256"],
            "immutable": True,
        },
        "tracking": {
            "authority": "https://git.integrolabs.net/Fortemi/fortemi/issues/1083",
            "matrix": "https://git.integrolabs.net/Fortemi/fortemi/issues/1082",
            "reactPresence": "https://git.integrolabs.net/Fortemi/fortemi-react/issues/379",
            "reactPgliteFull": "https://git.integrolabs.net/Fortemi/fortemi-react/issues/380",
            "reactAiwg": "https://git.integrolabs.net/Fortemi/fortemi-react/issues/381",
            "hotm": "https://git.integrolabs.net/Fortemi/hotm/issues/272",
        },
        "claimPolicy": "This descriptor specifies the next authority contract and enables no support, portability, backup, or parity claim without matrix receipts.",
    }
    rendered = json.dumps(document, indent=2) + "\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != rendered:
            raise SystemExit(f"{OUTPUT} is stale; regenerate it")
    else:
        OUTPUT.write_text(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

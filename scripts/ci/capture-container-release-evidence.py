#!/usr/bin/env python3
"""Capture a post-push OCI digest receipt and prove aliases are bound to it."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


POLICY = Path("docker/container-release-evidence-policy.json")
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
REVISION_RE = re.compile(r"^[0-9a-f]{40}$")


class EvidenceError(RuntimeError):
    """Raised when publication evidence cannot be proven."""


def split_reference(reference: str) -> tuple[str, str, str]:
    if "@" in reference:
        raise EvidenceError(f"digest references are not valid publication subjects: {reference}")
    last_slash = reference.rfind("/")
    colon = reference.rfind(":")
    if colon <= last_slash:
        raise EvidenceError(f"tagged image reference required: {reference}")
    repository, tag = reference[:colon], reference[colon + 1 :]
    registry = repository.split("/", 1)[0]
    if not registry or not tag:
        raise EvidenceError(f"invalid image reference: {reference}")
    return registry, repository, tag


def raw_manifest(reference: str) -> bytes:
    result = subprocess.run(
        ["docker", "buildx", "imagetools", "inspect", "--raw", reference],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode:
        detail = result.stderr.decode(errors="replace").strip()
        raise EvidenceError(f"cannot inspect {reference}: {detail}")
    if not result.stdout:
        raise EvidenceError(f"registry returned an empty manifest for {reference}")
    return result.stdout


def docker_manifest_descriptor(
    reference: str, expected_platforms: list[str]
) -> tuple[str, list[str]]:
    result = subprocess.run(
        ["docker", "manifest", "inspect", "--verbose", reference],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode:
        detail = result.stderr.decode(errors="replace").strip()
        raise EvidenceError(f"cannot inspect {reference} with docker manifest: {detail}")
    try:
        document = json.loads(result.stdout)
        descriptor = document["Descriptor"]
        platform = descriptor["platform"]
        platforms = [f"{platform['os']}/{platform['architecture']}"]
        digest = descriptor["digest"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        raise EvidenceError(
            "single-platform docker manifest fallback returned no authoritative descriptor"
        ) from error
    if not DIGEST_RE.fullmatch(digest):
        raise EvidenceError(f"registry returned an invalid descriptor digest: {digest}")
    if platforms != expected_platforms:
        raise EvidenceError(
            f"platform mismatch: expected {expected_platforms}, registry has {platforms}"
        )
    return digest, platforms


def manifest_details(raw: bytes, expected_platforms: list[str]) -> tuple[str, list[str]]:
    digest = f"sha256:{hashlib.sha256(raw).hexdigest()}"
    try:
        manifest = json.loads(raw)
    except json.JSONDecodeError as error:
        raise EvidenceError(f"registry returned invalid manifest JSON: {error}") from error

    media_type = manifest.get("mediaType", "")
    descriptors = manifest.get("manifests")
    if descriptors is None:
        if len(expected_platforms) != 1:
            raise EvidenceError(
                "a single-platform manifest cannot satisfy multiple expected platforms"
            )
        platforms = expected_platforms
    else:
        platforms = sorted(
            {
                (
                    f"{item.get('platform', {}).get('os')}/"
                    f"{item.get('platform', {}).get('architecture')}"
                )
                for item in descriptors
                if item.get("platform", {}).get("os")
                and item.get("platform", {}).get("architecture")
                and item.get("platform", {}).get("os") != "unknown"
            }
        )
    if sorted(expected_platforms) != platforms:
        raise EvidenceError(
            f"platform mismatch: expected {sorted(expected_platforms)}, registry has {platforms}"
        )
    if not media_type:
        raise EvidenceError("manifest has no mediaType")
    return digest, platforms


def inspect_reference(reference: str, expected_platforms: list[str]) -> tuple[str, list[str]]:
    try:
        return manifest_details(raw_manifest(reference), expected_platforms)
    except EvidenceError as error:
        unavailable = any(
            marker in str(error)
            for marker in (
                "not a docker command",
                'unknown command: "buildx"',
                "unknown flag: --raw",
            )
        )
        if len(expected_platforms) != 1 or not unavailable:
            raise
        return docker_manifest_descriptor(reference, expected_platforms)


def load_family(
    policy_path: Path, family_id: str, immutable_ref: str
) -> tuple[dict[str, Any], list[str]]:
    policy = json.loads(policy_path.read_text())
    family = policy.get("families", {}).get(family_id)
    if not family:
        raise EvidenceError(f"unknown image family: {family_id}")
    registry, _, tag = split_reference(immutable_ref)
    registry_policy = family.get("registries", {}).get(registry)
    if not registry_policy:
        raise EvidenceError(f"{family_id} is not approved for registry {registry}")
    expected = registry_policy.get("platforms")
    profile_id = registry_policy.get("publish_path_profile")
    if not expected or profile_id not in policy.get("publish_path_profiles", {}):
        raise EvidenceError(f"{family_id}/{registry} has an incomplete publish-path policy")
    patterns = family.get("immutable_tag_patterns", [])
    if not any(re.fullmatch(pattern, tag) for pattern in patterns):
        raise EvidenceError(f"{immutable_ref} is not an approved immutable tag")
    return policy, expected


def capture(
    policy_path: Path,
    family_id: str,
    revision: str,
    channel: str,
    immutable_ref: str,
    aliases: list[str],
    inspected_at: str,
) -> dict[str, Any]:
    if not REVISION_RE.fullmatch(revision):
        raise EvidenceError("source revision must be a full lowercase 40-character Git SHA")
    policy, expected_platforms = load_family(policy_path, family_id, immutable_ref)
    registry, repository, _ = split_reference(immutable_ref)

    digest, platforms = inspect_reference(immutable_ref, expected_platforms)
    if not DIGEST_RE.fullmatch(digest):
        raise EvidenceError(f"invalid computed digest: {digest}")

    alias_receipts = []
    for alias in aliases:
        alias_registry, alias_repository, _ = split_reference(alias)
        if alias_registry != registry or alias_repository != repository:
            raise EvidenceError(f"alias must use the immutable subject repository: {alias}")
        alias_digest, alias_platforms = inspect_reference(alias, expected_platforms)
        if alias_digest != digest:
            raise EvidenceError(
                f"alias drift: {alias} resolved to {alias_digest}, expected {digest}"
            )
        alias_receipts.append(
            {"reference": alias, "digest": alias_digest, "platforms": alias_platforms}
        )

    return {
        "_type": policy["artifact"]["format"],
        "schema_version": 1,
        "policy_issue": policy["policy_issue"],
        "family": family_id,
        "channel": channel,
        "source_revision": revision,
        "inspected_at": inspected_at,
        "registry": registry,
        "publish_path_profile": policy["families"][family_id]["registries"][registry][
            "publish_path_profile"
        ],
        "sbom_scope": policy["families"][family_id]["sbom_scope"],
        "subject": {
            "tagged_reference": immutable_ref,
            "digest": digest,
            "immutable_reference": f"{repository}@{digest}",
            "platforms": platforms,
        },
        "aliases": alias_receipts,
        "control_status": {
            name: details["status"] for name, details in policy["controls"].items()
        },
        "license_notice_status": policy["license_notices"]["status"],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", type=Path, default=POLICY)
    parser.add_argument("--family", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--channel", choices=("dev", "release"), required=True)
    parser.add_argument("--immutable-ref", required=True)
    parser.add_argument("--alias", action="append", default=[])
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        receipt = capture(
            args.policy,
            args.family,
            args.source_revision,
            args.channel,
            args.immutable_ref,
            args.alias,
            dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
        )
    except (EvidenceError, OSError, json.JSONDecodeError) as error:
        print(f"container release evidence failed: {error}", file=sys.stderr)
        return 1

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"recorded {receipt['subject']['immutable_reference']} in {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

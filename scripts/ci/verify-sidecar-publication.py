#!/usr/bin/env python3
"""Verify immutable sidecar publication policy is wired into CI."""

from __future__ import annotations

import sys
from pathlib import Path


WORKFLOW = Path(".gitea/workflows/publish-sidecar.yml")
PUBLISHER = Path("scripts/ci/publish-sidecar-release.sh")


def require(text: str, needle: str, source: Path, failures: list[str]) -> None:
    if needle not in text:
        failures.append(f"{source}: missing {needle!r}")


def main() -> int:
    failures: list[str] = []
    workflow = WORKFLOW.read_text()
    publisher = PUBLISHER.read_text()

    for needle in (
        "publish-sidecar-release.sh prepare",
        "publish-sidecar-release.sh immutable",
        "publish-sidecar-release.sh rolling",
        "build-linux-arm64:",
        "matric-api-aarch64-unknown-linux-gnu",
        "Colima Docker daemon is not native Linux arm64",
        'colima_profile="fortemi-sidecar-linux-arm64-${run_id}"',
        'delete_colima_profile "$colima_profile"',
        "for attempt in $(seq 1 60)",
    ):
        require(workflow, needle, WORKFLOW, failures)

    for forbidden in (
        'legacy_colima_home="${HOME}/.colima"',
        'colima_profile="fortemi-sidecar-linux-arm64"',
    ):
        if forbidden in workflow:
            failures.append(f"{WORKFLOW}: forbidden {forbidden!r}")

    for auth_option in (
        "BatchMode yes",
        "PreferredAuthentications publickey",
        "PasswordAuthentication no",
        "KbdInteractiveAuthentication no",
        "ConnectTimeout 30",
    ):
        if workflow.count(auth_option) < 2:
            failures.append(
                f"{WORKFLOW}: both mutsu jobs must set {auth_option!r}"
            )

    for needle in (
        'TAG="sidecar-${GITHUB_SHA:0:12}"',
        ".tag_name == $tag",
        "release_by_id",
        "wait_for_release_tag",
        '[[ -n "${EXISTING}" ]]',
        '[[ -n "${response}" ]]',
        "immutable release creation returned an unexpected response",
        "immutable release asset set mismatch",
        "removing pre-associated release asset",
        "immutable release checksum manifest replacement detected",
        "immutable release provenance replacement detected",
        "sha256sum -c",
        '"_type": "https://in-toto.io/Statement/v1"',
        '"predicateType": "https://slsa.dev/provenance/v1"',
        "matric-api-aarch64-unknown-linux-gnu",
        "prepare)",
    ):
        require(publisher, needle, PUBLISHER, failures)

    if failures:
        print("sidecar publication policy check failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("sidecar publication policy check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

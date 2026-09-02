#!/usr/bin/env python3
"""Verify release-only sidecar publication policy is wired into CI."""

from __future__ import annotations

import sys
from pathlib import Path


WORKFLOW = Path(".gitea/workflows/publish-sidecar.yml")
TEST_WORKFLOW = Path(".gitea/workflows/test.yml")
PUBLISHER = Path("scripts/ci/publish-sidecar-release.sh")
BOOT_AUTH = Path("scripts/ci/ensure-mutsu-boot-auth.sh")


def require(text: str, needle: str, source: Path, failures: list[str]) -> None:
    if needle not in text:
        failures.append(f"{source}: missing {needle!r}")


def job_block(text: str, job_name: str) -> str:
    """Return one top-level workflow job block without requiring a YAML parser."""
    lines = text.splitlines()
    start = next(
        (index for index, line in enumerate(lines) if line == f"  {job_name}:"),
        None,
    )
    if start is None:
        return ""
    end = len(lines)
    for index in range(start + 1, len(lines)):
        line = lines[index]
        if line.startswith("  ") and not line.startswith("    ") and line.endswith(":"):
            end = index
            break
    return "\n".join(lines[start:end])


def main() -> int:
    if len(sys.argv) > 3:
        print(
            "usage: verify-sidecar-publication.py [sidecar-workflow [test-workflow]]",
            file=sys.stderr,
        )
        return 2

    failures: list[str] = []
    workflow_path = Path(sys.argv[1]) if len(sys.argv) > 1 else WORKFLOW
    test_workflow_path = Path(sys.argv[2]) if len(sys.argv) > 2 else TEST_WORKFLOW
    workflow = workflow_path.read_text()
    test_workflow = test_workflow_path.read_text()
    publisher = PUBLISHER.read_text()
    boot_auth = BOOT_AUTH.read_text()

    for needle in (
        "\n  workflow_dispatch:\n",
        "publish-sidecar-release.sh prepare",
        "build-linux-arm64:",
        "matric-api-aarch64-unknown-linux-gnu",
        "Colima Docker daemon is not native Linux arm64",
        'colima_profile="fortemi-sidecar-linux-arm64-${run_id}"',
        'delete_colima_profile "$colima_profile"',
        "for attempt in $(seq 1 60)",
    ):
        require(workflow, needle, workflow_path, failures)

    release_guard = "startsWith(github.ref, 'refs/tags/v')"
    for job_name in ("runner-capacity", "publish-versioned", "handoff-suite"):
        block = job_block(workflow, job_name)
        if not block:
            failures.append(f"{workflow_path}: missing job {job_name!r}")
        elif release_guard not in block:
            failures.append(
                f"{workflow_path}:{job_name} is missing release-only tag guard"
            )

    handoff = job_block(test_workflow, "handoff-sidecar")
    if not handoff:
        failures.append(f"{test_workflow_path}: missing job 'handoff-sidecar'")
    elif release_guard not in handoff:
        failures.append(
            f"{test_workflow_path}:handoff-sidecar is missing release-only tag guard"
        )

    for forbidden in (
        "publish-sidecar-latest:",
        "github.ref == 'refs/heads/main'",
        "publish-sidecar-release.sh immutable",
        "publish-sidecar-release.sh rolling",
    ):
        if forbidden in workflow:
            failures.append(
                f"{workflow_path}: non-release sidecar path remains: {forbidden!r}"
            )

    if "\n  push:" in workflow:
        failures.append(
            f"{workflow_path}: automatic push trigger bypasses the capacity-one dispatch chain"
        )

    for forbidden in (
        'legacy_colima_home="${HOME}/.colima"',
        'colima_profile="fortemi-sidecar-linux-arm64"',
    ):
        if forbidden in workflow:
            failures.append(f"{workflow_path}: forbidden {forbidden!r}")

    for auth_option in (
        "BatchMode yes",
        "PreferredAuthentications publickey",
        "PasswordAuthentication no",
        "KbdInteractiveAuthentication no",
        "ConnectTimeout 30",
    ):
        if workflow.count(auth_option) < 2:
            failures.append(
                f"{workflow_path}: both mutsu jobs must set {auth_option!r}"
            )

    if workflow.count("scripts/ci/ensure-mutsu-boot-auth.sh") < 2:
        failures.append(
            f"{workflow_path}: both mutsu jobs must enforce boot-available SSH authorization"
        )

    for needle in (
        "SHA256:98FJbexEnVPwJAio08Qv53uEahv4u6V+wSoUQLyKFII",
        'authorized_keys_file="${authorized_keys_dir}/manitcor"',
        "AuthorizedKeysFile .ssh/authorized_keys /etc/ssh/authorized_keys/%u",
        "sudo -n /usr/sbin/sshd -t",
    ):
        require(boot_auth, needle, BOOT_AUTH, failures)

    for needle in (
        'TAG="sidecar-${CHAIN_SOURCE_SHA:0:12}"',
        'CHAIN_SOURCE_SHA="${CHAIN_SOURCE_SHA:-${GITHUB_SHA:-}}"',
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

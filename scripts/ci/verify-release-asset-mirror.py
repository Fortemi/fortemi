#!/usr/bin/env python3
"""Verify that versioned sidecars and GitHub releases share required evidence."""

from __future__ import annotations

import sys
from pathlib import Path


WORKFLOW = Path(".gitea/workflows/ci-builder.yaml")
SIDECAR = Path(".gitea/workflows/publish-sidecar.yml")
MIRROR = Path("scripts/ci/mirror-release-assets-to-github.sh")


def main() -> int:
    failures: list[str] = []
    workflow = WORKFLOW.read_text()
    sidecar = SIDECAR.read_text()
    mirror = MIRROR.read_text()

    checks = {
        WORKFLOW: (
            "scripts/ci/mirror-release-assets-to-github.sh",
            "GITEA_TOKEN: ${{ github.token }}",
        ),
        SIDECAR: (
            "publish-sidecar-release.sh prepare",
            "sidecar-provenance.intoto.json",
        ),
        MIRROR: (
            "SHA256SUMS.txt",
            "sidecar-provenance.intoto.json",
            "sha256sum -c SHA256SUMS.txt",
            "GitHub asset differs from Gitea source",
            "uploads.github.com",
            "GH_PUBLISH_TOKEN",
        ),
    }
    for path, needles in checks.items():
        text = {WORKFLOW: workflow, SIDECAR: sidecar, MIRROR: mirror}[path]
        for needle in needles:
            if needle not in text:
                failures.append(f"{path}: missing {needle!r}")

    if failures:
        print("release asset mirror policy check failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("release asset mirror policy check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

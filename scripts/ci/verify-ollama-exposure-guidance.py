#!/usr/bin/env python3
"""Reject quiet broad Ollama listeners and hard-coded Docker bridge guidance."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


FORBIDDEN = (
    "OLLAMA_HOST=0.0.0.0",
    "172.17.0.1:11434",
)
REQUIRED = {
    Path("README.md"): ("ollama-connectivity.md",),
    Path("installer/scripts/pull-models.sh"): (
        "HOST_GATEWAY_IP",
        "No host settings were changed.",
    ),
    Path("docker-compose.workstation.yml"): (
        "${FORTEMI_OLLAMA_BIND_ADDR:-127.0.0.1}:11434:11434",
    ),
    Path("docs/content/ollama-connectivity.md"): (
        "Local API Outside Docker",
        "Linux Docker to Host Ollama",
        "Remote or Shared Ollama",
        "authentication or mTLS",
        "exhaust GPU",
    ),
    Path(".gitea/workflows/ci-builder.yaml"): (
        "verify-ollama-exposure-guidance.py",
        "test_ollama_exposure_guidance.py",
    ),
}


def tracked_guidance(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
    )
    paths = []
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        path = Path(raw.decode())
        if path.parts and path.parts[0] in {"tests", "target", "dist"}:
            continue
        if path.suffix in {".md", ".sh", ".yml", ".yaml", ".example"} or path.name in {
            "README.md",
            ".env.example",
        }:
            paths.append(path)
    return paths


def validate(root: Path) -> list[str]:
    failures: list[str] = []
    for relative in tracked_guidance(root):
        text = (root / relative).read_text(errors="replace")
        for forbidden in FORBIDDEN:
            if forbidden in text:
                failures.append(f"{relative}: forbidden quiet exposure guidance {forbidden!r}")

    for relative, needles in REQUIRED.items():
        path = root / relative
        if not path.exists():
            failures.append(f"{relative}: required policy surface is missing")
            continue
        text = path.read_text()
        for needle in needles:
            if needle not in text:
                failures.append(f"{relative}: missing required policy marker {needle!r}")
    return failures


def main() -> int:
    failures = validate(Path("."))
    if failures:
        print("Ollama exposure guidance check failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("Ollama exposure guidance check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

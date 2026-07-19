#!/usr/bin/env python3
"""Reject unsafe Docker daemon access guidance in operator-facing files."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


SCAN_PATHS = (
    Path("README.md"),
    Path("build"),
    Path("docs"),
    Path(".gitea/workflows"),
    Path("scripts"),
)
TEXT_SUFFIXES = {
    "",
    ".bash",
    ".conf",
    ".ini",
    ".md",
    ".service",
    ".sh",
    ".txt",
    ".yaml",
    ".yml",
}
APPROVED_SOCKET_MOUNT_DOCS = {
    Path("build/README.md"),
    Path("build/RUNNER_SETUP.md"),
}
SELF = Path("scripts/ci/verify-docker-socket-guidance.py")

WORLD_WRITABLE_SOCKET = re.compile(
    r"""chmod\s+(?:-[^\s]+\s+)*(?:0?[0-7]{2,3}[2367]|(?:a|o)(?:\+|=)[A-Za-z]*w[A-Za-z]*)\s+
        ["']?(?:/var)?/run/docker\.sock""",
    re.IGNORECASE | re.VERBOSE,
)
SOCKET_MOUNT = re.compile(
    r"""(?:/var)?/run/docker\.sock\s*:|
        source\s*=\s*(?:/var)?/run/docker\.sock""",
    re.IGNORECASE | re.VERBOSE,
)
DOCKER_TCP = re.compile(
    r"""(?:DOCKER_HOST\s*=\s*["']?|docker\s+(?:-H|--host(?:=|\s+))\s*)tcp://""",
    re.IGNORECASE,
)
AUTHENTICATED_TLS = re.compile(
    r"""DOCKER_TLS_VERIFY\s*=\s*1|--tlsverify|mutual(?:ly)?\s+
        authenticated\s+TLS|mTLS|client\s+certificate""",
    re.IGNORECASE | re.VERBOSE,
)
REQUIRED_MOUNT_MARKERS = (
    "root-equivalent host control",
    "trusted",
    "opt-in",
    ":ro",
)


def iter_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for relative in SCAN_PATHS:
        path = root / relative
        if path.is_file():
            files.append(path)
        elif path.is_dir():
            files.extend(
                candidate
                for candidate in path.rglob("*")
                if candidate.is_file() and candidate.suffix.lower() in TEXT_SUFFIXES
            )
    return sorted(set(files))


def read_text(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def nearby_text(lines: list[str], index: int, radius: int = 5) -> str:
    return "\n".join(lines[max(0, index - radius) : index + radius + 1])


def verify(root: Path) -> list[str]:
    errors: list[str] = []

    for path in iter_files(root):
        relative = path.relative_to(root)
        if relative == SELF:
            continue
        text = read_text(path)
        if text is None:
            continue

        for match in WORLD_WRITABLE_SOCKET.finditer(text):
            errors.append(
                f"{relative}:{line_number(text, match.start())}: "
                "Docker socket must never be made world-writable"
            )

        for match in SOCKET_MOUNT.finditer(text):
            line = line_number(text, match.start())
            if relative not in APPROVED_SOCKET_MOUNT_DOCS:
                errors.append(
                    f"{relative}:{line}: Docker socket mount is outside the "
                    "reviewed trusted-builder threat-model docs"
                )
            else:
                lines = text.splitlines()
                context = nearby_text(lines, line - 1, radius=18).lower()
                missing = [
                    marker for marker in REQUIRED_MOUNT_MARKERS if marker not in context
                ]
                if missing:
                    errors.append(
                        f"{relative}:{line}: reviewed socket-mount guidance is "
                        f"missing nearby threat markers: {', '.join(missing)}"
                    )

        lines = text.splitlines()
        for index, line in enumerate(lines):
            if DOCKER_TCP.search(line) and not AUTHENTICATED_TLS.search(
                nearby_text(lines, index)
            ):
                errors.append(
                    f"{relative}:{index + 1}: Docker-over-TCP guidance must "
                    "require mutually authenticated TLS"
                )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", default=".")
    args = parser.parse_args()

    errors = verify(Path(args.root).resolve())
    if errors:
        print("Unsafe Docker daemon guidance detected:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Docker daemon guidance verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

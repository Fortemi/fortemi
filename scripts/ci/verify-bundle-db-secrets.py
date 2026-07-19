#!/usr/bin/env python3
"""Reject reusable database credentials in Docker bundle runtime artifacts."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


KNOWN_DEFAULT = re.compile(
    r"(?i)POSTGRES_PASSWORD[^\n]*(?:fortemi-local-dev|(?:^|[=:\"'])matric(?:$|[}\"']))"
)
CREDENTIAL_URL = re.compile(
    r"(?i)(?:postgres|postgresql)://[^/\s:@]+:[^@\s]+@"
)
ENTRYPOINT_FALLBACK = re.compile(
    r'POSTGRES_PASSWORD="\$\{POSTGRES_PASSWORD:-[^}]+\}"'
)


def verify(root: Path) -> list[str]:
    errors: list[str] = []
    dockerfile = (root / "Dockerfile.bundle").read_text(encoding="utf-8")
    compose = (root / "docker-compose.bundle.yml").read_text(encoding="utf-8")
    entrypoint = (root / "docker" / "bundle-entrypoint.sh").read_text(
        encoding="utf-8"
    )
    gitignore = (root / ".gitignore").read_text(encoding="utf-8")

    if re.search(r"(?m)^\s*ENV\s+POSTGRES_PASSWORD(?:=|\s)", dockerfile):
        errors.append("Dockerfile.bundle must not bake POSTGRES_PASSWORD")
    if CREDENTIAL_URL.search(dockerfile):
        errors.append("Dockerfile.bundle must not bake a credential-bearing URL")
    if KNOWN_DEFAULT.search(compose):
        errors.append("docker-compose.bundle.yml contains a reusable DB password")
    password_assignments = re.findall(
        r"(?m)^\s*-\s*POSTGRES_PASSWORD=(.*)$",
        compose,
    )
    if password_assignments != ["${POSTGRES_PASSWORD:-}"]:
        errors.append(
            "docker-compose.bundle.yml must have exactly one empty fail-closed "
            "POSTGRES_PASSWORD fallback"
        )
    if CREDENTIAL_URL.search(compose):
        errors.append(
            "docker-compose.bundle.yml contains a credential-bearing database URL"
        )
    if ENTRYPOINT_FALLBACK.search(entrypoint):
        errors.append("bundle entrypoint must not default POSTGRES_PASSWORD")

    guard_definition = entrypoint.find("require_postgres_password()")
    guard_call = entrypoint.find("\nrequire_postgres_password\n")
    url_construction = entrypoint.find('if [ -z "${DATABASE_URL:-}" ]')
    if (
        guard_definition < 0
        or guard_call < 0
        or url_construction < 0
        or guard_call > url_construction
    ):
        errors.append(
            "bundle entrypoint must validate POSTGRES_PASSWORD before "
            "constructing DATABASE_URL"
        )
    ignored_paths = {
        line.strip()
        for line in gitignore.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }
    if ".env" not in ignored_paths or ".env.bak" not in ignored_paths:
        errors.append("generated .env and .env.bak files must both be ignored")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", type=Path, default=Path("."))
    args = parser.parse_args()
    try:
        errors = verify(args.root.resolve())
    except (OSError, UnicodeDecodeError) as error:
        print(f"ERROR: cannot inspect bundle credential artifacts: {error}", file=sys.stderr)
        return 2

    if errors:
        print("Bundle database credential verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Bundle database credential verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

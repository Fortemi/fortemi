#!/usr/bin/env python3
"""Fail CI when an existing SQL migration is edited in place."""

from __future__ import annotations

import os
import subprocess
import sys
import hashlib
from pathlib import Path


MIGRATION_PREFIX = "migrations/"
CANONICAL_RESTORE_SHA384 = {
    "migrations/20260215000000_native_uuidv7.sql": "c4a8d7097ce200e9bd39d7bd70882403119c1181bbfa5999335d48ebd087e9703587297347bbef014974cb1699f07772",
}


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def output(*args: str, check: bool = True) -> str:
    return git(*args, check=check).stdout.strip()


def ensure_base_ref(base_ref: str) -> str:
    candidates = [
        f"origin/{base_ref}",
        base_ref,
        "HEAD^",
    ]

    for candidate in candidates:
        if git("rev-parse", "--verify", "--quiet", candidate, check=False).returncode == 0:
            return candidate

    fetch_ref = f"refs/heads/{base_ref}:refs/remotes/origin/{base_ref}"
    git("fetch", "--depth=200", "origin", fetch_ref)
    return f"origin/{base_ref}"


def merge_base(base_ref: str) -> str:
    proc = git("merge-base", base_ref, "HEAD", check=False)
    if proc.returncode == 0 and proc.stdout.strip():
        return proc.stdout.strip()
    return output("rev-parse", base_ref)


def changed_migration_files(base: str) -> list[str]:
    diff = output("diff", "--name-only", "--diff-filter=ACMRT", f"{base}..HEAD", "--", "migrations/*.sql")
    return [line for line in diff.splitlines() if line.startswith(MIGRATION_PREFIX)]


def exists_at(rev: str, path: str) -> bool:
    return git("cat-file", "-e", f"{rev}:{path}", check=False).returncode == 0


def file_bytes(rev: str, path: str) -> bytes:
    proc = subprocess.run(
        ["git", "show", f"{rev}:{path}"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return proc.stdout


def allowed_canonical_restore(path: str) -> bool:
    expected = CANONICAL_RESTORE_SHA384.get(path)
    if expected is None:
        return False
    return hashlib.sha384(Path(path).read_bytes()).hexdigest() == expected


def main() -> int:
    if not Path(".git").exists():
        print("migration immutability check skipped: not a git worktree")
        return 0

    base_ref_name = os.environ.get("GITHUB_BASE_REF") or os.environ.get("DEFAULT_BRANCH") or "main"
    base_ref = ensure_base_ref(base_ref_name)
    base = merge_base(base_ref)
    failures: list[str] = []

    for path in changed_migration_files(base):
        if not exists_at(base, path):
            continue
        if file_bytes(base, path) != Path(path).read_bytes():
            if allowed_canonical_restore(path):
                continue
            failures.append(path)

    if failures:
        print("Existing SQL migrations are immutable; add a new migration instead.", file=sys.stderr)
        for path in failures:
            print(f"- {path}", file=sys.stderr)
        return 1

    print("migration immutability check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

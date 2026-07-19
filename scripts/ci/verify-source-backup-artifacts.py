#!/usr/bin/env python3
"""Reject tracked editor and source-backup artifacts."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path, PurePosixPath


DEFAULT_ALLOWLIST = Path("scripts/ci/source-backup-artifacts.allowlist")
BACKUP_SUFFIXES = (".bak", ".orig")
MIN_RATIONALE_LENGTH = 12


def is_backup_artifact(path: str) -> bool:
    name = PurePosixPath(path).name
    return name.lower().endswith(BACKUP_SUFFIXES) or name.endswith("~")


def tracked_paths(root: Path) -> set[str]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        check=False,
        capture_output=True,
    )
    if result.returncode != 0:
        diagnostic = result.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git ls-files failed: {diagnostic}")
    return {
        path
        for path in result.stdout.decode("utf-8", errors="strict").split("\0")
        if path
    }


def parse_allowlist(path: Path) -> tuple[dict[str, str], list[str]]:
    entries: dict[str, str] = {}
    errors: list[str] = []
    if not path.is_file():
        return entries, [f"{path}: required allowlist file is missing"]

    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        return entries, [f"{path}: cannot read allowlist: {error}"]

    for line_number, raw_line in enumerate(lines, start=1):
        if not raw_line.strip() or raw_line.lstrip().startswith("#"):
            continue
        if "\t" not in raw_line:
            errors.append(
                f"{path}:{line_number}: expected path<TAB>rationale"
            )
            continue

        raw_entry, rationale = raw_line.split("\t", 1)
        entry = raw_entry.strip()
        rationale = rationale.strip()
        parsed = PurePosixPath(entry)
        if (
            not entry
            or parsed.is_absolute()
            or entry != parsed.as_posix()
            or "\\" in entry
            or any(part in {"", ".", ".."} for part in parsed.parts)
        ):
            errors.append(
                f"{path}:{line_number}: allowlist path must be normalized and relative"
            )
            continue
        if len(rationale) < MIN_RATIONALE_LENGTH:
            errors.append(
                f"{path}:{line_number}: allowlist rationale must be at least "
                f"{MIN_RATIONALE_LENGTH} characters"
            )
            continue
        if entry in entries:
            errors.append(f"{path}:{line_number}: duplicate allowlist path: {entry}")
            continue
        entries[entry] = rationale

    return entries, errors


def verify(root: Path, allowlist_path: Path) -> list[str]:
    try:
        tracked = tracked_paths(root)
    except (RuntimeError, UnicodeDecodeError) as error:
        return [str(error)]

    allowlist, errors = parse_allowlist(allowlist_path)
    artifacts = {path for path in tracked if is_backup_artifact(path)}

    for path in sorted(artifacts - allowlist.keys()):
        errors.append(
            f"{path}: tracked backup/editor artifact; recover through git history "
            "or move runtime output outside version-controlled source"
        )

    for path in sorted(allowlist):
        if path not in tracked:
            errors.append(f"{path}: stale allowlist entry is not tracked")
        elif path not in artifacts:
            errors.append(f"{path}: allowlist entry is not a backup/editor artifact")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--allowlist", default=None)
    args = parser.parse_args()

    root = Path(args.root).resolve()
    if args.allowlist:
        requested_allowlist = Path(args.allowlist)
        allowlist_path = (
            requested_allowlist
            if requested_allowlist.is_absolute()
            else root / requested_allowlist
        ).resolve()
    else:
        allowlist_path = root / DEFAULT_ALLOWLIST
    errors = verify(root, allowlist_path)
    if errors:
        print("Tracked source-backup artifact verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("tracked source-backup artifact verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

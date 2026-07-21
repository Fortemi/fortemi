#!/usr/bin/env python3
"""Verify that a docs shard contains every enumerated source exactly once."""

from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import sys
import tarfile


def content_digest(content: str) -> str:
    return hashlib.sha256(content.encode("utf-8")).hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--file-list", type=Path, required=True)
    parser.add_argument("--shard", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    expected: Counter[str] = Counter()
    source_paths: dict[str, list[str]] = defaultdict(list)

    for raw_path in args.file_list.read_text(encoding="utf-8").splitlines():
        path = Path(raw_path)
        if not path.is_file() or path.stat().st_size == 0:
            print(f"ERROR: enumerated source is missing or empty: {path}", file=sys.stderr)
            return 1
        digest = content_digest(path.read_text(encoding="utf-8"))
        expected[digest] += 1
        source_paths[digest].append(raw_path)

    try:
        with tarfile.open(args.shard, "r:gz") as archive:
            manifest_member = archive.extractfile("manifest.json")
            notes_member = archive.extractfile("notes.jsonl")
            if manifest_member is None or notes_member is None:
                raise ValueError("manifest.json or notes.jsonl is missing")
            manifest = json.load(manifest_member)
            notes = [json.loads(line) for line in notes_member if line.strip()]
    except (OSError, tarfile.TarError, ValueError, json.JSONDecodeError) as error:
        print(f"ERROR: cannot inspect docs shard: {error}", file=sys.stderr)
        return 1

    actual: Counter[str] = Counter()
    for index, note in enumerate(notes, start=1):
        content = note.get("original_content")
        if not isinstance(content, str):
            print(
                f"ERROR: notes.jsonl record {index} has no string original_content",
                file=sys.stderr,
            )
            return 1
        actual[content_digest(content)] += 1

    expected_count = sum(expected.values())
    manifest_count = manifest.get("counts", {}).get("notes")
    failures: list[str] = []
    if manifest_count != expected_count:
        failures.append(
            f"manifest notes count is {manifest_count!r}; expected {expected_count}"
        )
    if len(notes) != expected_count:
        failures.append(f"notes.jsonl has {len(notes)} records; expected {expected_count}")

    for digest, count in (expected - actual).items():
        paths = ", ".join(source_paths[digest])
        failures.append(f"missing {count} source occurrence(s): {paths}")
    for digest, count in (actual - expected).items():
        failures.append(f"unexpected note content {digest[:12]} ({count} occurrence(s))")

    if failures:
        print("ERROR: docs shard source coverage failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(f"Docs shard source coverage passed: {expected_count}/{expected_count} sources.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

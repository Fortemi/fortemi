#!/usr/bin/env python3
"""Reject workflow checkout patterns that race immutable event SHAs."""

from __future__ import annotations

import re
import sys
from pathlib import Path


WORKFLOW_GLOBS = (".gitea/workflows/*.yml", ".gitea/workflows/*.yaml")
EXACT_SHA_FETCH = re.compile(
    r'git fetch --quiet --no-tags(?: --depth=1)? origin '
    r'"\$\{GITHUB_SHA:\?GITHUB_SHA is required\}"'
)
FORBIDDEN_PATTERNS = {
    "shallow branch-tip clone": re.compile(r"git clone .*--depth 1 .*--branch"),
    "event SHA checkout after clone": re.compile(
        r"git checkout [^\n]*\$\{GITHUB_SHA(?::-HEAD)?\}"
    ),
    "workflow token embedded in Git URL": re.compile(
        r"https://token:\$\{\{\s*github\.token\s*\}\}@"
    ),
}


def workflow_files() -> list[Path]:
    files: list[Path] = []
    for pattern in WORKFLOW_GLOBS:
        files.extend(Path(".").glob(pattern))
    return sorted(files)


def verify_workflows(paths: list[Path]) -> list[str]:
    failures: list[str] = []
    exact_fetches = 0

    for path in paths:
        text = path.read_text()
        exact_fetches += len(EXACT_SHA_FETCH.findall(text))
        for label, pattern in FORBIDDEN_PATTERNS.items():
            for match in pattern.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                failures.append(f"{path}:{line}: {label}")

    if exact_fetches == 0:
        failures.append(
            "no fail-closed immutable GITHUB_SHA fetches found in Gitea workflows"
        )

    return failures


def main() -> int:
    failures = verify_workflows(workflow_files())
    if failures:
        print("Workflow checkout contract failed.", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("workflow checkout contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

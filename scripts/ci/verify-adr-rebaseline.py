#!/usr/bin/env python3
"""Verify ADR-088 through ADR-100 checkpoint governance metadata."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ADR_OWNERS = {
    "088": "Fortemi/fortemi#712",
    "089": "Fortemi/fortemi#710",
    "090": "Fortemi/fortemi#733",
    "091": "Fortemi/fortemi#711",
    "092": "Fortemi/fortemi#713",
    "093": "Fortemi/fortemi#734",
    "094": "Fortemi/fortemi#1017",
    "095": "Fortemi/fortemi#715",
    "096": "Fortemi/fortemi#716",
    "097": "Fortemi/fortemi#717",
    "098": "Fortemi/fortemi#714",
    "099": "Fortemi/fortemi#719",
    "100": "Fortemi-Enterprise/mcp-gate#2",
}
CHECKPOINT_HEADING = "## July 2026 checkpoint rebaseline"
CHECKPOINT_DATE = "2026-07-14"
FIELDS = (
    "Decision status",
    "Implementation phase",
    "Phase owner",
    "Checkpoint decision date",
)
STALE_DUPLICATE_TRACKERS = ("Fortemi/fortemi#1016", "Fortemi/fortemi#1019")
ACCEPTED_CHECKPOINT_STATUSES = {
    "090": "Accepted target architecture",
    "092": "Accepted; core contract implemented",
    "093": "Accepted target architecture",
}


def checkpoint_section(text: str) -> str | None:
    marker = f"{CHECKPOINT_HEADING}\n"
    if text.count(CHECKPOINT_HEADING) != 1 or marker not in text:
        return None
    remainder = text.split(marker, 1)[1]
    return remainder.split("\n## ", 1)[0]


def verify_checkpoint(text: str, adr: str, owner: str) -> list[str]:
    failures: list[str] = []
    section = checkpoint_section(text)
    if section is None:
        return [f"ADR-{adr}: expected exactly one checkpoint section"]

    for field in FIELDS:
        marker = f"**{field}:**"
        if section.count(marker) != 1:
            failures.append(f"ADR-{adr}: expected exactly one {field} field")

    expected_status = ACCEPTED_CHECKPOINT_STATUSES.get(adr, "Proposed")
    if expected_status not in section:
        failures.append(f"ADR-{adr}: missing status {expected_status!r}")
    if owner not in section:
        failures.append(f"ADR-{adr}: missing phase owner {owner}")
    if f"**Checkpoint decision date:** {CHECKPOINT_DATE}." not in section:
        failures.append(f"ADR-{adr}: missing checkpoint date {CHECKPOINT_DATE}")

    return failures


def verify_repository(root: Path) -> list[str]:
    failures: list[str] = []
    adr_files: list[Path] = []

    for adr, owner in ADR_OWNERS.items():
        matches = sorted(
            (root / "docs" / "architecture" / "adr").glob(f"ADR-{adr}-*.md")
        )
        if len(matches) != 1:
            failures.append(f"ADR-{adr}: expected one source file, found {len(matches)}")
            continue
        path = matches[0]
        adr_files.append(path)
        failures.extend(verify_checkpoint(path.read_text(), adr, owner))

    checklist = root / ".aiwg" / "architecture" / "adr-rebaseline-checklist-2026-07.md"
    roadmap = root / ".aiwg" / "planning" / "roadmap.md"
    governed_files = [*adr_files, checklist, roadmap]

    for path in governed_files:
        text = path.read_text()
        for stale in STALE_DUPLICATE_TRACKERS:
            if stale in text:
                failures.append(f"{path.relative_to(root)}: stale tracker {stale}")

    checklist_text = checklist.read_text()
    for owner in ADR_OWNERS.values():
        if owner not in checklist_text:
            failures.append(f"{checklist.relative_to(root)}: missing owner {owner}")
    if CHECKPOINT_DATE not in checklist_text:
        failures.append(f"{checklist.relative_to(root)}: missing {CHECKPOINT_DATE}")

    roadmap_text = roadmap.read_text()
    for marker in ("milestone #62", "Fortemi/fortemi#733", "Fortemi/fortemi#734"):
        if not re.search(re.escape(marker), roadmap_text, re.IGNORECASE):
            failures.append(f"{roadmap.relative_to(root)}: missing {marker}")

    return failures


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    failures = verify_repository(root)
    if failures:
        print("ADR rebaseline contract failed.", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("ADR rebaseline contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

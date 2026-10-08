#!/usr/bin/env python3
"""Fail CI when migrations and the managed PostgreSQL extension matrix disagree.

Every `CREATE EXTENSION` in migrations/*.sql is classified as:

- required: a bare statement; migrations fail if the extension is unavailable.
- optional: inside a `DO` block that traps errors (`EXCEPTION`) or checks
  `pg_available_extensions` first, so migrations continue without it.

The classification is compared against the machine-readable block in
docs/deployment/managed-postgres-compatibility.md. Matrix entries marked
`prerequisite` are extensions the schema depends on but that migrations do not
create (an administrator creates them before migrating).

Usage: verify-postgres-extension-matrix.py [repo-root]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


DOC_PATH = Path("docs/deployment/managed-postgres-compatibility.md")
BEGIN_MARKER = "<!-- extension-matrix:begin -->"
END_MARKER = "<!-- extension-matrix:end -->"
CLASSES = ("required", "optional", "prerequisite")

CREATE_RE = re.compile(
    r"\bCREATE\s+EXTENSION\s+(?:IF\s+NOT\s+EXISTS\s+)?(\"[^\"]+\"|[A-Za-z_][\w-]*)",
    re.IGNORECASE,
)
DO_RE = re.compile(r"\bDO\s+(\$[A-Za-z_]*\$)", re.IGNORECASE)
GUARD_RE = re.compile(r"\bEXCEPTION\b|\bpg_available_extensions\b", re.IGNORECASE)


def strip_comments(sql: str) -> str:
    """Remove -- and /* */ comments while leaving quoted text intact."""
    out: list[str] = []
    i, n = 0, len(sql)
    while i < n:
        ch = sql[i]
        if sql.startswith("--", i):
            end = sql.find("\n", i)
            i = n if end == -1 else end
        elif sql.startswith("/*", i):
            end = sql.find("*/", i + 2)
            i = n if end == -1 else end + 2
        elif ch == "'":
            end = i + 1
            while end < n:
                if sql[end] == "'" and sql.startswith("''", end):
                    end += 2
                elif sql[end] == "'":
                    break
                else:
                    end += 1
            out.append(sql[i : end + 1])
            i = end + 1
        else:
            out.append(ch)
            i += 1
    return "".join(out)


def guarded_spans(sql: str) -> list[tuple[int, int]]:
    """Return spans of DO blocks whose body traps extension failures."""
    spans: list[tuple[int, int]] = []
    for match in DO_RE.finditer(sql):
        tag = match.group(1)
        body_start = match.end()
        body_end = sql.find(tag, body_start)
        if body_end == -1:
            continue
        if GUARD_RE.search(sql[body_start:body_end]):
            spans.append((body_start, body_end))
    return spans


def scan_migrations(root: Path) -> dict[str, dict[str, list[str]]]:
    """Map extension name -> {"required": [locations], "optional": [locations]}."""
    found: dict[str, dict[str, list[str]]] = {}
    for path in sorted((root / "migrations").glob("*.sql")):
        sql = strip_comments(path.read_text(encoding="utf-8"))
        spans = guarded_spans(sql)
        for match in CREATE_RE.finditer(sql):
            name = match.group(1).strip('"').lower()
            pos = match.start()
            kind = (
                "optional"
                if any(start <= pos < end for start, end in spans)
                else "required"
            )
            line = sql.count("\n", 0, pos) + 1
            location = f"{path.relative_to(root)}:{line}"
            found.setdefault(name, {"required": [], "optional": []})[kind].append(
                location
            )
    return found


def parse_matrix(root: Path) -> dict[str, str]:
    doc = root / DOC_PATH
    if not doc.is_file():
        raise ValueError(f"{DOC_PATH} not found")
    text = doc.read_text(encoding="utf-8")
    start = text.find(BEGIN_MARKER)
    end = text.find(END_MARKER)
    if start == -1 or end == -1 or end < start:
        raise ValueError(f"{DOC_PATH}: missing {BEGIN_MARKER} ... {END_MARKER} block")
    matrix: dict[str, str] = {}
    for raw in text[start + len(BEGIN_MARKER) : end].splitlines():
        line = raw.strip()
        if not line or line.startswith("```") or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2 or parts[0] not in CLASSES:
            raise ValueError(
                f"{DOC_PATH}: bad matrix line {line!r}; "
                f"expected '<{'|'.join(CLASSES)}> <extension>'"
            )
        cls, name = parts[0], parts[1].lower()
        if name in matrix:
            raise ValueError(f"{DOC_PATH}: extension {name!r} listed twice")
        matrix[name] = cls
    return matrix


def compare(found: dict[str, dict[str, list[str]]], matrix: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for name, uses in sorted(found.items()):
        is_required = bool(uses["required"])
        where = ", ".join(uses["required"] or uses["optional"])
        declared = matrix.get(name)
        if declared is None:
            kind = "required" if is_required else "optional"
            errors.append(
                f"{name}: created as {kind} at {where} but missing from the "
                f"matrix in {DOC_PATH}; document provider support and add "
                f"'{kind} {name}'"
            )
        elif is_required and declared != "required":
            errors.append(
                f"{name}: created without a guard at {where} (required) but the "
                f"matrix lists it as {declared}"
            )
        elif not is_required and declared == "prerequisite":
            errors.append(
                f"{name}: created by migrations at {where} but the matrix lists "
                "it as prerequisite (not created by migrations)"
            )
        elif not is_required and declared == "required":
            errors.append(
                f"{name}: every CREATE EXTENSION is guarded ({where}) but the "
                "matrix lists it as required; mark it optional"
            )
    for name, declared in sorted(matrix.items()):
        if declared in ("required", "optional") and name not in found:
            errors.append(
                f"{name}: matrix lists it as {declared} but no migration "
                "creates it; remove the entry or mark it prerequisite"
            )
    return errors


def main(argv: list[str]) -> int:
    root = Path(argv[1]) if len(argv) > 1 else Path(".")
    try:
        matrix = parse_matrix(root)
    except ValueError as exc:
        print(f"extension matrix: {exc}", file=sys.stderr)
        return 1
    found = scan_migrations(root)
    errors = compare(found, matrix)
    if errors:
        print("PostgreSQL extension matrix is out of date:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    summary = ", ".join(f"{name}={matrix[name]}" for name in sorted(matrix))
    print(f"PostgreSQL extension matrix matches migrations ({summary})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

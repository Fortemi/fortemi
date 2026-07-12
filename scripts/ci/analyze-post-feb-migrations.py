#!/usr/bin/env python3
"""Inventory post-February migrations for the release upgrade gate."""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MIGRATIONS = ROOT / "migrations"
BASELINE = 20260215000000
LARGE_TABLES = {
    "note_original",
    "note_original_history",
    "embedding",
    "embeddings",
    "note_embeddings",
    "embedding_chunks",
    "chunks",
}


def normalize_sql(sql: str) -> str:
    sql = re.sub(r"--.*", " ", sql)
    sql = re.sub(r"/\*.*?\*/", " ", sql, flags=re.S)
    return re.sub(r"\s+", " ", sql).strip().lower()


def migration_version(path: Path) -> int:
    return int(path.name.split("_", 1)[0])


def main() -> None:
    entries = []
    totals = {
        "post_feb_migrations": 0,
        "data_backfills": 0,
        "index_builds": 0,
        "table_alters": 0,
        "large_table_touches": 0,
    }

    for path in sorted(MIGRATIONS.glob("*.sql"), key=migration_version):
        version = migration_version(path)
        if version <= BASELINE:
            continue

        sql = normalize_sql(path.read_text())
        data_backfill = bool(
            re.search(r"\bupdate\b", sql)
            or re.search(r"\binsert\s+into\b", sql)
            or re.search(r"\bdelete\s+from\b", sql)
        )
        index_builds = len(
            re.findall(
                r"\bcreate\s+(?:unique\s+)?index\s+(?:concurrently\s+)?(?:if\s+not\s+exists\s+)?",
                sql,
            )
        )
        table_alters = len(re.findall(r"\balter\s+table\b", sql))
        touched_large = sorted(table for table in LARGE_TABLES if re.search(rf"\b{re.escape(table)}\b", sql))

        totals["post_feb_migrations"] += 1
        totals["data_backfills"] += int(data_backfill)
        totals["index_builds"] += index_builds
        totals["table_alters"] += table_alters
        totals["large_table_touches"] += int(bool(touched_large))

        if data_backfill or index_builds or table_alters or touched_large:
            entries.append(
                {
                    "file": path.name,
                    "data_backfill": data_backfill,
                    "index_builds": index_builds,
                    "table_alters": table_alters,
                    "large_tables": touched_large,
                }
            )

    report = {"baseline_exclusive": BASELINE, "totals": totals, "risk_entries": entries}
    print(json.dumps(report, indent=2, sort_keys=True))

    if totals["post_feb_migrations"] == 0:
        raise SystemExit("FAIL: no post-February migrations found")
    if totals["data_backfills"] == 0 or totals["index_builds"] == 0:
        raise SystemExit("FAIL: expected post-February backfills and index builds in risk inventory")


if __name__ == "__main__":
    main()

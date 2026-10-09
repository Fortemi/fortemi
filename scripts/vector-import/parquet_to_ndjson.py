#!/usr/bin/env python3
"""Convert Parquet profile/deletion rows to Fortemi vector-import NDJSON."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any, Iterable

import pyarrow.parquet as pq


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Emit Fortemi external embedding run upload NDJSON from Parquet files."
    )
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--space-id", required=True)
    parser.add_argument("--profiles", required=True, type=Path)
    parser.add_argument("--deletions", type=Path)
    parser.add_argument("--previous-run-id")
    parser.add_argument("--template-version", required=True)
    parser.add_argument(
        "--created-at",
        default=dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        help="RFC3339 timestamp for the manifest; default is current UTC time.",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=1000,
        help="Parquet record batch size; default 1000.",
    )
    return parser.parse_args()


def json_default(value: Any) -> Any:
    if hasattr(value, "as_py"):
        return value.as_py()
    if isinstance(value, (dt.datetime, dt.date)):
        return value.isoformat()
    return str(value)


def parquet_rows(path: Path, batch_size: int) -> Iterable[dict[str, Any]]:
    parquet = pq.ParquetFile(path)
    for batch in parquet.iter_batches(batch_size=batch_size):
        for row in batch.to_pylist():
            yield row


def row_line(row_type: str, row: dict[str, Any]) -> bytes:
    out = {"type": row_type}
    out.update(row)
    return json.dumps(
        out,
        separators=(",", ":"),
        ensure_ascii=False,
        default=json_default,
    ).encode("utf-8")


def write_rows(
    temp,
    digest: "hashlib._Hash",
    row_type: str,
    path: Path,
    batch_size: int,
) -> int:
    count = 0
    for row in parquet_rows(path, batch_size):
        line = row_line(row_type, row)
        temp.write(line)
        temp.write(b"\n")
        digest.update(line)
        digest.update(b"\n")
        count += 1
    return count


def main() -> int:
    args = parse_args()
    if args.batch_size <= 0:
        raise SystemExit("--batch-size must be positive")

    digest = hashlib.sha256()
    with tempfile.NamedTemporaryFile("w+b") as temp:
        profile_count = write_rows(temp, digest, "profile", args.profiles, args.batch_size)
        deletion_count = 0
        if args.deletions:
            deletion_count = write_rows(temp, digest, "deletion", args.deletions, args.batch_size)
        temp.flush()
        temp.seek(0)

        manifest = {
            "type": "manifest",
            "run_id": args.run_id,
            "previous_run_id": args.previous_run_id,
            "space_id": args.space_id,
            "created_at": args.created_at,
            "counts": {"profiles": profile_count, "deletions": deletion_count},
            "template_version": args.template_version,
            "body_sha256": digest.hexdigest(),
        }
        sys.stdout.write(json.dumps(manifest, separators=(",", ":")) + "\n")
        sys.stdout.flush()
        shutil.copyfileobj(temp, sys.stdout.buffer)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

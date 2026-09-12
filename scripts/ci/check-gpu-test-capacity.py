#!/usr/bin/env python3
"""Refuse shared-host GPU tests when the designated single GPU lacks headroom."""

import csv
import io
import json
import subprocess
import sys
from pathlib import Path


def check_capacity(gpu_csv, meminfo):
    rows = list(csv.reader(io.StringIO(gpu_csv.strip())))
    if len(rows) != 1 or len(rows[0]) != 2:
        raise ValueError("GPU tests require exactly one visible GPU; routing is otherwise ambiguous")
    total, free = (int(value.strip()) for value in rows[0])
    if not 0 <= free <= total:
        raise ValueError("Invalid GPU memory observation")
    if free < 16384:
        raise ValueError(f"GPU test capacity unavailable: {free} MiB free; 16384 MiB required")
    available = None
    for line in meminfo.splitlines():
        fields = line.split()
        if fields and fields[0] == "MemAvailable:":
            if len(fields) != 3 or fields[2] != "kB":
                raise ValueError("Invalid host memory observation")
            available = int(fields[1])
    if available is None or available < 16 * 1024 * 1024:
        raise ValueError("GPU tests require at least 16 GiB available host RAM")
    return {"status": "PASS", "gpu_total_mib": total, "gpu_free_mib": free,
            "host_available_kib": available,
            "scope": "Point-in-time capacity check, not a GPU lease or runtime memory limit"}


def main():
    try:
        result = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.total,memory.free", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10, check=True,
        )
        print(json.dumps(check_capacity(result.stdout, Path("/proc/meminfo").read_text())))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"GPU preflight refused: {error}. Coordinate a capacity window; do not stop shared services automatically.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Verify the rendered bundle preserves the jobs diagnostic controls."""

import json
import sys


EXPECTED = {
    "FORTEMI_DIAGNOSTIC_PROFILE": "jobs",
    "RUST_LOG": "warn",
    "LOG_FORMAT": "text",
    "LOG_FILE": "/tmp/jobs-diagnostic.jsonl",
    "LOG_ANSI": "true",
    "JOB_TIMEOUT_SECS": "120",
    "JOB_STALE_REAP_INTERVAL_SECS": "7",
}


def main() -> int:
    document = json.load(sys.stdin)
    environment = document["services"]["fortemi"]["environment"]
    mismatches = {
        name: {"expected": expected, "actual": environment.get(name)}
        for name, expected in EXPECTED.items()
        if environment.get(name) != expected
    }
    if mismatches:
        print(
            "jobs diagnostic compose forwarding mismatch: "
            + ", ".join(sorted(mismatches)),
            file=sys.stderr,
        )
        return 1
    print("jobs diagnostic compose forwarding passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

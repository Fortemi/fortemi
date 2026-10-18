#!/usr/bin/env python3
"""Enforce #1221 Option B against resolved metadata and the full lock graph.

Generate input with the audit job's Cargo toolchain:
  cargo metadata --locked --offline --all-features --format-version 1 > metadata.json
  python3 scripts/ci/zip-exception-guard.py metadata.json --lockfile Cargo.lock
Omit metadata.json (or use '-') to read stdin. Requires Python 3.11+.
"""

import argparse
from datetime import date, datetime, timezone
import json
from pathlib import Path
import sys
import tomllib


# Single executable expiry; keep the ZIP comments in both policy files in sync.
EXCEPTION_EXPIRY = date(2026, 11, 16)
APPROVED_PARENTS = {
    "2.4.2": ("calamine", "0.26.1", None),
    "3.0.0": ("utoipa-swagger-ui", "9.0.2", "build"),
}


class GuardError(ValueError):
    pass


def is_zip9(package):
    return package["name"] == "zip" and package["version"].split(".")[0] == "9"


def identity(package):
    return package["name"], package["version"], package.get("source")


def label(package):
    return f'{package["name"]}@{package["version"]}'


def reachable(roots, edges, blocked=()):
    seen = set()
    pending = list(roots)
    while pending:
        node = pending.pop()
        if node in seen or node in blocked:
            continue
        seen.add(node)
        pending.extend(child for child, _ in edges[node])
    return seen


def check_graph(packages, edges, roots, graph_name, approved_edges=None):
    active = reachable(roots, edges)
    checked_edges = set()
    for child in active:
        package = packages[child]
        if package["name"] != "zip" or is_zip9(package):
            continue
        expected = APPROVED_PARENTS.get(package["version"])
        if expected is None:
            raise GuardError(f"{graph_name}: unapproved {label(package)}")
        parents = [
            (parent, kinds)
            for parent in active
            for target, kinds in edges[parent]
            if target == child
        ]
        if child in roots or not parents:
            raise GuardError(f"{graph_name}: {label(package)} has no approved parent")
        for parent, kinds in parents:
            consumer = packages[parent]
            if parent in roots:
                raise GuardError(
                    f"{graph_name}: workspace crate {label(consumer)} depends on {label(package)}"
                )
            if (consumer["name"], consumer["version"]) != expected[:2]:
                raise GuardError(
                    f"{graph_name}: extra consumer {label(consumer)} -> {label(package)}"
                )
            if kinds is not None and set(kinds) != {expected[2]}:
                raise GuardError(
                    f"{graph_name}: wrong edge kind {kinds!r} for "
                    f"{label(consumer)} -> {label(package)}"
                )
            edge = identity(consumer), identity(package)
            if approved_edges is not None and edge not in approved_edges:
                raise GuardError(f"{graph_name}: old ZIP edge missing from resolved metadata: {edge}")
            checked_edges.add(edge)

    # Every workspace-to-libbz2 path must pass through ZIP 9. Checking only its
    # immediate parent (bzip2) would miss a second consumer of that same parent.
    without_zip9 = reachable(roots, edges, {p for p in active if is_zip9(packages[p])})
    for node in without_zip9:
        if packages[node]["name"] == "libbz2-rs-sys":
            raise GuardError(f"{graph_name}: libbz2-rs-sys reachable without zip 9")
    return checked_edges


def check(metadata, lockfile, today=None):
    """Raise GuardError on drift. Date injection is for unit tests only."""
    if today is None:
        today = datetime.now(timezone.utc).date()
    if today > EXCEPTION_EXPIRY:
        raise GuardError(f"ZIP exceptions expired after {EXCEPTION_EXPIRY} (UTC today: {today})")

    packages = {p["id"]: p for p in metadata["packages"]}
    roots = set(metadata["workspace_members"])
    nodes = metadata["resolve"]["nodes"]
    edges = {
        node["id"]: [
            (dep["pkg"], [entry["kind"] for entry in dep["dep_kinds"]])
            for dep in node["deps"]
        ]
        for node in nodes
    }
    if not roots or not roots <= packages.keys() or not roots <= edges.keys():
        raise GuardError("metadata: missing workspace roots or resolved graph")
    for parent, dependencies in edges.items():
        if parent not in packages:
            raise GuardError(f"metadata: unknown package {parent}")
        for child, kinds in dependencies:
            if child not in packages or child not in edges or not kinds:
                raise GuardError(f"metadata: incomplete dependency edge from {parent}")
    approved = check_graph(packages, edges, roots, "metadata")

    locked = {identity(p): p for p in lockfile["package"]}
    if len(locked) != len(lockfile["package"]):
        raise GuardError("Cargo.lock: duplicate package identity")
    for node in reachable(roots, edges):
        if identity(packages[node]) not in locked:
            raise GuardError(f"metadata package absent from Cargo.lock: {label(packages[node])}")

    lock_edges = {}
    for key, package in locked.items():
        dependencies = []
        for dependency in package.get("dependencies", []):
            # Cargo.lock uses name [version] [(source)] to disambiguate packages.
            parts = dependency.split(" ", 2)
            matches = [
                candidate for candidate in locked
                if candidate[0] == parts[0]
                and (len(parts) < 2 or candidate[1] == parts[1])
                and (len(parts) < 3 or f"({candidate[2]})" == parts[2])
            ]
            if len(matches) != 1:
                raise GuardError(f"Cargo.lock: unresolved or ambiguous dependency {dependency!r}")
            dependencies.append((matches[0], None))
        lock_edges[key] = dependencies
    lock_roots = {identity(packages[root]) for root in roots}
    # Cargo.lock also covers optional/platform paths absent from this feature
    # resolution. It cannot prove edge kinds, so old ZIP edges must additionally
    # have been verified in metadata above.
    check_graph(locked, lock_edges, lock_roots, "Cargo.lock", approved)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("metadata", nargs="?", default="-", help="metadata JSON file, or - for stdin")
    parser.add_argument("--lockfile", type=Path, default=Path("Cargo.lock"))
    args = parser.parse_args(argv)
    try:
        if args.metadata == "-":
            metadata = json.load(sys.stdin)
        else:
            metadata = json.loads(Path(args.metadata).read_text())
        with args.lockfile.open("rb") as stream:
            lockfile = tomllib.load(stream)
        check(metadata, lockfile)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"ZIP exception guard: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"ZIP exception guard: PASS (expires {EXCEPTION_EXPIRY}, UTC)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

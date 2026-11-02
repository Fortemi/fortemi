#!/usr/bin/env python3
"""Require patched stable ZIP 9 in resolved metadata and the full lock graph.

Generate input with the audit job's Cargo toolchain:
  cargo metadata --locked --offline --all-features --format-version 1 > metadata.json
  python3 scripts/ci/zip-exception-guard.py metadata.json --lockfile Cargo.lock
Omit metadata.json (or use '-') to read stdin. Requires Python 3.11+.
"""

import argparse
import json
from pathlib import Path
import sys
import re
import tomllib


class GuardError(ValueError):
    pass


# zip 9.0.0-9.0.3 and every prerelease are affected by RUSTSEC-2026-0367..0369 or
# unreleased; only stable ZIP 9 releases from 9.0.4 on are allowed.
MINIMUM_PATCHED_ZIP = (9, 0, 4)


def is_zip9(package):
    if package["name"] != "zip":
        return False
    version = package["version"]
    if not re.fullmatch(r"9\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        return False
    return tuple(int(part) for part in version.split(".")) >= MINIMUM_PATCHED_ZIP


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


def check_graph(packages, edges, roots, graph_name):
    # Check every package, including optional/platform or disconnected entries.
    # There are no parent, edge-kind, version, or date exemptions.
    for package in packages.values():
        if package["name"] == "zip" and not is_zip9(package):
            raise GuardError(f"{graph_name}: unapproved {label(package)}")
    active = reachable(roots, edges)

    # Every workspace-to-libbz2 path must pass through ZIP 9. Checking only its
    # immediate parent (bzip2) would miss a second consumer of that same parent.
    without_zip9 = reachable(roots, edges, {p for p in active if is_zip9(packages[p])})
    for node in without_zip9:
        if packages[node]["name"] == "libbz2-rs-sys":
            raise GuardError(f"{graph_name}: libbz2-rs-sys reachable without zip 9")


def check(metadata, lockfile):
    """Raise GuardError on policy drift or incomplete graph input."""
    try:
        check_data(metadata, lockfile)
    except (KeyError, TypeError, AttributeError, IndexError) as error:
        raise GuardError(f"malformed graph input: {error}") from error


def check_data(metadata, lockfile):
    package_list = metadata["packages"]
    nodes = metadata["resolve"]["nodes"]
    if not isinstance(package_list, list) or not isinstance(nodes, list):
        raise GuardError("metadata: packages and nodes must be arrays")
    packages = {p["id"]: p for p in package_list}
    if len(packages) != len(package_list):
        raise GuardError("metadata: duplicate package id")
    for package in package_list:
        if not all(isinstance(package[key], str) and package[key]
                   for key in ("id", "name", "version")):
            raise GuardError("metadata: invalid package identity")
        if package.get("source") is not None and not isinstance(package["source"], str):
            raise GuardError("metadata: invalid package source")
    if len({identity(p) for p in package_list}) != len(packages):
        raise GuardError("metadata: duplicate package identity")
    members = metadata["workspace_members"]
    if not isinstance(members, list) or len(set(members)) != len(members):
        raise GuardError("metadata: invalid workspace members")
    roots = set(members)
    edges = {}
    for node in nodes:
        parent = node["id"]
        if parent in edges:
            raise GuardError("metadata: duplicate resolved node")
        if not isinstance(node["deps"], list):
            raise GuardError("metadata: dependencies must be an array")
        dependencies = []
        for dep in node["deps"]:
            kinds = dep["dep_kinds"]
            if not isinstance(kinds, list) or not kinds:
                raise GuardError(f"metadata: incomplete dependency edge from {parent}")
            for entry in kinds:
                if entry["kind"] not in (None, "build", "dev"):
                    raise GuardError(f"metadata: invalid dependency kind from {parent}")
                if entry["target"] is not None and not isinstance(entry["target"], str):
                    raise GuardError(f"metadata: invalid dependency target from {parent}")
            dependencies.append((dep["pkg"], [entry["kind"] for entry in kinds]))
        # Cargo supplies both forms. Renamed dependencies use pkg IDs here.
        if "dependencies" in node:
            ids = node["dependencies"]
            if not isinstance(ids, list) or set(ids) != {child for child, _ in dependencies}:
                raise GuardError("metadata: divergent dependency representations")
        edges[parent] = dependencies
    if not roots or not roots <= packages.keys() or not roots <= edges.keys():
        raise GuardError("metadata: missing workspace roots or resolved graph")
    if packages.keys() != edges.keys():
        raise GuardError("metadata: package/node graph mismatch")
    if metadata["resolve"].get("root") is not None and metadata["resolve"]["root"] not in roots:
        raise GuardError("metadata: resolve root is not a workspace member")
    for parent, dependencies in edges.items():
        for child, kinds in dependencies:
            if child not in packages or child not in edges:
                raise GuardError(f"metadata: incomplete dependency edge from {parent}")
    check_graph(packages, edges, roots, "metadata")

    if not isinstance(lockfile["package"], list):
        raise GuardError("Cargo.lock: packages must be an array")
    for package in lockfile["package"]:
        if not all(isinstance(package[key], str) and package[key]
                   for key in ("name", "version")):
            raise GuardError("Cargo.lock: invalid package identity")
        if package.get("source") is not None and not isinstance(package["source"], str):
            raise GuardError("Cargo.lock: invalid package source")
    locked = {identity(p): p for p in lockfile["package"]}
    if len(locked) != len(lockfile["package"]):
        raise GuardError("Cargo.lock: duplicate package identity")
    for node in packages:
        if identity(packages[node]) not in locked:
            raise GuardError(f"metadata package absent from Cargo.lock: {label(packages[node])}")

    lock_edges = {}
    for key, package in locked.items():
        dependencies = []
        raw_dependencies = package.get("dependencies", [])
        if not isinstance(raw_dependencies, list):
            raise GuardError("Cargo.lock: dependencies must be an array")
        for dependency in raw_dependencies:
            if not isinstance(dependency, str):
                raise GuardError("Cargo.lock: dependency must be a string")
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
    # Lock graph can include extra optional/platform paths, but every resolved
    # metadata edge must exist in it with the same source-qualified identity.
    for parent, dependencies in edges.items():
        locked_children = {child for child, _ in lock_edges[identity(packages[parent])]}
        for child, _ in dependencies:
            if identity(packages[child]) not in locked_children:
                raise GuardError("metadata dependency edge absent from Cargo.lock")
    check_graph(locked, lock_edges, lock_roots, "Cargo.lock")


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
    print("ZIP exception guard: PASS (stable ZIP 9 >=9.0.4; no ZIP waivers)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

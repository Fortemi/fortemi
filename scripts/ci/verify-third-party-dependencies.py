#!/usr/bin/env python3
"""Verify reviewed third-party container and package-feed inputs."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
from pathlib import Path
from typing import Any


MANIFEST = Path("docker/third-party-dependencies.json")
DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
FROM = re.compile(r"^\s*FROM\s+([^\s]+)", re.IGNORECASE | re.MULTILINE)
COMPOSE_IMAGE = re.compile(r"^\s*image:\s*(\S.*?)\s*$", re.MULTILINE)
DEFAULT_VALUE = re.compile(r"\$\{[^{}]*:-([^{}]*)\}")
URL = re.compile(r"https://[A-Za-z0-9._~:/?#\[\]@!$&'()*+,;=%-]+")
REQUIRED_IMAGE_FIELDS = {
    "id",
    "tag",
    "digest",
    "immutable_reference",
    "role",
    "privilege_boundary",
    "source_class",
    "upstream",
    "surfaces",
    "reviewed_at",
    "update_cadence_days",
    "rollback",
    "mirror_policy",
}
REQUIRED_FEED_FIELDS = {
    "id",
    "url",
    "role",
    "surface",
    "verification",
    "key_fingerprint",
    "version_policy",
    "sbom_evidence",
    "reviewed_at",
    "update_cadence_days",
    "rollback",
    "exception_owner",
    "exception_expires",
}
EXCLUDED_SCAN_PARTS = {".git", "node_modules", "target"}


def is_scannable(path: Path) -> bool:
    return not EXCLUDED_SCAN_PARTS.intersection(path.parts)


def parse_date(value: Any, label: str, errors: list[str]) -> dt.date | None:
    try:
        return dt.date.fromisoformat(str(value))
    except ValueError:
        errors.append(f"{label} must be an ISO date")
        return None


def resolve_defaults(value: str) -> str:
    previous = ""
    while previous != value:
        previous = value
        value = DEFAULT_VALUE.sub(lambda match: match.group(1), value)
    return value


def normalize_reference(reference: str) -> str:
    if reference.startswith("docker.io/library/"):
        return reference.removeprefix("docker.io/library/")
    if reference.startswith("docker.io/"):
        return reference.removeprefix("docker.io/")
    return reference


def dockerfiles(root: Path) -> list[Path]:
    return sorted(
        {
            path
            for pattern in ("Dockerfile*", "**/Dockerfile*")
            for path in root.glob(pattern)
            if path.is_file() and is_scannable(path.relative_to(root))
        }
    )


def compose_files(root: Path) -> list[Path]:
    return sorted(
        {
            path
            for pattern in ("*compose*.yml", "*compose*.yaml", "**/*compose*.yml", "**/*compose*.yaml")
            for path in root.glob(pattern)
            if path.is_file() and is_scannable(path.relative_to(root))
        }
    )


def load_manifest(root: Path, errors: list[str]) -> dict[str, Any] | None:
    path = root / MANIFEST
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        errors.append(f"{MANIFEST}: cannot read structured manifest: {error}")
        return None
    if not isinstance(data, dict):
        errors.append(f"{MANIFEST}: root must be an object")
        return None
    return data


def validate_review(
    item: dict[str, Any],
    label: str,
    today: dt.date,
    errors: list[str],
) -> None:
    reviewed = parse_date(item.get("reviewed_at"), f"{label}.reviewed_at", errors)
    cadence = item.get("update_cadence_days")
    if not isinstance(cadence, int) or cadence <= 0:
        errors.append(f"{label}.update_cadence_days must be a positive integer")
    elif reviewed is not None and reviewed + dt.timedelta(days=cadence) < today:
        errors.append(f"{label} review is stale")


def verify(root: Path, *, today: dt.date | None = None) -> list[str]:
    root = root.resolve()
    today = today or dt.date.today()
    errors: list[str] = []
    manifest = load_manifest(root, errors)
    if manifest is None:
        return errors

    if manifest.get("schema_version") != 1:
        errors.append(f"{MANIFEST}: schema_version must be 1")
    images = manifest.get("images")
    feeds = manifest.get("package_feeds")
    internal_patterns = manifest.get("internal_image_patterns")
    if not isinstance(images, list) or not images:
        errors.append(f"{MANIFEST}: images must be a non-empty array")
        return errors
    if not isinstance(feeds, list):
        errors.append(f"{MANIFEST}: package_feeds must be an array")
        return errors
    if not isinstance(internal_patterns, list):
        errors.append(f"{MANIFEST}: internal_image_patterns must be an array")
        return errors

    immutable_to_item: dict[str, dict[str, Any]] = {}
    declared_surfaces: dict[str, set[str]] = {}
    ids: set[str] = set()
    for index, item in enumerate(images):
        label = f"images[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{label} must be an object")
            continue
        missing = sorted(REQUIRED_IMAGE_FIELDS - item.keys())
        if missing:
            errors.append(f"{label} missing fields: {', '.join(missing)}")
            continue
        identifier = item["id"]
        if not isinstance(identifier, str) or not identifier:
            errors.append(f"{label}.id must be a non-empty string")
        elif identifier in ids:
            errors.append(f"{label}.id is duplicated: {identifier}")
        else:
            ids.add(identifier)
        digest = item["digest"]
        immutable = item["immutable_reference"]
        if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
            errors.append(f"{label}.digest must be a sha256 digest")
        if (
            not isinstance(immutable, str)
            or "@sha256:" not in immutable
            or not immutable.endswith(f"@{digest}")
        ):
            errors.append(f"{label}.immutable_reference must end with its digest")
        elif normalize_reference(str(item["tag"])) != immutable.rsplit("@", 1)[0]:
            errors.append(f"{label}.tag does not match immutable_reference")
        if isinstance(immutable, str):
            if immutable in immutable_to_item:
                errors.append(f"{label}.immutable_reference is duplicated")
            immutable_to_item[immutable] = item
        surfaces = item["surfaces"]
        if not isinstance(surfaces, list) or not surfaces:
            errors.append(f"{label}.surfaces must be a non-empty array")
            surfaces = []
        if isinstance(immutable, str):
            declared_surfaces[immutable] = set(map(str, surfaces))
        if not isinstance(item["privilege_boundary"], list) or not item["privilege_boundary"]:
            errors.append(f"{label}.privilege_boundary must be a non-empty array")
        for field in ("role", "source_class", "upstream", "rollback", "mirror_policy"):
            if not isinstance(item[field], str) or not item[field].strip():
                errors.append(f"{label}.{field} must be a non-empty string")
        validate_review(item, label, today, errors)

    internal_references = [
        item.get("contains")
        for item in internal_patterns
        if isinstance(item, dict) and isinstance(item.get("contains"), str)
    ]
    if len(internal_references) != len(internal_patterns):
        errors.append("internal_image_patterns entries require a string contains field")

    found_surfaces: dict[str, set[str]] = {
        immutable: set() for immutable in immutable_to_item
    }

    def classify(reference: str, surface: str) -> None:
        normalized = normalize_reference(reference)
        if any(
            normalized == internal or normalized.startswith(f"{internal}:")
            for internal in internal_references
        ):
            return
        item = immutable_to_item.get(normalized)
        if item is None:
            if "@sha256:" not in normalized:
                errors.append(
                    f"{surface}: third-party image is mutable or unreviewed: {normalized}"
                )
            else:
                errors.append(
                    f"{surface}: third-party image digest is absent from {MANIFEST}: "
                    f"{normalized}"
                )
            return
        found_surfaces[normalized].add(surface)

    for path in dockerfiles(root):
        relative = str(path.relative_to(root))
        text = path.read_text(encoding="utf-8")
        for reference in FROM.findall(text):
            classify(reference, relative)

    for path in compose_files(root):
        relative = str(path.relative_to(root))
        text = path.read_text(encoding="utf-8")
        for raw in COMPOSE_IMAGE.findall(text):
            classify(resolve_defaults(raw), relative)

    for immutable, expected in declared_surfaces.items():
        found = found_surfaces.get(immutable, set())
        if found != expected:
            errors.append(
                f"{MANIFEST}: {immutable} surfaces mismatch; "
                f"expected={sorted(expected)} found={sorted(found)}"
            )

    feed_urls: dict[str, dict[str, Any]] = {}
    for index, item in enumerate(feeds):
        label = f"package_feeds[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{label} must be an object")
            continue
        missing = sorted(REQUIRED_FEED_FIELDS - item.keys())
        if missing:
            errors.append(f"{label} missing fields: {', '.join(missing)}")
            continue
        url = item["url"]
        if not isinstance(url, str) or not url.startswith("https://"):
            errors.append(f"{label}.url must be an HTTPS URL")
            continue
        if url in feed_urls:
            errors.append(f"{label}.url is duplicated")
        feed_urls[url] = item
        surface = root / str(item["surface"])
        surface_text = ""
        if not surface.is_file():
            errors.append(f"{label}.surface does not exist: {item['surface']}")
        else:
            surface_text = surface.read_text(encoding="utf-8")
            if url not in surface_text:
                errors.append(
                    f"{label}.surface does not contain the reviewed feed URL"
                )
        fingerprint = str(item["key_fingerprint"])
        if not re.fullmatch(r"[0-9A-F]{40,64}", fingerprint):
            errors.append(f"{label}.key_fingerprint must be uppercase hex")
        for field in (
            "role",
            "verification",
            "version_policy",
            "sbom_evidence",
            "rollback",
            "exception_owner",
        ):
            if not isinstance(item[field], str) or not item[field].strip():
                errors.append(f"{label}.{field} must be a non-empty string")
        if surface_text and fingerprint not in surface_text:
            errors.append(f"{label}.surface does not verify the reviewed fingerprint")
        validate_review(item, label, today, errors)
        expires = parse_date(
            item["exception_expires"],
            f"{label}.exception_expires",
            errors,
        )
        if expires is not None and expires < today:
            errors.append(f"{label} exception is expired")

    for path in dockerfiles(root):
        relative = str(path.relative_to(root))
        for line_number, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(),
            start=1,
        ):
            if "deb " not in line and "nodesource.com" not in line:
                continue
            for url in URL.findall(line):
                if not any(url.startswith(reviewed) for reviewed in feed_urls):
                    errors.append(
                        f"{relative}:{line_number}: external package feed is "
                        f"unreviewed: {url}"
                    )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", default=".")
    args = parser.parse_args()

    errors = verify(Path(args.root))
    if errors:
        print("Third-party dependency verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Third-party dependency verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

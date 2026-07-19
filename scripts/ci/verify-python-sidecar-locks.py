#!/usr/bin/env python3
"""Verify reproducible, hash-locked Python sidecar dependency graphs."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


MANIFEST = Path("docker/python-sidecar-locks.json")
REGENERATOR = Path("scripts/lock-python-sidecars.sh")
VERIFIER = Path("scripts/ci/verify-python-sidecar-locks.py")
CI_WORKFLOW = Path(".gitea/workflows/ci-builder.yaml")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
DIRECT = re.compile(
    r"^([A-Za-z0-9_.-]+)(?:\[[A-Za-z0-9_,.-]+\])?={2,3}([A-Za-z0-9_.+!-]+)$"
)
LOCKED = re.compile(r"^([A-Za-z0-9_.-]+)={2,3}([^\s;\\]+)(?:\s*;.*)?\s*\\?$")
HASH = re.compile(r"^\s*--hash=sha256:([0-9a-f]{64})\s*\\?$")
URL = re.compile(r"https://[A-Za-z0-9._~:/?#\[\]@!$&'()*+,;=%-]+")


def normalize(name: str) -> str:
    return re.sub(r"[-_.]+", "-", name).lower()


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path: Path, errors: list[str]) -> dict[str, Any] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        errors.append(f"{path}: cannot read structured manifest: {error}")
        return None
    if not isinstance(value, dict):
        errors.append(f"{path}: root must be an object")
        return None
    return value


def parse_direct_requirements(
    path: Path,
    errors: list[str],
) -> dict[str, str]:
    packages: dict[str, str] = {}
    for line_number, raw in enumerate(
        path.read_text(encoding="utf-8").splitlines(),
        start=1,
    ):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = DIRECT.fullmatch(line)
        if match is None:
            errors.append(
                f"{path}:{line_number}: direct requirement must use an exact == or === version"
            )
            continue
        name = normalize(match.group(1))
        if name in packages:
            errors.append(f"{path}:{line_number}: duplicate direct requirement {name}")
        packages[name] = match.group(2)
    if not packages:
        errors.append(f"{path}: no direct requirements found")
    return packages


def parse_lock(path: Path, errors: list[str]) -> dict[str, tuple[str, set[str]]]:
    packages: dict[str, tuple[str, set[str]]] = {}
    current: str | None = None
    for line_number, raw in enumerate(
        path.read_text(encoding="utf-8").splitlines(),
        start=1,
    ):
        match = LOCKED.fullmatch(raw)
        if match is not None:
            name = normalize(match.group(1))
            if name in packages:
                errors.append(f"{path}:{line_number}: duplicate locked package {name}")
            packages[name] = (match.group(2), set())
            current = name
            continue
        hash_match = HASH.fullmatch(raw)
        if hash_match is not None:
            if current is None:
                errors.append(f"{path}:{line_number}: hash has no package entry")
            else:
                packages[current][1].add(hash_match.group(1))
            continue
        stripped = raw.strip()
        if stripped.startswith("--hash="):
            errors.append(f"{path}:{line_number}: only SHA-256 hashes are allowed")
            continue
        if raw.startswith("--") and not raw.startswith(
            ("--index-url ", "--extra-index-url ")
        ):
            errors.append(f"{path}:{line_number}: unsupported lock option {stripped!r}")
            continue
        if (
            stripped
            and not raw.startswith((" ", "#", "--"))
            and ("==" in stripped or any(token in stripped for token in (">=", "<=", "~=", " @ ")))
        ):
            errors.append(f"{path}:{line_number}: malformed or unpinned lock entry")
            current = None
    if not packages:
        errors.append(f"{path}: no locked packages found")
    for name, (_, hashes) in packages.items():
        if not hashes:
            errors.append(f"{path}: {name} has no SHA-256 hashes")
    return packages


def parse_date(value: Any, label: str, errors: list[str]) -> dt.date | None:
    try:
        return dt.date.fromisoformat(str(value))
    except ValueError:
        errors.append(f"{label} must be an ISO date")
        return None


def require_text(text: str, needle: str, label: str, errors: list[str]) -> None:
    if needle not in text:
        errors.append(f"{label}: missing {needle!r}")


def verify(root: Path, *, today: dt.date | None = None) -> list[str]:
    root = root.resolve()
    today = today or dt.date.today()
    errors: list[str] = []
    manifest = read_json(root / MANIFEST, errors)
    if manifest is None:
        return errors

    if manifest.get("schema_version") != 1:
        errors.append(f"{MANIFEST}: schema_version must be 1")
    if manifest.get("policy_issue") != 1076:
        errors.append(f"{MANIFEST}: policy_issue must be 1076")

    resolver = manifest.get("resolver")
    if not isinstance(resolver, dict):
        errors.append(f"{MANIFEST}: resolver must be an object")
        return errors
    expected_resolver = {
        "name": "uv",
        "version": "0.9.26",
        "python_version": "3.12",
        "exclude_newer": "2026-07-19T00:00:00Z",
        "source_builds": "forbidden",
        "regeneration_command": str(REGENERATOR),
    }
    for field, expected in expected_resolver.items():
        if resolver.get(field) != expected:
            errors.append(f"resolver.{field} must be {expected!r}")

    feeds = manifest.get("feeds")
    if not isinstance(feeds, list) or not feeds:
        errors.append(f"{MANIFEST}: feeds must be a non-empty array")
        return errors
    feed_urls: set[str] = set()
    for index, feed in enumerate(feeds):
        label = f"feeds[{index}]"
        if not isinstance(feed, dict):
            errors.append(f"{label} must be an object")
            continue
        url = feed.get("url")
        if not isinstance(url, str) or not url.startswith("https://"):
            errors.append(f"{label}.url must be an HTTPS URL")
        elif url in feed_urls:
            errors.append(f"{label}.url is duplicated")
        else:
            feed_urls.add(url)
        for field in ("id", "role", "verification"):
            if not isinstance(feed.get(field), str) or not feed[field].strip():
                errors.append(f"{label}.{field} must be a non-empty string")
        if "--require-hashes" not in str(feed.get("verification", "")):
            errors.append(f"{label}.verification must require pip hashes")

    locks = manifest.get("locks")
    if not isinstance(locks, list) or len(locks) != 2:
        errors.append(f"{MANIFEST}: locks must contain GLiNER and pyannote")
        return errors

    seen_services: set[str] = set()
    observed_urls: set[str] = set()
    for index, item in enumerate(locks):
        label = f"locks[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{label} must be an object")
            continue
        service = item.get("service")
        if service not in {"gliner", "pyannote"}:
            errors.append(f"{label}.service must be gliner or pyannote")
            continue
        if service in seen_services:
            errors.append(f"{label}.service is duplicated: {service}")
        seen_services.add(service)

        paths: dict[str, Path] = {}
        for field in ("requirements", "lock", "dockerfile", "workflow"):
            value = item.get(field)
            if not isinstance(value, str) or not value:
                errors.append(f"{label}.{field} must be a non-empty path")
                continue
            path = root / value
            paths[field] = path
            if not path.is_file():
                errors.append(f"{label}.{field} does not exist: {value}")
        if set(paths) != {"requirements", "lock", "dockerfile", "workflow"}:
            continue

        for field, path_field in (
            ("requirements_sha256", "requirements"),
            ("lock_sha256", "lock"),
        ):
            expected = item.get(field)
            if not isinstance(expected, str) or not SHA256.fullmatch(expected):
                errors.append(f"{label}.{field} must be a SHA-256 digest")
            elif file_sha256(paths[path_field]) != expected:
                errors.append(f"{label}.{field} does not match {item[path_field]}")

        direct = parse_direct_requirements(paths["requirements"], errors)
        locked = parse_lock(paths["lock"], errors)
        for name, version in direct.items():
            locked_entry = locked.get(name)
            if locked_entry is None:
                errors.append(f"{item['lock']}: missing direct package {name}")
            elif locked_entry[0].split("+", 1)[0] != version:
                errors.append(
                    f"{item['lock']}: {name} version {locked_entry[0]!r} "
                    f"does not match direct version {version!r}"
                )

        required = item.get("required_packages")
        if not isinstance(required, dict) or not required:
            errors.append(f"{label}.required_packages must be a non-empty object")
            required = {}
        for raw_name, expected_version in required.items():
            name = normalize(str(raw_name))
            if name not in locked:
                errors.append(f"{item['lock']}: missing required package {name}")
            elif locked[name][0] != expected_version:
                errors.append(
                    f"{item['lock']}: {name} must be {expected_version}, "
                    f"found {locked[name][0]}"
                )

        runtime = item.get("runtime_packages")
        if not isinstance(runtime, dict) or set(runtime) != set(required):
            errors.append(
                f"{label}.runtime_packages must match required package names"
            )
            runtime = {}

        lock_text = paths["lock"].read_text(encoding="utf-8")
        dockerfile = paths["dockerfile"].read_text(encoding="utf-8")
        workflow = paths["workflow"].read_text(encoding="utf-8")
        require_text(lock_text, str(REGENERATOR), item["lock"], errors)
        require_text(
            lock_text,
            f"--index-url https://download.pytorch.org/whl/{item.get('pytorch_backend')}",
            item["lock"],
            errors,
        )
        require_text(
            lock_text,
            "--extra-index-url https://pypi.org/simple",
            item["lock"],
            errors,
        )
        require_text(dockerfile, "COPY requirements.lock .", item["dockerfile"], errors)
        require_text(dockerfile, "--require-hashes", item["dockerfile"], errors)
        require_text(dockerfile, "--only-binary=:all:", item["dockerfile"], errors)
        require_text(dockerfile, "-r requirements.lock", item["dockerfile"], errors)
        for version in runtime.values():
            require_text(dockerfile, str(version), item["dockerfile"], errors)
            require_text(workflow, str(version), item["workflow"], errors)

        platforms = item.get("platforms")
        if service == "gliner":
            if item.get("accelerator") != "cpu":
                errors.append("GLiNER accelerator must be cpu")
            if item.get("pytorch_backend") != "cpu":
                errors.append("GLiNER PyTorch backend must be cpu")
            if platforms != ["linux/amd64", "linux/arm64"]:
                errors.append("GLiNER platforms must be linux/amd64 and linux/arm64")
            require_text(
                workflow,
                "--platform linux/amd64,linux/arm64",
                item["workflow"],
                errors,
            )
            forbidden = sorted(
                name
                for name in locked
                if name.startswith("nvidia-") or name in {"cuda-toolkit", "triton"}
            )
            if forbidden:
                errors.append(
                    f"{item['lock']}: CPU lock contains accelerator packages: "
                    f"{', '.join(forbidden)}"
                )
            if "cu13" in lock_text.lower():
                errors.append(f"{item['lock']}: CPU lock contains CUDA 13 content")
        else:
            if item.get("accelerator") != "cuda":
                errors.append("pyannote accelerator must be cuda")
            if item.get("cuda_version") != "12.6":
                errors.append("pyannote CUDA version must be 12.6")
            if item.get("pytorch_backend") != "cu126":
                errors.append("pyannote PyTorch backend must be cu126")
            if item.get("torchcodec_backend") != "cpu":
                errors.append("pyannote TorchCodec backend must be cpu")
            if platforms != ["linux/amd64"]:
                errors.append("pyannote platforms must contain only linux/amd64")
            if "--platform linux/amd64,linux/arm64" in workflow:
                errors.append(f"{item['workflow']}: pyannote must not publish arm64")
            torch = locked.get("torch", ("", set()))[0]
            torchaudio = locked.get("torchaudio", ("", set()))[0]
            if torch.split("+", 1)[0] != torchaudio.split("+", 1)[0]:
                errors.append(f"{item['lock']}: torch and torchaudio versions differ")
            if not any(name.startswith("nvidia-") and name.endswith("-cu12") for name in locked):
                errors.append(f"{item['lock']}: CUDA 12 runtime packages are missing")
            if "cu13" in lock_text.lower():
                errors.append(f"{item['lock']}: CUDA 13 content is forbidden")
            require_text(
                paths["requirements"].read_text(encoding="utf-8"),
                "torchcodec===0.11.1",
                item["requirements"],
                errors,
            )
            require_text(
                lock_text,
                "torchcodec===0.11.1",
                item["lock"],
                errors,
            )
            require_text(
                dockerfile,
                "AudioDecoder",
                item["dockerfile"],
                errors,
            )
            require_text(
                workflow,
                "get_all_samples()",
                item["workflow"],
                errors,
            )

        observed_urls.update(URL.findall(lock_text))
        observed_urls.update(URL.findall(dockerfile))

        reviewed = parse_date(item.get("reviewed_at"), f"{label}.reviewed_at", errors)
        cadence = item.get("update_cadence_days")
        if not isinstance(cadence, int) or cadence <= 0:
            errors.append(f"{label}.update_cadence_days must be a positive integer")
        elif reviewed is not None and reviewed + dt.timedelta(days=cadence) < today:
            errors.append(f"{label} review is stale")
        if not isinstance(item.get("rollback"), str) or not item["rollback"].strip():
            errors.append(f"{label}.rollback must be a non-empty string")

    if seen_services != {"gliner", "pyannote"}:
        errors.append(f"{MANIFEST}: both GLiNER and pyannote locks are required")
    unknown_urls = observed_urls - feed_urls
    if unknown_urls:
        errors.append(
            f"{MANIFEST}: unreviewed package feed URLs: {', '.join(sorted(unknown_urls))}"
        )
    unused_urls = feed_urls - observed_urls
    if unused_urls:
        errors.append(
            f"{MANIFEST}: unused package feed URLs: {', '.join(sorted(unused_urls))}"
        )

    regenerator_path = root / REGENERATOR
    if not regenerator_path.is_file():
        errors.append(f"{REGENERATOR}: missing lock regeneration script")
    else:
        regenerator = regenerator_path.read_text(encoding="utf-8")
        for needle in (
            'EXPECTED_UV_VERSION="0.9.26"',
            'PYTHON_VERSION="3.12"',
            'EXCLUDE_NEWER="2026-07-19T00:00:00Z"',
            "--generate-hashes",
            "--only-binary :all:",
            "--index-strategy first-index",
            "--torch-backend",
            "sed -E -i",
            "cmp -s",
        ):
            require_text(regenerator, needle, str(REGENERATOR), errors)

    ci_path = root / CI_WORKFLOW
    if not ci_path.is_file():
        errors.append(f"{CI_WORKFLOW}: missing CI workflow")
    else:
        ci = ci_path.read_text(encoding="utf-8")
        require_text(ci, str(VERIFIER), str(CI_WORKFLOW), errors)
        require_text(
            ci,
            "tests/test_verify_python_sidecar_locks.py",
            str(CI_WORKFLOW),
            errors,
        )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", default=".")
    args = parser.parse_args()

    errors = verify(Path(args.root))
    if errors:
        print("Python sidecar lock verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Python sidecar lock verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

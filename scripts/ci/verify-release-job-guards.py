#!/usr/bin/env python3
"""Verify secret-bearing workflow jobs cannot run for pull_request events."""

from __future__ import annotations

import re
import sys
from pathlib import Path


WORKFLOW_GLOB = (".gitea/workflows/*.yml", ".gitea/workflows/*.yaml")
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
IF_RE = re.compile(r"^    if:\s*(.+?)\s*$")
NEEDS_RE = re.compile(r"^    needs:\s*(.+?)\s*$")
PUBLISH_JOBS = {
    "publish-release",
    "publish-github",
}
RELEASE_JOB_NEEDS = {
    "verify-ghcr-release": {
        "publish-github",
    },
    "finalize-releases": {
        "publish-release",
        "publish-github",
        "verify-ghcr-release",
    },
}
RETIRED_RELEASE_JOBS = {
    "create-release",
    "create-github-release",
}
RETIRED_DEV_PUBLISH_JOBS = {
    "publish-dev",
    "publish-github-dev",
}
RELEASE_ONLY_CONTAINER_WORKFLOWS = {
    "build-builder.yaml": ("v*", "build-builder", "refs/tags/v"),
    "build-gliner.yaml": (
        "sidecar-gliner-v*",
        "build-gliner",
        "refs/tags/sidecar-gliner-v",
    ),
    "build-pyannote.yaml": (
        "sidecar-pyannote-v*",
        "build-pyannote",
        "refs/tags/sidecar-pyannote-v",
    ),
}
FINALIZER_REQUIRED_FRAGMENTS = (
    "ci/vault-fetch.gitea-release.spec",
    'API="https://git.integrolabs.net/api/v1/repos/${GITHUB_REPOSITORY}"',
    "https://api.github.com/repos/Fortemi/fortemi/releases",
)
REQUIRED_PUBLISH_NEEDS = {
    "verify-release-ref",
    "test-container",
    "integration-test",
    "audit",
    "deny",
    "mcp-lockfile-sync",
    "mcp-server-tests",
    "knowledge-shard-matrix",
}


def workflow_files(args: list[str]) -> list[Path]:
    if args:
        return [Path(arg) for arg in args]

    files: list[Path] = []
    for pattern in WORKFLOW_GLOB:
        files.extend(Path(".").glob(pattern))
    return sorted(files)


def has_pull_request_trigger(text: str) -> bool:
    return any(re.match(r"^\s*pull_request\s*:", line) for line in text.splitlines())


def job_blocks(text: str) -> list[tuple[str, str]]:
    lines = text.splitlines()
    in_jobs = False
    current_name: str | None = None
    current: list[str] = []
    blocks: list[tuple[str, str]] = []

    for line in lines:
        if not in_jobs:
            if line == "jobs:":
                in_jobs = True
            continue

        match = JOB_RE.match(line)
        if match:
            if current_name is not None:
                blocks.append((current_name, "\n".join(current)))
            current_name = match.group(1)
            current = [line]
            continue

        if current_name is not None:
            current.append(line)

    if current_name is not None:
        blocks.append((current_name, "\n".join(current)))

    return blocks


def job_if_expression(block: str) -> str:
    for line in block.splitlines():
        match = IF_RE.match(line)
        if match:
            return match.group(1)
        if re.match(r"^    steps\s*:", line):
            break
    return ""


def job_needs(block: str) -> set[str]:
    for line in block.splitlines():
        match = NEEDS_RE.match(line)
        if not match:
            continue
        value = match.group(1).strip()
        if value.startswith("[") and value.endswith("]"):
            return {item.strip() for item in value[1:-1].split(",") if item.strip()}
        return {value}
    return set()


def secret_bearing(block: str) -> bool:
    return "${{ secrets." in block


def excludes_pull_request(expression: str) -> bool:
    normalized = expression.replace('"', "'")
    return any(
        guard in normalized
        for guard in (
            "github.event_name == 'push'",
            "github.event_name != 'pull_request'",
            "github.event_name == 'workflow_dispatch'",
            "startsWith(github.ref, 'refs/tags/",
        )
    )


def main() -> int:
    failures: list[str] = []

    for path in workflow_files(sys.argv[1:]):
        text = path.read_text()
        blocks = job_blocks(text)
        job_names = {job_name for job_name, _ in blocks}
        block_by_name = dict(blocks)
        trigger_text = text.split("\njobs:", maxsplit=1)[0]

        release_only = RELEASE_ONLY_CONTAINER_WORKFLOWS.get(path.name)
        if release_only:
            tag_glob, job_name, ref_prefix = release_only
            if re.search(r"^\s+branches:\s*", trigger_text, re.MULTILINE):
                failures.append(
                    f"{path} container publisher must not have a branch trigger"
                )
            if f"tags: ['{tag_glob}']" not in trigger_text:
                failures.append(
                    f"{path} is missing release tag trigger {tag_glob}"
                )
            expression = job_if_expression(block_by_name.get(job_name, ""))
            required_guard = f"startsWith(github.ref, 'refs/tags/{ref_prefix.removeprefix('refs/tags/')}')"
            if required_guard not in expression:
                failures.append(
                    f"{path}:{job_name} is missing release-ref guard: {required_guard}"
                )
            if path.name in {"build-gliner.yaml", "build-pyannote.yaml"}:
                for retired_fragment in ("SHORT_SHA", "-main"):
                    if retired_fragment in text:
                        failures.append(
                            f"{path} sidecar release still emits retired commit or branch tags: {retired_fragment}"
                        )

        retired_dev_jobs = sorted(RETIRED_DEV_PUBLISH_JOBS & job_names)
        if retired_dev_jobs:
            failures.append(
                f"{path} still defines retired non-release container publish jobs: "
                f"{', '.join(retired_dev_jobs)}"
            )
        if re.search(r"^\s+publish_dev:\s*$", trigger_text, re.MULTILINE):
            failures.append(f"{path} still defines the retired publish_dev input")

        if not has_pull_request_trigger(text):
            continue

        if PUBLISH_JOBS <= job_names:
            missing_release_jobs = sorted(RELEASE_JOB_NEEDS.keys() - job_names)
            if missing_release_jobs:
                failures.append(
                    f"{path} is missing required release jobs: {', '.join(missing_release_jobs)}"
                )
            retired_release_jobs = sorted(RETIRED_RELEASE_JOBS & job_names)
            if retired_release_jobs:
                failures.append(
                    f"{path} still defines retired duplicate release jobs: {', '.join(retired_release_jobs)}"
                )
            finalizer = block_by_name.get("finalize-releases", "")
            for fragment in FINALIZER_REQUIRED_FRAGMENTS:
                if fragment not in finalizer:
                    failures.append(
                        f"{path}:finalize-releases is missing required release operation: {fragment}"
                    )

        for job_name, block in blocks:
            if job_name in PUBLISH_JOBS:
                missing = sorted(REQUIRED_PUBLISH_NEEDS - job_needs(block))
                if missing:
                    failures.append(
                        f"{path}:{job_name} publish job is missing required release gates: {', '.join(missing)}"
                    )
                if job_name == "publish-release" and "--require-complete" not in block:
                    failures.append(
                        f"{path}:{job_name} is missing the complete Knowledge Shard claim gate"
                    )
                expression = job_if_expression(block)
                if "startsWith(github.ref, 'refs/tags/v')" not in expression:
                    failures.append(
                        f"{path}:{job_name} is missing the release-only tag guard"
                    )

            if job_name in RELEASE_JOB_NEEDS:
                needs = job_needs(block)
                required_needs = RELEASE_JOB_NEEDS[job_name]
                missing = sorted(required_needs - needs)
                if missing:
                    failures.append(
                        f"{path}:{job_name} release job is missing required publish dependencies: {', '.join(missing)}"
                    )
                expression = job_if_expression(block)
                for required in sorted(required_needs):
                    guard = f"needs.{required}.result == 'success'"
                    if guard not in expression:
                        failures.append(
                            f"{path}:{job_name} release job is missing success guard: {guard}"
                        )

            if not secret_bearing(block):
                continue

            expression = job_if_expression(block)
            if not expression:
                failures.append(f"{path}:{job_name} uses secrets but has no job-level if guard")
            elif not excludes_pull_request(expression):
                failures.append(
                    f"{path}:{job_name} uses secrets but guard does not clearly exclude pull_request: {expression}"
                )

    if failures:
        print("Secret-bearing workflow jobs must not be runnable from pull_request events.", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("release job guard check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

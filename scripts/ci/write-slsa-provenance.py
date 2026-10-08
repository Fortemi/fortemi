#!/usr/bin/env python3
"""Write a SLSA v1 provenance predicate for a Fortemi image built by Gitea Actions."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
REVISION_RE = re.compile(r"^[0-9a-f]{40}$")
BUILD_TYPE = (
    "https://git.integrolabs.net/Fortemi/fortemi/src/branch/main/"
    "docs/deployment/image-promotion.md#build-type-gitea-actions-docker-v1"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--family", required=True)
    parser.add_argument("--subject", required=True, help="repository@sha256:digest")
    parser.add_argument("--source-uri", required=True, help="https Git repository URL")
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--source-ref", required=True)
    parser.add_argument("--workflow-path", required=True)
    parser.add_argument("--event-name", required=True)
    parser.add_argument("--builder-id", required=True)
    parser.add_argument("--invocation-id", required=True)
    parser.add_argument("--started-on", required=True)
    parser.add_argument("--finished-on", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def build_predicate(args: argparse.Namespace) -> dict:
    repository, _, digest = args.subject.partition("@")
    if not repository or not DIGEST_RE.fullmatch(digest):
        raise ValueError(f"subject must be repository@sha256:<64 hex>: {args.subject}")
    if not REVISION_RE.fullmatch(args.source_revision):
        raise ValueError("source revision must be a full lowercase 40-character Git SHA")
    if not args.source_uri.startswith("https://"):
        raise ValueError("source URI must be an https Git repository URL")
    return {
        "buildDefinition": {
            "buildType": BUILD_TYPE,
            "externalParameters": {
                "workflow": {
                    "repository": args.source_uri,
                    "ref": args.source_ref,
                    "path": args.workflow_path,
                },
                "family": args.family,
                "repository": repository,
            },
            "internalParameters": {"event_name": args.event_name},
            "resolvedDependencies": [
                {
                    "uri": f"git+{args.source_uri}@{args.source_ref}",
                    "digest": {"gitCommit": args.source_revision},
                }
            ],
        },
        "runDetails": {
            "builder": {"id": args.builder_id},
            "metadata": {
                "invocationId": args.invocation_id,
                "startedOn": args.started_on,
                "finishedOn": args.finished_on,
            },
        },
    }


def main() -> int:
    args = parse_args()
    try:
        predicate = build_predicate(args)
    except ValueError as error:
        print(f"slsa provenance: {error}", file=sys.stderr)
        return 1
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(predicate, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

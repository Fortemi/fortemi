#!/usr/bin/env python3
"""Fail closed when the container release-evidence policy or CI wiring drifts."""

from __future__ import annotations

import datetime as dt
import json
import re
import sys
from pathlib import Path


POLICY = Path("docker/container-release-evidence-policy.json")
CAPTURE = "scripts/ci/capture-container-release-evidence.py"
EXPECTED_FAMILIES = {"api", "bundle", "gliner", "pyannote", "builder", "testdb"}
EXPECTED_REGISTRIES = {"git.integrolabs.net", "ghcr.io"}
EXPECTED_BUILD_ARGS = {"VERSION", "GIT_SHA", "BUILD_DATE"}
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
REVISION_RE = re.compile(r"^[0-9a-f]{40}$")


def main() -> int:
    failures: list[str] = []
    try:
        policy = json.loads(POLICY.read_text())
    except (OSError, json.JSONDecodeError) as error:
        print(f"container release evidence policy check failed: {error}", file=sys.stderr)
        return 1

    if policy.get("schema_version") != 1 or policy.get("policy_issue") != 888:
        failures.append("policy schema_version/policy_issue must be 1/888")
    if policy.get("source_control", {}).get("oidc_identity_available") is not False:
        failures.append("policy must not claim a Gitea workflow OIDC identity")
    if policy.get("source_control", {}).get("github_pat_destination_only") is not True:
        failures.append("policy must classify the GitHub PAT as destination-only")
    if policy.get("artifact", {}).get("retention_days") != 365:
        failures.append("container release evidence must retain for 365 days")
    build_inputs = policy.get("build_inputs", {})
    if set(build_inputs.get("allowed_public_build_args", [])) != EXPECTED_BUILD_ARGS:
        failures.append("public Docker build arguments must use the reviewed allowlist")
    if build_inputs.get("secrets_as_build_args_allowed") is not False:
        failures.append("policy must forbid secrets in Docker build arguments")

    controls = policy.get("controls", {})
    for name in ("digest", "sbom", "provenance", "signature"):
        control = controls.get(name, {})
        if control.get("status") not in {"implemented", "deferred"}:
            failures.append(f"{name}: status must be implemented or deferred")
        if not control.get("owner"):
            failures.append(f"{name}: owner is required")
        if control.get("status") == "deferred":
            if not control.get("reason"):
                failures.append(f"{name}: deferred control requires a reason")
            try:
                revisit = dt.date.fromisoformat(control["revisit_by"])
                if revisit <= dt.date.fromisoformat(policy["reviewed_at"]):
                    failures.append(f"{name}: revisit_by must be after reviewed_at")
            except (KeyError, TypeError, ValueError):
                failures.append(f"{name}: deferred control requires an ISO revisit_by date")

    licenses = policy.get("license_notices", {})
    if licenses.get("status") != "pending-gate" or licenses.get("owner_issue") != 901:
        failures.append("license notices must retain the explicit #901 pending gate")
    if controls.get("sbom", {}).get("license_gate_issue") != 901:
        failures.append("SBOM expectations must retain the #901 license relationship")

    samples = policy.get("verification_samples", [])
    if {sample.get("family") for sample in samples} != {"api", "bundle"}:
        failures.append("published API and bundle verification samples are required")
    for sample in samples:
        if not DIGEST_RE.fullmatch(sample.get("digest", "")):
            failures.append("verification sample digest must be an immutable sha256")
        if not REVISION_RE.fullmatch(sample.get("source_revision", "")):
            failures.append("verification sample source_revision must be a full Git SHA")
        if sample.get("tagged_reference", "").split("/", 1)[0] != "ghcr.io":
            failures.append("verification sample must identify its tested registry")
        if sample.get("digest") == "" or sample.get("alias") == "":
            failures.append("verification sample must bind an alias to its digest")

    families = policy.get("families", {})
    profiles = policy.get("publish_path_profiles", {})
    if set(profiles) != {"gitea-direct", "ghcr-from-gitea-pat"}:
        failures.append("the Gitea-direct and GHCR-from-Gitea-PAT profiles are required")
    for profile_id, profile in profiles.items():
        if profile.get("workflow_origin") != "gitea-actions":
            failures.append(f"{profile_id}: current workflow origin must be Gitea Actions")
        if profile.get("oidc_identity") is not False:
            failures.append(f"{profile_id}: current publish paths must not claim OIDC")
        if set(profile.get("controls", [])) != {"digest", "sbom", "provenance", "signature"}:
            failures.append(f"{profile_id}: all four independent controls are required")
    future_jetson = policy.get("future_family_defaults", {}).get("jetson-l4t", {})
    if future_jetson.get("exposure") != "public-optional-runtime":
        failures.append("future Jetson/L4T images require a public optional-runtime default")
    if future_jetson.get("owner_issue") != 683:
        failures.append("future Jetson/L4T policy must retain #683 ownership")

    if set(families) != EXPECTED_FAMILIES:
        failures.append(f"families must be exactly {sorted(EXPECTED_FAMILIES)}")
    for family_id, family in families.items():
        if not family.get("exposure"):
            failures.append(f"{family_id}: exposure is required")
        if not family.get("sbom_scope"):
            failures.append(f"{family_id}: SBOM scan scope is required")
        if set(family.get("registries", {})) != EXPECTED_REGISTRIES:
            failures.append(f"{family_id}: both Gitea and GHCR registry paths are required")
        for registry, registry_policy in family.get("registries", {}).items():
            if not registry_policy.get("platforms"):
                failures.append(f"{family_id}/{registry}: expected platforms are required")
            if registry_policy.get("publish_path_profile") not in profiles:
                failures.append(f"{family_id}/{registry}: known publish path profile is required")
        patterns = family.get("immutable_tag_patterns", [])
        if not patterns:
            failures.append(f"{family_id}: immutable tag patterns are required")
        for pattern in patterns:
            try:
                re.compile(pattern)
            except re.error as error:
                failures.append(f"{family_id}: invalid immutable tag pattern: {error}")
        for workflow_name in family.get("workflows", []):
            workflow = Path(workflow_name)
            try:
                text = workflow.read_text()
            except OSError as error:
                failures.append(f"{family_id}: cannot read {workflow}: {error}")
                continue
            if CAPTURE not in text or f"--family {family_id}" not in text:
                failures.append(f"{family_id}: {workflow} does not capture its release evidence")
            if "container-release-evidence-" not in text or "actions/upload-artifact@" not in text:
                failures.append(f"{family_id}: {workflow} does not upload its evidence artifact")
            if "retention-days: 365" not in text:
                failures.append(f"{family_id}: {workflow} does not retain evidence for 365 days")
            build_args = set(re.findall(r"--build-arg\\s+[\"']?([A-Z][A-Z0-9_]*)", text))
            unexpected_args = build_args - EXPECTED_BUILD_ARGS
            if unexpected_args:
                failures.append(
                    f"{family_id}: {workflow} uses unreviewed build args {sorted(unexpected_args)}"
                )

    if "--provenance=false" in Path("scripts/ci/promote-ghcr-images.sh").read_text():
        provenance = controls.get("provenance", {})
        if provenance.get("status") != "deferred" or "OIDC" not in provenance.get("reason", ""):
            failures.append("disabled promotion provenance requires an explicit OIDC deferment")

    if failures:
        print("container release evidence policy check failed", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("container release evidence policy check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

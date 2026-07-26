# CI Workflow Audit

**Generated**: 2026-07-26T00:59:18Z
**Repo**: `Fortemi/fortemi`
**Workflow files scanned**: 10

## Findings

### CRITICAL — Bare `:latest` tags

The scan found mutable `:latest` aliases in `build-builder.yaml` and
`ci-builder.yaml`. These are publication or convenience aliases, not
workflow-level execution images. The workflows retain digest-pinned runner
containers and record pins in `ci/digests.txt`.

Mutable aliases remain non-authoritative: deployment and verification evidence
must use an immutable digest. No `:latest` match contributed to server build
run 5726.

### CRITICAL — PR-triggered jobs reference secrets

`ci-builder.yaml` has a `pull_request` trigger and contains secret-using jobs,
but every such job is guarded at job level to exclude pull requests:

- `sync-github-source`: `github.event_name == 'push'`
- `publish-dev` and `publish-github-dev`: main-branch push only
- release publication jobs: version-tag refs only
- release creation jobs: successful version-tag publication only

The lint gate also runs `scripts/ci/verify-release-job-guards.py`. No direct
fork-PR secret exposure was confirmed.

### HIGH — Unpinned actions

No tag-, branch-, or `latest`-pinned third-party actions were found.

### HIGH — Unpinned container images

No unpinned workflow-level `container:` or `image:` references were found.

### HIGH — `curl|sh` without hash check

No `curl | sh` installer pattern was found.

### MEDIUM — Pin manifest coverage

`ci/digests.txt` exists and records the pinned actions and workflow containers.

### INFO — Local reusable workflows

No local reusable workflow references were found.

### INFO — Server build failure

Push run 5726 failed during Debian package metadata retrieval while building
the standalone server image. Full build-suite dispatch run 5976 passed without
a source change, confirming a transient repository/proxy failure. The server
Dockerfile now applies bounded, fail-closed retries to both verified APT
transactions.

## Clean Checks

- All 10 Gitea workflow files were inventoried.
- Third-party actions are pinned to full commit SHAs.
- Workflow-level containers are digest pinned.
- No curl-to-shell installers were found.
- Secret-using jobs are guarded against pull-request execution.
- `ci/digests.txt` is present.

## Remediation Plan

1. Keep immutable digest references authoritative; treat `:latest` only as a
   convenience publication alias.
2. Keep the release-job guard verifier in the blocking lint suite.
3. Monitor the next push-triggered server build through Docker and isolated
   container verification.

## Follow-up Issues

No new issue is required for this build remediation.

## References

- `ci/digests.txt`
- `scripts/ci/verify-release-job-guards.py`
- Gitea Actions run 5726
- Gitea Actions run 5976

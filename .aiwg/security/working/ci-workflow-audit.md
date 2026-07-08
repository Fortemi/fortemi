# CI Workflow Audit

**Generated**: 2026-06-22T05:28:58Z
**Repo**: `/home/roctinam/dev/fortemi`
**Workflow files scanned**: 8

## Findings

### CRITICAL - Bare `:latest` tags

The workflows intentionally publish mutable `:latest` and `bundle-latest` tags for release images. That may remain a product policy decision, but the release audit should require immutable digest references in user-facing install docs and should record that mutable tags are not verification evidence.

- `.gitea/workflows/build-builder.yaml:47` - `-t ${GHCR_IMAGE}:latest \`
- `.gitea/workflows/build-builder.yaml:49` - `-t ${GITEA_IMAGE}:latest \`
- `.gitea/workflows/build-builder.yaml:57` - `docker tag ${GHCR_IMAGE}:latest ${GHCR_IMAGE}:${VERSION}`
- `.gitea/workflows/build-builder.yaml:58` - `docker tag ${GITEA_IMAGE}:latest ${GITEA_IMAGE}:${VERSION}`
- `.gitea/workflows/build-builder.yaml:70` - `-t ${GHCR_IMAGE}:latest \`
- `.gitea/workflows/build-builder.yaml:72` - `-t ${GITEA_IMAGE}:latest \`
- `.gitea/workflows/build-builder.yaml:80` - `docker tag ${GHCR_IMAGE}:latest ${GHCR_IMAGE}:${VERSION}`
- `.gitea/workflows/build-builder.yaml:81` - `docker tag ${GITEA_IMAGE}:latest ${GITEA_IMAGE}:${VERSION}`
- `.gitea/workflows/build-builder.yaml:91` - `docker run --rm ${IMAGE}:latest rustc --version`
- `.gitea/workflows/build-builder.yaml:181` - `push_with_retry ${GHCR_BUILDER}:latest`
- `.gitea/workflows/build-builder.yaml:183` - `push_with_retry ${GHCR_TESTDB}:latest`
- `.gitea/workflows/build-builder.yaml:191` - `push_with_retry ${GITEA_BUILDER}:latest`
- `.gitea/workflows/build-builder.yaml:193` - `push_with_retry ${GITEA_TESTDB}:latest`
- `.gitea/workflows/ci-builder.yaml:631` - `docker tag ${IMAGE}:${VERSION} ${IMAGE}:latest`
- `.gitea/workflows/ci-builder.yaml:634` - `push_with_retry ${IMAGE}:latest`
- `.gitea/workflows/ci-builder.yaml:871` - `docker tag ${IMAGE}:${VERSION} ${IMAGE}:latest`
- `.gitea/workflows/ci-builder.yaml:874` - `push_with_retry ${IMAGE}:latest`

July 2026 checkpoint update: user-facing Docker verification docs now show how to resolve `ghcr.io/fortemi/fortemi@sha256:...` references from versioned tags. The generated GitHub release-note template includes the same `docker image inspect ... RepoDigests` commands. Mutable tags remain convenience aliases, not verification evidence.

### CRITICAL - PR-triggered jobs reference secrets

No direct critical exposure was confirmed. The workflows that run on `pull_request` (`test.yml`, `docsite-build.yml`, `ci-builder.yaml`) do not have obvious secret-using publish steps that are intended to run on pull requests. The mixed `ci-builder.yaml` workflow does contain secret-using jobs, but those jobs are push/tag-gated.

July 2026 checkpoint update: `scripts/ci/verify-release-job-guards.py` now runs in the `ci-builder.yaml` lint job. It fails if a workflow that has a `pull_request` trigger contains a `${{ secrets.* }}` job without a job-level `if:` guard that clearly excludes pull-request execution.

Residual risk: the verifier is a static guard check, not a full Gitea evaluator. Keep the explicit job guards in the workflows and add live CI evidence when the next release/tag workflow runs.

### HIGH - Unpinned actions

Resolved in the July 2026 checkpoint pinning slice. `actions/checkout`, `actions/upload-artifact`, and `actions/download-artifact` are pinned to full SHAs and recorded in `ci/digests.txt`.

- `actions/upload-artifact@v3` -> `ff15f0306b3f739f7b6fd43fb5d26cd321bd4de5`
- `actions/download-artifact@v3` -> `9bc31d5ccc31df68ecc42ccf4149144866c47d8a`

### HIGH - Unpinned container images

No unpinned workflow-level `container:` or `image:` references were found by the audit pattern. `docsite-deploy.yml` uses a digest-pinned `node:20` container and records it in `ci/digests.txt`.

### HIGH - `curl|sh` without hash check

No `curl | sh` installer pattern was found.

### MEDIUM - Pin manifest coverage

`ci/digests.txt` exists and records the currently pinned `node:20` container, `redis:7-alpine` service image, `actions/checkout`, `actions/upload-artifact`, and `actions/download-artifact`.

## Clean Checks

- Workflow inventory scanned 8 Gitea workflow files.
- No workflow-level unpinned container image references found.
- No `curl | sh` installer pattern found.
- No direct fork-PR secret exposure confirmed in the inspected workflows.
- `ci/digests.txt` exists.

## Remediation Plan

1. Runner isolation mode, token permissions, and package-publish PAT posture are now documented in `build/RUNNER_SETUP.md` and `docs/content/ci-cd.md`.
2. Collect live CI evidence from the next guarded pull-request run and the next tag/manual release run using `.aiwg/security/working/ci-live-evidence-runbook-2026-07.md` to confirm the static guard matches Gitea runtime behavior.
3. Keep package-registry readiness separate from CI hardening. A publish-and-consume verification run is still required before private package distribution readiness is claimed.

## Follow-up Issues

- `ci(actions): pin artifact actions and add release-job secret guard lint`
- Cross-link release artifact issues (#641, #643, #886) to the CI hardening owner.

## References

- GitHub Actions secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- Gitea Actions token permissions: https://docs.gitea.com/usage/actions/token-permissions
- Gitea Actions comparison / package authorization notes: https://docs.gitea.com/usage/actions/comparison

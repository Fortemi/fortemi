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

### CRITICAL - PR-triggered jobs reference secrets

No direct critical exposure was confirmed. The workflows that run on `pull_request` (`test.yml`, `docsite-build.yml`, `ci-builder.yaml`) do not have obvious secret-using publish steps that are intended to run on pull requests. The mixed `ci-builder.yaml` workflow does contain secret-using jobs, but those jobs are push/tag-gated.

Residual risk: comments in `ci-builder.yaml` document prior Gitea job-gating misbehavior on tag/main conditions. Release job guard coverage should be explicit before relying on mixed workflow gating for secrets.

### HIGH - Unpinned actions

`actions/checkout` is pinned to a full SHA and recorded in `ci/digests.txt`. Artifact actions are still tag-pinned and are already listed as deferred in the manifest.

- `.gitea/workflows/publish-sidecar.yml:78` - `uses: actions/upload-artifact@v3`
- `.gitea/workflows/publish-sidecar.yml:243` - `uses: actions/upload-artifact@v3`
- `.gitea/workflows/publish-sidecar.yml:249` - `uses: actions/upload-artifact@v3`
- `.gitea/workflows/publish-sidecar.yml:262` - `uses: actions/download-artifact@v3`
- `.gitea/workflows/publish-sidecar.yml:370` - `uses: actions/download-artifact@v3`
- `.gitea/workflows/ci-builder.yaml:440` - `uses: actions/upload-artifact@v3`
- `.gitea/workflows/ci-builder.yaml:446` - `uses: actions/upload-artifact@v3`

### HIGH - Unpinned container images

No unpinned workflow-level `container:` or `image:` references were found by the audit pattern. `docsite-deploy.yml` uses a digest-pinned `node:20` container and records it in `ci/digests.txt`.

### HIGH - `curl|sh` without hash check

No `curl | sh` installer pattern was found.

### MEDIUM - Pin manifest coverage

`ci/digests.txt` exists and records the currently pinned `node:20` container and `actions/checkout` action. It also explicitly lists the unpinned artifact actions as a deferred hardening pass.

## Clean Checks

- Workflow inventory scanned 8 Gitea workflow files.
- No workflow-level unpinned container image references found.
- No `curl | sh` installer pattern found.
- No direct fork-PR secret exposure confirmed in the inspected workflows.
- `ci/digests.txt` exists.

## Remediation Plan

1. Pin `actions/upload-artifact@v3` and `actions/download-artifact@v3` to full commit SHAs and update `ci/digests.txt`.
2. Add explicit release-job guard tests or a CI lint that proves secret-using jobs cannot run for `pull_request` events.
3. Keep mutable `:latest` release tags only as convenience aliases; publish and document immutable digest verification for Docker users.
4. For Gitea self-hosted runners, document runner isolation mode, token permissions, and whether package-publish jobs rely on PATs because Gitea package authorization is incomplete.
5. Re-run this audit after the pinning and guard changes land.

## Follow-up Issues

- `ci(actions): pin artifact actions and add release-job secret guard lint`
- Cross-link release artifact issues (#641, #643, #886) to the CI hardening owner.

## References

- GitHub Actions secure use reference: https://docs.github.com/en/actions/reference/security/secure-use
- Gitea Actions token permissions: https://docs.gitea.com/usage/actions/token-permissions
- Gitea Actions comparison / package authorization notes: https://docs.gitea.com/usage/actions/comparison

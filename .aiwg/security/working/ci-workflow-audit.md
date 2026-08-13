# CI/CD Publication Audit

**Generated**: 2026-08-13T18:48:40Z
**Repo**: `Fortemi/fortemi`
**Workflow files scanned**: 11
**Release checked**: `v2026.7.19` (`5ea08229c9f1565122df5f8e6906e89d98dc7e75`)
**Current main checked**: `af06e74c6f08ac112e2bf77266ce4c9697bdca6b`

## Outcome

The current publication chain is healthy at both supported boundaries:

- Stable: Gitea Actions run `6531` completed release image publication,
  GHCR promotion, Gitea release creation, and GitHub release creation for
  `v2026.7.19`. Sidecar run `6533` completed the versioned native artifacts.
- Rolling main: run `6595` completed internal dev publication and GHCR dev
  promotion for `af06e74`; run `6597` deployed the matching documentation.
- GitHub Releases and Gitea Releases both expose the same four native binaries,
  `SHA256SUMS.txt`, and `sidecar-provenance.intoto.json` for `v2026.7.19`.
- GHCR stable aliases resolve to the versioned manifests, and rolling aliases
  resolve to images built from current main.

The remaining user-facing defect is outside this repository: `fortemi.com`
routes its Download links to an in-page install section and does not link the
GitHub release authority. `Fortemi/fortemi.com#37` fixes the links and adds a
regression gate; binary assets remain on GitHub Releases rather than being
copied onto the website host.

## Live Publication Evidence

### GitHub and Gitea releases

Latest stable release: `v2026.7.19`.

Required assets present in both release systems:

- `matric-api-x86_64-unknown-linux-gnu`
- `matric-api-aarch64-unknown-linux-gnu`
- `matric-api-x86_64-apple-darwin`
- `matric-api-aarch64-apple-darwin`
- `SHA256SUMS.txt`
- `sidecar-provenance.intoto.json`

Public authority: `https://github.com/Fortemi/fortemi/releases/latest`.

### GHCR

Stable aliases match their versioned subjects:

| Reference | Manifest digest |
|---|---|
| `ghcr.io/fortemi/fortemi:2026.7.19` | `sha256:9f063e10e6c08d414223ed7bfc0d9a38b1d82aeb95fa989cc71bb9a595906bf8` |
| `ghcr.io/fortemi/fortemi:latest` | `sha256:9f063e10e6c08d414223ed7bfc0d9a38b1d82aeb95fa989cc71bb9a595906bf8` |
| `ghcr.io/fortemi/fortemi:bundle-2026.7.19` | `sha256:9df4f96bc11ada7ca7c54deaf387f6af263bc30f36349d3b88d09e4ddda24092` |
| `ghcr.io/fortemi/fortemi:bundle-latest` | `sha256:9df4f96bc11ada7ca7c54deaf387f6af263bc30f36349d3b88d09e4ddda24092` |

Rolling references are current for `af06e74`:

| Reference | Manifest digest |
|---|---|
| `ghcr.io/fortemi/fortemi:main` | `sha256:9419f600bbee85ffa37033d33f0c0c85245d9ef077885e76776531c1f26b98ea` |
| `ghcr.io/fortemi/fortemi:bundle-main` | `sha256:3c800259182f435a7486ee3a12f3b25b61126ae165b1c6d48c16460c7fc9b04f` |

`gliner-latest` and `pyannote-latest` also resolve successfully. Those image
families publish independently when their build inputs change; they are not
expected to receive every Fortemi server release version.

## Workflow Security Findings

### CRITICAL - Bare `:latest` execution images

No workflow execution container uses an unpinned `:latest` image. Matches in
`build-builder.yaml` and `ci-builder.yaml` are intentional publication aliases.
They are not verification subjects; immutable tags and registry-read digests
remain authoritative.

### CRITICAL - Pull-request jobs reference secrets

`ci-builder.yaml` accepts pull requests and contains secret-using publish jobs,
but the jobs carry explicit push/tag guards. The blocking
`scripts/ci/verify-release-job-guards.py` check passes. No fork-PR secret
exposure was confirmed.

### HIGH - Unpinned actions

No tag-, branch-, or `latest`-pinned third-party actions were found. Checkout,
upload-artifact, and download-artifact references use full commit SHAs.

### HIGH - Unpinned workflow containers

No unpinned workflow-level `container:` or `image:` reference was found.
Node workflow containers are digest pinned.

### HIGH - `curl | sh` without verification

No `curl | sh` installer pattern was found in workflow files.

### MEDIUM - Pin manifest

`ci/digests.txt` exists and records the workflow action and container pins.

## Policy Checks

The following blocking repository checks pass on current main:

- `scripts/ci/verify-release-job-guards.py`
- `scripts/ci/verify-release-asset-mirror.py`
- `scripts/ci/verify-container-release-evidence.py`
- `scripts/ci/verify-sidecar-publication.py`

## Residual Risks

- Image SBOM, authenticated provenance, and signatures remain explicitly
  deferred in `docker/container-release-evidence-policy.json`; registry-derived
  digest receipts are the implemented trust control. Revisit date: 2026-10-15.
- Native binary provenance is published, but signing remains open under Gitea
  issue `#916`.
- Release success is proven for the latest stable release and current main, not
  for every historical tag. `v2026.7.19` is the corrective public release after
  the July publication sequence.
- `fortemi.com` link correctness must be enforced in the website repository so
  a marketing-site change cannot silently replace the GitHub release authority.
- The green `Fortemi/fortemi.com#37` CI run reports 11 npm advisories (1
  moderate, 10 high) during `npm ci`; the previously completed zero-advisory
  gate regressed, so `Fortemi/fortemi.com#30` was reopened with run `69`
  evidence. This is an existing website toolchain risk, not a release-link
  regression.

## Remediation Plan

1. Merge `Fortemi/fortemi.com#37`, which makes server download controls link to
   `https://github.com/Fortemi/fortemi/releases/latest` and HotM download
   controls link to `https://github.com/Fortemi/HotM/releases/latest`.
2. Keep its executable website check for those exact release authorities.
3. Keep `latest` aliases as convenience only and retain digest-qualified
   references in deployment and rollback records.
4. Complete native binary signing under `#916`; do not represent unauthenticated
   provenance as a signature.
5. Resolve the reopened `Fortemi/fortemi.com#30` npm audit findings and make the
   zero-advisory acceptance criterion executable in website CI.

## References

- `.gitea/workflows/ci-builder.yaml`
- `.gitea/workflows/publish-sidecar.yml`
- `scripts/ci/mirror-release-assets-to-github.sh`
- `docker/container-release-evidence-policy.json`
- `ci/digests.txt`
- Gitea Actions runs `6531`, `6533`, `6595`, and `6597`
- Gitea issues `#888`, `#887`, and `#916`

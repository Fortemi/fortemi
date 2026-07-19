# Container Release Evidence

Fortemi records registry-derived evidence after every image publication. The
receipt is a CI artifact, not an assertion copied from build output: the
publisher reads the raw OCI manifest back from each destination registry,
computes its `sha256` digest, verifies the expected platform set, and proves
that each mutable alias resolves to the same manifest.

The governing machine-readable policy is
`docker/container-release-evidence-policy.json`. CI validates that policy and
its workflow coverage with
`scripts/ci/verify-container-release-evidence.py`.

## Image Families and Publish Paths

| Family | Intended exposure | Gitea registry | GHCR | Expected platform |
|---|---|---|---|---|
| API | Public runtime | Direct build and push | Promotion from the exact Gitea image | `linux/amd64` |
| Bundle | Public runtime | Direct build and push | Gitea amd64 promotion plus arm64 build | Gitea: `linux/amd64`; GHCR: `linux/amd64`, `linux/arm64` |
| GLiNER | Public optional runtime | Direct multi-platform push | Direct multi-platform push | `linux/amd64`, `linux/arm64` |
| pyannote | Public optional runtime | Direct push | Direct push | `linux/amd64` |
| Builder | Public CI infrastructure, not an end-user runtime | Direct push | Direct push | `linux/amd64` |
| Test database | Public CI infrastructure, not an end-user runtime | Direct push | Direct push | `linux/amd64` |

Gitea is the source-control and workflow origin for every path. Registry
credentials are fetched from Vault. The GitHub PAT authorizes writes to GHCR;
it does not establish source provenance and must never be described as an OIDC
or workload identity.

The approved Docker build arguments (`VERSION`, `GIT_SHA`, and `BUILD_DATE`)
are public metadata. Registry credentials, the Hugging Face token, and all
other secrets remain runtime or login inputs and must not be passed as Docker
build arguments.

### GitHub/OIDC-capable path

No current Fortemi image is built or published by a GitHub-hosted workflow.
If that changes, a separate policy profile must bind the GitHub repository,
workflow reference, event/ref, and subject digest to the expected GitHub OIDC
claims. GitHub artifact attestations or keyless Sigstore signatures may be
enabled only on that path and only with verification that rejects every other
identity. Moving only the destination to GHCR, or using a GitHub PAT from
Gitea, does not activate this profile.

## Evidence Artifact

Each publishing job uploads an artifact named
`container-release-evidence-<path>-<run-id>` with 365-day retention. A receipt
contains:

- the full source Git revision and publication channel;
- the immutable publication tag used as the receipt subject;
- the registry-derived manifest digest and digest-qualified reference;
- the expected and observed platform set;
- every mutable alias and the digest it resolved to during publication;
- independent status values for digest, SBOM, provenance, signature, and
  license-notice controls.

Publication fails when the immutable tag is not policy-approved, a platform is
missing, an alias resolves to another digest, or a receipt cannot be captured.
Mutable aliases such as `main`, `latest`, and `bundle-latest` are convenience
references only. Deployment and rollback records must use the receipt's
`immutable_reference`.

## Control Status

| Control | Status | Meaning |
|---|---|---|
| Registry digest receipt | Implemented | Every registry path is read back and recorded after push. |
| SBOM | Deferred | No reviewed generator, attachment format, and retention lifecycle is deployed on the self-hosted runners. |
| Authenticated provenance | Deferred | Gitea Actions does not currently provide the reviewed OIDC identity required for authenticated SLSA provenance. |
| Image signature | Deferred | No approved managed signing key/KMS path or workflow OIDC identity is provisioned. |
| License and third-party notices | Pending gate | Issue `#901` owns the packaged license and notice set. |

Deferred controls are not represented by empty files or unauthenticated
claims. The policy assigns owners and a `2026-10-15` revisit date. A control
may become `implemented` only when its verification command and durable
artifact location are added to the policy and CI.

## Verify a Receipt

Download the evidence artifact for the publication job, then compare its
subject to the registry. Pull the digest-qualified subject, not its mutable
alias:

```bash
DIGEST="$(jq -r '.subject.digest' \
  container-release-evidence/ghcr-api-release.json)"
docker pull "ghcr.io/fortemi/fortemi@${DIGEST}"
```

For a Gitea-registry receipt, authenticate first and use the same receipt
field:

```bash
docker login git.integrolabs.net
DIGEST="$(jq -r '.subject.digest' \
  container-release-evidence/gitea-api-release.json)"
docker pull "git.integrolabs.net/fortemi/fortemi@${DIGEST}"
```

For a single-platform API release, independently recompute the recorded
manifest digest:

```bash
VERSION=2026.7.0
REF="ghcr.io/fortemi/fortemi:${VERSION}"
docker buildx imagetools inspect --raw "${REF}" | sha256sum
jq -r '.subject.digest, .subject.immutable_reference, .subject.platforms[]' \
  container-release-evidence/ghcr-api-release.json
```

For the multi-platform bundle, confirm both platforms as well as the index
digest:

```bash
VERSION=2026.7.0
REF="ghcr.io/fortemi/fortemi:bundle-${VERSION}"
docker buildx imagetools inspect "${REF}"
docker buildx imagetools inspect --raw "${REF}" | sha256sum
jq '.subject, .aliases' \
  container-release-evidence/ghcr-bundle-release.json
```

The digest printed by `sha256sum` must equal `.subject.digest` after adding the
`sha256:` prefix. Every `.aliases[].digest` must match it. A mismatch is a
release-integrity failure: stop rollout, preserve the receipt and workflow
logs, restore the prior digest-qualified deployment reference, and investigate
the registry tags before republishing.

The policy's `verification_samples` records the live checks performed against
release `2026.7.1`: the API/`latest` pair resolved to
`sha256:1b079d858104d114c043be1e089a727a3e69e5fad885c48d809296ae9da65b03`,
and the two-platform bundle/`bundle-latest` pair resolved to
`sha256:cfad953beeefb38313db35a4c44dbf0667776044c80eff5ada7091fa3e7c04c2`.

## Ownership Boundaries

Multi-platform image publication remains owned by issue `#623`. Future
Jetson/L4T build and runtime work remains owned by `#683`; those images inherit
this policy's public optional-runtime defaults when they are introduced.
License and third-party notice packaging remains the `#901` gate. This policy
records those dependencies without moving their implementation into the
release-evidence workflow.

## Rollback

Use the last accepted receipt for the affected family and registry. Deploy its
`immutable_reference`; do not retag an unverified local image. If the mutable
alias moved unexpectedly, leave it unchanged until the incident record
captures both the observed and expected digests. After remediation, rerun the
normal publication workflow so the replacement has a new durable receipt.

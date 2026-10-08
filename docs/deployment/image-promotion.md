# Image Signatures, SBOMs and Registry Promotion

Fortemi release images are signed by digest. Each signed digest also carries
an SPDX SBOM attestation and a SLSA v1 provenance attestation. This guide
covers verification, copying images into your own registry (ECR or another
private registry) by digest, and admission policy. Release CI evidence is
described in [Container Release Evidence](../content/container-release-evidence.md).

## What each release digest carries

The `sign-release-images` job in `.gitea/workflows/ci-builder.yaml` runs after
the Gitea and GHCR publication jobs. It reads the digests from the
registry-derived release receipts. It signs nothing by tag. For every
published digest, in both `git.integrolabs.net/fortemi/fortemi` and
`ghcr.io/fortemi/fortemi`, it produces:

| Evidence | Format | Attached to |
|---|---|---|
| Signature | cosign key-based signature, Sigstore bundle, OCI referrer | The digest. For multi-platform indexes, the index and every platform manifest (`--recursive`) |
| SBOM | SPDX 2.3 JSON from syft 1.52.0, one per platform manifest | Attestation type `spdxjson` on the platform manifest, and on the index for multi-platform images |
| Provenance | SLSA v1 predicate (`https://slsa.dev/provenance/v1`) | Attestation type `slsaprovenance1` on the index or manifest, and each platform manifest |

Before the release entries are finalized, the same job verifies every
signature and attestation against the committed public key. It also checks
that the provenance subject is the signed digest and that the provenance
`gitCommit` is the released source revision. The SBOM files, provenance
predicates and a per-digest receipt are uploaded as the
`container-supply-chain-evidence-<run-id>` CI artifact, retained for 365 days.

Image families come from the receipt list: the API image (also used by the
worker and migration Job), the MCP image `fortemi-mcp` and the bundle. Every
release tag is a `linux/amd64` + `linux/arm64` index, so the index and both
platform manifests are signed, and each platform manifest carries its own
SBOM. A new family is covered as soon as its publication job records a
receipt; the workflow needs no other change.

### Public GHCR packages

GHCR creates a new package as private. `ghcr.io/fortemi/fortemi` is public.
`ghcr.io/fortemi/fortemi-mcp` is created by the first release that publishes
it, and an organization owner must then set it to public once (GitHub →
Fortemi organization → Packages → `fortemi-mcp` → Package settings → Change
visibility → Public). Until then `verify-ghcr-release` fails its anonymous
pull and the release entries are not finalized; re-run that job after the
change.

### Trust model

- **Key-based, not keyless.** Gitea Actions on the self-hosted runners has no
  OIDC workload identity, so Sigstore keyless (Fulcio) is not used. The
  signing key is an OpenBao Transit `ecdsa-p256` key that cannot be exported.
  CI asks Transit to sign through a least-privilege AppRole. The private key
  never reaches the runner.
- **No public transparency log.** Signatures are not uploaded to the public
  Rekor instance, so internal registry names stay private. Verifiers must pass
  `--insecure-ignore-tlog=true`. The warning cosign prints is expected: trust
  rests on the published public key, not on log inclusion.
- **Provenance is authenticated by the release key.** No OIDC identity backs
  it. It records what the release workflow observed: repository, tag ref,
  commit, workflow path and run URL. It reaches SLSA Build L2-style
  authenticity only to the extent that the release key and the CI AppRole are
  protected.
- **Tooling.** CI pins cosign v3.1.3. Signatures use the Sigstore bundle format
  stored as OCI referrers. Use cosign 3.x to verify. Older clients and older
  admission controllers may not read bundle-format signatures.

## Public key

The release public key is committed at `docs/security/cosign.pub` in the
signed source tree. Read it from a checkout of a signed release tag, not from a
copy found elsewhere:

```bash
git clone https://git.integrolabs.net/Fortemi/fortemi.git
cd fortemi
git verify-tag v2026.10.0   # release tags are signed with the OpenBao release GPG key
cp docs/security/cosign.pub /tmp/fortemi-cosign.pub
```

> **Status:** provisioned 2026-10-08. The key is the OpenBao Transit key
> `transit/keys/fortemi-release-cosign` (`ecdsa-p256`, version 1,
> non-exportable, deletion disabled). The `ci-fortemi` AppRole carries the
> sign/read-only policy `ci-fortemi-cosign`, and the repository variable
> `COSIGN_KEY_REF` is `hashivault://fortemi-release-cosign`. Images built
> before the first release tag after that date are unsigned. Do not trust a key
> from any other source.

Internal verifiers that can reach OpenBao can also use the KMS reference:
`--key hashivault://fortemi-release-cosign`. This needs a token with `read` on
`transit/keys/fortemi-release-cosign`.

## Verify an image

Always resolve and pin the digest first. Never verify or deploy a mutable tag:

```bash
PUB=/tmp/fortemi-cosign.pub
IMAGE=ghcr.io/fortemi/fortemi
DIGEST="$(docker buildx imagetools inspect --raw "${IMAGE}:bundle-2026.10.0" | sha256sum | awk '{print "sha256:"$1}')"
REF="${IMAGE}@${DIGEST}"

# Signature
cosign verify --key "$PUB" --insecure-ignore-tlog=true "$REF"

# SBOM attestation(s): one SPDX document per platform
cosign verify-attestation --key "$PUB" --insecure-ignore-tlog=true \
  --type spdxjson "$REF" > sbom-attestations.jsonl

# SLSA v1 provenance
cosign verify-attestation --key "$PUB" --insecure-ignore-tlog=true \
  --type slsaprovenance1 "$REF" > provenance.jsonl
```

Decode verified payloads with `jq`:

```bash
# Source commit, ref and CI run recorded in the provenance
jq -r '.payload | @base64d | fromjson | .predicate
  | .buildDefinition.resolvedDependencies[0].digest.gitCommit,
    .buildDefinition.externalParameters.workflow.ref,
    .runDetails.metadata.invocationId' provenance.jsonl

# Package inventory from the SBOM
jq -r '.payload | @base64d | fromjson | .predicate.packages[]
  | [.name, .versionInfo] | @tsv' sbom-attestations.jsonl | sort -u | head
```

Check that the commit equals the release tag's commit
(`git rev-list -n1 v2026.10.0`). The provenance subject is checked by
`verify-attestation` against the digest you passed.

To fetch attestations without verifying them, for example for an SBOM
scanner, use `cosign download attestation`. The output is not trusted until it
passes `verify-attestation`:

```bash
cosign download attestation --predicate-type https://spdx.dev/Document "$REF" |
  jq '.dsseEnvelope.payload | @base64d | fromjson | .predicate' > fortemi.spdx.json
```

The CI artifact `container-supply-chain-evidence-<run-id>` holds the same SBOM
files, with their SHA-256 values in `receipts/*.json`.

## Promote to a customer registry

Copy by digest and include the referrers, so the signature and attestations
move with the image. Then verify again at the destination. `oras copy -r`
copies an image together with its referring artifacts:

```bash
PUB=/tmp/fortemi-cosign.pub
SRC=ghcr.io/fortemi/fortemi
DIGEST=sha256:<digest from the release receipt or the verification step>
DST=123456789012.dkr.ecr.us-east-1.amazonaws.com/fortemi/fortemi

# 1. Verify at the source
cosign verify --key "$PUB" --insecure-ignore-tlog=true "${SRC}@${DIGEST}"

# 2. Authenticate to the destination (ECR example)
aws ecr get-login-password --region us-east-1 |
  oras login --username AWS --password-stdin "${DST%%/*}"
aws ecr get-login-password --region us-east-1 |
  docker login --username AWS --password-stdin "${DST%%/*}"

# 3. Copy the exact digest plus signatures and attestations. For a
#    multi-platform index this copies both platform images as well.
oras copy -r "${SRC}@${DIGEST}" "${DST}:bundle-2026.10.0"

# 4. Re-verify at the destination by digest; the digest must not change
cosign verify --key "$PUB" --insecure-ignore-tlog=true "${DST}@${DIGEST}"
cosign verify-attestation --key "$PUB" --insecure-ignore-tlog=true --type spdxjson "${DST}@${DIGEST}" >/dev/null
cosign verify-attestation --key "$PUB" --insecure-ignore-tlog=true --type slsaprovenance1 "${DST}@${DIGEST}" >/dev/null
```

Notes:

- Registries without the OCI referrers API store signatures and attestations
  under the fallback tag `sha256-<digest-hex>`. Exempt those tags from
  lifecycle expiry. Each new referrer rewrites that index tag, so ECR tag
  immutability must allow it, for example through an exclusion filter for
  `sha256-*`.
- `crane copy "${SRC}@${DIGEST}" "${DST}:<tag>"` copies only the image. If you
  use it, copy the referrers separately (`oras copy -r` or
  `cosign save`/`cosign load`), then run step 4. A copy that fails step 4 is
  not promoted.
- cosign 3.x deprecates `cosign copy`, and it does not carry bundle-format
  signatures reliably. Do not use it for promotion.

### Countersign with your own key (optional)

Organizations that admit only their own signatures can countersign the same
digest after step 4, for example with AWS KMS:

```bash
cosign sign --key awskms:///alias/platform-image-signing "${DST}@${DIGEST}"
cosign attest --key awskms:///alias/platform-image-signing --type spdxjson \
  --predicate fortemi.spdx.json "${DST}@${DIGEST}"
```

Keep the Fortemi signature as well. Admission policy can require both: the
Fortemi key proves origin, your key proves internal review.

## Admission policy examples

Pin workloads by digest, `image: <registry>/fortemi/fortemi@sha256:...`.

### Kyverno

```yaml
apiVersion: kyverno.io/v1
kind: ClusterPolicy
metadata:
  name: fortemi-signed-images
spec:
  validationFailureAction: Enforce
  webhookTimeoutSeconds: 30
  rules:
    - name: verify-fortemi
      match:
        any:
          - resources:
              kinds: [Pod]
      verifyImages:
        - imageReferences:
            - "123456789012.dkr.ecr.us-east-1.amazonaws.com/fortemi/fortemi*"
          mutateDigest: true
          verifyDigest: true
          required: true
          attestors:
            - entries:
                - keys:
                    publicKeys: |-
                      -----BEGIN PUBLIC KEY-----
                      <contents of docs/security/cosign.pub>
                      -----END PUBLIC KEY-----
                    rekor:
                      ignoreTlog: true
                    ctlog:
                      ignoreSCT: true
          attestations:
            - type: https://slsa.dev/provenance/v1
              attestors:
                - entries:
                    - keys:
                        publicKeys: |-
                          -----BEGIN PUBLIC KEY-----
                          <contents of docs/security/cosign.pub>
                          -----END PUBLIC KEY-----
                        rekor:
                          ignoreTlog: true
                        ctlog:
                          ignoreSCT: true
              conditions:
                - all:
                    - key: "{{ buildDefinition.externalParameters.workflow.repository }}"
                      operator: Equals
                      value: https://git.integrolabs.net/Fortemi/fortemi
```

### Sigstore policy-controller

Use a `ClusterImagePolicy` whose `images[].glob` matches your mirrored
repository and whose authority is a `key` with the contents of
`docs/security/cosign.pub`. Configure the authority so it does not require
Rekor inclusion, because Fortemi does not upload to the public log, and add an
`attestations` entry for `slsaprovenance1`.

For either controller, confirm in staging that your installed version reads
Sigstore bundle-format (OCI referrer) signatures. The quickest check: deploy
one verified digest and confirm that an unsigned digest of the same repository
is rejected.

## Operator key provisioning

Release CI cannot sign until an operator completes these steps once. CI never
generates, stores or commits key material.

1. **Create the Transit key** (non-exportable, `ecdsa-p256`, deletion
   disabled):

   ```bash
   export VAULT_ADDR=https://rca-g2.s9.internal:8200
   export VAULT_CACERT=ci/trust/integro-labs-root-ca-g2.crt
   bao write -f transit/keys/fortemi-release-cosign type=ecdsa-p256 exportable=false allow_plaintext_backup=false
   bao write transit/keys/fortemi-release-cosign/config deletion_allowed=false
   ```

2. **Grant the CI AppRole sign and read only.** Attach
   `ci/vault-ci-fortemi-cosign.hcl` to the `ci-fortemi` AppRole policy set, or
   to a dedicated signing AppRole. It grants `update` on
   `transit/sign/fortemi-release-cosign/*` and `read` on
   `transit/keys/fortemi-release-cosign`, and nothing else.

3. **Publish the public key** through a reviewed commit:

   ```bash
   mkdir -p docs/security
   VAULT_TOKEN=<operator token> cosign public-key --key hashivault://fortemi-release-cosign > docs/security/cosign.pub
   ```

4. **Set the repository Actions variables** (variables, not secrets):

   | Name | Value |
   |---|---|
   | `COSIGN_KEY_REF` | `hashivault://fortemi-release-cosign` |
   | `COSIGN_TRANSIT_MOUNT` | `transit` (only if the mount differs) |
   | `COSIGN_VAULT_ADDR` | OpenBao HTTPS address whose certificate chains to `ci/trust/integro-labs-root-ca-g2.crt`. If unset, `VAULT_ADDR` is used. |

   The existing `VAULT_CI_ROLE_ID`/`VAULT_CI_SECRET_ID` secrets log in. The
   token is used only for signing and is revoked when the step exits.

   *Fallback (not recommended):* set `COSIGN_KEY_REF=env://COSIGN_PRIVATE_KEY`
   and add the secrets `COSIGN_PRIVATE_KEY`, holding an encrypted cosign key
   generated offline, and `COSIGN_PASSWORD`. This places private key material
   in CI.

5. **Prove it** with the next release tag. The `sign-release-images` log must
   end with `signed-attested-verified` for every digest. Then change the
   `sbom`, `provenance` and `signature` controls in
   `docker/container-release-evidence-policy.json` from `pending-activation`
   to `implemented`.

**Rotation:** create `fortemi-release-cosign-v2` and commit the new public key
alongside the old one. Switch `COSIGN_KEY_REF`, then keep the old key
published for as long as images signed with it are supported. Transit
`rotate` would change the public key under the same name and break
verification of older images, so use a new key name instead.

## Build type: gitea-actions-docker v1

The `buildType` URI in Fortemi provenance points here. It means: a Docker
image built by `docker build` from the repository at
`resolvedDependencies[0]`, on a self-hosted Gitea Actions runner, by the
workflow at `externalParameters.workflow.path` for the tag ref
`externalParameters.workflow.ref`. `externalParameters.family` names the image
family and `externalParameters.repository` the registry repository.
`runDetails.metadata.invocationId` is the Gitea Actions run URL.
`runDetails.builder.id` identifies the `matric-builder` runner pool.

## Local dry run

`scripts/ci/sign-container-images.sh` can be exercised end to end against a
throwaway local registry and a throwaway key that lives outside the
repository:

```bash
TMP="$(mktemp -d)"
scripts/ci/install-supply-chain-tools.sh "$TMP/bin"; export PATH="$TMP/bin:$PATH"
docker run -d --name reg -p 127.0.0.1:5055:5000 \
  registry@sha256:a3d8aaa63ed8681a604f1dea0aa03f100d5895b6a58ace528858a7b332415373
docker tag <any local image> localhost:5055/fortemi/fortemi:test
docker push localhost:5055/fortemi/fortemi:test
(cd "$TMP" && COSIGN_PASSWORD=throwaway cosign generate-key-pair)
D="$(docker buildx imagetools inspect --raw localhost:5055/fortemi/fortemi:test | sha256sum | cut -c1-64)"
echo "api localhost:5055/fortemi/fortemi@sha256:$D" > "$TMP/subjects"
SYFT_REGISTRY_INSECURE_USE_HTTP=true COSIGN_PASSWORD=throwaway \
COSIGN_KEY_REF="$TMP/cosign.key" COSIGN_PUBLIC_KEY="$TMP/cosign.pub" SIGNING_REQUIRED=1 \
SOURCE_REVISION="$(git rev-parse HEAD)" SOURCE_URI=https://git.integrolabs.net/Fortemi/fortemi \
SOURCE_REF=refs/tags/vTEST scripts/ci/sign-container-images.sh --subjects "$TMP/subjects" --out "$TMP/out"
docker rm -f reg; rm -rf "$TMP"
```

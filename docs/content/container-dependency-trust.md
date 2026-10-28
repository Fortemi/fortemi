# Third-Party Container Dependency Trust

Fortemi treats consumed third-party containers and image-build package feeds as
a separate trust domain from Fortemi-published images. Fortemi release
provenance, SBOMs, signatures, or registry digests do not attest to Redis,
Speaches, Ollama, llama.cpp, Docker Official Images, pgvector, or an
external package repository.

The machine-readable authority is
`docker/third-party-dependencies.json`. It records each reviewed source tag,
multi-architecture manifest digest, role, privilege boundary, source class,
surface, review date and cadence, rollback behavior, and mirror policy.
`scripts/ci/verify-third-party-dependencies.py` fails CI when a Compose or
Dockerfile input is absent from that record, lacks a digest, drifts across
surfaces, exceeds its review cadence, or introduces an unreviewed package feed.
Python packages installed into the GLiNER and pyannote sidecars have a separate
machine-readable authority at `docker/python-sidecar-locks.json`.
`scripts/ci/verify-python-sidecar-locks.py` rejects input or lock checksum
drift, open-ended requirements, unhashed distributions, unreviewed indexes,
unsupported platforms, and CPU/CUDA graph mismatches.

## Runtime Inventory

| Dependency | Role | Main privilege boundary |
|---|---|---|
| Redis 7 Alpine | Required bundle cache | Bundle network and cache volume |
| Speaches CPU | Optional CPU transcription | Bundle network and model cache |
| Speaches CUDA 12.6.3 | Optional accelerated transcription | GPU device, bundle network, and model cache |
| Ollama | Workstation/demo inference | GPU device, host model path, and published host port |
| llama.cpp server | Optional inference | Read-only model path, published host port, optional GPU |
| pgvector PostgreSQL 18 | Bundle/test runtime base | Database files and runtime package sources |
| Debian, Node, Python, Rust | Runtime/build bases | Files copied into release images or trusted build execution |

The manifest also classifies the Docker Debian package feed used only by the CI
builder. NodeSource is not a bundle or builder input: Node and npm are copied
from the reviewed, digest-pinned Docker Official Image instead of adding that
external repository.

## Disabled Autoheal

`ops-autoheal` is disabled pending a reviewed replacement tracked in the
project issue tracker. The package review of
`willfarrell/autoheal:1.2.0` found unpatched HIGH/CRITICAL advisories in its
installed packages on both amd64 and arm64. Its Alpine 3.13 base reached
[end of support on 2022-11-01](https://alpinelinux.org/releases/).
[Alpine's public 3.13 security database](https://secdb.alpinelinux.org/v3.13/main.json)
records fixes newer than the installed curl 7.77.0-r1
([CVE-2021-22945](https://curl.se/docs/CVE-2021-22945.html)), zlib 1.2.11-r3
([CVE-2022-37434](https://nvd.nist.gov/vuln/detail/CVE-2022-37434)), and apk-tools
2.12.5-r0 ([CVE-2021-36159](https://nvd.nist.gov/vuln/detail/CVE-2021-36159)).
The image review was not renewed.

The autoheal service and its image inventory entry are removed. The manifest
schema requires live surfaces and has no disabled-image representation. The
installer rejects `FORTEMI_AUTOHEAL_MODE=ops-autoheal` and `COMPOSE_PROFILES`
containing `ops-autoheal`; the exposure preflight rejects that profile, a
rendered autoheal service, and all Docker socket mounts. Keep
`FORTEMI_AUTOHEAL_MODE=disabled` and remove the profile from existing settings.
Use service restart policies with operator health monitoring.

Re-enablement requires a reviewed replacement, restoration of the
small service block from Git history and its inventory entry, and coordinated
updates to the installer, preflight, tests, and operator guidance. Docker socket
access remains root-equivalent host control even with a read-only mount.

## Builder Feed Review (2026-10-18)

This review is tracked in the project issue tracker and was checked
**2026-10-18** using public upstream sources. The proposed builder pins
are `docker-ce-cli=5:29.9.1-1~debian.12~bookworm` and
`docker-buildx-plugin=0.38.0-1~debian.12~bookworm`. Both are published in Docker's
Debian bookworm stable Packages indexes for
[amd64](https://download.docker.com/linux/debian/dists/bookworm/stable/binary-amd64/Packages.gz)
and [arm64](https://download.docker.com/linux/debian/dists/bookworm/stable/binary-arm64/Packages.gz).
Comparing the indexed versions with Debian version ordering found these to be
the newest versions of both packages on both architectures at review time.

### Advisory evidence

- **GHSA-gwr2-q96m-6682 / CVE-2026-93325 (HIGH):**
  [the advisory](https://github.com/docker/buildx/security/advisories/GHSA-gwr2-q96m-6682)
  describes Bake entitlement consent bypass with `--progress=rawjson`. Its
  structured fields still report `>=0.21.0` and no patched version, but its
  Patches section says v0.37.2 fixes the issue. Independently, the
  [v0.37.2 release notes](https://github.com/docker/buildx/releases/tag/v0.37.2)
  explicitly link this GHSA and record that raw JSON progress now enforces
  entitlement consent. The subsequent
  [v0.38.0 release](https://github.com/docker/buildx/releases/tag/v0.38.0)
  includes that fix; the incomplete structured range is not evidence that
  0.38.0 remains affected.
- **GHSA-p54p-jq4x-rc28 / CVE-2026-103433 (MEDIUM):**
  [the advisory](https://github.com/docker/buildx/security/advisories/GHSA-p54p-jq4x-rc28)
  lists affected versions through v0.37.1 and patched versions **v0.37.2+**.
  The v0.37.2 release notes also record the fixes for implicit secret paths and
  local OCI layout contexts. Therefore 0.38.0 includes this fix too.
- **Docker CLI 29.9.1 notice check:** reviewed all published advisories in
  [docker/cli](https://github.com/docker/cli/security/advisories) (2) and
  [moby/moby](https://github.com/moby/moby/security/advisories) (26), using their
  public repository advisory APIs with `per_page=100`. The CLI's HIGH
  [GHSA-p436-gjf2-799p](https://github.com/docker/cli/security/advisories/GHSA-p436-gjf2-799p)
  is Windows-only and fixed in 29.2.0; its other notice is fixed in 20.10.9.
  Moby's published version ranges and fixes do not identify an outstanding
  HIGH or CRITICAL affecting this CLI pin. For example, the latest HIGH
  [GHSA-7cfq-22r6-qp73](https://github.com/moby/moby/security/advisories/GHSA-7cfq-22r6-qp73)
  is fixed in Engine 29.8.2. The
  [Docker 29.9.1 release notes](https://docs.docker.com/engine/release-notes/29/#2991)
  were checked alongside earlier security fixes: the new security notices in
  29.9.1 concern containerd fixes, rather than an unresolved CLI notice. This
  builder installs the client; the Docker daemon runs on its host.
- **Buildx 0.38.0 notice check:** reviewed all 3 published
  [docker/buildx advisories](https://github.com/docker/buildx/security/advisories)
  and the 0.37.2/0.38.0 release notes. Besides the two fixes above,
  [GHSA-m4gq-fm9h-8q75](https://github.com/docker/buildx/security/advisories/GHSA-m4gq-fm9h-8q75)
  is MEDIUM and fixed in 0.21.3. No outstanding HIGH or CRITICAL affecting
  0.38.0 was identified in these notices.
- **OSV cross-check:** POSTs to the public
  [OSV query API](https://google.github.io/osv.dev/post-v1-query/) at
  `https://api.osv.dev/v1/query`, with ecosystem `Go`, package
  `github.com/docker/cli`, version `29.9.1`, and package
  `github.com/docker/buildx`, version `0.38.0`, each returned `{}` (no notices).
  A supplemental query for `github.com/docker/docker` at `29.9.1` returned
  GHSA-pxq6-2prw-chj9 and GO-2026-4883, GO-2026-4887, GO-2026-5617,
  GO-2026-5668, GO-2026-5746. Those legacy module records have open-ended
  ranges. Their corresponding upstream Engine advisories place fixes at
  29.3.1 (plugin privilege/AuthZ checks) or 29.5.1 (`docker cp`/archive
  extraction); they concern daemon code rather than this CLI package.
  Empty OSV results alone are not proof of absence: the upstream notices and
  explicit Buildx fix evidence above determine this review's conclusion.

This public notice review found no HIGH or CRITICAL affecting either proposed
pin. It does not constitute a binary or transitive-package scan of a rebuilt
image. The apt feed remains a digest exception, retaining HTTPS, the verified
signing-key fingerprint and exact package versions. Its review date is
2026-10-18 and its exception expires **2026-11-17**.

### Remaining builder exposures and qualification

- The published **fortemi-builder:latest still carries Buildx 0.35.0** until
  the next release tag publishes the rebuilt image via `build-builder.yaml`.
  That version is affected by both Bake advisories above. Per the operator's
  acceptance criteria, **titan-host-runner is inactive** and is the only
  runner mapping `matric-builder` to that image; no active runner uses it.
  Tracked in the project issue tracker.
- **Titan host Buildx v0.30.1** remains exposed to both Bake advisories. Its
  upgrade is owned by sysops and tracked in its issue tracker.
  This source-pin change does not upgrade the host binary.

The ci-builder Lint step runs `scripts/ci/check-buildx-policy.py` over every
file under `.gitea/`, `scripts/` and `build/`, excluding the check and its test.
It does not try to parse shell, YAML or Dockerfile syntax. It joins line
continuations and removes quote characters and backslashes. Because `buildx`
is used in this tree, any line containing the substring `bake` then fails, and
so does any line containing `rawjson`. Escaped spellings such as `bake\n`
still match. The rule applies across the whole tree, so a command assembled
from a variable defined in another file is caught.

Unrelated mentions are allowed only as exact lines listed in the check, each
with a reason. Deliberate obfuscation, such as splitting the word across
variables or decoding it at run time, is left to code review.

The pinned builder was qualified locally on 2026-10-18 without publishing it.
It was built with a resource-capped, throwaway BuildKit builder, and nothing
was pushed or tagged in a registry. Inside the image:

- the installed packages are `docker-ce-cli=5:29.9.1-1~debian.12~bookworm` and
  `docker-buildx-plugin=0.38.0-1~debian.12~bookworm`;
- `docker buildx version` reports v0.38.0;
- `scripts/ci/verify-third-party-dependencies.py` passes.

The local image ID was
`sha256:45173a34a77bf828b3648e4b6872239b4e0a85547df329ff59705d3466a56700`, and
its `containerimage.digest` was
`sha256:3a583a74d105eb2d4919b5c6108976b4a42b3e5bdbeec1dc352b6d39330967fb`. The
published image is rebuilt only by the next release tag. Autoheal's separate
criterion 7 package check is complete: HIGH/CRITICAL advisories were found,
the retired image inventory entry was removed, and the profile remains disabled
pending the reviewed replacement tracked in the project issue tracker.

## Python Sidecar Locks

GLiNER is a CPU-only image published for `linux/amd64` and `linux/arm64`. Its
lock selects the PyTorch CPU wheel channel and rejects NVIDIA, CUDA, and Triton
packages. pyannote is published for `linux/amd64`; one CUDA 12.6-capable image
serves both the GPU profiles and the edge CPU profile. Its coordinated
`torch`/`torchaudio` pair can execute without a GPU while retaining CUDA 12.6
support when the NVIDIA device is present. TorchCodec is pinned to the CPU wheel
from the release line compatible with the coordinated PyTorch version. A strict
identity specifier prevents pip from substituting a same-version CUDA wheel, so
built-in audio decoding does not silently load a different CUDA ABI.

Both Dockerfiles install only wheels from the reviewed lock with
`--require-hashes` and `--only-binary=:all:`. PyPI, the PyTorch CPU index, and
the PyTorch CUDA 12.6 index are the only package feeds. Exact versions plus
the lock's SHA-256 allowlist prevent another artifact or dependency from being
accepted merely because an index returns it first.

To update either graph, install exactly `uv 0.9.26`, edit the direct
`requirements.txt`, and regenerate both locks:

```bash
scripts/lock-python-sidecars.sh
python3 scripts/ci/verify-python-sidecar-locks.py
python3 -m unittest tests/test_verify_python_sidecar_locks.py
```

Review every version and feed change, update the checksums and review date in
`docker/python-sidecar-locks.json`, then build both sidecars. The regeneration
script resolves for Python 3.12, excludes releases newer than its recorded
cutoff, forbids source distributions, and proves the GLiNER graph is identical
for amd64 and arm64.

Rollback the direct requirement file and its generated lock as one unit from
the prior reviewed commit. Rebuild GLiNER on both architectures. For pyannote,
prove CPU-mode startup, TorchCodec audio decoding, and CUDA 12.6 imports before
republishing; never reuse a rolling sidecar tag as rollback evidence.

## Verify a Dependency

Always verify the complete `tag@sha256` reference from the manifest. A tag by
itself is a discovery alias, not trust evidence.

```bash
image='redis:7-alpine@sha256:6ab0b6e7381779332f97b8ca76193e45b0756f38d4c0dcda72dbb3c32061ab99'
docker pull "$image"
docker image inspect "$image" --format '{{json .RepoDigests}}'
```

For an index with multiple platforms, inspect the registry manifest before
rollout and confirm the index digest remains the reviewed value:

```bash
docker manifest inspect \
  'ghcr.io/ggml-org/llama.cpp:server@sha256:b832a7b7252a90a79a1e8d23d9be3ac5261a33224f60682dff0cade412fa55d3'
```

Do not replace a reviewed digest merely because the source tag moved. Review
the upstream change, supported architectures, vulnerability/SBOM evidence, and
runtime compatibility; update the manifest and every declared surface in one
change; then run:

```bash
python3 scripts/ci/verify-third-party-dependencies.py
```

Rollback restores the prior reviewed digest. Preserve database/model volumes
unless that dependency's manifest entry requires a compatibility migration.
Autoheal remains disabled pending a reviewed replacement; do not restore its
retired image.

## Customer and Air-Gapped Mirrors

Copy every platform required by the deployment and verify the resulting mirror
manifest. Registry copies are accepted only as complete `tag@sha256`
references; a mirror tag alone is not sufficient. If a registry rewrites the
manifest and produces a different digest, review and record that mirror digest
under the customer's change-control process.

Set the relevant override to the complete approved mirror reference:

```bash
FORTEMI_REDIS_IMAGE=registry.example/approved/redis:7-alpine@sha256:<APPROVED_MIRROR_DIGEST>
FORTEMI_SPEACHES_CPU_IMAGE=registry.example/approved/speaches:cpu@sha256:<APPROVED_MIRROR_DIGEST>
FORTEMI_SPEACHES_CUDA_IMAGE=registry.example/approved/speaches:cuda@sha256:<APPROVED_MIRROR_DIGEST>
FORTEMI_WORKSTATION_OLLAMA_IMAGE=registry.example/approved/ollama:workstation@sha256:<APPROVED_MIRROR_DIGEST>
FORTEMI_LLAMACPP_IMAGE=registry.example/approved/llama.cpp:server@sha256:<APPROVED_MIRROR_DIGEST>
```

The bundle exposure preflight rejects mutable overrides for bundle services.
Customer-specific mirror digests are deployment approvals and do not silently
replace the repository's reviewed defaults.

## Ownership

- #990 owns consumed third-party image/feed inventory, digest locks, review
  cadence, mirror guidance, and drift lint.
- #888 owns Fortemi-published image digest, SBOM, provenance, and signing
  evidence.
- #937 established the explicit Docker socket trust boundary; re-enablement of
  the disabled `ops-autoheal` service is tracked in the project issue tracker.
- #1075 owns removal of the retired llama.cpp registry namespace.
- #1076 owns exact, hash-locked Python package graphs, supported sidecar
  platforms, and the PyTorch CPU/CUDA compatibility boundary.

## Retained image review 2026-10-28

The eleven image entries last reviewed on September 24 were reviewed again against
their recorded origin, privilege boundary, live source surfaces, mirror and rollback
policy. Their registry manifests were fetched read-only from Docker Hub or GHCR;
every raw SHA-256 matched its existing immutable pin. The public manifests and
value-free receipt are retained in `docs/evidence/container-dependency-review-20261028/`.
No layers were downloaded or executed, and no mutable tags or running services
were changed. The Keycloak entry and package-feed review dates retain their own
existing reviews.

The disposition retains these exact revisions for their current declared roles.
This renews the inventory review; it does not claim current vulnerability freedom,
upstream signatures, fresh runtime acceptance, or suitability for a broader privilege
boundary. Existing dependency audits, runtime package freshness, release validation,
and signed publication remain required. Review cadence and failure policy remain
unchanged. Any version/digest update requires its own compatibility validation.

## Retained Python sidecar review 2026-10-28

The two sidecar lock records retain their exact requirements and lock SHA-256
identities, platforms, CPU/CUDA package sources and rollback procedures. The three
recorded package-feed endpoints returned HTTPS200 to read-only HEAD requests.
The source guards continue to require hashes, binary distributions and the declared
backend/platform split. Public receipt: `docs/evidence/python-sidecar-review-20261028/`.
No wheel, model or runtime was downloaded or executed, and no lock was regenerated.
This is a trust-inventory renewal, not a vulnerability scan or new platform/runtime
qualification. Existing sidecar verification and release gates remain required;
the 30-day review cadence is unchanged.

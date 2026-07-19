# Third-Party Container Dependency Trust

Fortemi treats consumed third-party containers and image-build package feeds as
a separate trust domain from Fortemi-published images. Fortemi release
provenance, SBOMs, signatures, or registry digests do not attest to Redis,
Speaches, Ollama, llama.cpp, autoheal, Docker Official Images, pgvector, or an
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
| autoheal 1.2.0 | Optional operations helper | Root-equivalent Docker daemon control; default off |
| Ollama | Workstation/demo inference | GPU device, host model path, and published host port |
| llama.cpp server | Optional inference | Read-only model path, published host port, optional GPU |
| pgvector PostgreSQL 18 | Bundle/test runtime base | Database files and runtime package sources |
| Debian, Node, Python, Rust | Runtime/build bases | Files copied into release images or trusted build execution |

The manifest also classifies the Docker Debian package feed used only by the CI
builder. NodeSource is not a bundle or builder input: Node and npm are copied
from the reviewed, digest-pinned Docker Official Image instead of adding that
external repository.

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
For autoheal, disable `ops-autoheal` before investigating or rolling back.

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
FORTEMI_AUTOHEAL_IMAGE=registry.example/approved/autoheal:1.2.0@sha256:<APPROVED_MIRROR_DIGEST>
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
- #937 owns whether the Docker-socket-bearing autoheal service can run and
  keeps it behind explicit `ops-autoheal`.
- #1075 owns removal of the retired llama.cpp registry namespace.
- #1076 owns exact, hash-locked Python package graphs, supported sidecar
  platforms, and the PyTorch CPU/CUDA compatibility boundary.

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

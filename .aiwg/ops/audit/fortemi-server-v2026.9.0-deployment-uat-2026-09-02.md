---
session_id: fortemi-server-v2026.9.0-deployment-uat-2026-09-02
started_at: 2026-09-02T00:55:17-04:00
completed_at: 2026-09-02T01:18:56-04:00
status: complete
host: titan
operator: roctinam
service: fortemi-server
release: v2026.9.0
release_commit: e4fd622adc5b6759d69483d56eefeae7b0c462ca
release_image: ghcr.io/fortemi/fortemi:bundle-2026.9.0
image_index_digest: sha256:1c6298790ac30dbddfb9160289dcdd68f6302e8186cd30b6ea62ac73786f1231
test_correction_commits:
  - 356238f2a0b52692bac7807486ab20eb2f6da345
  - b46c127e1b78b0f53f14c4849db23d0973a0622b
---

# Fortemi Server v2026.9.0 Deployment and MCP UAT Audit

## Outcome

The signed `v2026.9.0` release was verified, deployed twice under the CPU-safe
host-stability profile, and removed completely after validation. The first
canonical run exposed three stale UAT tag expectations. GitHub #67 corrected
them without changing runtime code. GitHub #68 then added missing live evidence
for the `manual-note-link-v1` route. The final full run passed all executed
assertions: 31 files, 559 declared tests, 558 passed, 0 failed, 1 intentional
skip, 0 missing, and cleanup 3/3.

## Publication verification

| Check | Result | Evidence |
|---|---|---|
| Signed version tag and release commit | `PASS` | `v2026.9.0` peels to `e4fd622a` |
| Required GitHub assets | `PASS` | Four native binaries, `SHA256SUMS.txt`, provenance |
| Required Gitea assets | `PASS` | Same six named assets |
| Checksums | `PASS` | `sha256sum -c` passed for both downloaded sets |
| Cross-registry byte parity | `PASS` | Every required asset was byte-identical |
| SLSA provenance | `PASS` | Four exact subjects, release commit and workflow run 50677 |
| GHCR API image | `PASS` | `sha256:1244ac93ddc0316599e7de6be7979fb61376a40f587398f38dd0f0f9c11d1f80` |
| GHCR bundle image | `PASS` | `sha256:1c6298790ac30dbddfb9160289dcdd68f6302e8186cd30b6ea62ac73786f1231` |
| Required publication item missing | `MISSING: none` | Release config's six assets were present in both releases |
| Standalone binary `--version` smoke | `N/A` | Binary has no CLI version mode; startup correctly fails closed without explicit attachment-scan policy |

GitHub release: <https://github.com/Fortemi/fortemi/releases/tag/v2026.9.0>.
The release-only publication pipeline completed in Gitea run 50582; no routine
branch build in this UAT published a container.

## Deployment controls

- Project: `fortemi-uat-2026-9-0`
- Network: isolated `10.202.0.0/24`
- Containers: exact release bundle plus pinned Redis only
- Compose controls: `--no-build --pull always --no-deps`
- Host bindings: loopback ports 3000 and 3001
- Restart policy: `no`
- GPU: device reservations reset, `NVIDIA_VISIBLE_DEVICES=void`, CPU Open3D
- Authentication: generated mode-0600 database secret and OAuth client;
  process-local bearer with `mcp read write`
- Optional services: autoheal, Whisper, GLiNER, Pyannote, Ollama, and other
  support sidecars not started

Docker inspection matched release version `2026.9.0`, revision `e4fd622a`, local
image ID `sha256:ff920a21d41795c5119de014ab302299b8f38c6c54b604d066a5405ed9019225`,
and the published bundle digest.

## UAT trace

1. Initial exact-tag suite: 558 declared, 554 passed, 3 failed, 1 skipped, 0
   missing, cleanup 3/3. Failures were stale special-character tag expectations.
2. GitHub #67 / signed `356238f2`: focused tag 11/11 and edge 30/30; full suite
   558 declared, 557 passed, 0 failed, 1 skipped, 0 missing.
3. GitHub #68 / signed `b46c127e`: focused links 11/11; final full suite 559
   declared, 558 passed, 0 failed, 1 skipped, 0 missing, cleanup 3/3.

The final links phase observed `manual-note-link-v1` initial creation, exact
replay with stable persisted identity and timestamp, and outgoing graph
visibility. This establishes live community persistence for GitHub #61. Hosted
authorization remains bounded to the producer hosted integration tests and
OpenAPI metadata; the local bundle was not represented as hosted tenant
evidence.

## Health and crash-risk evidence

API liveness/readiness and MCP health passed before and after both runs. The
final Fortemi and Redis containers were healthy with zero restarts, no OOM kill,
restart policy `no`, and no GPU device requests. Final logs had zero matches for
panic, `handler_panicked`, fatal, traceback, out-of-memory, OOM kill, or the
semantic-chunk UTF-8 panic signatures associated with GitHub #56.

The GPU remained healthy and outside the deployment: final evidence recorded an
RTX 4090 at 35 C, about 34 W, 1552 MiB in use by the host desktop/other host
workloads, and 9 percent utilization. UAT made no Docker GPU request.

## Cleanup and runner state

Both UAT teardown operations removed exactly two containers, five volumes, and
the isolated network. Temporary environment, OAuth response, and credential
material were deleted. Verification found zero UAT containers, networks,
volumes, or listeners on ports 3000/3001.

The temporary release runner `fortemi-release-v2026.9.0` was stopped, deleted
from Gitea, and its exact `/tmp` directory removed after release-tag workflows
completed. The permanent `titan-host-runner` remained offline and
`gitea-runner-host.service` remained disabled/inactive. A residual release
BuildKit helper was stopped without deleting its cache. Any later Fortemi-named
containers were CI job containers for the delivered commits, not deployed
services or support sidecars.

## Boundary

The supported-platform run 50694 passed Linux x86_64, native Linux arm64, and
native macOS arm64 for its pinned Knowledge Shard `full-v1` matrix. That matrix
does not validate the live manual-link route and does not prove current shared
schema or broad suite portability. The AIWG static index, Knowledge Shard state
transfer, and Fortemi persistence planes remain separate, and the suite audit
remains `NO-GO`.

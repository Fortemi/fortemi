# Fortemi v2026.9.0 Live MCP UAT Result

## Disposition

`PASS` for the exact published `v2026.9.0` runtime when exercised with the
delivered UAT corrections from GitHub #67 and #68. The final canonical run
declared 559 tests across 31 files, executed 558 passing assertions, reported 0
failures, retained 1 intentional branding skip, found 0 missing files, and
completed cleanup 3/3.

The original tag-aligned suite did not pass: it declared 558 tests, passed 554,
failed 3 stale tag assertions, retained 1 intentional skip, found 0 missing
files, and completed cleanup 3/3. That failure is preserved below rather than
being rewritten as a release pass.

## Target identity

| Field | Value |
|---|---|
| Release | `v2026.9.0` |
| Release commit | `e4fd622adc5b6759d69483d56eefeae7b0c462ca` |
| Release image | `ghcr.io/fortemi/fortemi:bundle-2026.9.0` |
| OCI index digest | `sha256:1c6298790ac30dbddfb9160289dcdd68f6302e8186cd30b6ea62ac73786f1231` |
| Linux amd64 manifest | `sha256:98bd91453448c5483adadaa951ff6906a9172f452168f45cd9df462a9c94ce26` |
| Local image ID | `sha256:ff920a21d41795c5119de014ab302299b8f38c6c54b604d066a5405ed9019225` |
| API | `http://127.0.0.1:3000` |
| MCP | `http://127.0.0.1:3001` |
| First deployment start | `2026-09-02T04:55:28.265104059Z` |
| Final deployment start | `2026-09-02T05:11:25.454219867Z` |
| Final evidence collected | `2026-09-02T01:18:42-04:00` |

The image labels reported version `2026.9.0` and revision
`e4fd622adc5b6759d69483d56eefeae7b0c462ca`, matching the signed release tag.

## Run history and issue correction

| Run | Test source | Declared | Passed | Failed | Skipped | Missing | Result |
|---|---|---:|---:|---:|---:|---:|---|
| Initial exact-tag suite | `e4fd622a` | 558 | 554 | 3 | 1 | 0 | `FAIL` |
| Canonical-tag correction | `356238f2` | 558 | 557 | 0 | 1 | 0 | `PASS` |
| Manual-link live coverage | `b46c127e` | 559 | 558 | 0 | 1 | 0 | `PASS` |

The initial failures were `TAG-007`, `EDGE-006`, and `EDGE-026`. Each expected
dots, colons, or punctuation-only components to be accepted even though the
released canonical grammar permits one to five non-empty `/`-separated Unicode
alphanumeric, dash, or underscore components. The API correctly returned the
stable generic validation problem. GitHub #67 tracked the test defect.

Signed commit `356238f2a0b52692bac7807486ab20eb2f6da345` retains positive Unicode,
dash, and underscore coverage and converts the three stale cases to explicit
rejection checks. The checks also verify that neither the submitted tag nor a
note-content sentinel appears in the MCP error. Focused results were 11/11 for
tag operations and 30/30 for edge cases.

The canonical MCP links phase did not exercise the newly released HTTP mutation.
GitHub #68 tracked that evidence gap. Signed commit
`b46c127e1b78b0f53f14c4849db23d0973a0622b` adds a live release test without
changing the release image or moving the tag. The focused links phase passed
11/11, including the new contract check.

## Manual-note-link-v1 live evidence

The final run created two isolated notes through MCP, then called
`POST /api/v1/notes/{source}/links` with `kind: explicit` and score `0.75` using
the same process-local OAuth bearer:

- The first request returned `201`, `created: true`, correct source-to-target
  direction, kind, score, persisted UUID, and timestamp.
- The identical replay returned `200`, `created: false`, and the same persisted
  UUID and timestamp.
- `get_note_links` observed the persisted explicit outgoing edge.
- Existing cleanup removed both notes and the cascading link.

This is live community/archive-scoped persistence and idempotency evidence for
GitHub #61. It is not hosted-mode tenant evidence. Hosted write scope,
target-visibility, and transaction-scoped tenant enforcement are established by
the producer's hosted integration tests and the typed OpenAPI authorization
metadata. HotM's immutable OpenAPI consumer gate and Linux/macOS desktop builds
provide the consumer-side evidence.

## Release and consumer receipts

- Release build, security, contract, container, GPU, and publication gate:
  [Gitea run 50582](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/50582)
- Exact-tag comprehensive test matrix:
  [Gitea run 50670](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/50670)
- Native sidecar publication:
  [Gitea run 50677](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/50677)
- Supported-platform aggregate:
  [Gitea run 50694](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/50694)
- Release-tag documentation deployment:
  [Gitea run 50724](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/50724)
- HotM Linux/macOS desktop matrix:
  [Gitea run 50471](https://git.integrolabs.net/Fortemi/HotM/actions/runs/50471)
- HotM pinned OpenAPI consumer gate:
  [Gitea run 50472](https://git.integrolabs.net/Fortemi/HotM/actions/runs/50472)

The supported-platform run validates the separately pinned Knowledge Shard
`full-v1` matrix. It is not evidence for the live manual-link mutation and does
not establish broader suite parity.

## Safety and post-run health

Both deployments used `.aiwg/working/compose.host-stability.yml`. They removed
GPU reservations, set `NVIDIA_VISIBLE_DEVICES=void`, selected CPU Open3D, used
restart policy `no`, and started exactly Fortemi and Redis with dependency
traversal disabled. No local image build, container publication, optional
sidecar start, or registry mutation occurred. The explicit `10.202.0.0/24`
subnet did not overlap an existing Docker network.

Before teardown of the final deployment:

- API `/livez` returned `live`, `/readyz` returned `ready`, and MCP `/health`
  returned HTTP transport status `ok`.
- Fortemi and Redis were healthy with zero restarts and no OOM kill.
- Fortemi had no Docker GPU device request.
- Logs contained zero panic, `handler_panicked`, fatal, traceback,
  out-of-memory, OOM-kill, or semantic-chunk panic matches.
- The RTX 4090 remained healthy at 35 C and low utilization during evidence
  collection.

Teardown removed both containers, all five fresh UAT volumes, the isolated
network, generated environment/credential material, and listeners on ports
3000 and 3001. No deployed Fortemi service or optional support sidecar remained.

## Secret handling

Each fresh bundle generated its MCP OAuth client inside the data volume with a
mode-0600 credential file. The UAT shell removed outer quoting, acquired a
bearer token through `client_secret_basic` with `mcp read write` scope, kept all
credential and bearer values process-local, and deleted each temporary token
response and environment directory. No credential, bearer, Authorization
header, Hugging Face token, or submitted redaction sentinel is included in this
result or the committed evidence.

## Bounded limitations

Ollama and optional Whisper, GLiNER, and Pyannote services were intentionally not
started. This run does not validate generation, embedding inference, vision
inference, transcription, entity extraction, or speaker diarization behavior.

This result validates the live Fortemi persistence plane and MCP/HTTP surface
for the exact published release image plus the delivered test-only corrections.
It does not treat the AIWG static index, Knowledge Shard state-transfer formats,
and Fortemi persistence schemas as a shared schema. It makes no `core-v1`,
`full-v1`, or `record-v1` compatibility claim and no unqualified parity,
complete-backup, or portability claim while the suite audit remains `NO-GO`.

# Fortemi v2026.8.3 Live MCP UAT Result

## Disposition

`PASS` for the signed and published `v2026.8.3` release. The canonical
31-file MCP suite declared 558 tests, executed 557 passing assertions, reported
0 failures, retained 1 intentional branding skip, found 0 missing files, and
completed cleanup 3/3.

## Target identity

| Field | Value |
|---|---|
| Release | `v2026.8.3` |
| Release commit | `842f6a6b1d9835f4f3f60cc9f30372c4008bf779` |
| Release image | `ghcr.io/fortemi/fortemi:bundle-2026.8.3` |
| OCI index digest | `sha256:da34c4aac922912c99880bd0a6cfbe1cfcb4d77612c5fedb06232e998ba32d0a` |
| Local image ID | `sha256:937adecc8b390705cc8823c3d9f0c902d4d0a2a87fbca85253f4e54c0f0844f6` |
| API | `http://127.0.0.1:3000` |
| MCP | `http://127.0.0.1:3001` |
| Deployment start | `2026-08-31T20:33:32.983030623Z` |
| Evidence collected | `2026-08-31T16:41:37-04:00` |

The image's OCI labels reported version `2026.8.3` and revision
`842f6a6b1d9835f4f3f60cc9f30372c4008bf779`, matching the signed release
commit.

## Safety profile

The run used `.aiwg/working/compose.host-stability.yml` as a temporary UAT
override. It removed all Docker GPU device reservations, set
`NVIDIA_VISIBLE_DEVICES=void`, selected CPU Open3D behavior, and set every UAT
service restart policy to `no`. Docker inspection confirmed the running Fortemi
container had no device request, zero restarts, no OOM kill, and healthy state.

The first Compose invocation exposed that the bundle's dependency graph would
pull the optional GLiNER image even when only Redis and Fortemi were named. The
invocation was terminated before it created any container or volume. The fresh
project was then started with dependency traversal disabled. Exactly two
containers existed for the run: the released Fortemi bundle and Redis. No local
image build, container publication, optional media-sidecar execution, or
registry mutation occurred. An explicit `10.201.0.0/24` UAT subnet avoided the
host's shared predefined address pools.

## MCP regression result

The release-aligned `mcp-server/tests/run-all.sh` suite executed sequentially
using Streamable HTTP with OAuth client-credentials authentication.

| Measure | Result |
|---|---:|
| Declared files | 31 |
| Declared tests | 558 |
| Passed assertions | 557 |
| Failed assertions | 0 |
| Intentional skips | 1 |
| Missing files | 0 |
| Cleanup | 3/3 passed |

All files passed, including CRUD, attachments, search, memory search, links,
archives, SKOS, PKE, jobs, observability, memories, OAuth, API management, the
consolidated 43-tool surface, feature chains, data export, annotations, and
cleanup.

## Post-run health

- API `/livez` returned `live` and `/readyz` returned `ready`.
- MCP `/health` returned HTTP transport status `ok`.
- Fortemi and Redis remained healthy with zero restarts and no OOM kill.
- The Fortemi log from this container start contained zero fatal, panic,
  `handler_panicked`, traceback, out-of-memory, or OOM-kill matches.
- Docker inspection continued to show no GPU device request.
- Teardown removed both containers, the isolated network, and all five fresh
  UAT volumes.

## Bounded limitations

Ollama and the optional Whisper, GLiNER, and Pyannote services were intentionally
not started for this CPU-safe MCP run. Startup and background jobs correctly
reported configured inference providers unreachable. This result therefore does
not validate generation, embedding inference, vision inference, transcription,
entity extraction, or speaker diarization runtime behavior.

Those omissions do not weaken the result for the canonical MCP regression
suite, which passed without bypassing MCP operations. They remain explicit
environment/capability limitations rather than inferred passes.

## Secret handling

The bundle generated its MCP OAuth client inside the fresh data volume with a
mode-0600 credential file. The UAT process removed the file's shell quoting,
acquired a bearer token through the documented `client_secret_basic` flow,
kept credential and bearer values process-local, and unset the bearer after
execution. No credential, bearer value, Hugging Face token, or Authorization
header was written to this result or its test log.

## Evidence boundary

This result validates the live Fortemi persistence plane and MCP surface for the
exact published release image. It does not treat the AIWG static index,
Knowledge Shard state-transfer formats, and Fortemi persistence schemas as a
shared schema. It makes no `core-v1`, `full-v1`, or `record-v1` compatibility
claim and does not claim unqualified full parity, complete backup, or suite
portability while the suite audit remains `NO-GO`.

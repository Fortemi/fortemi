# Fortemi v2026.8.2 Live MCP UAT Result

## Disposition

`PASS` for the signed and published `v2026.8.2` release. The canonical
31-file MCP suite declared 558 tests, executed 557 passing assertions, reported
0 failures, retained 1 intentional branding skip, found 0 missing files, and
completed cleanup 3/3.

## Target identity

| Field | Value |
|---|---|
| Release | `v2026.8.2` |
| Release commit | `6882a0ad1a2db8414823dd81ba40179cab58ef3a` |
| Release image | `ghcr.io/fortemi/fortemi:bundle-2026.8.2` |
| OCI index digest | `sha256:6c9a243ac5337c685bdf48043cb048b02564d607ae0bda73878bdc4b2f69c76a` |
| Local image ID | `sha256:3404f985a0b757002538fc463adbd4ddf2e100c1a76c7fb29740e4fce7ccc0a4` |
| API | `http://127.0.0.1:3000` |
| MCP | `http://127.0.0.1:3001` |
| Deployment start | `2026-08-31T14:12:41.680650829Z` |
| Evidence collected | `2026-08-31T10:19:51-04:00` |

The image's OCI labels reported version `2026.8.2` and revision
`6882a0ad1a2db8414823dd81ba40179cab58ef3a`, matching the signed release
commit.

## Safety profile

The run used `.aiwg/working/compose.host-stability.yml` as a temporary UAT
override. It removed all Docker GPU device reservations, set
`NVIDIA_VISIBLE_DEVICES=void`, selected CPU Open3D behavior, and set every UAT
service restart policy to `no`. Docker inspection confirmed the running Fortemi
container had no device request, zero restarts, no OOM kill, and healthy state.

Only the released Fortemi bundle and Redis were started. No local image build,
container publication, optional media-sidecar execution, or registry mutation
occurred. An explicit `10.200.0.0/24` UAT subnet avoided the host's exhausted
Docker predefined address pools.

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
- The Fortemi log from this container start contained no fatal, panic,
  traceback, or thread-panic match.
- Docker inspection continued to show no GPU device request.

## Bounded limitations

Ollama and the optional Whisper, GLiNER, and Pyannote services were intentionally
not started for this CPU-safe MCP run. Startup correctly reported the configured
Ollama generation provider unreachable. This result therefore does not validate
generation, embedding inference, vision inference, transcription, entity
extraction, or speaker diarization runtime behavior.

Those omissions do not weaken the result for the canonical MCP regression
suite, which passed without bypassing MCP operations. They remain explicit
environment/capability limitations rather than inferred passes.

## Secret handling

The bundle generated its MCP OAuth client inside the fresh data volume with a
mode-0600 credential file. The UAT process acquired a bearer token through the
client-credentials flow, kept credential and bearer values process-local, and
unset the bearer after execution. No credential, bearer value, Hugging Face
token, or Authorization header was written to this result.

## Evidence boundary

This result validates the live Fortemi persistence plane and MCP surface for the
exact published release image. It does not treat the AIWG static index,
Knowledge Shard state-transfer formats, and Fortemi persistence schemas as a
shared schema. It makes no `core-v1`, `full-v1`, or `record-v1` compatibility
claim and does not claim unqualified full parity, complete backup, or suite
portability while the suite audit remains `NO-GO`.

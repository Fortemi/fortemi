# Fortemi v2026.8.0 Live MCP UAT Result

## Disposition

`FAIL` — the signed release failed 14 assertions. The remediated local candidate
reduced the result to 3 failed assertions across 2 authorization-gated findings,
but it is not a published release and the acceptance criterion requires every
assertion to pass.

## Target identity

| Field | Value |
|---|---|
| Release | `v2026.8.0` |
| Commit | `c76e5ef72dcf039acfe78b1e7b254cba30a79b8d` |
| Release image | `ghcr.io/fortemi/fortemi:bundle-2026.8.0` |
| Release OCI index | `sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b` |
| Candidate image | `fortemi:uat-fix` |
| Candidate image ID | `sha256:ef9e63af6e79e1168b31e28697b860aa605127f5f233c66a771581ace32c564d` |
| API | `http://127.0.0.1:3000` |
| MCP | `http://127.0.0.1:3001` |

OAuth clients and bearer tokens were created dynamically for each run. Secret
values were not written to this result or emitted in command output.

## Run summary

| Run | Files | Tests | Passed | Failed | Missing | Cleanup |
|---|---:|---:|---:|---:|---:|---:|
| Signed release discovery run | release-aligned subset | 284 | 270 | 14 | n/a | 3/3 |
| Candidate run before final archive fixes | 31 | 558 | 551 | 6 | 0 | 3/3 |
| Final candidate run | 31 | 558 | 554 | 3 | 0 | 3/3 |

The enhanced runner continued after failing files and reported missing declared
files as failures. This ensured the final counts cover the complete manifest.

## Final candidate phase results

- Schema, branding, error responses, preflight, seed data, CRUD, attachments,
  vision, search, memory search, tags, collections, links, embeddings, document
  types, edge cases, templates, versioning, archives, SKOS, jobs, observability,
  memories, OAuth, API management, feature chains, annotations, and cleanup all
  passed.
- Archive tests passed 24/24, SKOS 50/50, jobs 28/28, observability 18/18,
  memories 21/21, OAuth 13/13, and API management 8/8.
- Targeted vision plus multi-memory regression execution passed 24/24 before the
  full run.

## Remaining failures

| Test | Failure | Status |
|---|---|---|
| `PKE-002` | Short-passphrase rejection does not expose the required safe validation contract | Issue filing and implementation held for explicit security authorization |
| `MB-002` | Consolidated backup snapshot cannot authenticate `pg_dump` in the bundle runtime | Same gated snapshot finding |
| `BACK-013` | Data-export snapshot reaches the same runtime credential failure | Same gated snapshot finding |

The snapshot failure appears through two MCP tool surfaces but is one underlying
finding. Drafts are retained under `.aiwg/working/`; neither gated issue was filed
or implemented without the required authorization.

## Environment limitation

Pyannote is an optional sidecar and remained unhealthy because `HF_TOKEN` was not
provided. Redis, Whisper, GLiNER, the Fortemi API, and MCP were healthy. The UAT
does not claim speaker-diarization parity or full suite portability from this
environment.

## Evidence boundary

This result validates the live Fortemi persistence plane only. It does not treat
the AIWG static index or Knowledge Shard state-transfer formats as the same
schema, and it makes no `core-v1`, `full-v1`, or `record-v1` compatibility claim.

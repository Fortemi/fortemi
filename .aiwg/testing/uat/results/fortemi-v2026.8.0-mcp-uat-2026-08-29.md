# Fortemi v2026.8.0 Live MCP UAT Result

## Disposition

`PASS` for the remediated local candidate: all 31 declared files ran, 557
executed assertions passed, 0 failed, 0 files were missing, one declared test was
skipped, and cleanup passed 3/3. The signed `v2026.8.0` release remains the
immutable release that failed its original UAT; this candidate result does not
represent a new published release.

## Target identity

| Field | Value |
|---|---|
| Release under audit | `v2026.8.0` |
| Release commit | `c76e5ef72dcf039acfe78b1e7b254cba30a79b8d` |
| Release image | `ghcr.io/fortemi/fortemi:bundle-2026.8.0` |
| Release OCI index | `sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b` |
| Candidate image | `fortemi:uat-fix` |
| Candidate image ID | `sha256:78bb6f8c76be0c40900af964e8b892486686e409b36aa62e0bea284eedfcb74d` |
| Candidate version label | `2026.8.0-uatfix.3` |
| Candidate revision | `82fd467f74b78829bd4cfc88b600c347600b43c0` |
| API | `http://127.0.0.1:3000` |
| MCP | `http://127.0.0.1:3001` |

OAuth clients and bearer tokens were created dynamically for each run. Secret
values were held in memory, were not written to this result, and were not emitted
in command output. The final mode-0600 execution log passed a secret-pattern scan.

## Run summary

| Run | Files | Tests | Passed | Failed | Skipped | Missing | Cleanup |
|---|---:|---:|---:|---:|---:|---:|---:|
| Signed release discovery run | release-aligned subset | 284 | 270 | 14 | 0 | n/a | 3/3 |
| Candidate before final archive fixes | 31 | 558 | 551 | 6 | 1 | 0 | 3/3 |
| Candidate after archive fixes | 31 | 558 | 554 | 3 | 1 | 0 | 3/3 |
| Fresh authorization baseline | 31 | 558 | 553 | 4 | 1 | 0 | 3/3 |
| Credential and PKE candidate | 31 | 558 | 555 | 2 | 1 | 0 | 3/3 |
| Final remediated candidate | 31 | 558 | 557 | 0 | 1 | 0 | 3/3 |

The failure-complete runner continued after failing files and reported missing
declared files as failures. In the final run, every file passed; the branding
file reported 6/7 because one declared assertion was intentionally skipped.

## Final candidate phase results

- All 31 files passed, including PKE 22/22, consolidated tools 66/66, data export
  19/19, archives 24/24, SKOS 50/50, jobs 28/28, and cleanup 3/3.
- Both database snapshot surfaces passed after the bundle-local administrative
  dump path was introduced.
- API health, authenticated readiness, and MCP health returned HTTP 200 before
  and after the run.
- The application log contained no `fatal` or `panic` match during the run.

## Resolved findings

| Test | Resolution | Traceability |
|---|---|---|
| `PKE-002`, `PKE-021` | Typed short-passphrase failures return safe HTTP 400 validation guidance; all other cryptographic diagnostics remain redacted | Issue #1121; signed commit `1c9730e1f687b15fbaaeef9b0802cb3aceaffc24` |
| `MB-002`, `BACK-013` | Bundle authentication fallback was added, then whole-database bundle dumps were routed through a fixed local peer-authenticated administrator to include forced-RLS rows | Issue #1120; signed commits `1c9730e1f687b15fbaaeef9b0802cb3aceaffc24` and `82fd467f74b78829bd4cfc88b600c347600b43c0` |

The bundle-only administrative child removes `PGPASSWORD`, `PGPASSFILE`, and
`POSTGRES_PASSWORD` from its environment and places no secret in argv. The
application role remains non-superuser and `NOBYPASSRLS`; non-bundle deployments
retain their configured libpq path.

## Environment limitation

Pyannote is an optional sidecar and remained unavailable because `HF_TOKEN` was
not provided. Redis, Whisper, GLiNER, the Fortemi API, and MCP were healthy. The
UAT does not claim speaker-diarization parity or full suite portability from this
environment.

This limitation describes the original execution window. A separate
post-IT-Ops-465 supplement records the later healthy Pyannote runtime, a real
diarization request, and a fresh passing 31-file MCP regression run:
`fortemi-v2026.8.0-mcp-uat-post-itops-465-2026-08-30.md`.

## Evidence boundary

This result validates the live Fortemi persistence plane only. It does not treat
the AIWG static index or Knowledge Shard state-transfer formats as the same
schema, and it makes no `core-v1`, `full-v1`, or `record-v1` compatibility claim.

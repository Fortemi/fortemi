# Fortemi v2026.8.0 Candidate Post-IT-Ops-465 UAT Supplement

## Disposition

`PASS` for the already-remediated local candidate after the Pyannote runtime
prerequisite was provisioned through IT Ops issue `roctinam/itops#465`.

This supplement does not change the historical signed `v2026.8.0` release
result and does not represent a new published release. It closes only the
speaker-diarization environment limitation recorded by the 2026-08-29 UAT.

## Target identity

| Field | Value |
|---|---|
| Candidate commit | `88a42ca8edf245cd96d93f742266ade5ab218eff` |
| Candidate image | `fortemi:uat-fix` |
| Candidate image ID | `sha256:78bb6f8c76be0c40900af964e8b892486686e409b36aa62e0bea284eedfcb74d` |
| Pyannote image | `ghcr.io/fortemi/fortemi:pyannote-latest` |
| Pyannote image ID | `sha256:ac517b8d579a07663fa45f219c8867b115cb99aa2ebd141db4258071a37d4e31` |
| API | `http://127.0.0.1:3000` |
| MCP | `http://127.0.0.1:3001` |
| Execution window | 2026-08-30 16:31:12-04:00 to 16:35:19-04:00 |

## Speaker-diarization validation

- IT Ops issue `roctinam/itops#465` was closed before validation.
- Pyannote reported healthy with model
  `pyannote/speaker-diarization-3.1` on CPU, zero container restarts, and a
  present runtime binding. No sensitive value was displayed.
- A real 45,312-byte English audio fixture was submitted to `/diarize` with
  one-to-two-speaker bounds. The sidecar returned HTTP 200 and one valid RTTM
  segment for one detected speaker, with zero malformed lines.
- The temporary fixture was removed after the request.
- Fortemi's health payload advertised `speaker_diarization=true`.

## MCP regression run

The release-aligned 31-file MCP suite ran sequentially against the live
candidate with a fresh in-memory OAuth client.

| Measure | Result |
|---|---:|
| Declared files | 31 |
| Declared tests | 558 |
| Passed assertions | 557 |
| Failed assertions | 0 |
| Skipped assertions | 1 |
| Missing files | 0 |
| Cleanup | 3/3 passed |

API health, API readiness, MCP health, and Pyannote health returned HTTP 200
before and after the run. Fortemi, Redis, Whisper, GLiNER, and Pyannote were all
healthy afterward with zero container restarts. The Fortemi application log
contained no `fatal` or `panic` match during the run, and the Pyannote log
contained no traceback, error, or exception match from its current start.

## Secret-handling evidence

- OAuth client material and the bearer value remained process-local and were
  unset after execution.
- No Hugging Face value was printed, copied into the repository, or included in
  this result.
- The mode-0600 execution log produced zero matches for the enforced token,
  bearer-header, and client-secret patterns.
- The direct interactive `obliteratus-local` login remained unavailable to the
  operator shell; validation used the provisioned service runtime rather than
  exposing the AppRole bootstrap to this session.

Restricted execution evidence:
`.aiwg/working/fortemi-mcp-uat-pyannote-20260830-163112.log`.

## Evidence boundary

This supplement validates the live Fortemi persistence plane and the local
speaker-diarization sidecar. It does not equate the AIWG static index, Knowledge
Shard state-transfer formats, and Fortemi persistence schemas. It makes no
`core-v1`, `full-v1`, or `record-v1` compatibility claim and does not claim
unqualified full parity, complete backup, or suite portability.

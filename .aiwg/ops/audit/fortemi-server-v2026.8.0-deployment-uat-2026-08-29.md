---
session_id: fortemi-server-v2026.8.0-deployment-uat-2026-08-29
started_at: 2026-08-29T22:33:55-04:00
completed_at: 2026-08-30T10:48:06-04:00
status: complete
host: titan
operator: roctinam
service: fortemi-server
release: v2026.8.0
release_commit: c76e5ef72dcf039acfe78b1e7b254cba30a79b8d
release_image: ghcr.io/fortemi/fortemi:bundle-2026.8.0
image_index_digest: sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b
candidate_image: fortemi:uat-fix
candidate_image_id: sha256:78bb6f8c76be0c40900af964e8b892486686e409b36aa62e0bea284eedfcb74d
candidate_revision: 82fd467f74b78829bd4cfc88b600c347600b43c0
---

# Fortemi Server v2026.8.0 Deployment and MCP UAT Audit

## Scope and disposition

The latest signed release was resolved, verified, deployed, and exercised through
the live MCP surface. That immutable release failed UAT and exposed actionable
defects. Issues #1104 through #1121 were filed and addressed in signed commits on
`main`; the final remediated local candidate passed its full MCP UAT gate. The
candidate is not a published release.

The final candidate run executed all 31 declared files and 558 declared tests:
557 executed assertions passed, 0 failed, one was skipped, no file was missing,
and cleanup passed 3/3. No full-parity, complete-backup, or portability claim is
made.

## Release and signing evidence

- Authoritative Gitea release: `v2026.8.0`, published 2026-08-26.
- Release commit: `c76e5ef72dcf039acfe78b1e7b254cba30a79b8d`.
- OCI index digest: `sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b`.
- Linux amd64 manifest: `sha256:01919acdf692ba9358abd5b5013be37e752f9a5051a0392e924d2ec8ba8ab8f4`.
- Gitea release assets contained four native binaries, `SHA256SUMS.txt`, and
  provenance.
- `tools/ci/verify-signed-tag.sh v2026.8.0` passed against authorized fingerprint
  `9292EFCBB0EA41BECEEFDAFA9C1B8CE0E0E09C33`.
- Signing policy separates creation and verification: creation uses the
  configured TPM/OpenBao adapter, while verification uses isolated public-key
  `gpg`. Both remediation commits were verified against repository fingerprint
  `62297562B1C7053088F405DB0117DAAA677A5BF2`.

## Deployment evidence

- Deployment profile: authenticated, API and MCP bound to loopback, Redis plus
  extraction sidecars, support archive disabled.
- Docker's default address pools were exhausted. The project retained the
  explicit isolated subnet `10.200.0.0/24`; no existing Docker network was
  removed.
- Host Ollama 0.33.2 remained available only through the resolved Docker
  host-gateway address.
- Only the Fortemi container was recreated for each candidate; the persistent
  volume, network, Redis, and extraction services were preserved.
- The final candidate used image ID
  `sha256:78bb6f8c76be0c40900af964e8b892486686e409b36aa62e0bea284eedfcb74d`,
  version label `2026.8.0-uatfix.3`, and revision
  `82fd467f74b78829bd4cfc88b600c347600b43c0`.
- API health, authenticated readiness, and MCP health returned HTTP 200 before
  and after UAT. Redis, Whisper, and GLiNER were healthy.
- Pyannote remained unavailable because the optional sidecar has no `HF_TOKEN`;
  this unmet environment prerequisite is not represented as a product pass.

## Backup and privilege-boundary evidence

- A clean-volume candidate start without a bypass correctly classified PostGIS
  extension objects as non-user data and completed migrations.
- A separate non-empty `v2026.8.0` volume was upgraded without a backup bypass.
  Its archive was restored to a temporary database and preserved the expected
  sentinel row; the temporary resources were removed afterward.
- The live snapshot credential fallback reached `pg_dump`, exposing a second
  root cause: a whole-database dump under the application role cannot include a
  table protected by forced row-level security.
- The all-in-one bundle now routes only whole-database dump operations through a
  fixed local peer-authenticated PostgreSQL administrator. The administrative
  child strips `PGPASSWORD`, `PGPASSFILE`, and `POSTGRES_PASSWORD`; no secret is
  placed in argv.
- The application role remains non-superuser and `NOBYPASSRLS`. Native
  deployments and per-memory schema operations retain their configured libpq
  path.
- Historical tiny artifacts created by the signed release remain in the
  persistent test volume. They were not deleted without a verified ownership
  and integrity basis; issue #1105 covers cleanup of exact newly failed basenames.

## Validation evidence

| Validation | Result |
|---|---|
| Release-aligned live MCP run | 284 tests; 270 passed; 14 failed; cleanup 3/3 |
| Candidate before authorized security fixes | 558 tests; 553 passed; 4 failed; 1 skipped; 0 missing; cleanup 3/3 |
| Candidate after credential and PKE fix | 558 tests; 555 passed; 2 failed; 1 skipped; 0 missing; cleanup 3/3 |
| Final independent MCP UAT | 31 files; 558 declared; 557 passed; 0 failed; 1 skipped; 0 missing; cleanup 3/3 |
| Full `matric-api` binary test target | 974 passed; 0 failed |
| `matric-api` clippy with warnings denied | Passed |
| Rust formatting, Node/Bash syntax, and diff whitespace | Passed |
| Isolated remediation-commit signature verification | Passed |
| Final secret scan | No token or client-secret pattern detected in the mode-0600 UAT log |

The final application log review found no unexpected `panic` or `fatal` match.
The exact MCP result is recorded in
`.aiwg/testing/uat/results/fortemi-v2026.8.0-mcp-uat-2026-08-29.md`; the restricted
execution log is `.aiwg/working/fortemi-mcp-uat-final-20260830-104403.log`.

## Issue traceability

Issues #1104 through #1119 cover the earlier bundle gate, artifact cleanup, MCP
authorization and version reporting, archive isolation and cloning semantics,
job provenance, reference typing, missing-note access logging, memory counts,
failure-complete UAT, and associated CI/platform findings.

Issue #1120 covers the bundle snapshot authentication and forced-RLS dump path.
Issue #1121 covers the safe PKE short-passphrase validation contract. Their
remediation is in signed commits `1c9730e1f687b15fbaaeef9b0802cb3aceaffc24`
and `82fd467f74b78829bd4cfc88b600c347600b43c0`; both issue surfaces pass the final
full MCP UAT.

## Rollback

To return to the signed release, restore the Compose image to
`ghcr.io/fortemi/fortemi:bundle-2026.8.0` and recreate only the `fortemi` service.
The database volume must not be rolled backward after newer migrations without a
verified compatible restore. Remove the Ollama systemd drop-in and restart Ollama
to revoke the Docker-gateway listener.

---
session_id: fortemi-server-v2026.8.0-deployment-uat-2026-08-29
started_at: 2026-08-29T22:33:55-04:00
completed_at: 2026-08-30T00:14:58-04:00
status: complete-with-open-findings
host: titan
operator: roctinam
service: fortemi-server
release: v2026.8.0
release_commit: c76e5ef72dcf039acfe78b1e7b254cba30a79b8d
release_image: ghcr.io/fortemi/fortemi:bundle-2026.8.0
image_index_digest: sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b
candidate_image: fortemi:uat-fix
candidate_image_id: sha256:ef9e63af6e79e1168b31e28697b860aa605127f5f233c66a771581ace32c564d
---

# Fortemi Server v2026.8.0 Deployment and MCP UAT Audit

## Scope and disposition

The latest signed release was resolved, verified, deployed, and exercised through
the live MCP surface. Release UAT failed and exposed actionable defects. The safe
findings were filed as issues #1104 through #1116, addressed in a local candidate,
and retested. The candidate is not a published or signed release.

The final candidate run executed all 31 declared files and 558 assertions: 554
passed, 3 failed, 0 files were missing, and cleanup passed 3/3. The remaining
failures map to two security-sensitive findings held for issue-specific user
authorization: runtime database snapshot credentials (two test surfaces) and the
PKE short-passphrase error contract. Therefore the UAT disposition remains
`FAIL`; no full-parity, complete-backup, or portability claim is made.

## Release and signing evidence

- Authoritative Gitea release: `v2026.8.0`, published 2026-08-26.
- Release commit: `c76e5ef72dcf039acfe78b1e7b254cba30a79b8d`.
- OCI index digest: `sha256:195f95d689608c6b1f9c66565bdccc567d910dd2a33339081bffee223fed355b`.
- Linux amd64 manifest: `sha256:01919acdf692ba9358abd5b5013be37e752f9a5051a0392e924d2ec8ba8ab8f4`.
- Gitea release assets contained four native binaries, `SHA256SUMS.txt`, and
  provenance.
- `tools/ci/verify-signed-tag.sh v2026.8.0` passed against authorized fingerprint
  `9292EFCBB0EA41BECEEFDAFA9C1B8CE0E0E09C33`.
- Signing policy separates creation and verification: tag creation uses the
  configured TPM/OpenBao signing adapter; verification intentionally overrides
  that adapter with an isolated public-key `gpg` invocation. A generic
  `git tag -v` failure under the creation adapter is not release evidence.

## Deployment evidence

- Deployment profile: authenticated, API and MCP bound to loopback, Redis plus
  extraction sidecars, support archive disabled.
- Docker's default address pools were exhausted. The project was assigned the
  explicit isolated subnet `10.200.0.0/24`; no existing Docker network was
  removed.
- Host Ollama 0.33.2 was exposed only on `172.17.0.1:11434` for the container
  gateway. The reviewed systemd drop-in is retained as rollback evidence.
- The release was first deployed from the immutable release image. After issue
  remediation, only the Fortemi container was replaced by `fortemi:uat-fix`;
  the persistent volume, network, Redis, and extraction services were preserved.
- At final observation the candidate API was healthy and ready, MCP health was
  `ok`, and the application container used image ID
  `sha256:ef9e63af6e79e1168b31e28697b860aa605127f5f233c66a771581ace32c564d`.
- Redis, Whisper, and GLiNER were healthy. Pyannote remained unavailable because
  the optional sidecar has no `HF_TOKEN`; this is an unmet environment
  prerequisite and is not represented as a product pass.

## Backup and upgrade-gate evidence

- A clean-volume candidate start without a bypass correctly classified PostGIS
  extension objects as non-user data and completed migrations.
- A separate non-empty `v2026.8.0` volume was upgraded by the candidate without a
  backup bypass. It produced a 122,398-byte, mode-0600, postgres-owned backup with
  SHA-256 `a3db338e6f904e13e341ce5fb07f0bb42583d18ac27bf953af58cf85e0efbd01`.
- The isolated upgrade advanced migrations from `20260824010500` to
  `20260828190000`. The application role remained non-superuser without
  `BYPASSRLS`.
- The resulting archive was restored to a temporary database and the expected
  sentinel row count was 1. The exact temporary container, volume, and restore
  database were removed afterward.
- Historical tiny artifacts created by the signed release remain in the
  persistent test volume. They were not deleted without a verified ownership
  and integrity basis; issue #1105 covers cleanup of the exact newly failed
  basename on future attempts.

## Validation evidence

| Validation | Result |
|---|---|
| Release-aligned live MCP run | 284 tests; 270 passed; 14 failed; cleanup 3/3 |
| Candidate targeted vision and memories | 24/24 passed |
| Candidate full failure-complete MCP run | 558 tests; 554 passed; 3 failed; 0 missing; cleanup 3/3 |
| Full `matric-api` binary test target | 966 passed; 0 failed |
| Targeted archive clone semantic-graph regression | Passed |
| Targeted duplicate archive target regression | Passed |
| Archive provenance schema-context regression | Passed |
| Hosted tenant-posture regression | Passed |
| Backup gate shell smoke and static verifier | Passed |
| Rust checks, Node/Bash syntax, and diff whitespace | Passed |

The job failure warnings observed during the final log review were generated by
intentional negative-path UAT cases. No unexpected panic or fatal startup error
was observed. The exact MCP result is recorded in
`.aiwg/testing/uat/results/fortemi-v2026.8.0-mcp-uat-2026-08-29.md`.

## Issue traceability

Safe findings were filed as #1104-#1116. Their fixes cover the bundle
pre-migration gate, failed-artifact cleanup, MCP auth and version reporting,
tenant-aware archive access, lossless archive cloning, archive-job provenance,
reference-status typing, missing-note access logging, memory overview counts,
and failure-complete MCP UAT execution.

The two issue drafts requiring explicit authorization are retained under
`.aiwg/working/` and were not filed or implemented in this session.

## Rollback

To return to the signed release, restore the Compose image to
`ghcr.io/fortemi/fortemi:bundle-2026.8.0` and recreate only the `fortemi` service.
The database volume must not be rolled backward after newer migrations without a
verified compatible restore. Remove the Ollama systemd drop-in and restart Ollama
to revoke the Docker-gateway listener.

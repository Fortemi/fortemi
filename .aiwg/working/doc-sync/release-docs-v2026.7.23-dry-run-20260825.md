# Release Documentation Dry-Run Audit

**Date:** 2026-08-25  
**Direction:** code-to-docs  
**Mode:** dry run; no product, documentation, or version files changed  
**Baseline:** `v2026.7.22..9a43c755`  
**Candidate:** `v2026.7.23` (subject to release-owner confirmation)

## Result

**NOT READY for the changelog/announcement gate.** The range contains 38 commits
and 274 changed files (`+24,089/-929`), including hosted-only foundations, six
database migrations, MCP recovery/security fixes, chunk-offset correctness, and
release-container hardening. `CHANGELOG.md` has an empty `Unreleased` section,
the four release-version authorities remain `2026.7.22`, and no candidate
announcement exists.

## Findings (maximum 10)

| ID | Severity | Exact missing or stale item | Proposed resolution |
|---|---|---|---|
| RDS-01 | Blocker | `CHANGELOG.md` has no coverage for any commit in the range. | Add a `2026.7.23` section with the five bounded highlight groups below; update `[Unreleased]` and add the comparison link. |
| RDS-02 | Blocker | `Cargo.toml`, seven workspace package entries in `Cargo.lock`, `mcp-server/package.json`, and the root/package entries in `mcp-server/package-lock.json` still say `2026.7.22`. | Bump all four authorities in lockstep to the selected candidate before the version gate. |
| RDS-03 | Blocker | `docs/releases/v2026.7.23-announcement.md` is absent. | Create it from the current announcement pattern. Include the six forward migrations (`20260824010000` through `20260824010500`), normal automatic migration behavior, hosted prerequisites, CE behavior, verification, and rollback-to-restored-destination guidance. Do not say “no schema migration.” |
| RDS-04 | High | README claims “opt-in authentication” and “schema isolation … for tenant separation,” while this range distinguishes CE archive schemas from hosted shared-schema forced RLS. Its hosted/team example also omits `FORTEMI_MULTI_TENANT=true`. | Qualify the feature/security tables: CE remains synthetic single-tenant; archive schemas are memory boundaries, not hosted tenant boundaries; hosted auth is mandatory and feature-gated. Link the hosted PostgreSQL role runbook instead of presenting the bundle example as hosted-ready. |
| RDS-05 | High | Public configuration coverage omits hosted startup requirements enforced in code: distinct `MIGRATION_DATABASE_URL`, AWS-KMS-enabled build plus `FORTEMI_AWS_KMS_KEY_ID`, and hosted-auth build/issuer authority. | Add an explicit hosted-only configuration subsection or link the authoritative runbooks. Keep credentials out of examples; name only variable classes and fail-closed behavior. |
| RDS-06 | High | `docs/content/embedding-pipeline.md` calls chunk sizes, overlaps, and offsets “char” values. `ChunkerConfig` and semantic spans are now UTF-8 byte based and preserve LF/CRLF/lone-CR source spans. | Change the units to UTF-8 bytes, state that boundaries remain valid scalar boundaries, and replace the illustrative `char overlap` wording. Mention the CRLF/non-ASCII regression fix in the release entry (#1100, related #1098). |
| RDS-07 | Medium | `docs/content/api.md` does not document hosted-only `POST/GET /api/v1/user/secrets`, `DELETE /api/v1/user/secrets/{id}`, `POST /api/v1/inference/embed`, or `GET /api/v1/inference/catalog`. | Add feature-gated summaries or an explicit link to `docs/operations/hosted-user-credentials.md`; say hosted completion/stream rejects inline keys and caller destinations. |
| RDS-08 | High | `docs/content/troubleshooting.md` recommends `printenv | grep MCP_CLIENT` and a `curl -u "$MCP_CLIENT_ID:$MCP_CLIENT_SECRET"` diagnostic, undermining the new boot-log masking guarantee. | Replace with redacted presence/length/file-pointer checks and never print or place the secret in a command argument. Release notes should say boot logs no longer echo the secret, not claim all operator commands are safe. |
| RDS-09 | Medium | New hosted runbooks are absent from README’s Documentation index: PostgreSQL roles, durable audit sink, KMS rotation, stored credentials, inference destination policy, and inference resilience. | Add a clearly marked internal/hosted operations group. Do not imply these are supported by the public CE image. |
| RDS-10 | High | A release summary could overstate `65a77ccd` (“complete … production gates”). The runbooks retain open live-KMS/provider receipts, complete-backup limits for encrypted `user_secrets`, and incomplete quota plan/billing integration. | Use “bounded hosted foundation” and list the remaining gates. Do not claim hosted readiness, complete backup, full portability, or unqualified suite parity. Knowledge Shard claims, if any, must name an exact profile; this range does not change that contract. |

## Proposed Release Highlights

1. **Hosted foundation (feature-gated):** shared-schema forced-RLS tenant scope,
   distinct migration/runtime PostgreSQL roles, durable authorization audit,
   AWS KMS startup canary, Redis request admission, encrypted user credentials,
   outbound inference destination controls, and account-scoped circuit breakers.
2. **MCP recovery and secret hygiene:** stale Streamable HTTP session IDs return
   404 so clients re-initialize; bundle boot summaries no longer echo the MCP
   client secret.
3. **Chunk correctness:** semantic chunking preserves source UTF-8 byte offsets
   across LF, CRLF, lone CR, and non-ASCII input, with bounded long-span splits.
4. **Release build reliability:** Rust builders are pinned to 1.92, release
   builds are locked, and the configurable 16 MiB rustc stack guard prevents
   the observed thin-LTO compiler stack failure.
5. **CI evidence:** hosted schema/role tests are serialized and scoped,
   protected quota tests receive Redis, MCP session expiry is regression-tested,
   cargo-audit bootstrap is hardened, and sidecar auth fetch uses system Git.

## Release Claim Boundary

The announcement should explicitly say that Community Edition remains the
default public path and hosted routes require an internal feature-enabled
build plus deployment-specific PostgreSQL, Redis, KMS, identity, and receipt
evidence. The release adds forward migrations; rollback requires restoring the
pre-migration snapshot to a separate destination, not destructive down SQL.


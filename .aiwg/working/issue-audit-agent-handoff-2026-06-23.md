# Issue Audit Agent Handoff

Date: 2026-06-23
Repo: `/home/roctinam/dev/fortemi`
Tracker: `Fortemi/fortemi` on Gitea
Authenticated tracker user: `roctibot`

## User Direction

Continue the detailed open-issue audit. The user explicitly authorized updating, commenting, closing, deferring, rewriting, and creating issues as needed. They also asked that bridge/proxy stories honestly compare Fortemi's approach with FlechetteLabs Tollbooth and preserve optional compatibility for users who want to leverage Tollbooth.

The active user intent is not just summary generation: keep auditing issue slices, align older issues with current designs/plans, and ask focused planning questions where decisions are genuinely open.

## Required Operating Rules

- Start every issue-audit slice with `aiwg discover "<need>"`; prefer the discovered `issue-audit` workflow when it appears.
- Inspect current repo code and current tracker state before relying on prior notes.
- Use tracker state as authoritative for issue status, labels, milestones, and comments.
- Mutations are allowed by the user request, but keep them scoped and evidence-based.
- Do not revert dirty worktree changes. Many existing modified/untracked files appear unrelated or generated.
- Use `grep`, `find`, and `sed`; `rg` is unavailable in this workspace.
- Use `multi_tool_use.parallel` for independent local reads/tracker reads where practical.
- Browse current external sources when facts can change or when using best-practice claims; cite sources in the final response.
- Do not mark any persistent goal complete; the work is ongoing.

## Current Dirty Worktree

Observed dirty files included:

- `.aiwg/aiwg.config`
- multiple `.aiwg/frameworks/.../working/...` files
- `.aiwg/working/issue-planner-storage/issue-backlog.md`
- `AIWG.md`, `CLAUDE.md`
- `Dockerfile`, `docker-compose.yml`
- docs and generated AIWG artifacts
- untracked `AGENTS.md`, `.aiwg/AIWG.md`, planning/research/security directories

Treat these as user/generated state unless a new task directly requires editing them.

## Completed Issue-Audit Slices

### Release, Native, Docker Distribution

- Created #992: `fix(docker): replace bundle default database credentials with generated or operator-supplied secrets`
- Cross-linked #968 and #989.
- Commented #990.

### OAuth and MCP Auth DTOs

- Created #993: `fix(oauth): remove unsafe Debug output from token and client-secret DTOs`
- Cross-linked #968.
- Corrected labels.

### Attachments, Realtime, Provider Media

- Created #994: `fix(attachments): bound pre-validation buffering for upload and provider-media ingestion paths`
- Cross-linked #970, #981, and #922.

### Search, Embeddings, Cache

- Created #995: `fix(embeddings): route embedding jobs through effective provider/config resolver`
- Cross-linked #666, #929, and #975.

### Bridge, Proxy, Tollbooth

- Commented #872, #864, and #920.
- Direction established: Tollbooth can be an optional local gateway/proxy compatibility path, but Fortemi should keep its own hosted identity, policy, audit, billing, and provider-selection boundaries explicit.

### Trusted Ingress and Forwarded Headers

- Created #996: `fix(ingress): trust forwarded host/proto/IP headers only from configured proxies`
- Cross-linked #950, #966, #967, #864, #872, #983, and #920.

### Rate-Limit and Request Identity

- Commented #714, #996, #908, and #898.
- Main alignment: trusted client identity must precede hosted rate limiting; Redis outage behavior and quota visibility need explicit contracts.

### OAuth and MCP Registration

- Commented #972, #944, #926, and #941.
- Main alignment: distinguish CIMD from DCR; pin issuer/public URL behavior; clarify PKCE and client auth expectations.

### Stored Secrets, KMS, BYO Provider Key

- Commented #730, #731, #946, and #897.
- No new issue created.

### Backup, Import, Export Trust Boundary

- Updated #991 milestone to Hosted auth launch gate.
- Commented #991, #978, #927, and #900.

### MCP Remote Transport and Auth

- Commented #921, #940, #914, #917, #987, and #899.

### Supply Chain, Docker, Release

- Commented #930, #982, #937, #916, #888, and #990.

### Realtime Voice, Call Recording Consent, Provider Deletion

- Commented #986, #981, #952, #969, and #900.

### Hosted Ingress, Control Plane, Ad-Hoc AI

- Commented #988: inbound-source activation lifecycle, default disabled/draft, supervisor-validated config, redacted config, DLQ retention.
- Commented #951: ingest-token mint/revoke as privileged service-credential lifecycle, owner binding, replay contract, rate limit, Redis readiness, no token leakage.
- Commented #963: ad-hoc AI sanitized provider/streaming error contracts.
- Commented #962: document-type detection as data-processing endpoint and `DetectDocumentType`/`UseDocumentType` actions.

### Object-Level Auth and Private Data Projection

- Commented #960: resolver needed for versions/full-doc/links/provenance/access analytics; avoid `fetch_tx` access counter before auth.
- Commented #959: note/collection export response shape, title-derived filenames, partial exports, audit/cache/large limit.
- Commented #961: attachment derivative lookup must not trust `extracted_metadata` JSON as auth source; need `resolve_attachment_for_action`.
- Commented #958: provenance owner/tenant binding for named locations/devices.

### Jobs, Retry, External Queue

- Commented #971: job identity, dedup, idempotency keys; `JobResult::Retry(String)` free-form issue; `queue_deduplicated()` scope too broad; payload-derived archive scope; worker events expose raw errors.
- Commented #931: strict enum parsing before retry/dead-letter statuses or external envelopes.
- Commented #954: queue-control authorization must cover diagnostic/raw-error access and projections.
- Commented #895: payload-derived pause scope blocker for external queue adapters.

## Latest Interrupted Slice: Browser/Public API Surface

This slice was mostly completed before interruption. Do a quick readback before reporting.

### Issues Inspected

- #966
- #965
- #964
- #967
- #974
- #953

### Code Evidence Found

- `parse_allowed_origins()` defaults to `http://localhost:3000`, parses raw header values, and silently drops invalid origins.
- Global `CorsLayer` uses `AllowOrigin::list(parse_allowed_origins())` and `allow_credentials(true)`.
- `is_public_route()` exempts `/health*` and `/api/v1/health*` by prefix.
- `/docs`, `/openapi.yaml`, and `/asyncapi.yaml` are public/auth-exempt.
- Swagger UI has `try_it_out_enabled(true)`.
- Cache middleware treats non-API/static docs/schema as `public, max-age=3600`; health is `no-cache`.
- Attachment derivative routes set direct public cache headers and VTT wildcard CORS; this is already tied to #961 and #966.
- No central runtime security-header layer found for `X-Content-Type-Options`, CSP/frame-ancestors, `Referrer-Policy`, `Permissions-Policy`, HSTS, COOP/CORP/COEP.
- `ApiError::Database` and `ApiError::Internal` return raw internal strings.
- `BlobMissing` returns `expected_path` and `storage_backend`.
- Backup paths return `pg_dump error: {stderr}`.
- Audio/vision provider errors can map backend messages into responses.

### Tracker Updates Already Posted

- #964 comment id 70843: replace prefix-based public health exemptions with generated/explicit public-probe contract.
- #965 comment id 70844: Swagger `try it out` must be treated as auth/browser surface, not just documentation.
- #967 comment id 70846: security headers and cache policy need one response-class matrix.

### Suggested Readback

Before finalizing this slice, read comments on #964, #965, #967, and optionally #966. If #966 already covers browser-origin/session/token policy, no new comment is needed. If not, add a narrow comment tying credentialed CORS, CSRF, and docs `try it out` to the same response-class matrix.

### Sources Used For Latest Slice

- OWASP API8 Security Misconfiguration: https://owasp.org/API-Security/editions/2023/en/0xa8-security-misconfiguration/
- OWASP API9 Improper Inventory Management: https://owasp.org/API-Security/editions/2023/en/0xa9-improper-inventory-management/
- OWASP CORS Testing Guide: https://owasp.org/www-project-web-security-testing-guide/latest/4-Web_Application_Security_Testing/11-Client-side_Testing/07-Testing_Cross_Origin_Resource_Sharing
- OWASP CSRF Prevention Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html
- OWASP HTTP Headers Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/HTTP_Headers_Cheat_Sheet.html
- Kubernetes probe guidance: https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/

## Immediate Next Actions

1. Run `aiwg discover "issue audit CORS CSRF public OpenAPI health status error security headers hosted browser origin"`.
2. Read back #964, #965, #967, and #966 comments to verify posted tracker state.
3. If #966 does not already cover browser credential policy, comment there; otherwise do not duplicate.
4. Finalize the browser/public-surface slice with a concise summary:
   - Updated #964, #965, #967.
   - No issues closed or created in that slice.
   - #966 inspected; either already covered or newly commented.
   - Include the external sources above.
5. Ask these focused planning questions:
   - Should hosted `/health` remain a minimal public compatibility alias, or should only `/health/live` and `/health/ready` be public while `/health` becomes protected?
   - Should public hosted Swagger disable `try it out` entirely, or should `/docs` be authenticated operator-only?
   - Do we want RFC 9457 Problem Details now, or keep the current JSON error shape with `code` and `request_id` first?

## Good Next Audit Slices

- Browser/API error response sanitization and RFC 9457/problem-details migration.
- Public OpenAPI/AsyncAPI inventory separation from operator schema.
- Admin/operator diagnostics and health route partitioning.
- Data retention/deletion across providers, attachments, backups, and call recordings.
- Hosted billing/quota/provider-cost accounting.
- Plugin/extension trust boundary and sandboxing.
- Multi-tenant object authorization resolver coverage across notes, collections, attachments, provenance, exports, and analytics.

## Final Response Template For Current Agent

Use this only after readback:

> Created `.aiwg/working/issue-audit-agent-handoff-2026-06-23.md` with the current tracker state, completed issue-audit slices, latest browser/public-surface comments, source links, and exact next actions for the next agent.
>
> The new agent should start by reading back #964/#965/#967/#966, then finish the browser/public-surface slice before moving to the next audit cluster.

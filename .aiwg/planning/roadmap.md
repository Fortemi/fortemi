# Fortemi Delivery Roadmap

> **Status:** Active — reference until all planned phases are complete.
> **Created:** 2026-06-24 · **Last updated:** 2026-06-25
> **Tracker:** Gitea `Fortemi/fortemi` (authoritative). Issue numbers below are Gitea issues.
> **Source:** Synthesized from the 2026-06-21→23 open-issue audit sweep (#746–#1006), the milestone structure, and the 2026-06-23 interactive product-decision Q&A (31 decisions recorded on-issue).

## How to use this document

- This is the **living plan of record**. Update the checkboxes and status as work lands.
- Future agents should treat this as the **main operating project plan** until all phases are complete or the operator explicitly replaces it. For "what next" / "continue the plan" / "advance roadmap" requests, run `aiwg discover "advance roadmap"` and use `fortemi-roadmap-skill`.
- Phases are sequenced by **dependency**, not dates. (Velocity here is human+AI non-scalar; do not add time estimates — express scope as issues/sequence.)
- Each phase lists its **gating dependency** and the **issues** in it. Issue lists are anchors, not exhaustive — the milestone is the full set.
- The **strategic pivot is #853**: the open BSL desktop build ships *without* the advanced-auth/multi-tenant stack, so **Phase 1 (open-build GA) is NOT blocked by Phase 2 (hosted/licensed)** — they parallelize.

### Status legend
`[ ]` not started · `[~]` in progress · `[x]` complete · `[!]` blocked (note blocker)

### Tier labels (Gitea)
- `tier/licensed-server` (#574) — advanced auth + multi-tenant hosting; omitted from the open build (per #853).
- `tier/open-build` (#575) — single-user desktop; basic MCP; local/API-key auth.

### Infrastructure prerequisites (`roctinam/devops`)

Phases 2–3 (licensed-server / bridge) require the EE org, repos, and private Cargo registry per **ADR-095** (CE/EE distribution) and **ADR-096** (private registry). Repo-creation requests filed 2026-06-24 in `roctinam/devops`:
- **devops#33** — stand up `Fortemi-Enterprise` private Gitea org (gates all EE repos).
- **devops#34** — stand up private Cargo registry (ADR-096; also migrates `fortemi-auth` off the SSH-git dep).
- **devops#35–#43** — 9 EE private repos: auth-providers, rbac, audit-sinks, billing, search-backends, job-backends, kms, mcp-gate, distribution.
- **devops#44** — `Fortemi/aiwg-fortemi-skills` public/MIT (CE Tier-1; was "proposed" in ADR-095).

Near-term EE repos gating **Phase 2**: kms (devops#41 → #897), mcp-gate (devops#42), rbac (devops#36 → #956–963), audit-sinks (devops#37 → #910), billing (devops#38 → #877+), distribution (devops#43). Existing CE repos: `Fortemi/fortemi`, `HotM`, `fortemi-react`, `fortemi-auth` (private), `licensing` (private).

---

## Phase 0 — Foundation contracts (cross-cutting; unblock everything)

**Gating:** none — start here. Most other phases consume these.

- [~] **#710** AuthorizationPolicy route/action inventory — single source of truth for the auth-exempt set, object policy, docs exposure (#1000), and admin-gating. (2026-06-25: executable route policy inventory added with registered-route coverage tests; auth public/admin helpers now delegate to the inventory for the current credential-management gate. Core `AuthorizationPolicy` / `Action` / `Resource` / `Decision` / `Obligation` contract added in `matric-core` with `AllowAllPolicy`, `RoleBasedPolicy`, tenant matching, and MCP-vs-REST scope-family tests. API route inventory now builds concrete policy inputs and attaches them to protected request extensions. API stack invokes `AuthorizationPolicy` from those request extensions with `AllowAllPolicy` preserving current behavior and deny/indeterminate/error decisions failing closed as 403. Multi-tenant startup now selects `RoleBasedPolicy`; personal mode keeps `AllowAllPolicy`; representative REST write, credential-management admin, backup-restore admin, and realtime transport allow/deny coverage exercises the real policy bridge. Realtime transport policy inputs require MCP transport scope. The in-process `RoleBasedPolicy` evaluation target is documented and covered by a regression budget test. Auth exemption consumers now keep system-health and realtime provider callbacks public while leaving inline-proof webhooks method-aware. Incoming webhook receiver policy inputs now mirror that method boundary: public signed `POST` stays inline-proof while GET/PATCH/DELETE management on the same slug route carries admin `webhook_control` scope metadata. Route-param resource IDs now carry explicit unnormalized-candidate metadata so future object-policy and audit consumers do not mistake path parameters for backing-store verified resource bindings. Authorization allow/deny/error decisions now emit best-effort `fortemi.audit` records with policy id/version, sanitized subject/resource/action metadata, and deny reason codes. Hosted `RoleBasedPolicy` now fails closed for route-param resources marked as requiring backing-store normalization, while personal `AllowAllPolicy` preserves current local behavior. Note route UUIDs now run a backing-store existence lookup before hosted policy evaluation and only clear the unnormalized-resource guard when the note exists; missing/invalid/lookup-error paths remain fail-closed. Attachment route UUIDs now resolve through file storage and carry parent note metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error paths remain fail-closed and note-scoped attachment routes do not misread note IDs as attachment IDs. Archive/memory route names now resolve through the archive registry and carry archive id/schema/default metadata before clearing the hosted unnormalized-resource guard; missing/lookup-error paths remain fail-closed and collection routes skip name lookup. Collection route UUIDs now resolve through the collection repository and carry id/name/parent/note-count metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error/root-route paths remain fail-closed or lookup-free, and note move routes do not misread note IDs as collection IDs. Template route UUIDs now resolve through the template repository and carry safe id/name/format/default-tag-count/collection metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error/collection/unrelated document-type routes remain fail-closed or lookup-free. Document-type route names now resolve through the document-type catalog and carry id/category/system/active/attachment metadata before clearing the hosted unnormalized-resource guard; missing/lookup-error paths remain fail-closed and collection routes skip name lookup. Job route UUIDs now resolve through the job queue and carry job type/status/parent-note/cost-tier metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error paths remain fail-closed and collection routes skip job lookup. Outbound webhook-control route UUIDs now resolve through the webhook repository and carry safe webhook state/retry metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error paths remain fail-closed, collection routes skip lookup, and incoming receiver slugs are not treated as control IDs. Inbound-source route names now resolve through the inbound source registry and carry safe source id/name/kind/enabled metadata before clearing the hosted unnormalized-resource guard; missing/lookup-error paths remain fail-closed and collection routes skip name lookup. PKE keyset `name_or_id` routes now resolve through the keyset repository, carry safe id/name/address/label metadata while keeping key material out of policy metadata, and keep missing/lookup-error/collection/unrelated inbound routes fail-closed. Credential-management API-key route UUIDs now resolve through the OAuth repository and carry safe id/prefix/name/scope/activity/expiration/rate-limit metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error/collection/unrelated PKE routes remain fail-closed or lookup-free. Taxonomy concept route UUIDs now resolve through the SKOS repository and carry safe concept id/primary-scheme/status/count metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error/collection/scheme/secondary-target routes remain fail-closed or lookup-free until their own bindings are explicit. Taxonomy scheme route UUIDs now resolve through the SKOS repository and carry safe scheme id/notation/version/activity/system/date-presence metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error/collection/concept routes remain fail-closed or lookup-free. Taxonomy collection route UUIDs now resolve through the SKOS repository and carry safe collection id/scheme/ordered/date-presence metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error/root/concept/scheme routes remain fail-closed or lookup-free. Taxonomy relation delete routes now verify both source and target SKOS concepts and carry safe endpoint ids/schemes/relation-type metadata before clearing the hosted unnormalized-resource guard; missing/invalid/lookup-error and relation collection routes remain fail-closed or lookup-free. The remaining handler-local admin helper is documented as transitional defense in depth delegated to the route inventory.)
- [~] **#967** RFC 9457 error contract (`application/problem+json`) — *decided: clean pre-GA break.* Centralized `ProblemType` registry at `ApiError::into_response()`; internal-cause vs public-detail split; `request_id` extension; one `assert_problem()` test helper. Evaluate `problem_details` crate (axum 0.8). (2026-06-25: first boundary slice added in-house RFC 9457 `ProblemType` / `ProblemDetails` mapping for `ApiError`, returns `application/problem+json`, redacts raw `Database`/`Internal` details and blob storage paths from client bodies, and covers representative regressions. Request-id slice adds middleware that copies `x-request-id` into RFC 9457 bodies and exposes the header to browser clients. OAuth server-error slice maps `OAuthApiError` database/internal server failures to generic problem responses without raw OAuth/database details. Auth/rate-limit middleware slice maps 401/403/429 middleware failures to problem responses and keeps scope/policy reasons out of client bodies.)
- [~] **#910** AuditEvent / TracingSink / bounded AuditBuffer — audit baseline. (2026-06-25: first core slice added `AuditEvent`, `AuditSink`, `TracingSink`, bounded `AuditBuffer`, event failure policies, pre-buffer redaction/sanitization, a low-risk process startup producer, and a mapping note for localized audit tables.)
- [ ] **#968 / #974** hosted secret inventory + telemetry redaction taxonomy.
- [ ] **#926 / #928 / #933** fail-closed startup: require issuer in multi-tenant; reject invalid security booleans; validate rate-limit env before constructing limiter.

## Phase 1 — Open BSL desktop GA  `tier/open-build`

**Gating:** Phase 0 contracts (partial — error shape, config validation). **Parallel to Phase 2.**
Single-user desktop product; no multi-tenancy or advanced OAuth.

- [ ] **#884** finish ADR-094 sidecar AuthContext + middleware coverage; basic MCP any-client on the user's own data via local/API-key auth.
- [ ] **#950** webhook receive/validate hardening (per-route body cap, validate-public decision, 401/404 uniformity).
- [ ] **#994 / #970 / #922** attachment & upload hardening (pre-validation buffering bounds; malware/scan gate + quarantine; TUS finalization).
- [ ] **#995 / #976 / #979 / #929 / #975 / #885** embeddings & search correctness (effective-embedding fingerprint; all-or-nothing `embed_texts()`; transaction rollback; query-provider resolver; cache lineage; semantic filter parity).
- [ ] **#971 / #909 / #931** jobs: delayed-retry/backoff + poison quarantine; readiness probe + graceful SIGTERM; fail-closed on unknown job type/status.
- [ ] **#927 / #980** backup/restore: operator-local restore (break-glass), strict `backup.conf` parser, restore drills, verified-artifact-only.
- [ ] **#989 / #937 / #992 / #990 / #973 / #982** Docker bundle local-first (127.0.0.1 default, generated DB secret, rendered-config preflight, image pinning, autoheal behind `ops-autoheal` profile, no default socket mount).
- [ ] **#997 / #1002** observability: hosted-safe logging defaults + LOG_FORMAT contract.
- [ ] **#1004 → #999 / #1001 / #998 / #1000** docs-contract runner (advisory+baseline → blocking; redacted output; portable, CI-owned) + leakage guard.

## Phase 2 — Licensed-server / hosted multi-tenant GA  `tier/licensed-server`  (milestone #62)

**Gating:** Phase 0. **Parallel to Phase 1.** This is the hosted/enterprise product.

- [ ] **Tenant isolation / RLS** — ADR-090 shared-schema RLS: #726 / #728 / #729 / #733. *(The isolation floor for everything below.)*
- [ ] **Object-level authorization** #956–#963 — note CRUD/status/versions/sharing, attachments, collections/templates/export, provenance, SKOS governance, memory admin/federated search, document-type use, ad-hoc AI gating.
- [ ] **Control-plane admin-gating** #945–#964 (incl. #946 inference config, #954 queue control, #955 graph/embedding control, #949 webhook mgmt, #978 backup inventory).
- [ ] **Advanced OAuth** (decided 2026-06-23): #1003 capabilities/profile source-of-truth → #943 (P0) consent/redirect → #972/#944 CIMD-first + gated DCR → #941 PKCE/S256 + client-auth → #917 resource/audience (API+MCP) → #924 scope cleanup → #1005 same-client introspection/revocation.
- [ ] **KMS** #897 (P0) AWS launch contract + #910; #911/#912 (Vault/GCP) deferred. **PKE** #947/#948.
- [ ] **Privacy/DSAR** #900 / #969 / #892 / #961 (retention); **inbound connectors** #988 / #920 / #968; **realtime/Twilio** #952 / #981 / #986 / #951 / #953.

## Phase 3 — Universal Model Gateway / Bridge  (milestones #60 foundation, #61 expansion)

**Gating:** Phase 0; benefits from Phase 2 auth/metering.

- [ ] **#873** canonical protocol-adapter framework → **#864/#865** chat (non-stream/stream) → **#866** `/v1/models` (strict OpenAI shape) → **#867** typed `RoutePlan`/`BridgeResolvedModel`.
- [ ] Metering/cost #877/#878/#879/#880; per-consumer policy #870/#871; session logging #868.
- [ ] Provider/cache/privacy #985 (strip external router cache; retention=eligibility; coarse classes) / #969.
- [ ] Provider expansion (#61, blocked on foundation): #874 Anthropic, #875 Gemini, #876 vLLM/LiteLLM/Azure/Bedrock, #869 embeddings.

## Phase 4 — Referenced (BYO) storage  (milestones #58 follow-up, #59 v2 deferred)

**Gating:** design P0s first. Large workstream (~70 issues).

- [ ] **Design P0s** #890 (root-handle vs canonical-path) / #902 (error contract) / #903 (scanner ignore/secret registry) / #904 (derived layout/symlink-safe cleanup) / #905 (clone/export/import/backup semantics).
- [ ] **Core impl** backend/source (#748/#749/#751), registry/migration (#752/#753/#754/#755), scan walker + hash + ingest (#757/#758/#760/#761/#762/#763), quarantine (#759/#765/#776), API (#771/#773/#774/#775), MCP (#777/#778/#779).
- [ ] **Security regression suite** — epic #746, #780/#781/#782/#783, CI gate #797.
- [ ] **Docs/ops** — epic #747, #784–#789, #798–#802.

## Phase 5 — Streaming realtime (#63) + Native distribution (#64)

**Gating:** can proceed alongside; distribution gates final GA packaging.

- [ ] Streaming: #906 (cost/default mode) / #915 / #939 (outbox backpressure + payload minimization) / #896 (ADR refresh).
- [ ] Native distribution: #64 packaging/service lifecycle/CI publish; supply-chain #916/#888/#887/#886; licensing notices #901/#894.

---

## Immediate next actions (decisions + implementation patterns ready)

1. **#1005** — same-client introspection/revocation guard (P1; lands before #917). Plan in comment 71419.
2. **#988** — legacy enabled-row quarantine (first inbound guard PR). Plan in comment 71424.
3. **#967** — RFC 9457 migration (centralized `ProblemType` registry). Pattern in comment 71652.
4. **#710** — stand up the route/action inventory (unblocks Phases 0/1/2).

## Critical-path P0 blockers to schedule

#943 (OAuth consent/redirect) · #897 (AWS KMS contract) · #926 (issuer required) · Referenced-storage design set #890/#902/#903/#904/#905 · #797 (Referenced storage CI gate).

---

## Product decisions captured 2026-06-23 (index → owning issue)

All recorded as "Operator product decision" comments on-issue. Keystones: **#853** (BSL-desktop vs licensed-server boundary), **#967** (RFC 9457).

| Area | Decision | Issue |
|---|---|---|
| **Architecture** | Advanced auth + multi-tenant hosting = licensed-server only, separable from BSL desktop build | #853 |
| Public docs | Minimal curated consumer docs; no public generated schema; OAuth excluded; operator-only schema path | #965 |
| Docs leakage guard | Re-scoped from "public projection" to "no-leakage + operator-gating"; Swagger operator-only, try-it-out off | #1000 |
| Error contract | Adopt RFC 9457 (clean pre-GA break) + centralized registry pattern | #967 |
| Rate limit | No legacy `X-RateLimit-*`; standard `RateLimit`/`Retry-After` only | #898 |
| OAuth tiering | Any client basic MCP; advanced auth licensed-server-only; PKCE/S256, no `plain` | #941 |
| OAuth resource | RFC 8707 resource/audience for API+MCP together | #917 |
| OAuth registration | CIMD-first + gated/deprecated DCR fallback | #972 |
| OAuth scope | Remove `delete` until #710 defines it; one canonical scope source; subset enforcement | #924 |
| OAuth introspect/revoke | Same-client only + separate privileged operator path | #1005 |
| OAuth profiles | Single `OAuthCapabilities` source of truth drives discovery/metadata/fixtures | #1003 |
| Backup restore | Hosted = operator-local break-glass only; verified Fortemi-produced artifacts only | #927 / #978 |
| Backup config | Strict key/value parser (no shell `source`); periodic restore drills as gate | #980 |
| Docker bundle | Local-first default (127.0.0.1, generated secret); rendered-`compose config` preflight | #989 |
| Docker autoheal | Behind explicit `ops-autoheal` profile; no default socket mount | #937 |
| Bridge models | Strict OpenAI `/v1/models`; omit unverifiable models for strict tenants; strict slug resolver | #866 |
| Bridge routing | Typed `RoutePlan`/`BridgeResolvedModel` before any bridge route | #867 |
| Provider cache | Strip external router cache controls; retention = model eligibility; coarse classes to clients | #985 |
| Embeddings | One persisted effective-embedding fingerprint across jobs/query/cache/restore | #995 |
| Embedding validation | Cardinality/order/dimension in `EmbeddingBackend` trait + defensive job-level | #976 |
| Search degrade | FTS fallback with explicit degraded flag + audit (never silent) | #929 |
| MCP legacy SSE | Disabled by default; pin to 2025-11-25 Streamable HTTP | #940 |
| MCP test harness | In-process app/session-manager factory in main `npm test` | #899 |
| MCP tool output | Structured metadata in hosted (no live-token curl); scanner-safe placeholders local | #987 |
| Streaming auth | Distinct short-lived `<STREAM_TOKEN>` class; no ordinary tokens in query | #953 |
| DSAR receipts | Metadata + TTL only; never recoverable cached payload | #900 |
| Tollbooth | Generic OpenAI-compatible proxy docs; Tollbooth as one example only | #983 |
| Docs placeholders | Scanner-safe placeholders enforced everywhere incl MCP docs + generated examples | #999 |
| Default creds | None as guidance; only local-dev/test fixtures allowlisted | #1001 |
| Docs-contract | Advisory+baseline → blocking; redacted output; portable; CI-owned | #1004 |

## References
- Memory: `bsl-desktop-vs-licensed-server-boundary`, `fortemi-rfc9457-error-contract`.
- Milestones: #60 Bridge foundation · #61 Bridge expansion (blocked) · #62 Hosted auth & multi-tenancy launch gate · #63 Streaming realtime phase 1 · #64 Native server distribution · #58 Referenced storage follow-up · #59 Referenced storage v2 (deferred).
- Audit history: `.aiwg/working/new-agent-handoff-prompt-2026-06-23.md`, `.aiwg/working/issue-audit-agent-handoff-2026-06-23.md`.

## Progress log

- 2026-06-25 — Roadmap handoff clarified: `.aiwg/planning/roadmap.md` is the main operating project plan for future agents; `fortemi-roadmap-skill` is the resume/advance procedure.
- 2026-06-25 — #710 route/action inventory slice landed: `matric-api` now has an executable route policy inventory, coverage tests against registered Axum routes, and auth helper delegation for public routes plus credential-management admin gating.
- 2026-06-25 — #710 core policy contract slice landed: `matric-core` now exports `AuthorizationPolicy`, action/resource/decision/obligation types, `AllowAllPolicy`, and a starter `RoleBasedPolicy` proving MCP transport scope does not imply REST write.
- 2026-06-25 — #710 policy-input handoff slice landed: route policy rows now build `Action`/`Resource`/`AuthzContext` inputs and auth middleware attaches them to protected request extensions without enforcing hosted policy yet.
- 2026-06-25 — #710 authorize middleware slice landed: API stack now invokes `AuthorizationPolicy` from protected request extensions with `AllowAllPolicy` preserving current behavior and deny-path coverage proving fail-closed 403 handling.
- 2026-06-25 — #710 hosted policy selection slice landed: `FORTEMI_MULTI_TENANT=true` now selects `RoleBasedPolicy`, personal mode keeps `AllowAllPolicy`, and representative REST mutation allow/deny tests exercise the real policy bridge.
- 2026-06-25 — #710 admin/high-risk policy coverage slice landed: credential-management and backup-restore routes now have real `RoleBasedPolicy` allow/deny tests, and the remaining handler-local admin helper is documented as transitional defense in depth delegated to route inventory.
- 2026-06-25 — #710 realtime transport scope slice landed: realtime transport policy inputs now require MCP transport scope, with `RoleBasedPolicy` tests proving read scope is denied and `mcp` scope is allowed.
- 2026-06-25 — #710 policy performance target slice landed: documented the in-process `RoleBasedPolicy` average evaluation budget and added a regression test to catch high-latency policy work.
- 2026-06-25 — #967 RFC 9457 boundary slice landed: `ApiError` now emits `application/problem+json` with a Fortemi problem-type registry and redacts raw internal/database/blob storage diagnostics from client bodies.
- 2026-06-25 — #710 auth-exemption consistency slice landed: route-policy public checks now keep system-health and realtime provider callbacks public while preserving method-aware inline-proof webhook exemption.
- 2026-06-25 — #967 request-id slice landed: problem responses now include the propagated `x-request-id` as the RFC 9457 `request_id` extension member, with CORS exposure for browser clients.
- 2026-06-25 — #967 OAuth server-error slice landed: `OAuthApiError` database/internal `server_error` responses now use generic RFC 9457 problem details instead of exposing raw OAuth/database diagnostics.
- 2026-06-25 — #967 auth/rate-limit middleware slice landed: central auth, authorization-policy, admin-scope, and rate-limit middleware failures now return RFC 9457 problem details without serializing scope strings or policy reasons.
- 2026-06-25 — #710 resource-normalization metadata slice landed: route-policy inputs now mark route-param IDs as unnormalized candidates and flag object/admin/high-risk resources that still require backing-store normalization before final object-policy enforcement.
- 2026-06-25 — #910 core audit baseline slice landed: `matric-core` now exposes `AuditEvent`, `AuditSink`, `TracingSink`, bounded `AuditBuffer`, failure policy and redaction tests, plus `matric-api` emits a low-risk process startup audit event.
- 2026-06-25 — #710 authorization-audit slice landed: authorization allow/deny/error decisions now emit sanitized `fortemi.audit` events carrying policy id/version, action/resource metadata, and deny reason codes without scope strings or raw policy errors.
- 2026-06-25 — #710 hosted resource-normalization guard slice landed: hosted `RoleBasedPolicy` now denies unnormalized route-param resources before policy evaluation and audits `InvalidResource`, while personal `AllowAllPolicy` remains compatible.
- 2026-06-25 — #710 note resource-normalization slice landed: note route UUIDs now run a backing-store existence lookup before hosted policy evaluation and clear `resource_id_normalized` only for existing notes; missing, invalid, or lookup-error paths keep the hosted fail-closed guard.
- 2026-06-25 — #710 attachment resource-normalization slice landed: attachment route UUIDs now resolve through file storage, attach parent note metadata, and clear `resource_id_normalized` only for existing attachments; missing, invalid, lookup-error, and note-scoped attachment routes keep the hosted fail-closed guard.
- 2026-06-25 — #710 archive resource-normalization slice landed: archive and memory route names now resolve through the archive registry, attach archive id/schema/default metadata, and clear `resource_id_normalized` only for existing archives; missing, lookup-error, and collection routes keep the hosted fail-closed guard.
- 2026-06-25 — #710 document-type resource-normalization slice landed: document-type route names now resolve through the catalog, attach id/category/system/active/attachment metadata, and clear `resource_id_normalized` only for existing document types; missing, lookup-error, and collection routes keep the hosted fail-closed guard.
- 2026-06-25 — #710 job resource-normalization slice landed: job route UUIDs now resolve through the job queue, attach job type/status/parent-note/cost-tier metadata, and clear `resource_id_normalized` only for existing jobs; missing, invalid, lookup-error, and collection routes keep the hosted fail-closed guard.
- 2026-06-25 — #710 webhook-control resource-normalization slice landed: outbound webhook route UUIDs now resolve through the webhook repository, attach safe state/retry metadata, and clear `resource_id_normalized` only for existing webhooks; missing, invalid, lookup-error, collection, and incoming receiver routes keep the hosted fail-closed/control-boundary behavior.
- 2026-06-25 — #710 inbound-source resource-normalization slice landed: inbound connector route names now resolve through the inbound source registry, attach safe source id/name/kind/enabled metadata, and clear `resource_id_normalized` only for existing sources; missing, lookup-error, collection, and unrelated webhook-control routes keep the hosted fail-closed guard.
- 2026-06-25 — #710 PKE keyset resource-normalization slice landed: PKE keyset route `name_or_id` values now resolve through the keyset repository, attach safe id/name/address/label metadata, and clear `resource_id_normalized` only for existing keysets; missing, lookup-error, collection, and unrelated inbound routes keep the hosted fail-closed guard.
- 2026-06-25 — #710 collection resource-normalization slice landed: collection route UUIDs now resolve through the collection repository, attach safe id/name/parent/note-count metadata, and clear `resource_id_normalized` only for existing collections; missing, invalid, lookup-error, root, and note-move routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 template resource-normalization slice landed: template route UUIDs now resolve through the template repository, attach safe id/name/format/default-tag-count/collection metadata, and clear `resource_id_normalized` only for existing templates; missing, invalid, lookup-error, collection, and unrelated document-type routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 credential-management resource-normalization slice landed: API-key route UUIDs now resolve through the OAuth repository, attach safe id/prefix/name/scope/active/expiration/rate-limit metadata, and clear `resource_id_normalized` only for existing API keys; missing, invalid, lookup-error, collection, and unrelated PKE routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 taxonomy concept resource-normalization slice landed: SKOS concept route UUIDs now resolve through the SKOS repository, attach safe id/primary-scheme/status/count metadata, and clear `resource_id_normalized` only for existing concepts; missing, invalid, lookup-error, collection, scheme, and secondary-target routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 taxonomy scheme resource-normalization slice landed: SKOS scheme route UUIDs now resolve through the SKOS repository, attach safe id/notation/version/activity/system/date-presence metadata, and clear `resource_id_normalized` only for existing schemes; missing, invalid, lookup-error, collection, and concept routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 taxonomy collection resource-normalization slice landed: SKOS collection route UUIDs now resolve through the SKOS repository, attach safe id/scheme/ordered/date-presence metadata, and clear `resource_id_normalized` only for existing collections; missing, invalid, lookup-error, root, concept, and scheme routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 taxonomy relation resource-normalization slice landed: SKOS relation delete routes now verify both source and target concepts, attach safe endpoint id/scheme/relation-type metadata, and clear `resource_id_normalized` only when both endpoints exist; missing, invalid, lookup-error, and relation collection routes keep the hosted fail-closed or lookup-free boundary.
- 2026-06-25 — #710 incoming webhook receiver method-boundary slice landed: route policy inputs now keep signed receiver `POST` calls public inline-proof while classifying GET/PATCH/DELETE management calls on the same slug route as admin `webhook_control` actions, matching the auth-exemption boundary.

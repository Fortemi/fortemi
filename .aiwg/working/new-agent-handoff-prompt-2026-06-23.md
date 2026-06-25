# New Agent Handoff Prompt

```text
You are continuing an ongoing Fortemi issue-audit thread in /home/roctinam/dev/fortemi.

User objective:
Continue the detailed open-issue audit. Use AIWG skill discovery, current repo/tracker state, and internet research for best practices where needed. Update, comment, close, defer, rewrite, or create issues when evidence supports it. Keep older issues aligned with current designs and plans. Ask focused planning questions to fill knowledge or planning gaps.

Hard rules:
- Start each audit slice with: aiwg discover "<need>"
- Prefer the discovered issue-audit workflow when it is returned.
- Inspect current repo and current Gitea tracker state before relying on prior notes.
- Tracker: Fortemi/fortemi on Gitea. Authenticated user has been roctibot.
- Current date for planning language is June 23, 2026. Some tracker timestamps may display June 22 because of timezone/context differences; use exact dates when commenting.
- Mutations are allowed, but keep them scoped, evidence-based, and non-duplicative.
- Do not revert dirty worktree changes. Treat existing modified/untracked files as user/generated unless directly relevant.
- rg is unavailable; use grep/find/sed.
- Use multi_tool_use.parallel for independent reads where practical.
- Browse current external sources when citing best practices, volatile facts, third-party APIs, security/privacy guidance, or current standards. Cite links in summaries.
- Do not mark the persistent goal complete. This is ongoing backlog audit work.
- End each work slice with concise status and focused planning questions.

Fresh-agent bootstrap:
1. `cd /home/roctinam/dev/fortemi`
2. `aiwg discover "issue-audit continue Fortemi open issue audit implementation planning"`
3. `aiwg show skill issue-audit`
4. `git status --short`
5. Read this file, then skim `.aiwg/working/issue-audit-agent-handoff-2026-06-23.md` only for older context.
6. Pick one current slice from the "Recommended next audit slices" section unless the user redirects.
7. Re-read the live Gitea tracker comments for the issue cluster you pick before touching the tracker.
8. Inspect the current repo code/docs for that cluster before relying on this handoff.

Immediate objective for the new agent:
- Continue the issue audit without duplicating already-posted findings. The highest-signal next work is implementation planning or implementation follow-through, not another broad audit pass.
- The embeddings/search provider-routing slice is now audited through #995 comment 71477. Do not repost response-cardinality/job-zip truncation findings unless implementation or product decisions change.
- Good next slices are bridge route/catalog implementation follow-through, MCP harness/legacy-SSE implementation follow-through, docs-contract implementation, backup/restore implementation, or another implementation slice where code/product decisions have changed.
- OAuth and inbound connectors already have current first-PR candidates:
  - OAuth: #1005 same-client introspection/revocation guard from comment 71419 can land before #917 resource/audience storage.
  - Inbound connectors: #988 legacy enabled-row quarantine from comment 71424 must be handled before relying only on future default-disabled creation.
- Only comment if fresh code, tracker state, or product decisions reveal a non-duplicative sequencing gap. Otherwise, produce a concrete implementation boundary, open questions, or code changes if requested.

Current continuation state:
- This is an ongoing issue-audit and planning thread, not a finished task. Do not mark any persistent goal complete.
- OAuth is audited/planned through #1005 comment 71419, plus #917 comment 71413, #944 comment 71415, and #1003 comment 71417. Treat #1005 comment 71419, #1003 comments 71261/71296/71417, #917 comments 71265/71219/71413, #924 comment 71295, #941 comment 71216, #944 comment 71415, plus #944/#972 earlier comments as existing state. Re-read live tracker comments before acting.
- MCP is audited through #899 comment 71326 and #940 comment 71320, with prior #917/#918/#914/#921 refinements. Do not repost harness/session-binding/resource/protocol/Origin/legacy-SSE findings unless implementation or product decisions change.
- Bridge route/catalog is audited through #866 comment 71507, #866 comment 71252, and #867 comment 71254. Current code still lacks bridge `/v1/models`, strict bridge resolver, and typed `RoutePlan`, but existing #866/#867/#877/#969/#985 comments cover the gaps, including the second native `/api/v1/chat/models` catalog separation. Remaining useful work is implementation or product decisions, not another audit comment.
- Docs-contract is audited through #1004 comment 71484, plus #1004 comments 71456/71431/71360/71273, #999 comment 71276, and #1001 comment 71278. Remaining useful work is runner/rule-pack implementation or product decisions around blocking/advisory mode, baseline format, redacted scanner outputs, and detector precision/negative fixtures, not another generic audit comment.
- Inbound connectors are audited/planned through #988 comment 71424, #974 comment 71386, and #988 comment 71388, plus prior #988/#974 comments 71284/71285 and #988/#968/#920 comments 71239/71240/71242. Remaining useful work is implementation or product decisions around legacy enabled-row quarantine, activation, redaction, secret refs, destination policy, event admission, DLQ, and public health identity exposure.
- Hosted runtime logging/API error telemetry is audited through #997 comment 71332, #967 comment 71335, and #974 comment 71336.
- Backup/restore hardening is audited/planned through #980 comment 71499, #991 comment 71461, #980 comment 71440, and #978 comment 71442, in addition to #980 71348, #978 71350, and prior #927/#978/#980/#991/#923/#1001 comments. Remaining useful work is implementation or product decisions around hosted restore availability, artifact lifecycle states, collision-safe artifact identity/publication, JSON import/export operation state/idempotency, and redacted operator diagnostics.
- Docker/native distribution hardening is audited/planned through #989 comment 71462, plus #990 comments 70547/70798, #992 comment 71070, and #982 comments 69297/70788. Remaining useful work is implementation or product decisions around rendered Compose preflight, loopback/default exposure profiles, generated/operator-supplied DB secrets, third-party image lock/manifest, and Docker socket/autoheal profile.
- Embeddings/search provider routing, batching, cache lineage, and embedding job transaction behavior are audited through #995 comment 71477, plus #995 body, #976 comments 68597/68694/69370/70410, #979 comments 68926/70103, #975 comments 68606/69373/70636, #682 vector-dimension comments, and #929 query-provider comments. #995 comment 71477 covers response cardinality/index validation and job-level `chunks.zip(vectors)` truncation. Do not repost unless code/product decisions change.
- Bridge/proxy/Tollbooth direction remains: Tollbooth can be referenced as optional local/dev or compatibility proxy tooling, but Fortemi hosted design must keep Fortemi-owned identity, policy, audit, billing, provider routing, destination policy, and privacy boundaries explicit. Do an honest methodology comparison before proposing compatibility language. Do not reuse native `/api/v1/models` or `/api/v1/chat/models` response shapes for bridge `/v1/models`.
- Current MCP implementation order remains: first land either an in-process Express app/session-manager factory or a child-process mocked-introspection harness in the main `npm test` path, then wire #940 legacy-SSE default policy, then #918/#921/#914/#917 coverage. After #899 comment 71326 and recheck sections 43 and 51 below, make that harness protocol-profile-aware so 2025-11-25 sessionful behavior is explicit and future 2026-07-28 stateless behavior is not accidentally implied.

Active resume point for the next agent:
- Start a new audit/planning slice with `aiwg discover "<need>"`. Recommended next work is implementation planning/follow-through, not another broad audit pass.
- Embeddings/search was just updated: #995 comment 71477 owns response cardinality/index validation and job-level silent truncation. Useful next embeddings work is implementation or a failing-test boundary across #995/#976/#979/#975/#682/#929, not another comment.
- MCP has now been rechecked again against live tracker comments and code without a new tracker comment. Do not repeat the MCP audit unless code/product decisions have changed; useful MCP work is implementation planning or implementation.
- OAuth and inbound connectors should be revisited only if implementation has changed or the user asks for those areas. Avoid another generic audit comment in either cluster.
- The immediate OAuth first PR candidate remains #1005 same-client introspection/revocation guard from comment 71419 because it can land before #917 resource/audience storage. Do not grant a broad MCP/resource-server introspection exception until #917 stores protected-resource/audience data.
- The immediate inbound first concern remains #988 legacy enabled-row quarantine from comment 71424; future default-disabled creation alone is not sufficient.
- Avoid reposting generic docs-contract advisory/baseline, one-off-guard migration, scanner-output redaction, broad-pattern false-positive, or `sk-` provider-key boundary comments already covered by #1004 comments 71273/71360/71431/71456/71484, #999 comment 71276, and #1001 comment 71278. Useful next docs-contract work is implementation or actual runner/rule-pack work.
- Avoid reposting generic backup/restore evidence, inventory, upload/restore trust-boundary, artifact-lifecycle, timestamp-collision/artifact-id, or JSON import/export operation-state comments already covered by #980 comments 71348/71440/71499, #978 comments 71350/71442, and #991 comment 71461. Useful next backup work is implementation or product decisions.
- Avoid reposting generic Docker bundle host-bind/default-secret/image-pin/socket comments already covered by #989/#990/#992/#982. The newest Docker refinement is #989 comment 71462: validate rendered `docker compose ... config`, not only raw source text.

Important local artifacts:
- This prompt file is `.aiwg/working/new-agent-handoff-prompt-2026-06-23.md`; read it first.
- `.aiwg/working/issue-audit-agent-handoff-2026-06-23.md` has earlier completed slices and tracker history. It is older background context, not the active resume point.
- The worktree is dirty with many pre-existing modified/untracked files. Do not clean it up unless the user asks.

Completed audit work so far:
- Created #992: fix(docker): replace bundle default database credentials with generated or operator-supplied secrets.
- Created #993: fix(oauth): remove unsafe Debug output from token and client-secret DTOs.
- Created #994: fix(attachments): bound pre-validation buffering for upload and provider-media ingestion paths.
- Created #995: fix(embeddings): route embedding jobs through effective provider/config resolver.
- Created #996: fix(ingress): trust forwarded host/proto/IP headers only from configured proxies.
- Commented bridge/proxy/Tollbooth issues #872, #864, #920, #983. Boundary: Tollbooth is optional local/dev compatibility or research proxy only, not a Fortemi hosted dependency.
- Commented browser/public API surface issues #964, #965, #967. #966 already covered CORS/CSRF/browser policy.
- Commented #967 on API error shape: current request-id middleware exists, but ApiError::into_response() still returns `{ "error": message }`; raw Database/Internal strings and BlobMissing path fields remain.
- Commented #965 on OpenAPI/AsyncAPI inventory: avoid a separate manual schema allowlist; public schema filtering should be projected from the #710 route/action inventory. Current public docs routes expose a broad static ApiDoc path list.
- Commented #955 on admin/operator graph diagnostics: classify diagnostics as policy-sensitive, not harmless reads.
- Commented #961 on retention/deletion: attachment delete semantics must include derived artifacts and DSAR receipts.
- Commented #713 and #714 on hosted quota/provider cost: include realtime/audio provider spend in the shared UsageMeter and admission/quota matrix.

Recent completed slices:

1. Tollbooth / bridge proxy env scoping
- Ran aiwg discover "issue-audit bridge proxy Tollbooth compatibility methodology OpenAI compatible gateway".
- Inspected #864/#872/#873/#920/#983 plus code/docs.
- Found that reqwest enables system proxy environment variables by default and the repo has several direct Client::new()/Client::builder() call sites without a central outbound client policy wrapper.
- Posted #983 comment 70899: Tollbooth proxy env vars must be scoped to the client/agent process or disposable client container, not the Fortemi API/server process. Warn about global env in shared shells/compose/systemd; use NO_PROXY/process isolation.
- Posted #920 comment 70900: outbound destination policy must decide whether each destination profile honors system proxies, disables them with ClientBuilder::no_proxy(), or uses an operator-approved outbound proxy profile.
- Sources checked: https://github.com/FlechetteLabs/Tollbooth, https://github.com/FlechetteLabs/Tollbooth/blob/main/docs/security.md, https://docs.rs/reqwest/latest/reqwest/

2. Bridge debug/content capture and replay policy
- Ran aiwg discover "issue-audit bridge debug content capture replay privacy telemetry redaction session logs".
- Inspected #984/#868/#900/#974 and current chat streaming/event replay code.
- Code evidence: native chat streaming persists ChatStreamFrame::delta() JSON content into ChatStreamStore; resume_chat_stream() replays stored frames verbatim; EventBus has in-memory SSE replay buffer; incoming webhook idempotency can replay cached response bodies.
- Posted #984 comment 70908: include temporary replay/resumption buffers as a first-class sink in bridge debug/content-capture policy. Use `ephemeral_replay_buffer` / `resumption_buffer` as distinct classifications from notes/traces/reports/raw debug.
- Posted #974 comment 70909: telemetry for replay/resumption should expose bounded metadata only, not raw frame data/session ids/prompts/deltas/tool args/provider blobs unless #984 authorizes protected capture.
- Sources checked: OpenTelemetry GenAI observability, OpenTelemetry sensitive data/security guidance, OWASP Logging Cheat Sheet, OWASP LLM Top 10.

3. Webhook idempotency/replay cache
- Ran aiwg discover "issue-audit webhook idempotency replay Redis retention secrets logging privacy".
- Inspected #949/#950/#925/#974, crates/matric-api/src/services/idempotency_store.rs, and receive_incoming_webhook() in crates/matric-api/src/main.rs.
- Code evidence: IdempotencyStore uses keys like idem:{slug}:{key} for 24h by default; stores body_hash, response_status, and response_body; accepted webhook response includes slug, provider, schema_ref, parsed payload, and side_effect, so Redis can retain accepted inbound payload content for FORTEMI_IDEMPOTENCY_TTL.
- Posted #950 comment 70911: idempotency cache is a temporary payload-retention sink. Define `temporary_replay_cache`, prefer minimal accepted envelope instead of full parsed payload, move away from raw caller-key Redis keys, add fixtures for oversized/control/secret/path-like keys, and document Idempotency-Key as an expired draft/convention rather than finalized RFC.
- Posted #900 comment 70912: add incoming webhook idempotency/replay cache rows to DSAR/retention matrix, including scope, key fingerprint policy, payload minimization, TTL/expiry, Redis encryption/access assumptions, export/access, erasure before TTL, Redis persistence snapshots/backups, and receipt wording.
- Sources checked: Stripe idempotent requests, GitHub webhook best practices, Stripe webhooks, IETF Idempotency-Key draft.

4. Realtime provider-media RecordingUrl sink audit
- Ran aiwg discover "issue-audit webhook Redis event outbox retention backpressure replay privacy", then inspected realtime/logging sinks.
- Inspected #915/#939/#593/#900/#974/#952/#981/#986 and crates/matric-api/src/main.rs.
- Code evidence: receive_incoming_webhook() applies side effects, emits generic incoming_webhook.received outbox payload including parsed payload and side_effect, returns accepted response including parsed payload and side_effect, and caches that response body in Redis when Idempotency-Key exists. apply_twilio_voice_webhook() / queue_twilio_recording_transcription() store raw RecordingUrl in note metadata, session metadata, and job payload. call_event_outbox_for_control_event() copies RecordingAvailable { url } to call_event payload.
- Posted #981 comment 70920: expand raw RecordingUrl cleanup beyond note/session/job metadata; do not copy raw/signed URLs into call_event, incoming_webhook.received, accepted webhook responses, idempotency replay caches, logs, audit events, transcript metadata, or DSAR exports. Prefer RecordingSid/CallSid plus provider account/receiver scope; keep only redacted URL metadata/hash/fetch decision/scan/deletion status if needed.
- Posted #900 comment 70921: DSAR matrix must model transient provider-media RecordingUrl, durable provider object refs, local audio/transcript artifacts, and temporary replay/cache sinks separately. Suggested receipt categories include raw_provider_url_not_retained, provider_object_ref_retained_for_deletion_coordination, local_recording_deleted, provider_delete_requested, provider_delete_confirmed, provider_manual_action_required, backup_beyond_use_until_expiry.
- Sources checked: Twilio Recording API, Twilio recording callbacks help article, OWASP Logging Cheat Sheet, OpenTelemetry sensitive-data guidance.

5. Hosted logging defaults / LOG_FORMAT drift
- Ran aiwg discover "issue-audit hosted logging defaults RUST_LOG LOG_FORMAT telemetry redaction sensitive data".
- Inspected crates/matric-api/src/main.rs, docs/content/configuration.md, docs/content/operations.md, Docker/service examples, and #974.
- Code evidence: logging init defaults LOG_FORMAT to text; only exact json selects JSON and all other values silently use text. RUST_LOG fallback is matric_api=debug,tower_http=debug. Docs conflict: configuration.md says RUST_LOG default info and LOG_FORMAT pretty with pretty/json/compact; operations.md says runtime default matric_api=debug,tower_http=debug and LOG_FORMAT=text.
- Created #997: fix(observability): make logging defaults and format config hosted-safe and docs-consistent.
- Added labels to #997: P1: High, bug, documentation, effort/small, infrastructure, security, testing.
- Posted #974 comment 70934 cross-linking #997 as the narrower logging-default/config guard. #974 remains field-level telemetry classification/redaction/sink policy.
- Sources checked: OWASP Logging Cheat Sheet, OpenTelemetry handling sensitive data, NIST SP 800-92 Rev. 1 draft.

6. Plugin/extension trust boundary and admission policy
- Ran aiwg discover "issue-audit plugin extension trust boundary sandboxing supply chain hosted tenant".
- Inspected #935/#722/#712/#716/#721 plus plugin docs: docs/architecture/adr/ADR-088-plugin-architecture-strategy.md, .aiwg/architecture/plugin-contract-spec.md, .aiwg/process/plugin-certification.md, .aiwg/templates/ee-plugin-crate/*, ADR-095/096.
- Existing #935 covers Class A default vs Class B/C sidecar exceptions and notes plugin trait/contract code/protos are not implemented. Existing #722 covers broad signing/certification infrastructure, SLSA v1.2, Sigstore blob signing/verify-bundle, Class A vs sidecar distinctions, and Cargo registry checksum limits.
- Gap found: plugin-certification.md says certification is not mandatory for a plugin to function; ADR-095 permits community plugins; the template targets Class A and publishes to fortemi-ee; there is no explicit hosted admission policy matrix by deployment mode, surface, and trust tier.
- Posted #722 comment 70942: add explicit plugin admission policy by deployment mode and surface. Include deployment modes local_dev, self_hosted_single_tenant, customer_enterprise, Fortemi_hosted, regulated_hosted; surfaces such as AuthorizationPolicy, AuditSink, KeyProvider, UsageMeter/QuotaPolicy, OAuthProvider, extraction/content processor, job backend, MCP gate/tool extension, search/vector backend, backup/archive provider; minimum accepted trust tiers operator_private, author_signed, verified, certified, first_party_only. Hosted high-risk control-plane surfaces should require certified or first_party_only; self-hosted may allow operator_private with local trust root/audit record.
- Posted #935 comment 70944: cross-link trust-tier admission with Class A vs Class B/C sidecar decisions. Sidecar is not automatically safe; Class A first-party can be safe if release-time verification/certification is strong enough.
- Sources checked: SLSA v1.2 build requirements/provenance, Sigstore Cosign blob/container verification docs, Cargo alternate registry/registry index docs.

7. MCP/tool-output secret leakage refinement
- Ran aiwg discover "issue-audit MCP bridge tool output secret leakage bearer token API key redaction".
- Inspected #987/#993/#968/#899/#974/#953/#913 and mcp-server/index.js, mcp docs, and MCP tests.
- Code evidence: apiRequest() attaches session bearer or fallback FORTEMI_API_KEY at mcp-server/index.js:101-115; on upstream non-2xx it includes raw upstream response body in thrown Error at lines 135-147; the top-level MCP tool handler serializes error.message into model-visible tool result content with isError=true at lines 3539-3543; get_documentation streaming docs include Authorization: Bearer mm_at_xxx and ?token=mm_at_xxx examples at lines 5560-5574.
- Posted #987 comment 70946: treat MCP tool execution errors and returned documentation as tool-output secret surfaces; do not include raw upstream REST response bodies by default; add regression coverage for synthetic secrets/control chars in errors/logs/docs/curl output; coordinate with #953 for query-token examples.
- Posted #968 comment 70948: add MCP content, structuredContent, annotations, embedded resource links, docs strings, tool execution errors, JSON-RPC errors, stdout/stderr, and transcript/export copies to the hosted secret inventory.
- Posted #899 comment 70951: extend MCP regression suite to cover tool execution errors and documentation output, not only successful curl_command responses.
- Sources checked: MCP 2025-11-25 tools spec, MCP security best practices, OWASP REST sensitive information in HTTP requests, OWASP Secrets Management Cheat Sheet, NSA MCP Security Design Considerations May 2026.

8. Runtime inference config / audit endpoint refinement
- Ran aiwg discover "issue-audit runtime inference config audit endpoint provider secret redaction hosted admin".
- Inspected #946/#968/#974/#963/#920/#730/#731 and crates/matric-api/src/handlers/inference_config.rs.
- Code evidence: UpdateInferenceConfigRequest and provider partials derive Debug while containing provider api_key/base_url fields (lines 120-183); TestConnectionRequest derives Debug with base_url/api_key (1869-1879); redact_api_key shows first 8 chars or full short values (232-243); write_audit_row hard-codes changed_by="anonymous" and callers mostly pass source_ip None (252-270); atomic probes build/log/return full URLs and raw probe errors in probe_failures (1238-1337); test_connection logs full base_url, probes after string-prefix URL validation, logs probe failures, and returns model names/capabilities (1952-2061); audit endpoint returns before_json/after_json/changed_by/source_ip rows (1766-1858).
- Posted #946 comment 70959: admin/actor/probe acceptance refinement. Handlers should consume AuthContext/principal for audit rows, separate UI masks from audit fingerprints, remove/redact Debug, return stable probe reason codes, and test short-key/URL/provider-error stripping.
- Posted #968 comment 70961: add representation taxonomy (`ui_display_mask`, `operator_fingerprint`, `presence_flag`, `secret_reference_id`, `raw_secret_material`) and classify inference config request/debug surfaces.
- Posted #974 comment 70964: add telemetry fixtures for atomic probe failures, test_connection URLs/model lists, DB/audit errors, CRLF/control chars, credential-bearing URLs, internal hostnames, raw provider errors, and safe provider/profile ids.
- Sources checked: OWASP Authorization Cheat Sheet, OWASP API5:2023 Broken Function Level Authorization, OWASP Logging Cheat Sheet, OpenTelemetry handling sensitive data.

9. Backup/restore command stderr and script secret handling
- Ran aiwg discover "issue-audit backup restore stderr secret handling pg_dump PGPASSWORD temp scratch logs hosted".
- Inspected #927/#978/#980/#974 and scripts/backup.sh plus restore code in crates/matric-api/src/main.rs.
- Code evidence in scripts/backup.sh: `/etc/fortemi/backup.conf` is loaded with `source`, so it is executable shell; defaults include `PGUSER=matric`, `PGPASSWORD=matric`, `PGHOST=localhost`, `PGDATABASE=matric`; `BACKUP_TEMP_DIR` defaults to `/tmp/fortemi-backup`; pg_dump, rsync, and S3 CLI stderr/stdout are copied into logs; raw `BACKUP_REMOTE_RSYNC` is logged/used; remote cleanup interpolates the remote path into an ssh command; verification only checks local file existence and size >= 1024 bytes.
- Code/docs evidence in API restore: memory_scoped_restore() and database_backup_restore() invoke `psql -U matric -h localhost -d matric` with `PGPASSWORD=matric`; full restore hardcodes post-restore SQL for database `matric`; raw post-restore stderr is logged and can be returned in API response messages. Backup docs still show `/etc/Fortemi/backup.conf`, `PGPASSWORD=matric`, and inline restore commands with default credentials.
- Posted #980 comment 70967: scheduled backup script refinement. Treat backup.conf as executable operator code unless replaced with a strict parser; production validation should deny default credentials/default `/tmp`; verify scratch dir safety; add checksum/manifest per destination; use `pg_restore --list`; make verbose logs redaction-aware; harden remote retention deletion.
- Posted #927 comment 70968: API restore trust-boundary refinement. Use configured maintenance credentials instead of hardcoded `matric/matric/localhost/matric`; derive database/schema for post-restore SQL; return stable status/reason codes and protected diagnostics; add tests for non-default DB names, default denial, and post-restore stderr redaction.
- Posted #974 comment 70970: add backup/restore telemetry fixtures for script CLI outputs, raw psql stderr, backup paths, and destination URIs. Desired shape is metadata only: event, backup/restore id, destination class, tool phase, status/reason, duration/size/count.
- Posted #968 comment 70971: add backup config/maintenance credentials to the secret inventory: executable backup.conf, PGPASSWORD, DATABASE_URL, maintenance credentials, S3 credentials, rsync SSH, age paths, remote URIs, plaintext/compressed/encrypted dump classes, and hosted/native production denial of dev defaults.
- Sources checked: NIST SP 800-34 Rev.1 contingency planning, PostgreSQL pg_restore docs, PostgreSQL CVE-2025-8714, PostgreSQL CVE-2025-8715, OWASP File Upload Cheat Sheet.

10. RateLimit / Retry-After / rate-limit docs-contract guard
- Ran aiwg discover "issue-audit RateLimit Retry-After RateLimit headers hosted quota provider cost".
- Inspected #898/#908/#933/#714 and their comments plus `crates/matric-api/src/main.rs`, `docs/content/api.md`, `docs/content/authentication.md`, `docs/content/use-cases.md`, `docs/content/configuration.md`, and `docs/architecture/adr/ADR-098-per-tenant-rate-limits-quotas.md`.
- Current code/docs evidence remains: runtime uses only `RATE_LIMIT_ENABLED`, `RATE_LIMIT_REQUESTS`, and `RATE_LIMIT_PERIOD_SECS`; constructs a process-wide `governor::RateLimiter::direct(...)`; current 429 path returns JSON only with no `Retry-After`, `RateLimit`, `RateLimit-Policy`, or `X-RateLimit-*`; docs still advertise legacy `X-RateLimit-*`, non-runtime `RATE_LIMIT_PER_MINUTE` / `RATE_LIMIT_PER_TENANT`, and older split `RateLimit-Limit` / `RateLimit-Remaining` / `RateLimit-Reset`.
- External standards check: current HTTPAPI RateLimit document is still `draft-ietf-httpapi-ratelimit-headers-11` (published 2026-05-23, expires 2026-11-24), defining `RateLimit-Policy` and combined `RateLimit` fields; RFC 6585 defines 429 and says responses may include `Retry-After`; RFC 9110 defines `Retry-After` as HTTP-date or delay-seconds; OWASP API4 recommends endpoint-specific resource-consumption limits and provider spending limits.
- Created #998: `test(docs): prevent unsupported rate-limit headers and env vars from reappearing in docs`. Labels applied: P2: Medium, area/rest-api, ci/cd, documentation, effort/small, testing.
- Posted #898 comment 70988 cross-linking #998 and clarifying that #998 is a regression guard only. #898 remains docs/content reconciliation; #908 remains the narrow current `Retry-After` implementation; #933 remains config validation; #714 remains hosted tenant quota enforcement.
- Sources checked: IETF draft-ietf-httpapi-ratelimit-headers-11, RFC 6585, RFC 9110, OWASP API4:2023 Unrestricted Resource Consumption.

11. MCP docs/query-token and scanner-hostile placeholder cleanup
- Ran aiwg discover "issue-audit MCP docs query token cleanup bearer URL examples tool output secrets".
- Inspected #953/#987/#968/#913 and comments plus `docs/content/real-time-events.md`, `docs/content/authentication.md`, `docs/content/job-monitoring.md`, `docs/content/media-integration-guide.md`, `docs/content/mcp-deployment.md`, `docs/content/mcp-troubleshooting.md`, `docs/content/operators-guide.md`, `mcp-server/README.md`, and `mcp-server/index.js`.
- Current code/docs evidence: `real-time-events.md` uses `?token=mm_at_xxx` / `?token=mm_key_xxx` and `Authorization: Bearer mm_at_xxx`; `mcp-server/index.js::get_documentation(topic="streaming")` returns `Authorization: Bearer mm_at_xxx`, `?token=mm_at_xxx`, and `new EventSource('/api/v1/events?types=job&token=mm_at_xxx')`; `authentication.md` includes realistic `mm_key_...`, `mm_at_...`, `registration_access_token`, and `MATRIC_MEMORY_API_KEY=mm_key_dev_test_only_12345`; MCP deployment docs include `MCP_CLIENT_SECRET=secret_xyz789`.
- Existing issue split remains: #953 owns hosted SSE/WS stream auth/query-token compatibility, #987 owns runtime MCP tool-output/error redaction, #968 owns the broad secret taxonomy, and #913 owns MCP protected-resource scope/annotation docs. A narrower docs/example hygiene issue was still useful.
- Created #999: `test(docs): replace scanner-hostile token examples and lint auth placeholders`. Labels applied: P2: Medium, area/auth, ci/cd, documentation, effort/small, mcp-api, testing.
- Posted cross-links: #953 comment 71007, #987 comment 71009, #968 comment 71011, #913 comment 71015.
- #999 acceptance: static docs and MCP-returned docs should use scanner-safe placeholders such as `<ACCESS_TOKEN>`, `<API_KEY>`, `<STREAM_TOKEN>`, `<MCP_CLIENT_SECRET>`; EventSource/query-token docs should explain browser compatibility, short-lived/audience-bound stream tokens, and access-log redaction; CI/docs lint should reject token-shaped examples outside explicit scanner-test allowlists.
- Sources checked: MCP 2025-11-25 tools spec, MCP 2025-11-25 authorization spec, OWASP REST Security Cheat Sheet sensitive information in HTTP requests, OWASP WebSocket Security Cheat Sheet.

12. Provider prompt-cache / provider retention / external router response cache
- Ran aiwg discover "issue-audit provider prompt cache retention AI provider data controls bridge privacy".
- Inspected #985/#900/#969/#877 plus #880/#872/#864 and current provider/router code/docs.
- Current code/docs evidence: no Fortemi prompt-cache policy surface was found beyond unrelated HTTP cache middleware and provider docs; `crates/matric-inference/src/provider_profiles.rs` models provider profiles without cache/retention/data-control fields; `ChatUsage` still tracks only `prompt_tokens`, `completion_tokens`, and `total_tokens`; the OpenRouter profile is a meta-router with default `HTTP-Referer` and `X-Title` headers, but docs expose provider slugs/env vars without retention/cache posture; `CompleteRequest` derives Debug while containing provider/api_key/base_url/messages, which remains covered by #968/#946.
- Posted #985 comment 71023: provider-cache acceptance should distinguish `external_router_response_cache_policy` / `router_response_cache_used`; hosted defaults should strip or reject external router response-cache controls such as `X-OpenRouter-Cache` unless #880/#969 explicitly permit them; model `cache_control_surface` across automatic provider cache, explicit provider prompt cache, router sticky routing, external router response cache, Fortemi exact response cache, and Fortemi semantic response cache; add `regional_cache_constraint`; treat provider/model extended-retention requirements as model eligibility predicates for ZDR/no-retention tenants.
- Posted #880 comment 71025: keep Fortemi-owned response caching separate from external router response caching. OpenRouter beta `X-OpenRouter-Cache` is processor-held response data before provider calls; usage can be zero on cache hits. Track `fortemi_response_cache_hit` separately from `external_router_response_cache_hit`, and classify external response caches separately for DSAR/privacy.
- Posted #877 comment 71027: provider-cache accounting should capture `provider_prompt_cache_surface`, `provider_prompt_cache_retention`, `provider_cache_usage_source`, cache read/write token buckets (`cache_read_tokens`, `cache_write_tokens`, `cache_write_5m_tokens`, `cache_write_1h_tokens`, `cached_input_tokens`), raw usage sidecar, `external_router_response_cache_hit`, `external_router_response_cache_ttl`, and `regional_cache_constraint_satisfied`.
- Posted #969 comment 71028: privacy/subprocessor catalog should distinguish provider prompt caching, router sticky routing, and external router response caching. Rows should include `cache_control_surface`, `retention_control_source`, `effective_cache_retention`, `router_sticky_routing`, `external_response_cache`, `regional_endpoint_or_inference_required`, `tenant_disable_replace_supported`, and `notice_surface`.
- Sources checked: OpenAI prompt caching and data controls docs, Anthropic prompt caching docs, OpenRouter prompt caching/sticky routing docs, OpenRouter response caching docs, OpenRouter provider logging/privacy docs.

13. OpenAPI/AsyncAPI public-private schema projection guard
- Ran aiwg discover "issue-audit OpenAPI AsyncAPI public private schema docs exposure route inventory".
- Inspected #965/#996/#710 and #965/#710 comments plus `crates/matric-api/src/main.rs`, docs mentions, and open tracker state.
- Current code/docs evidence: `/docs`, `/openapi.yaml`, and `/asyncapi.yaml` remain public/auth-exempt; Swagger UI uses `try_it_out_enabled(true)`; `openapi_yaml()` serves `ApiDoc::openapi().to_yaml()` without exposure-profile filtering; `asyncapi_yaml()` is generated independently from `matric_core::asyncapi::build_asyncapi_spec(...)`; cache middleware treats docs/schema as `public, max-age=3600`; `openapi_spec_is_valid()` only validates basic generation and expected endpoint groups, not router/policy/public-schema leakage.
- Created #1000: `test(api-docs): prove public schema exposure is projected from route policy inventory`. Labels applied: P2: Medium, area/auth, area/rest-api, ci/cd, documentation, effort/small, security, testing. Milestone: Hosted auth and multi-tenancy launch gate.
- Posted #965 comment 71042: #1000 is the narrow CI/test guard; #965 remains design owner and #710 remains route/action inventory source. Public schema should be projected from route/action inventory, not a second hand-maintained allowlist.
- Posted #710 comment 71045: route/action inventory should drive auth exemption, policy class, public-schema filtering, operator full-schema generation, AsyncAPI channel exposure, Swagger UI availability, cache/security-header class, and OpenAPI security requirements. Add `docs_exposure_class` and `cache_header_class` or equivalent obligations per external route/channel row.
- Sources checked: OWASP API9:2023 Improper Inventory Management, OpenAPI Specification v3.2.0 security requirements, AsyncAPI server security docs.

14. Backup/default credential docs lint
- Ran aiwg discover "issue-audit backup docs example secrets default credentials PGPASSWORD hosted-safe documentation lint".
- Inspected #980/#968/#992/#997 and #980/#968/#999/#1000 comments plus `docs/`, `scripts/`, Dockerfiles/compose, and `.aiwg` examples.
- Current code/docs evidence: `docs/content/backup.md` shows `/etc/Fortémi/backup.conf` with `PGUSER=matric` and `PGPASSWORD=matric`, plus restore/migration commands using inline `PGPASSWORD=matric`; `docs/content/configuration.md` lists `DATABASE_URL` default as `postgres://matric:matric@localhost:5432/matric` and has production-looking `matric:matric@db.internal` examples; use-case/troubleshooting/integration/testing docs contain `postgres://matric:matric...`; `scripts/backup.sh` help text documents `PGPASSWORD` default `matric`; some `.aiwg` and script production-test examples use `PGPASSWORD=matric`.
- Existing issue split: #980 owns runtime scheduled backup hardening, #992 owns Docker bundle runtime default credentials, #999 owns token/API-key/client-secret placeholder lint, and #968 owns the broad secret taxonomy. A narrow docs/default-credential lint issue was still useful.
- Created #1001: `test(docs): lint default credential and backup secret examples by deployment profile`. Labels applied: P2: Medium, ci/cd, documentation, effort/small, security, testing. Milestone: Native server distribution backlog.
- Posted cross-links: #968 comment 71065, #980 comment 71068, #992 comment 71070, #999 comment 71072.
- #1001 acceptance: public/operator docs should not present `matric:matric`, `PGPASSWORD=matric`, `POSTGRES_PASSWORD=matric`, or credential-bearing production DSNs as general guidance; backup docs should use scanner-safe placeholders or secret-file/profile guidance; CI/docs lint should reject default credential examples unless explicitly local-dev/test allowlisted; taxonomy should distinguish `development_default`, `test_fixture_secret`, `operator_supplied_secret`, and `generated_runtime_secret`.
- Sources checked: OWASP Secrets Management Cheat Sheet, Docker Compose secrets guidance, CISA Secure by Demand guide/default-password guidance.

15. Logging/config docs-contract guard
- Ran aiwg discover "issue-audit logging config docs contract RUST_LOG LOG_FORMAT debug defaults hosted lint".
- Inspected #997/#974/#998 and comments plus `crates/matric-api/src/main.rs`, `docs/content/configuration.md`, `docs/content/operations.md`, `.env.example`, and open tracker state.
- Current code/docs evidence: runtime supports `LOG_FORMAT=json` and treats all other values as text; comments say `LOG_FORMAT` is `json` or `text`, default `text`; unset `RUST_LOG` still falls back to `matric_api=debug,tower_http=debug`; `configuration.md` says `RUST_LOG` default `info`, `LOG_FORMAT` default `pretty`, and supported values `pretty/json/compact`; `operations.md` lists debug fallback and `LOG_FORMAT=text`; docs include unqualified debug/trace examples; `.env.example` uses safer `RUST_LOG=info` but does not define the full contract.
- Existing issue split: #997 owns runtime/config/doc fix, #974 owns field-level telemetry classification/redaction, and #998/#999/#1000/#1001 are sibling docs-contract guards. A narrow logging docs-contract guard was still useful.
- Created #1002: `test(docs): prevent hosted-unsafe logging defaults and unsupported LOG_FORMAT docs drift`. Labels applied: P2: Medium, ci/cd, documentation, effort/small, infrastructure, security, testing. Milestone: Hosted auth and multi-tenancy launch gate.
- Posted cross-links: #997 comment 71094, #974 comment 71097, #998 comment 71100.
- #1002 acceptance: CI/docs lint should fail if public/operator docs list unsupported active `LOG_FORMAT` values such as `pretty`/`compact` unless runtime implements them; fail if docs claim hosted/default `RUST_LOG` is debug after #997 changes the fallback; allow debug/trace examples only in explicit local-development or protected-diagnostic-mode sections; cover configuration, operations, `.env.example`, Docker/compose/systemd examples, and generated config references.
- Sources checked: OWASP Logging Cheat Sheet, OWASP A09 Security Logging and Monitoring Failures, OpenTelemetry handling sensitive data, NIST SP 800-92 Rev. 1 draft.

16. MCP runtime model-visible output sanitizer refinement
- Ran aiwg discover "issue-audit MCP runtime tool output sanitizer errors documentation secret leakage structuredContent".
- Inspected #987/#968/#999/#899 and comments plus `mcp-server/index.js`, MCP docs/tests, and current grep evidence.
- Current code evidence: `apiRequest()` attaches session or fallback `FORTEMI_API_KEY` to outbound calls, then includes raw upstream response body in thrown error text on non-2xx; top-level tool handling serializes successful results with `JSON.stringify(result, null, 2)` into MCP `content` and failures as `Error: ${error.message}` with `isError: true`; grep still finds many independent curl builders interpolating `Authorization: Bearer ${...}` in upload/download/backup/archive/attachment paths; `get_documentation` returns `mm_at_*` query/header examples; some direct `fetch()` branches build bearer headers for server-side work and must not leak returned objects/errors/logs.
- Decided not to create a new issue: #987 already owns runtime MCP tool-output redaction, #899 owns current-runtime regression tests, #968 owns broad secret taxonomy, #999 owns docs placeholder lint.
- Posted #987 comment 71105: implement one model-visible `sanitizeMcpOutput()` boundary plus shared `buildSafeCurlCommand()` / `safeAuthInstruction()` helper, not piecemeal string replacement. Sanitize `content`, `structuredContent`, annotations/resource links, and `isError` content; use stable status/code/request-id errors instead of raw upstream bodies; treat docs as tool output.
- Posted #899 comment 71107: tests should assert the sanitizer boundary through real MCP tool handler for success content, future structured fields, `isError` errors, `get_documentation`, stdout/stderr where practical, and both HTTP session-token and stdio/API-key fallback modes.
- Posted #968 comment 71109: classify MCP post-handler output surfaces as secret-exposure surfaces, including content text, structuredContent, annotations, resource links, errors, returned docs, generated curl snippets, stdout/stderr, and transcript/export copies.
- Sources checked: MCP 2025-11-25 tools spec/security considerations, OWASP REST sensitive-information-in-requests guidance, OWASP Secrets Management, and current MCP security research on output filtering.

17. OpenRouter/provider route policy and privacy
- Ran aiwg discover "issue-audit OpenRouter provider route policy privacy final provider routing bridge downstream providers"; top match was issue-audit.
- Inspected #867/#877/#920/#969/#985 and comments plus `crates/matric-inference/src/provider_profiles.rs`, `crates/matric-inference/src/provider.rs`, and `crates/matric-inference/src/openai/types.rs`.
- Current code evidence: `ProviderProfile` does not model data-retention, route-policy, router verification, or privacy/subprocessor posture fields; OpenRouter is modeled as a normal OpenAI-compatible profile with default base URL, generation-only support, and `HTTP-Referer` / `X-Title`; `ProviderConfig` has `http_referer` and `x_title` but no downstream/final provider, router mode, retention, logging, ZDR, region, or provider-family policy fields; `ChatUsage` is still only prompt/completion/total tokens.
- Decided not to create a new issue. Existing #867/#877/#969/#985 already own the design surface; the remaining gap was router-policy rendering/verifiability.
- Posted #867 comment 71118: require `router_policy_rendered` / `downstream_constraints_rendered` and `router_policy_verification` outcomes for router profiles; strict hosted tenants fail closed when hard constraints are unsupported or unverifiable; cache hits/router errors should record `downstream_provider_unknown_via_router`; public `/v1/models` should expose coarse policy classes only.
- Posted #877 comment 71119: add ledger evidence fields including `downstream_constraints_rendered`, `router_policy_verified`, `router_policy_verification_source`, `final_downstream_provider_source`, `router_metadata_requested`, `router_metadata_received`, and `downstream_fallbacks_observed`.
- Posted #969 comment 71120: privacy/subprocessor register should separate `configured_router_controls` from `verified_final_processor_evidence`, and distinguish "Fortemi configured router to require X" from "Fortemi verified final downstream provider Y under policy X."
- Sources checked: OpenRouter provider routing docs, OpenRouter provider logging/data retention docs, OpenRouter ZDR docs, OpenRouter sovereign/EU routing and data-collection docs, OpenRouter router metadata docs, LiteLLM router/load-balancing/proxy config docs.

18. OAuth/DCR metadata and docs truthfulness
- Ran aiwg discover "issue-audit OAuth DCR metadata docs truthfulness public discovery PKCE client secret hosted"; top match was issue-audit.
- Inspected #941/#944/#972/#965/#913/#924/#926/#917 and comments plus OAuth runtime/docs: `crates/matric-api/src/main.rs`, `crates/matric-core/src/models.rs`, `crates/matric-db/src/oauth.rs`, `docs/content/authentication.md`, and MCP SDK auth metadata shapes in `mcp-server/node_modules/@modelcontextprotocol/sdk`.
- Current code/docs evidence: `oauth_discovery()` always advertises `registration_endpoint`, secret-based token endpoint auth methods, and `code_challenge_methods_supported: ["S256", "plain"]`; `AuthorizationServerMetadata` cannot represent `client_id_metadata_document_supported`; `/oauth/register` returns `registration_client_uri` for `/oauth/register/{client_id}` although only `POST /oauth/register` is routed; DCR always creates secret-bearing confidential clients; docs repeat open DCR, `plain`, `client_secret_expires_at: 0`, `registration_access_token`, and `registration_client_uri` examples.
- Decided to create a narrow regression-guard issue rather than duplicate implementation owners. Existing #941/#944/#972/#924/#926 own the runtime decisions; #1003 owns proving metadata/docs match the chosen profile.
- Created #1003: `test(oauth): assert discovery metadata matches hosted registration, PKCE, and client-auth profiles`. Labels applied: P2, area/auth, ci/cd, documentation, effort/small, mcp-api, security, testing.
- Posted cross-links: #941 comment 71127, #944 comment 71129, #972 comment 71131, #924 comment 71134, #965 comment 71136.
- #1003 acceptance: add OAuth discovery/profile contract tests or docs-contract rules covering local/dev, self-hosted, hosted strict, and compatibility modes. Hosted strict should not advertise open DCR, management URI/token fields, `plain` PKCE, unsupported client auth methods, localhost issuer fallback, unsupported scopes, or CIMD support before implementation.
- Sources checked: MCP 2025-11-25 authorization spec, OpenAI Apps SDK authentication docs, RFC 7591 Dynamic Client Registration, RFC 7592 Dynamic Client Registration Management, RFC 9700 OAuth 2.0 Security BCP.

19. Docs-contract guard consolidation
- Ran aiwg discover "issue-audit docs contract guard consolidation env vars headers routes secrets schema examples"; top match was issue-audit.
- Inspected #998/#999/#1000/#1001/#1002/#1003 and comments plus `scripts/ci/`, `.gitea/workflows/test.yml`, root `package.json`, existing OpenAPI/runtime contract tests, and focused guard scripts.
- Current repo evidence: there are focused CI guard scripts such as `scripts/ci/forbid-insecure-auth-defaults.sh`, but no shared docs-contract scanner, rule-pack convention, profile vocabulary, or CI entry point for the growing docs/runtime contract cluster. Root `package.json` only exposes `docs:build`; `.gitea/workflows/test.yml` runs Rust tests/doc tests but no dedicated docs-contract step.
- Created #1004: `test(docs-contract): add shared profile-aware scanner for docs/runtime contract drift`. Labels applied: P2, ci/cd, documentation, effort/small, security, testing.
- Posted cross-links: #998 comment 71159, #999 comment 71161, #1000 comment 71163, #1001 comment 71165, #1002 comment 71167, #1003 comment 71170.
- #1004 acceptance: provide a shared runner such as `scripts/ci/docs-contract.sh` or a small Rust/Node/Python checker, rule-pack metadata for owner issue/severity/profiles/fixtures/paths/remediation, shared deployment profiles (`local_dev`, `test_fixture`, `self_hosted_operator`, `native_distribution`, `hosted_strict`, named compatibility modes), local command, and CI integration. #998-#1003 remain issue-owned rule packs.
- No new external research was needed for this consolidation slice; it organized existing issue evidence and local CI/docs structure.

20. Inbound source connector config redaction
- Ran aiwg discover "issue-audit inbound source connector config redaction opaque config secrets hosted"; top match was issue-audit.
- Inspected #988/#968/#710 and comments plus `crates/matric-api/src/main.rs`, `crates/matric-db/src/inbound_sources.rs`, `crates/matric-jobs/src/inbound/{sse,redis_stream,kafka,supervisor}.rs`, `crates/matric-core/src/models.rs`, and `migrations/20260614130000_inbound_sources.sql`.
- Current code evidence: `/api/v1/inbound-sources` still creates/lists/deletes registrations through normal authenticated routes; `InboundSource` and `CreateInboundSourceRequest` derive `Debug` and carry raw `config`; list responses return raw config; `enabled` defaults true; supervisor starts enabled rows. Concrete configs now include `SseConfig` with arbitrary headers and URL, `RedisStreamConfig` with raw Redis URL, and `KafkaConfig` with `sasl_password`, `sasl_username`, `dead_letter_topic`, and arbitrary `extra`; those config structs also derive `Debug`.
- Decided not to create a duplicate issue. #988 already owns inbound-source auth/redaction/activation; the remaining work was to refine concrete schema, secret-reference, destination-policy, telemetry, and DLQ expectations.
- Posted #988 comment 71178: require typed kind-specific DTOs, secret references instead of inline credentials/headers/passwords in hosted mode, manual redacted Debug, constrained SSE headers/Kafka `extra`, validation profile/version evidence before activation, redacted list/get DTOs, and DLQ/external Kafka DLQ classification.
- Posted #968 comment 71179: add inbound-source config, SSE headers, Redis URLs, Kafka SASL/broker/dead-letter settings, connector Debug/panic/test surfaces, and DLQ payload/error surfaces to the hosted secret inventory.
- Posted #920 comment 71181: extend shared outbound destination policy to inbound connector profiles (`inbound_sse_source`, `inbound_redis_stream_source`, `inbound_kafka_broker`, `inbound_kafka_dead_letter_topic`) with create/update and supervisor-start validation, redirect/proxy handling for SSE, and hosted egress defense-in-depth.
- Posted #974 comment 71183: add connector config/connect/DLQ telemetry fixtures so URLs, headers, passwords, arbitrary config, backend errors, and DLQ payloads do not leak through logs, metrics, health, or support-safe diagnostics.
- Sources checked: OWASP Secrets Management Cheat Sheet, OWASP SSRF Prevention Cheat Sheet, OWASP API6:2023 Unrestricted Access to Sensitive Business Flows, OWASP Logging Cheat Sheet.

21. MCP auth/test implementation readiness
- Ran aiwg discover "issue-audit MCP auth test implementation readiness Origin protocol version auth binding legacy SSE"; top match was issue-audit.
- Inspected #899/#914/#917/#918/#921/#940 and comments plus `mcp-server/index.js`, `mcp-server/package.json`, `mcp-server/package-lock.json`, `mcp-server/tests/helpers/mcp-client.js`, `mcp-server/tests/oauth.test.js`, `mcp-server/tests/preflight.test.js`, and `mcp-server/test-mcp-connectivity.js`.
- Current code/test evidence: HTTP mode still installs `cors({ origin: '*' })` with allowed headers `Content-Type`, `Authorization`, and `MCP-Session-Id` only; both root Streamable HTTP and legacy `/sse` + `/messages` remain enabled and startup-advertised; `validateBearerToken()` accepts active tokens with `mcp`/`read`/`admin` scope and returns raw token only; sessions store `{ transport, token, type }`; GET/messages run with stored session token; DELETE closes by session id after only validating some bearer. Main `npm test` runs `node --test tests/*.test.js`, not `test-mcp-connectivity.js`. The shared MCP test client does not send `MCP-Protocol-Version` and has no Origin/preflight override; connectivity's two-session smoke test uses one API key for both sessions.
- Decided not to create a new issue. The existing MCP cluster has the right owners; the gap was implementation sequencing and deterministic test harness shape.
- Posted #899 comment 71194: require deterministic HTTP-mode MCP test harness in the main `node --test` path, either by extracting an app/session-manager factory or using a child process with mocked Fortemi API/introspection. Add distinguishable token fixtures, `MCP-Protocol-Version` defaulting/overrides, Origin overrides, no-session-side-effect assertions, and conditional legacy SSE coverage based on #940.
- Posted #918 comment 71197: update the shared MCP HTTP test client to record negotiated protocol version and send `MCP-Protocol-Version` on subsequent POST/GET/DELETE by default, with overrides for missing-header fallback and unsupported-version 400 tests.
- Posted #940 comment 71199: decide legacy SSE policy before final #899 tests. If disabled by default, tests should assert `/sse`/`/messages` unavailable and not startup-advertised; if enabled, tests must cover Origin, resource/audience, session binding, scope, and no side effects.
- Rechecked during handoff update: the latest live comments still make #899/#918/#940 the right implementation-sequencing anchors, while #914/#917/#921 remain implementation prerequisites. Avoid duplicate audit comments unless new code has landed or a product decision changes #940. A concrete next implementation plan would be: harness/factory or child-process mock first, strict legacy-SSE flag/default second, then CORS/protocol/Origin/session/resource tests through that harness.
- Sources checked: MCP 2025-11-25 transports and authorization specs, GitHub advisory GHSA-w48q-cv73-mx4w / CVE-2025-66414, NVD CVE-2026-25536.

22. Bridge public model catalog policy classes
- Ran aiwg discover "issue-audit bridge public model catalog policy classes provider cache router privacy"; top match was issue-audit.
- Inspected #866/#867/#877/#969/#985 and comments plus `crates/matric-api/src/handlers/models.rs`, `crates/matric-api/src/handlers/chat.rs`, `crates/matric-inference/src/provider_profiles.rs`, and `crates/matric-inference/src/provider.rs`.
- Current code evidence: native `GET /api/v1/models` returns `{ models, defaults, providers }` and includes provider defaults/health/local heuristics; it is not OpenAI-compatible bridge shape. `ProviderProfile`/`ProviderConfig` model backend/base URL/API key/capabilities/headers but not route class, privacy posture, data retention, provider prompt-cache, router mode, downstream verification, region, or accounting class. Unknown provider prefixes in slugs can fall back through default-provider behavior unless bridge policy explicitly prevents silent rerouting.
- Decided not to create a duplicate issue. #866 owns bridge `/v1/models`; #867 owns route policy; #985 owns provider prompt-cache; #877/#969 own accounting/privacy evidence.
- Posted #866 comment 71208: default bridge `/v1/models` should remain OpenAI-compatible (`id`, `object`, `created`, `owned_by`) unless Fortemi deliberately adds opt-in extension fields or a separate admin catalog. Public hints, if any, must be coarse classes (`capability_class`, `route_class`, `privacy_policy_class`, `cache_policy_class`, `price_metering_class`, `region_class`) derived from the same policy path used for invocation. Raw provider order, route weights, base URLs, API-key presence, exact downstream provider, cache keys/session IDs, raw pricing, and provider metadata stay operator/audit-only.
- Posted #985 comment 71209: cache catalog fields should use coarse vocabulary only: `provider_prompt_cache`, `router_sticky_cache`, `external_router_response_cache`, `fortemi_response_cache`, and `cache_retention_class`. Router profiles with unverifiable downstream cache/retention posture should be omitted for strict hosted tenants or rendered only as unknown/fail-closed/review-required, never as a positive privacy/cache claim.
- Sources checked: OpenAI Models API reference; existing issue sources for OpenRouter routing/logging/ZDR/cache docs, Anthropic prompt caching, Gemini model metadata, and LiteLLM router behavior.

23. OAuth implementation readiness
- Ran aiwg discover "issue-audit OAuth implementation readiness DCR CIMD PKCE hosted metadata docs contract"; top match was issue-audit.
- Inspected #972/#941/#944/#924/#1003 plus #917/#913/#899 and comments, and read `crates/matric-api/src/main.rs`, `crates/matric-core/src/models.rs`, `crates/matric-db/src/oauth.rs`, `docs/content/authentication.md`, and focused MCP SDK auth paths.
- Current code evidence: `oauth_discovery()` still hardcodes `registration_endpoint`, secret-based token auth methods, `delete` in `scopes_supported`, and `S256` plus `plain`; `AuthorizationServerMetadata` still lacks `client_id_metadata_document_supported`; `oauth_register()` still returns `registration_client_uri` for `/oauth/register/{client_id}` although no management route exists; `parse_client_credentials()` still requires Basic/body secret before the grant branch, so public-client `none` cannot work; authorization/token request structs do not accept the RFC 8707 `resource` parameter; tokens/introspection still use `aud: client_id`.
- Decided not to create duplicate issues. #1003 owns discovery/docs contract guard, #941 owns PKCE/public-client/token-auth method behavior, #944 owns DCR gating and registration management truthfulness, #924 owns scopes and requested-scope subset enforcement, #917 owns resource/audience binding.
- Posted #1003 comment 71212: implement a profile-driven OAuth contract harness/fixture (`local_dev`, `self_hosted_operator`, `hosted_strict`, compatibility modes) before piecemeal metadata/docs edits. Hosted-strict negative fixtures should prove no open DCR, no unsupported management fields, no `plain`, no unsupported auth methods, no `delete` unless real, and no CIMD flag before runtime support.
- Posted #941 comment 71216: sequence PKCE/S256 with token-endpoint auth-method dispatch. `token_endpoint_auth_method=none` should be supported only for public/native/CIMD authorization-code clients with S256, never for client-credentials; `private_key_jwt` should not be advertised until implemented; registered method mismatches need tests.
- Posted #917 comment 71219: make AS-side `resource` parameter parsing/validation/minting an explicit prerequisite for MCP validation tests. Add `resource` to authorization/token flows, persist or derive protected-resource audience on codes/tokens, return it via introspection, and keep API-resource tokens distinct from MCP-resource tokens.
- Sources checked: MCP 2025-11-25 authorization and client-registration guidance, MCP draft changelog/client-registration page, OpenAI Apps SDK authentication docs, RFC 9700 OAuth Security BCP, plus vendored MCP SDK 1.29.0 auth behavior.

24. Docs-contract implementation sequencing
- Ran aiwg discover "issue-audit docs-contract implementation sequencing shared scanner rule packs CI profile fixtures"; top matches were issue-audit and doc-sync. Used issue-audit because this was tracker sequencing, not content synchronization.
- Inspected #1004/#998/#999/#1000/#1001/#1002/#1003 and comments, plus `.gitea/workflows/test.yml`, `.gitea/workflows/ci-builder.yaml`, `scripts/ci/forbid-insecure-auth-defaults.sh`, `scripts/ci/forbid-provider-imports.sh`, `scripts/pre-commit.sh`, and root `package.json`.
- Current repo evidence: `ci-builder.yaml` already runs focused shell guards in the lint job, but `test.yml` has no docs-contract step; root `package.json` only exposes `docs:build`; `forbid-insecure-auth-defaults.sh` depends on `rg`, while this workspace currently has no `rg` binary. #1004 had no comments before this slice.
- Decided not to create new issues. #1004 owns the shared runner/harness; #998-#1003 remain domain rule-pack owners.
- Posted #1004 comment 71227: split implementation into layers. Land `scripts/ci/docs-contract.sh` as stable entrypoint with profile default, deterministic output, local command; start as fast lint-style job near existing CI guards with advisory/blocking mode; wire #999 and #1001 as first blocking text rule packs; add manifest/profile/allowlist shape; add runtime-backed #998/#1002/#1003 later; keep #1000 partly in Rust/API inventory tests.
- Posted #999 comment 71234: recommend #999 as one of the first blocking rule packs for token/API-key/client-secret placeholders and model-visible MCP docs strings.
- Posted #1001 comment 71236: recommend #1001 as the other first blocking rule pack for default database/backup credentials and credential-bearing DSNs, with profile-aware local/test allowlists.
- No new external research was needed; this slice used existing issue sources and current local CI/tooling evidence.

25. Inbound connector implementation sequencing
- Ran aiwg discover "issue-audit inbound connector implementation sequencing secret refs redacted DTO activation hosted"; top match was issue-audit.
- Inspected #988/#968/#920/#974 and comments, plus `crates/matric-api/src/main.rs`, `crates/matric-core/src/models.rs`, `crates/matric-db/src/inbound_sources.rs`, `crates/matric-jobs/src/inbound/{supervisor,registry,sse,redis_stream,kafka}.rs`, and `migrations/20260614130000_inbound_sources.sql`.
- Current code evidence: `CreateInboundSourceRequest.enabled` defaults true and create persists opaque JSON directly; `InboundSource`/`CreateInboundSourceRequest` derive `Debug` and carry raw `config`; list returns raw config; supervisor starts every enabled row without schema/profile/destination validation evidence; concrete configs still accept inline SSE headers, Redis URL, Kafka SASL fields, `extra`, and DLQ topic fields.
- Decided not to create duplicate issues. #988 owns inbound-source API/supervisor changes, #968 owns secret taxonomy/refs, #920 owns destination policy, and #974 owns telemetry/DLQ/log redaction.
- Posted #988 comment 71239: split implementation into a fail-closed guard PR and migration/refactor PR. First PR should make hosted create default draft/disabled, admin-gate create/delete/list, reject `enabled=true` on create outside privileged profiles, return redacted DTOs, and remove/redact derived Debug. Later migration should add validation evidence columns/table, typed DTOs, secret refs, and explicit validate/activate/deactivate actions.
- Posted #968 comment 71240: define connector secret-reference resolver boundary. Persist connector config refs and safe metadata; resolve raw SSE/Redis/Kafka credentials only in supervisor/connector factory; use stable failure reason codes; test create/update, stored row, list/get, supervisor build, connector construction, DLQ/error, and metrics/log paths.
- Posted #920 comment 71242: inbound connector destination policy needs create/update-time validation plus start/connect-time revalidation. SSE uses HTTP policy; Redis/Kafka need scheme/broker/domain/TLS/private-network/egress classification; Kafka `dead_letter_topic` is an external diagnostic sink.
- Sources checked: OWASP SSRF Prevention and Secrets Management guidance, Redis security guidance, Kafka client/security guidance, plus existing issue sources.

26. Bridge route/catalog implementation sequencing
- Ran aiwg discover "issue-audit bridge route catalog implementation sequencing OpenAI compatible models policy classes router privacy"; top match was issue-audit.
- Inspected #866/#867/#877/#969/#985 and current comments, plus `crates/matric-api/src/handlers/models.rs`, `crates/matric-inference/src/provider.rs`, `crates/matric-inference/src/provider_profiles.rs`, `crates/matric-inference/src/openai/types.rs`, and bridge route grep evidence.
- Current code evidence: there is still no OpenAI-compatible bridge `/v1/models` route; native `GET /api/v1/models` returns `{ models, defaults, providers }`; `ProviderProfile` / `ProviderConfig` still model backend/base URL/API-key/capabilities/OpenRouter headers only; `ProviderRegistry::parse_slug()` intentionally treats unknown `foo:bar` prefixes and empty known prefixes such as `openai:` as default-provider model names; tests currently lock that permissive native behavior in.
- Decided not to create duplicate issues. #866 owns bridge `/v1/models`; #867 owns route-policy selection; #877/#969/#985 already own accounting/privacy/cache evidence.
- Posted #866 comment 71252: bridge implementation should not reuse the native permissive slug parser unchanged. Add a bridge-specific strict resolver mode before `/v1/models`: known provider-qualified slugs, bare allowed virtual aliases, and rejected unknown `prefix:model` / empty qualified models. Keep native Ollama-compatible parsing unless separately migrated. Add fixtures for `azure:gpt-4`, `foo:model`, `openai:`, an authorized exact provider slug, and an authorized virtual alias.
- Posted #867 comment 71254: make exact provider slug vs virtual alias a typed route-plan decision before adding `X-Fortemi-Route`. Suggested `RoutePlan` / `BridgeResolvedModel` variants: `ExactProvider`, `VirtualAlias`, and `Rejected`. Apply consumer policy, destination policy, privacy/region/cache constraints, and downstream-router rendering before provider request/retry. Unsupported `cheapest`/`best-value` still wait for #877/#878.
- Sources checked: OpenAI Models API reference, OpenRouter provider routing, OpenRouter prompt/response caching, LiteLLM proxy load-balancing/hidden aliases/fallback docs.

27. OAuth implementation sequencing
- Ran aiwg discover "issue-audit OAuth implementation sequencing DCR CIMD PKCE hosted metadata docs contract"; top match was issue-audit.
- Inspected #972/#941/#944/#924/#1003/#917 and current comments, plus `crates/matric-api/src/main.rs`, `crates/matric-core/src/models.rs`, `crates/matric-db/src/oauth.rs`, OAuth migration/test files, and docs-contract context.
- Current code evidence: `oauth_discovery()` and `oauth_protected_resource()` still hardcode metadata separately; discovery advertises `registration_endpoint`, `client_secret_basic`, `client_secret_post`, `S256`, `plain`, and `delete`; `AuthorizationServerMetadata` lacks CIMD support fields; `ClientRegistrationRequest` / `OAuthClient` lack JWKS/JWKS URI despite DB columns; token endpoint auth parsing requires Basic/body secret before grant-specific dispatch; client-credentials can inherit the client's full stored scope when no requested scope is provided; authorization/token request structs and DB tables do not carry `resource`; introspection returns `aud: client_id` rather than a stored protected-resource audience.
- Decided not to create duplicate issues. #1003 owns the profile/discovery/docs contract guard, #941 owns token-endpoint auth dispatch/PKCE/public-client behavior, #944/#972 own DCR/CIMD/pre-registration policy, #924 owns scope truth/subset enforcement, and #917 owns resource/audience binding.
- Posted #1003 comment 71261: make the OAuth profile contract a runtime/source-of-truth builder rather than docs grep or static snapshots. Suggested `OAuthDeploymentProfile` / `OAuthCapabilities` consumed by discovery, protected-resource metadata, and docs-contract fixtures. Include registration mode, actual token auth methods, PKCE policy, supported scopes, issuer/public URL, resource/audience support, and hosted-strict negative tests.
- Posted #917 comment 71265: resource/audience validation needs a schema/model/storage slice before MCP validation can be truthful. Add resource/audience fields to authorization codes and tokens, parse/validate RFC 8707 `resource` in authorization/token requests, return stored resource via introspection, and keep API-resource tokens distinct from MCP-resource tokens.
- Sources checked: MCP 2025-11-25 authorization spec, OpenAI Apps SDK authentication docs, RFC 9700 OAuth 2.0 Security BCP, RFC 7591 Dynamic Client Registration, RFC 7592 Dynamic Client Registration Management, RFC 8707 Resource Indicators.

28. Docs-contract implementation follow-through
- Ran aiwg discover "issue-audit docs-contract implementation follow-through shared scanner rule packs CI profile fixtures"; top matches were issue-audit and doc-sync. Used issue-audit because this was tracker sequencing, not content synchronization.
- Inspected #1004/#998/#999/#1000/#1001/#1002/#1003 and latest comments, plus `scripts/ci/forbid-insecure-auth-defaults.sh`, `scripts/ci/forbid-provider-imports.sh`, `scripts/pre-commit.sh`, `.gitea/workflows/ci-builder.yaml`, `.gitea/workflows/test.yml`, root `package.json`, and focused grep evidence for the first rule packs.
- Current repo evidence: there is still no `docs-contract` script, package script, CI step, or pre-commit hook; `package.json` exposes only `docs:build`; `ci-builder.yaml` runs only the existing provider-import and insecure-auth-default guards; `scripts/pre-commit.sh` runs the provider-import guard but not the insecure-auth-default guard; `forbid-insecure-auth-defaults.sh` depends on `rg`, and `rg` is absent in this workspace.
- First-rule-pack evidence: #999 still has many active matches in public docs and MCP-returned docs, including `mcp-server/index.js` streaming docs with `Authorization: Bearer mm_at_xxx`, query `token=mm_at_xxx`, and `EventSource(...token=mm_at_xxx)` examples. #1001 still has many active matches across docs/scripts/workflows for `matric:matric`, `PGPASSWORD=matric`, `POSTGRES_PASSWORD=matric`, and credential-bearing Postgres DSNs; some workflow/test-script matches are legitimate `test_fixture` uses.
- Decided not to create duplicate issues. #1004 remains the shared harness owner; #999 and #1001 remain first rule-pack owners.
- Posted #1004 comment 71273: #999/#1001 are still good first rule packs, but should start advisory or with a tracked baseline unless cleanup lands in the same PR. Add runner/manifest/profile/dependency/output first, then convert #999/#1001 to blocking after cleanup or explicit local_dev/test_fixture allowlists. Exclude vendored dependency trees such as `mcp-server/node_modules`, but include `mcp-server/index.js` and model-visible docs strings.
- Posted #999 comment 71276: start advisory/baseline unless cleanup lands; scan model-visible docs and static docs for Fortemi/provider-shaped secrets; block only after zero active findings or explicit allowlists.
- Posted #1001 comment 71278: start advisory/baseline unless cleanup lands; avoid broad path exemptions for all scripts/workflows because those paths contain both legitimate test credentials and operator-facing drift.
- Sources checked: OWASP Secrets Management Cheat Sheet, GitHub secret scanning docs/custom-pattern behavior, and SARIF output conventions for stable rule IDs and file/line reporting.

29. Inbound connector implementation follow-through
- Ran aiwg discover "issue-audit inbound connector implementation follow-through secret refs redacted DTO activation hosted"; top match was issue-audit.
- Inspected #988/#968/#920/#974 and latest comments, plus `crates/matric-api/src/main.rs`, `crates/matric-core/src/models.rs`, `crates/matric-db/src/inbound_sources.rs`, `crates/matric-jobs/src/inbound/{supervisor,sse,redis_stream,kafka}.rs`, and `migrations/20260614130000_inbound_sources.sql`.
- Current code evidence still matches prior comments: inbound-source create/list/delete endpoints are ordinary route handlers; `CreateInboundSourceRequest.enabled` defaults true; `InboundSource` and create DTOs derive `Debug` with raw opaque config; repository persists raw `config`; list returns raw config; supervisor starts all enabled rows; connector configs still use inline SSE headers, Redis URL, Kafka SASL credentials, arbitrary Kafka `extra`, and optional external `dead_letter_topic`.
- New gap found in this slice: connector activation is also an event-admission/schema problem. `InboundSupervisor::process()` accepts any non-empty connector-derived `event_type` and non-null `payload`, then emits an `event_outbox` row with `entity_type = "inbound_event"` and raw `{ source, offset, event_type, payload }`. SSE/Redis/Kafka config can derive event type from upstream payload fields or configured defaults; inbound sources do not yet have a recorded payload schema/ref, allowed event-type namespace, or transform contract like incoming webhooks do.
- Decided not to create a duplicate issue. #988 remains the inbound-source API/supervisor owner; #974 owns telemetry/data-sink handling for the resulting outbox/DLQ rows.
- Posted #988 comment 71284: add event-admission/schema validation to activation. Activation should require connector config validation plus allowed event-type namespace/prefixes, payload schema ref/version, transform version, and raw/minimized/projected payload mode. Hosted defaults should reserve external connector event namespaces and prevent spoofing internal names such as `note.created`, `call_event`, `job.failed`, or future billing/audit event names.
- Posted #974 comment 71285: treat `event_outbox` rows with `entity_type = inbound_event` as retained inbound payload sinks, not ordinary metrics. Add fixtures for PII, secret-shaped values, credential-bearing URLs, oversized JSON, CR/LF/control chars, and internal event names; telemetry should emit stable admission/result reason codes and safe counts/sizes only.
- Sources checked: OWASP API6:2023 Unrestricted Access to Sensitive Business Flows, OWASP SSRF Prevention Cheat Sheet / Top 10 SSRF guidance, Redis security docs, Apache Kafka SASL security docs, and Confluent SASL/PLAIN guidance.

30. OAuth implementation follow-through
- Ran aiwg discover "issue-audit OAuth implementation follow-through DCR CIMD PKCE resource audience scope hosted"; top match was issue-audit.
- Inspected #972/#941/#944/#924/#1003/#917 and latest comments, plus `crates/matric-api/src/main.rs`, `crates/matric-core/src/models.rs`, `crates/matric-db/src/oauth.rs`, `crates/matric-db/migrations/002_oauth.sql`, OAuth tests grep evidence, and docs grep evidence.
- Current code evidence still matches prior comments: discovery/protected-resource metadata is hand-built and hardcodes open DCR, secret-based token methods, `S256` + `plain`, and `delete`; token endpoint requires Basic/body secret before grant dispatch; DCR always creates secret/confidential clients and returns management fields; authorization/token requests and DB rows have no `resource`; introspection returns `aud: client_id`.
- New gap found in this slice: scope source of truth also includes DB seed state. `migrations/002_oauth.sql` seeds `oauth_scope` with `delete`, while runtime `ALLOWED_SCOPES` rejects `delete` and metadata/docs advertise it. `AuthPrincipal::has_scope("delete")` tests make `admin` imply delete, but not `write` or `mcp`. No current OAuth endpoint tests surfaced for requested-scope subset enforcement.
- Decided not to create duplicate issues. #924 remains the scope model/requested-scope enforcement owner; #1003 remains the profile/discovery/docs contract owner.
- Posted #924 comment 71295: require one canonical scope source across runtime constants, metadata, DB seed/default rows, docs, and tests. If `delete` is not real, remove it from metadata/docs and seed/profile expectations or mark it historical/internal; if real, add it to `ALLOWED_SCOPES`, define hierarchy semantics, and test registration/authorization/token/refresh/route/MCP enforcement. Add endpoint regressions for a `read` client requesting `admin`, `mcp`, or `delete` and getting `invalid_scope`.
- Posted #1003 comment 71296: include DB seed/default scope rows in OAuth profile fixtures, not only static metadata and docs. Add hosted-strict negative for `delete` unless #924 promotes it, plus a `scope_source_of_truth` fixture proving advertised scopes, accepted scopes, and seeded scopes agree or are explicitly internal/non-advertised.
- Sources checked: RFC 6749 `invalid_scope`, RFC 8707 Resource Indicators, RFC 9700 PKCE security BCP, OpenAI Apps SDK auth token verification guidance, plus existing MCP/OpenAI/RFC sources from prior OAuth slice.

31. Bridge route/catalog implementation follow-through recheck
- Ran aiwg discover "issue-audit bridge route catalog implementation follow-through OpenAI compatible models policy classes router privacy"; top match was issue-audit.
- Re-read latest #866/#867/#877/#969/#985 comment state and rechecked `crates/matric-api/src/handlers/models.rs`, `crates/matric-inference/src/provider.rs`, `crates/matric-inference/src/provider_profiles.rs`, `crates/matric-inference/src/openai/types.rs`, and bridge route grep evidence.
- Current code evidence still matches comments 71252/71254: there is no OpenAI-compatible bridge `/v1/models` route; native `GET /api/v1/models` still returns `{ models, defaults, providers }`; `ProviderProfile` / `ProviderConfig` still model backend/base URL/API-key/capabilities/OpenRouter headers only; `ProviderRegistry::parse_slug()` still has permissive native behavior for unknown prefixes and empty known prefixes such as `openai:`.
- No new tracker comment was posted. Existing comments already cover the actionable bridge gaps: #866 comment 71252 owns strict bridge slug/catalog resolver and OpenAI-compatible renderer; #867 comment 71254 owns typed `RoutePlan` / `BridgeResolvedModel`; #877 comment 71119 owns downstream-router evidence/accounting fields; #969 comment 71120 owns privacy/subprocessor evidence split; #985 comment 71209 owns coarse cache-policy catalog classes.
- New agents should not restart bridge audit unless implementation lands, #866/#867/#985 product choices are answered, or the user specifically asks to produce an implementation plan. If implementation work starts, first create the strict bridge resolver/typed route plan before exposing `/v1/models` or forwarding `X-Fortemi-Route`.
- No new external research was needed for this recheck; it used current issue comments and local code state.

32. MCP implementation follow-through and 2026-07-28 spec-watch
- Ran aiwg discover "issue-audit MCP implementation follow-through Origin protocol version auth binding legacy SSE"; top match was issue-audit.
- Re-read #899/#914/#917/#918/#921/#940 comments and rechecked `mcp-server/index.js`, `mcp-server/package.json`, `mcp-server/package-lock.json`, `mcp-server/tests/helpers/mcp-client.js`, `mcp-server/tests/preflight.test.js`, `mcp-server/tests/oauth.test.js`, and `mcp-server/test-mcp-connectivity.js`.
- Current code evidence still matches prior MCP comments: `npm test` only runs `node --test tests/*.test.js`; `test-mcp-connectivity.js` is separate; the helper sends `Mcp-Session-Id` but not `MCP-Protocol-Version`; there is wildcard CORS; no Origin validator; no app-level protocol-version wrapper; token-only session records; legacy `/sse` + `/messages`; Streamable HTTP `GET`/`DELETE`; and startup output advertises SSE.
- New external planning input: MCP 2026-07-28 release candidate was published May 21, 2026. It is breaking and moves to a stateless protocol core: no initialize/initialized handshake, no `Mcp-Session-Id`, no GET/DELETE session lifecycle for that profile, request metadata headers such as `Mcp-Method` / `Mcp-Name`, and legacy HTTP+SSE is deprecated/compatibility-only. Stable 2025-11-25 remains the immediate target unless Fortemi explicitly adopts the RC.
- Posted #940 comment 71320: keep near-term launch gates pinned to stable 2025-11-25, disable legacy `/sse` + `/messages` by default unless a named compatibility need exists, add review/removal date if compatibility flag exists, and track future stateless MCP profile separately.
- Posted #899 comment 71326: make the deterministic HTTP-mode MCP test harness protocol-profile-aware. First profile can be `2025-11-25-streamable-sessionful`; future 2026-07-28 behavior should be a pending/separate profile unless product adopts the RC.
- Sources checked: MCP 2026-07-28 release-candidate blog, draft 2026-07-28 Streamable HTTP page, stable MCP 2025-11-25 transport/authorization docs, GitHub advisory GHSA-w48q-cv73-mx4w.
- Do not repost generic MCP harness/session-binding/resource/protocol/Origin/SSE comments unless code or product decisions change. Useful next MCP work is implementation sequencing or actual implementation.

33. Hosted runtime logging/API error telemetry follow-through
- Ran aiwg discover "issue-audit hosted runtime logging API error telemetry follow-through redaction docs drift"; top match was issue-audit.
- Re-read #997/#974/#967 plus #1002/#1004 comments and rechecked `crates/matric-api/src/main.rs`, `crates/matric-api/src/handlers/*`, `docs/content/configuration.md`, `docs/content/operations.md`, `.env.example`, Dockerfile, docker-compose, scripts, and focused grep evidence.
- Current code evidence still matches the open issues: logging bootstrap falls back to `matric_api=debug,tower_http=debug`; `LOG_FORMAT` only treats exact `json` specially while docs still mention unsupported `pretty` / `compact`; `ApiError::into_response()` still emits `{ "error": message }` with raw `Database`/`Internal` strings; `OAuthApiError::Database` returns raw DB error text as OAuth `server_error`; `BlobMissing` exposes path/backend; no central security-header layer was found beyond cache policy; direct telemetry call sites still log raw URLs, query text, AI responses, DTMF, subprocess stderr, and broad `%e` strings.
- Posted #997 comment 71332: make #997 the small logging-bootstrap/profile PR first. Add `LoggingConfig` parser, explicit `LOG_FORMAT`/`LOG_ANSI` validation, hosted-safe unset `RUST_LOG`, safe startup event fields, focused tests, and docs alignment before #1002 becomes blocking.
- Posted #967 comment 71335: define request-id/error-boundary mechanics before one-off handler edits. Existing `x-request-id` is the right primitive, but `ApiError::into_response()` lacks request context; add middleware/builder/extractor or phased error redesign, with compatibility choice between `{ error, code, request_id }` and RFC 9457.
- Posted #974 comment 71336: treat telemetry redaction as a shared instrumentation boundary. Define field classes/helpers and cross-cutting fixtures before editing the many direct `tracing::*` call sites; source instrumentation should avoid prohibited content before relying on OpenTelemetry/exporter redaction.
- Sources checked: RFC 9457 Problem Details, OWASP Error Handling/REST/Logging guidance, OpenTelemetry sensitive-data handling guidance.
- Do not repost generic logging-default/error-boundary/telemetry-redaction-boundary comments unless code or product decisions change. Useful next work is implementation planning or actual implementation.

34. Backup/restore hardening follow-through
- Ran aiwg discover "issue-audit backup restore hardening follow-through stderr secret handling pg_dump pg_restore drills hosted"; top match was issue-audit.
- Re-read issue comments for #927, #978, #980, #991, #923, and #1001. Existing comments already cover admin/scope enforcement, archive upload trust boundaries, uploaded SQL restore trust, memory-scoped restore sequencing, PostgreSQL toolchain/version gates, backup inventory disclosure/resource use, sidecar/evidence manifests, shell-sourced `backup.conf`, default-credential docs lint, and legacy JSON import validate/preview/apply separation.
- Completed focused code review of the remaining backup script/API areas: rest of `scripts/backup.sh`; route mounts/auth classification; `backup_import`, `backup_trigger`, `backup_status`, `list_backups`, `get_backup_info`, database/memory backup download, snapshot/upload/restore, and `knowledge_archive_upload`.
- Confirmed existing comments already cover most observed hazards: hardcoded credentials, raw stderr, path exposure, synchronous hashing/manifest parsing, GET status directory creation, script temp/config/remote cleanup hazards, memory-scoped destructive restore before snapshot, and full in-memory SQL/archive handling.
- New non-duplicative gap found: HTTP/API backup producers create or stream restore-capable dumps outside the scheduled backup evidence/verification model.
  - `backup_trigger()` and `database_backup_snapshot()` run plain `pg_dump`, gzip stdout, save current descriptive `BackupMetadata`, and do not write the richer #980 evidence record.
  - `database_backup_download()` and `memory_backup_download()` synchronously run `pg_dump`, gzip the full stdout in memory, and return it directly; failures return raw `pg_dump` stderr.
  - API-created/uploaded restore-capable files appear in the same browser/listing, but #978 inventory needs to distinguish descriptive metadata from machine-verifiable evidence/provenance.
- Posted #980 comment 71348: require one `BackupArtifact` / evidence sidecar contract across scheduled script, API trigger, API snapshot, direct download, upload/import, and pre-restore snapshots; classify direct downloads as backup-create/download jobs; do not let plain SQL API dumps bypass toolchain/evidence/hosted-credential gates.
- Posted #978 comment 71350: inventory should separate `descriptive_metadata` from `evidence_manifest`, `verification_status`, and `provenance`; classify producer/trust/restore capability; use #980 evidence instead of recomputing missing evidence on normal list/info GETs.
- Sources checked: PostgreSQL current `pg_dump` and `pg_restore` docs, CISA ransomware backup fact sheet, NIST SP 800-34 Rev. 1 contingency planning, OWASP API4 and File Upload guidance.
- Do not repost backup/restore comments unless code or product decisions change. Useful next backup work is implementation planning or actual implementation around #927/#978/#980/#991.

35. Backup/restore artifact lifecycle planning
- Ran aiwg discover "issue-audit backup restore implementation planning BackupArtifact evidence sidecar hosted API restore download inventory"; top match was issue-audit.
- Re-read live #980/#978/#927/#991 comments and current backup/restore code plus `scripts/backup.sh`.
- Current repo evidence still shows descriptive `BackupMetadata` only, final-path writes before evidence/verification, uploaded restore-capable files written directly into `BACKUP_DEST`, restore eligibility derived from filename/extension/existence, inventory trust inferred from filesystem scans, and scheduled script destination copies before the current size-only verification.
- New non-duplicative gap found: the shared `BackupArtifact` evidence model needs an atomic lifecycle state machine, not only a sidecar schema. Without states, partially written, failed-verification, uploaded-unverified, stale-evidence, WORM-retained, or beyond-use artifacts can appear as normal backups in inventory/restore.
- Posted #980 comment 71440: define artifact states such as `creating`, `stored_unverified`, `verifying`, `verified`, `failed`, `quarantined`, `expired`, `deleted_tombstone`, and `beyond_use_retained`; producers should stage then atomically publish artifact/evidence; restore/download eligibility should derive from evidence state/profile, not filename; tests should cover interrupted producers, failed verification, stale hashes, uploaded artifacts without evidence, and restore refusing non-eligible states.
- Posted #978 comment 71442: backup inventory should consume the #980 lifecycle state, expose safe coarse status, and avoid recomputing evidence or exposing paths/raw hashes/raw manifests on normal GETs.
- Sources checked: PostgreSQL current `pg_dump` and `pg_restore` docs, NIST SP 800-34 Rev. 1, and CISA ransomware backup guidance.
- Do not repost backup/restore evidence, inventory, upload/restore trust-boundary, or artifact-lifecycle comments unless implementation or product decisions land. Useful next backup work is actual implementation or product answers around hosted restore availability and artifact lifecycle policy.

36. Docs-contract implementation follow-through
- Ran aiwg discover "issue-audit docs-contract implementation follow-through shared scanner rule packs CI profile fixtures"; top matches were issue-audit and doc-sync. Used issue-audit because this was tracker triage, not a documentation synchronization/fix pass.
- Re-read #1004/#999/#1001/#1002/#998/#1003/#1000/#997 comments and current CI/script state.
- Current repo evidence still matches prior docs-contract comments: no `scripts/ci/docs-contract.sh`, no package script, no docs-contract CI step, no shared allowlist/baseline file; `forbid-insecure-auth-defaults.sh` still depends on `rg` while `rg` is absent; #999/#1001 pattern hits remain active across docs, MCP model-visible docs, workflows, scripts, Docker/compose, and test fixtures.
- New non-duplicative gap found: Fortemi already has a CI-blocking docs/config guard (`scripts/ci/forbid-insecure-auth-defaults.sh`) with separate hardcoded exclusions and dependency behavior. #1004 should decide whether to migrate one-off guards into the shared runner or intentionally keep them separate, otherwise Fortemi will grow two scanner/allowlist frameworks.
- Posted #1004 comment 71360: treat existing one-off CI guards as migration inputs; convert hardcoded exclusions into manifest entries with owner/profile/reason/expiry; support full-repo CI and fast local/staged-file mode; run fast text rules in pre-commit; add dependency preflight; keep stable rule ids and file/line output compatible with future SARIF.
- Sources checked: Git hooks docs, GitHub secret scanning/custom-pattern docs, SARIF 2.1.0 specification.
- Do not repost docs-contract comments unless implementation, docs cleanup, or product decisions land. Useful next docs-contract work is implementation planning or actual runner/rule-pack work.

37. Docs-contract scanner-output sink planning
- Ran aiwg discover "issue-audit docs-contract implementation planning shared scanner rule packs CI profile fixtures baseline SARIF"; top matches were issue-audit and doc-sync. Used issue-audit because this was tracker planning, not a documentation synchronization/fix pass.
- Re-read #1004/#999/#1001/#1002 comments and current docs-contract CI/script state.
- Current repo evidence still shows no `scripts/ci/docs-contract.sh`, no package script, no docs-contract CI step, and no shared allowlist/baseline file. `scripts/ci/forbid-insecure-auth-defaults.sh` prints raw `rg -n` matches; active #999/#1001 hits include token-shaped examples, query-token examples, default DB credentials, `PGPASSWORD=matric`, and credential-bearing Postgres DSNs across docs, scripts, workflows, and MCP model-visible docs.
- New non-duplicative gap found: docs-contract findings, CI logs, tracked baselines, and future SARIF exports can become a new disclosure sink if they store or upload raw matched secret-like literals.
- Posted #1004 comment 71431: scanner output should include file/line/rule/owner/profile/severity/category, but not full matched token/URL/password values by default; baseline entries should use stable fingerprints or normalized finding ids; SARIF/export support should use redaction-aware fields deliberately; regression fixtures should prove redaction for #999/#1001 examples.
- Sources checked: OWASP Secrets Management Cheat Sheet, GitHub push-protection/secret-scanning docs, and SARIF 2.1.0 redaction-aware behavior.
- Do not repost docs-contract advisory/baseline, one-off guard migration, or scanner-output redaction comments unless implementation, docs cleanup, or product decisions land. Useful next docs-contract work is actual runner/rule-pack implementation or product decisions around blocking/advisory mode and redacted baseline format.

38. Inbound connector implementation follow-through
- Ran aiwg discover "issue-audit inbound connector implementation follow-through secret refs redacted DTO activation hosted event admission schema validation"; top match was issue-audit.
- Re-read #988/#968/#920/#974 live comments and current inbound source API/supervisor/connector code.
- Current repo evidence still matches prior comments: `CreateInboundSourceRequest.enabled` defaults true; create persists opaque JSON; list returns raw `config`; `InboundSource` and connector configs derive `Debug`; supervisor starts all enabled rows without validation evidence; SSE/Redis/Kafka configs carry inline URLs/headers/SASL/extra/DLQ settings; event admission only checks non-empty event type and non-null payload.
- Existing comments already cover default-disabled activation, redacted DTOs, secret refs, destination policy, DLQ/payload sinks, and event-admission/schema validation.
- New non-duplicative gap found: `/api/v1/health/streaming` is auth-exempt and returns `inbound` metrics keyed by raw connector `name`. Connector names are operator/API-supplied identifiers and can reveal tenant/source/partner/broker/environment information, plus per-source activity/error/lag counters, even after raw `config` is fixed.
- Posted #974 comment 71386: treat `/api/v1/health/streaming` as a telemetry sink with hosted exposure profile; public health should expose coarse or aggregate status only; per-connector names/activity/lag/errors should require operator diagnostics or safe aliases/opaque ids.
- Posted #988 comment 71388: connector redaction must include display identity, not just raw `config`; define internal id, operator label/display name, source class, and support-safe alias; avoid raw `name` as public metric label/key.
- Sources checked: OWASP Logging Cheat Sheet, OWASP API3:2023 Broken Object Property Level Authorization, OWASP API9:2023 Improper Inventory Management.
- Do not repost inbound connector comments unless code or product decisions change. Useful next inbound work is implementation planning or actual implementation around #988/#968/#920/#974.

39. OAuth introspection/revocation caller-authorization follow-through
- Ran aiwg discover "issue-audit OAuth implementation follow-through DCR CIMD PKCE resource audience scope hosted metadata docs contract"; top match was issue-audit.
- Re-read #1003/#917/#924/#941/#944/#972 comments and current OAuth code/docs/tests.
- Current repo evidence still matches prior OAuth comments: discovery/protected-resource metadata is hand-built and hardcodes open DCR, secret-based token methods, `S256` + `plain`, and `delete`; DCR always creates secret/confidential clients and returns management fields; authorization/token requests and DB rows have no `resource`; introspection returns `aud: client_id`; token endpoint requires Basic/body secret before grant dispatch.
- New non-duplicative gap found: `/oauth/introspect` and `/oauth/revoke` authenticate a client but do not authorize that caller against the submitted token. `oauth_introspect()` calls `introspect_token()` without caller/client/resource context; `oauth_revoke()` calls `revoke_token()` without ownership/resource context; DB methods hash-match any access/refresh token and reveal or revoke across clients. Existing tests do not prove client A cannot introspect or revoke client B's token.
- Created #1005: `fix(oauth): bind introspection and revocation to authorized token clients/resources`. Labels: P1, area/auth, area/rest-api, bug, effort/small, security, testing.
- Posted cross-links: #917 comment 71413, #944 comment 71415, and #1003 comment 71417.
- Sources checked: RFC 7662 Token Introspection and RFC 7009 Token Revocation.
- Do not repost generic OAuth profile/resource/scope/PKCE/DCR/introspection-revocation comments unless code or product decisions change. Useful next OAuth work is implementation planning or actual implementation around #1003/#917/#1005/#924/#941/#944/#972.

40. OAuth #1005 implementation planning
- Ran aiwg discover "issue-audit OAuth implementation planning introspection revocation caller authorization resource audience profile contract"; top match was issue-audit.
- Re-read #1005/#917/#944 current comments and current OAuth introspection/revocation code.
- Current code evidence still shows #1005 can land in two phases: `oauth_token` rows already carry `client_id`, while `oauth_authorization_code` / `oauth_token` do not yet carry protected-resource/audience fields from #917.
- Posted #1005 comment 71419: first PR should add caller-aware repository methods such as `introspect_token_for_client()` and `revoke_token_for_client()`, enforce same-client metadata/revocation for ordinary OAuth clients, preserve `{ active: false }`/200 behavior for unknown or unauthorized tokens, and add client A vs client B tests. Defer broad MCP/resource-server introspection exceptions until #917 stores resource/audience and an explicit first-party/MCP introspection credential policy exists.
- Sources checked: RFC 7662 Token Introspection and RFC 7009 Token Revocation.
- Do not repost #1005 sequencing unless implementation lands or product chooses a different caller model.

41. Inbound connector legacy-enabled quarantine planning
- Ran aiwg discover "issue-audit inbound connector implementation planning secret refs redacted DTO activation destination policy event admission DLQ health"; top match was issue-audit.
- Re-read #988/#968/#920/#974 comments and current inbound API/repository/supervisor/migration code.
- Current code evidence still shows `enabled` defaults true in both `CreateInboundSourceRequest` and `migrations/20260614130000_inbound_sources.sql`; `list_enabled()` returns every enabled row; the supervisor starts rows without validation evidence; no migration currently quarantines existing enabled rows.
- Posted #988 comment 71424: first guard PR needs an upgrade/quarantine path for existing `enabled=true` rows without validation evidence. Hosted/default should fail closed; local/self-hosted grandfathering must be explicit, audited, and bounded. Supervisor start should require current validation evidence even if legacy rows still say enabled.
- Sources checked: OWASP Authorization, Secrets Management, and SSRF Prevention Cheat Sheets.
- Do not repost inbound connector generic activation/redaction/destination/event-admission comments unless code or product decisions change. Useful next inbound work is implementation around #988 comment 71424 plus earlier #968/#920/#974 dependencies.

42. Bridge route/catalog and Tollbooth recheck
- Ran aiwg discover "issue-audit bridge route catalog implementation follow-through OpenAI models route policy Tollbooth compatibility"; top match was issue-audit.
- Re-read live #866/#867/#877/#969/#985 plus Tollbooth/proxy issues #872/#864/#983/#920.
- Current repo evidence still shows no bridge `/v1/models` route, native `/api/v1/models` remains Fortemi operational/admin shape, `ProviderRegistry::parse_slug()` remains permissive for native Ollama-style compatibility, and `ProviderProfile` still lacks typed route/privacy/cache/downstream-router policy fields.
- Rechecked OpenAI Models list shape; current docs still define `object: "list"` with `data[]` model entries containing `id`, `created`, `object: "model"`, and `owned_by`.
- Rechecked Tollbooth upstream; current repo still positions it as a transparent proxy for inspecting/debugging/modifying agent-container traffic and is AGPL-3.0. Existing comments already cover local/dev-only compatibility, traffic persistence/replay/mutation risks, proxy env scoping, local network exposure, and the distinction between optional client-side compatibility and Fortemi hosted methodology.
- No new tracker comment was posted because #866 comment 71252, #867 comment 71254, #872 comments 70649/69940, #983 comments 70899/70504/70189, and #920 comment 70900 already cover the current code and methodology gaps.
- Sources checked: OpenAI Models API reference and Tollbooth GitHub README.
- Do not repost bridge/Tollbooth methodology comments unless bridge implementation, docs, or product policy changes. Useful next bridge work is actual implementation around strict bridge resolver, `RoutePlan`, `/v1/models`, and docs/tests.

43. MCP implementation follow-through recheck
- Ran aiwg discover "issue-audit MCP implementation follow-through harness app factory protocol version legacy SSE Origin session binding"; top match was issue-audit.
- Re-read live #899/#940/#918/#921/#914/#917 comments and inspected current MCP server/test code.
- Current repo evidence still matches the existing issue cluster:
  - `mcp-server/package.json` still runs only `node --test tests/*.test.js`; `test-mcp-connectivity.js` is separate and still real-integration/child-process oriented.
  - `test-mcp-connectivity.js` still initializes with protocolVersion `2024-11-05`, does not send `MCP-Protocol-Version`, and explicitly tests `/sse`.
  - `mcp-server/index.js` still enables wildcard CORS with allowed headers `Content-Type`, `Authorization`, and `MCP-Session-Id`, omitting `MCP-Protocol-Version`.
  - `mcp-server/index.js` still registers legacy `/sse` + `/messages` whenever `MCP_TRANSPORT=http`, and startup output advertises both StreamableHTTP and SSE.
  - Streamable HTTP and legacy SSE session records still store `{ transport, token, type }`; subsequent POST/GET/DELETE and `/messages` use the stored token but do not bind requests to a non-secret auth subject/client/resource/session binding.
  - `/health` exposes aggregate session counts by transport only; this did not reveal a new sensitive-output gap beyond existing diagnostics/logging concerns.
- Rechecked current MCP references. The 2025-11-25 Streamable HTTP transport guidance still requires Origin validation for incoming connections and proper authentication. The 2026-07-28 release candidate still points toward a stateless protocol core, so do not accidentally encode that as current product behavior unless Fortemi explicitly chooses a future-profile follow-up.
- No new tracker comment was posted. Existing #899 comment 71326, #940 comment 71320, and #918/#921/#914/#917 comments already cover the current gaps.
- Useful next MCP work is implementation:
  - choose app/session-manager factory versus child-process mocked-introspection harness;
  - add protocol-profile-aware tests for stable 2025-11-25 sessionful Streamable HTTP behavior;
  - make legacy `/sse` + `/messages` disabled by default or compatibility-flagged per #940;
  - then land `MCP-Protocol-Version`, Origin validation, session binding, and resource/audience work in the #918/#921/#914/#917 order.
- Sources checked: MCP 2025-11-25 Streamable HTTP transports spec and MCP 2026-07-28 release candidate post.
- Do not repost generic MCP comments unless code/product decisions change. If a new agent starts here, move from audit to implementation boundary, failing tests, or product questions.

44. Docs-contract detector precision / false-positive recheck
- Ran aiwg discover "issue-audit continue Fortemi open issue audit implementation planning"; top match was issue-audit.
- Re-read #1004/#999/#1001/#1002 live bodies and comments, then inspected current `scripts/ci`, `.gitea/workflows`, `package.json`, `scripts/pre-commit.sh`, docs, and `mcp-server/index.js`.
- Current repo evidence still shows no `scripts/ci/docs-contract.sh`, no rule-pack directory, no package script, no docs-contract CI step, no pre-commit hook, and no shared baseline/allowlist file. Existing one-off `scripts/ci/forbid-insecure-auth-defaults.sh` still depends on `rg`; this workspace still has no `rg`.
- Focused #999 evidence still shows real scanner-hostile examples in docs and model-visible MCP docs: `mm_at_*`, `mm_key_*`, query `token=mm_at_xxx`, `secret_xyz789`, provider-key placeholders, and `Authorization: Bearer mm_at_xxx`.
- Fresh non-duplicative gap found: a naive first #999 detector for every `secret_` substring would also catch non-credential uses such as `secret_data`, `secret_set`, `client_secret_basic`, and `client_secret_post`. Those are metadata/prose/code terms, not necessarily leaked or scanner-hostile placeholder values.
- Focused #1001 evidence still shows real DB/backup credential drift in docs/config/backup examples plus legitimate test fixture credentials in workflows/scripts.
- Posted #1004 comment 71456: require detector classes and negative fixtures before blocking. Suggested classes include `fortemi_token_placeholder`, `provider_key_placeholder`, `credential_value_in_header`, `credential_value_in_query`, `oauth_metadata_field`, and `noncredential_secret_word`. Blocking should apply to credential-like values in sensitive contexts, not every field/prose word containing `secret`.
- Sources checked: GitHub secret scanning custom-pattern docs and GitHub secret-scanning alert docs, especially custom-pattern dry-run/testing and contextual/paired detection to reduce false positives.
- Do not repost docs-contract false-positive comments unless implementation lands or a product decision changes the first rule-pack scope. Useful next docs-contract work is implementation: runner skeleton, manifest schema, positive/negative fixtures, redacted output, and advisory/baseline mode.

45. Backup JSON import/export operation-state and idempotency recheck
- Ran aiwg discover "issue-audit backup restore artifact lifecycle hosted restore evidence verification implementation planning"; top match was issue-audit.
- Re-read live #980/#978/#927/#991 bodies and comments, then inspected current backup routes, `scripts/backup.sh`, and backup/import code in `crates/matric-api/src/main.rs`.
- Current repo evidence still matches existing #980/#978/#927 comments: scheduled backup verification is local size-only, API backup producers write final-looking artifacts before evidence, inventory computes hashes/manifests on GET, upload/restore paths accept restore-capable artifacts without evidence state, and restore still uses hardcoded local PostgreSQL credentials and synchronous `psql`.
- Fresh non-duplicative gap found in #991 scope: legacy JSON backup import/export remains synchronous and non-idempotent even though docs architecture sketches job-style export/import. `backup_import()` mutates notes/templates/collections as it loops, `dry_run=true` is not a persisted validation/preview operation, `replace` can soft-delete before later failures, and NLP jobs can be queued per item. A retry after timeout/client disconnect can duplicate writes and jobs.
- Posted #991 comment 71461: model JSON backup export/import as long-running operations with `backup_operation_id`, durable state/status/result resources, idempotency key/request-hash handling, persisted validation/preview/apply phases, and no raw note content in operation evidence.
- Sources checked: Microsoft REST long-running operation guidance and the expired IETF Idempotency-Key draft. Treat Idempotency-Key as a convention/reference, not a finalized standard.
- Do not repost backup idempotency/LRO comments unless implementation lands or product chooses to deprecate/disable the legacy synchronous JSON path. Useful next backup work is implementation or product decision: hosted restore API vs operator-local flow, `BackupArtifact` lifecycle schema, and whether JSON backup import/export stays as an async operation or becomes deprecated/operator-only.

46. Docker/native bundle rendered-config preflight recheck
- Ran aiwg discover "issue-audit Docker native distribution bundle default credentials port binding third-party images security implementation planning"; top match was issue-audit.
- Re-read live #989/#990/#992/#982 bodies and comments, then inspected current `docker-compose.bundle.yml`, `Dockerfile.bundle`, `.env.example`, docs, and compose/Dockerfile references.
- Current repo evidence still shows the Docker bundle combines several issue-owned risks:
  - API/MCP ports render as all-interface host publishing via `${API_HOST_PORT:-3000}:3000` and `${MCP_HOST_PORT:-3001}:3001`;
  - bundle/runtime DB defaults still include `POSTGRES_PASSWORD=matric`, `POSTGRES_USER=matric`, and `DATABASE_URL=postgres://matric:matric@localhost:5432/matric`;
  - third-party/helper images still use mutable tags such as `willfarrell/autoheal`, `ghcr.io/speaches-ai/speaches:latest-*`, and Fortemi `bundle-latest` convenience tags;
  - autoheal still mounts `/var/run/docker.sock:/var/run/docker.sock:ro`;
  - `.env.example` says the bundle has sensible defaults but does not define `API_HOST_BIND` / `MCP_HOST_BIND` or one exposure profile tying bind address, auth, issuer URL, DB secrets, and image trust together.
- Fresh non-duplicative gap found: raw grep/lint is insufficient because `.env`, Compose profiles, and overlay files can change the effective deployment. The release/native distribution gate should inspect rendered Compose config, not just source text.
- Posted #989 comment 71462: add rendered-config preflight using `docker compose -f docker-compose.bundle.yml config`. It should fail non-loopback API/MCP exposure unless explicit exposure profile, auth, issuer URL, allowed origins/resource URLs, and non-default DB secrets are present; it should also report #990/#937 profile warnings/failures for mutable images and Docker socket mounts.
- Sources checked: Docker port publishing docs, Docker Compose secrets docs, and Docker image digest docs.
- Do not repost Docker bundle rendered-config comments unless implementation lands or product decisions change the bundle exposure profile. Useful next Docker work is implementation: host-bind variables, generated/secret-file DB credential path, rendered-config CI/preflight, image lock/manifest, and autoheal/socket profile decision.

47. Embeddings/search provider-routing cardinality recheck
- Ran aiwg discover "issue-audit embeddings search provider routing batch limits cache lineage job transactions implementation planning"; top matches included auto-provenance and issue-audit. Use issue-audit for this tracker audit slice.
- Re-read live #995/#976/#979/#975:
  - #995 owns routing background embedding jobs through the effective provider/config resolver instead of startup `OllamaBackend::from_env()`.
  - #976 owns OpenAI embedding batching and current provider limit handling; existing comments already cover max input count/token sum, empty inputs, split batching, and dimension validation at a high level.
  - #979 owns rollback/commit behavior around `store_result` in default embedding-set storage.
  - #975 owns search-cache lineage gaps for mode, embedding_set, provider/model/dimension, active constraints, raw query logging, and semantic/hybrid fail-closed posture.
- Code evidence inspected so far:
  - `crates/matric-inference/src/openai/backend.rs` sends the full `texts` slice in one embeddings request. It sorts response data by provider index but does not visibly validate count equality, complete/unique/in-range indexes, or vector dimensionality before returning vectors.
  - `crates/matric-api/src/handlers/jobs.rs` calls `self.backend.embed_texts(&chunks)` and then zips `chunks.into_iter().zip(vectors)` for storage. If a backend returns fewer vectors than chunks, storage can silently truncate unless another layer catches it.
  - The same default-set branch still commits before checking `store_result`, as #979 already covers.
  - `crates/matric-api/src/main.rs::search_notes` still builds cache keys before mode/embedding_set resolution and still creates query embeddings through direct `OllamaBackend::from_env().ok()`, which may already be covered by #975/#929.
  - `crates/matric-api/src/services/search_cache.rs` hashes only normalized query, tags, and collection id.
- Re-read #682 and #929 before posting; they already own vector length/dimension policy and search query-provider/fail-open behavior. The uncovered part was response count/index integrity and job-level truncation.
- Checked current official OpenAI Create embeddings API reference: response `data[]` includes an embedding `index` described as the embedding's position in the list of embeddings, while `dimensions` remains optional for `text-embedding-3` and later.
- Posted #995 comment 71477:
  - `EmbeddingBackend::embed_texts()` should be all-or-nothing: non-empty input returns exactly one vector per input in logical order or errors with safe metadata.
  - OpenAI-compatible responses should validate complete unique in-range indexes before sorting/returning; split-batch aggregation in #976 should preserve global input order.
  - Job storage should defensively check `vectors.len() == chunks.len()` before `chunks.zip(vectors)` and before any delete/replace transaction; dimension checks should plug into #682/#995 effective contract.
  - Regression coverage should include fewer vectors than chunks, extra vectors, duplicate indexes, missing indexes, and out-of-range indexes; existing embeddings should survive these failures with #979.
- Do not repost embeddings/search cardinality, dimension, query-provider, cache-lineage, or transaction comments unless implementation or product decisions change. Useful next embeddings work is implementation/failing tests across #995/#976/#979/#975/#682/#929.

48. Docs-contract provider-key detector precision recheck
- Ran aiwg discover "issue-audit docs-contract implementation planning shared scanner rule packs CI profile fixtures redacted output baseline"; top matches were issue-audit and doc-sync. Used issue-audit because this was tracker planning, not a docs synchronization pass.
- Re-read live #1004/#999/#1001/#1002/#998 comments and inspected current `scripts/ci`, `.gitea/workflows`, `package.json`, and `scripts/pre-commit.sh`.
- Current repo evidence remains unchanged for the main implementation gap:
  - no shared `scripts/ci/docs-contract.sh`;
  - no docs-contract package script;
  - no docs-contract CI step;
  - no docs-contract pre-commit hook;
  - existing `scripts/ci/forbid-insecure-auth-defaults.sh` still depends on `rg`, while this workspace has no `rg`.
- Focused scans confirmed active #999/#1001/#998/#1002 drift still exists in docs and model-visible MCP docs, but most of that is already covered by prior comments.
- Fresh non-duplicative gap found: a naive provider-key detector for `sk-` catches ordinary hyphenated prose/code words such as `task-specific`, `mask-aware`, and similar non-credential terms. #1004 comment 71456 covered broad false positives generally; #1004 comment 71484 adds the provider-key-specific boundary/context fixture requirement.
- Posted #1004 comment 71484:
  - provider-key rules should require credential context or token boundary, such as env var assignment, auth header, JSON `api_key`, or provider-shaped token at a start/quote/space boundary;
  - add negative fixtures for `task-specific`, `mask-aware`, `risk-based`, and similar hyphenated prose;
  - keep raw matched values redacted in output/baseline;
  - keep the first blocking subset to context-rich findings, with broad token substrings advisory until fixtures are stable.
- Do not repost docs-contract advisory/baseline, one-off guard migration, scanner-output sink, generic false-positive, or provider-key-boundary comments unless implementation lands or product decisions change. Useful next docs-contract work is actual runner/rule-pack implementation.

49. Backup artifact identity and collision-safe publication recheck
- Ran aiwg discover "issue-audit backup restore implementation planning artifact lifecycle evidence sidecar hosted restore idempotency verification"; top match was issue-audit.
- Re-read live #980/#978/#927/#991/#923/#1001 comments and inspected current backup/restore code plus `scripts/backup.sh`.
- Current repo evidence still matches existing comments:
  - API backup/restore paths use hardcoded local PostgreSQL credentials, plain SQL/gzip paths, raw stderr, final-path writes, descriptive `BackupMetadata`, filename-derived trust, and restore eligibility without evidence lifecycle;
  - scheduled `scripts/backup.sh` still sources executable config, defaults to `matric/matric`, writes through temp/final paths, does size-only local verification, and lacks evidence manifests/destination verification;
  - inventory/status/list/info still expose paths/hash/metadata and do work from GETs.
- Fresh non-duplicative gap found: artifact identity is still filename/timestamp based at one-second resolution. API producers use `Utc::now().format("%Y%m%d_%H%M%S")`; `scripts/backup.sh` uses `date '+%Y%m%d_%H%M%S'`; persisted producers write directly to final-looking paths with `File::create`, `std::fs::write`, `cp`, or `aws s3 cp`. Concurrent backups in the same second can collide, truncate, overwrite, or produce sidecar/artifact mismatches.
- Posted #980 comment 71499:
  - add stable server-generated `artifact_id` / `backup_id` independent of display filename;
  - include the id or collision-resistant suffix in final object names;
  - use exclusive create/staging plus atomic publish locally, and no-overwrite/conditional publication or explicit collision evidence for remote destinations;
  - write metadata/evidence atomically with lifecycle state transitions;
  - test same-second snapshots/uploads/pre-restore backups, existing target filename, sidecar write failure after artifact write, and restore/inventory lookup by id rather than ambiguous filename.
- Do not repost backup evidence, inventory, upload/restore trust-boundary, JSON idempotency, lifecycle, or timestamp-collision/artifact-id comments unless implementation lands or product decisions change. Useful next backup work is actual implementation planning or code changes.

50. Bridge native catalog separation recheck
- Ran aiwg discover "issue-audit bridge route catalog implementation follow-through OpenAI models route policy Tollbooth compatibility"; top match was issue-audit.
- Re-read live #866/#867/#877/#969/#985/#983 comments and inspected current model/provider routing code.
- Current repo evidence remains:
  - no bridge `/v1/models` route;
  - no `BridgeModelCatalogEntry`, `RoutePlan`, or `BridgeResolvedModel` type;
  - `ProviderRegistry::parse_slug()` remains intentionally permissive for native/Ollama compatibility and tests lock unknown-prefix fallback to default provider;
  - native `GET /api/v1/models` returns Fortemi operational shape `{ models, defaults, providers }`;
  - native `GET /api/v1/chat/models` returns UI/local chat metadata such as context window, thinking capability, speed estimates, parameter size/family, and `default_model`.
- Existing #866/#867 comments already cover strict bridge resolver, OpenAI-compatible model-list shape, public coarse policy classes, router policy rendering, exact provider slug vs virtual alias typing, and Tollbooth local/dev-only compatibility boundaries.
- Fresh non-duplicative gap found: prior #866 comments emphasized not reusing `/api/v1/models`, but not the second native `/api/v1/chat/models` route. That route is also too local/UI-oriented for ordinary bridge clients.
- Posted #866 comment 71507:
  - bridge `/v1/models` should use an explicit bridge renderer, not native `ListModelsResponse` or `ListChatModelsResponse`;
  - add a negative renderer test proving `/v1/models` does not include native chat/UI fields like `context_window`, `estimated_available_context`, `supports_thinking`, `thinking_type`, `speed_tok_s`, `parameter_size`, `family`, or `default_model`;
  - keep richer local chat/model diagnostics on native/authenticated or operator/admin surfaces.
- Do not repost bridge strict-slug, route-plan, public model-catalog, Tollbooth methodology, native `/api/v1/models`, or native `/api/v1/chat/models` separation comments unless implementation or product decisions change. Useful next bridge work is actual implementation/failing tests.

51. MCP implementation follow-through no-repost recheck
- Ran aiwg discover "issue-audit MCP implementation follow-through harness app factory protocol version legacy SSE Origin session binding"; top match was issue-audit.
- Re-read live #899/#940/#918/#921/#914/#917 comments and inspected current MCP server/test code.
- No new tracker comment was posted. Existing comments already cover the current gaps through #899 comment 71326, #940 comment 71320, #918 comment 71197, #921 comment 70768, #914 comment 70772, and #917 comments 71265/71413.
- Current code/test evidence still matches the existing MCP issue cluster:
  - `mcp-server/package.json` still runs only `node --test tests/*.test.js`; `test-mcp-connectivity.js` remains separate as the live/child-process integration script.
  - `mcp-server/tests/helpers/mcp-client.js` still sends `Mcp-Session-Id` after initialization but does not record or send `MCP-Protocol-Version`, and it has no Origin/preflight override controls.
  - `test-mcp-connectivity.js` still initializes with protocolVersion `2024-11-05`, omits `MCP-Protocol-Version` on subsequent requests, and explicitly tests `/sse`.
  - `mcp-server/index.js` still installs wildcard CORS with allowed headers `Content-Type`, `Authorization`, and `MCP-Session-Id`, omitting `MCP-Protocol-Version`.
  - `mcp-server/index.js` still registers legacy `/sse` + `/messages` whenever `MCP_TRANSPORT=http`, and startup output still advertises both StreamableHTTP and SSE.
  - Streamable HTTP and legacy SSE session records still store `{ transport, token, type }`; subsequent root POST/GET/DELETE and `/messages` use stored token context and do not compare a non-secret auth subject/client/resource/session binding.
- Useful next MCP work is implementation, not another audit comment:
  - choose app/session-manager factory versus child-process mocked-introspection harness;
  - put deterministic HTTP-mode tests in the main `npm test` path;
  - make the harness profile-aware for stable `2025-11-25-streamable-sessionful`;
  - decide #940 legacy-SSE default or compatibility flag before freezing test expectations;
  - then wire #918 `MCP-Protocol-Version`, #921 Origin validation, #914 session binding, and #917 resource/audience validation.
- Do not repost generic MCP harness/session-binding/resource/protocol/Origin/legacy-SSE comments unless code changes or #940/product answers land.

Known current code observations to re-check before acting:
- reqwest clients are not centralized under an outbound policy factory.
- ApiDoc is a broad static utoipa path list and docs/OpenAPI/AsyncAPI routes are public auth-exempt.
- Chat stream replay stores raw delta frame data temporarily for resume.
- Webhook idempotency cache may retain parsed payload content in Redis for the configured TTL.
- Runtime/logging defaults and raw error logging are now covered through #997 71332, #967 71335, and #974 71336; recheck only after code/docs change.

Recommended next audit slices:
1. Docs-contract implementation planning:
   Return after a docs-contract implementation PR, docs cleanup, or product decision lands. The useful next decision is runner/rule-pack implementation and redacted baseline/output behavior, not more audit comments; generic advisory/baseline, one-off guard migration, scanner-output sink, broad-pattern false-positive, and provider-key-boundary findings are covered through #1004 71484/71456/71431/71360, #999 71276, and #1001 71278.

2. Backup/restore hardening implementation:
   Return only when implementation starts or product answers land for #927/#978/#980/#991. The useful next work is actual `BackupArtifact` lifecycle/evidence implementation, stable artifact ids/collision-safe publication, hosted restore policy decisions, or async/idempotent JSON import/export implementation; generic evidence/inventory/upload-restore/artifact-lifecycle, timestamp-collision/artifact-id, and JSON operation-state comments are covered through #980 71499/71440/71348, #978 71442/71350, and #991 71461.

3. Bridge route/catalog implementation follow-through:
   Return only when bridge implementation starts or product answers #866/#867/#985/#983. The useful next work is actual strict bridge resolver, `RoutePlan`, `/v1/models`, and docs/test implementation; generic strict-slug, route-plan, model-catalog, native-catalog separation, and Tollbooth methodology comments are already covered by #866 71507/71252, #867 71254, #872, and #983.

4. MCP implementation follow-through:
   Return when #940 has a product answer or when implementation starts. The useful next decision is harness/app-factory versus child-process mocked-introspection runner with explicit 2025-11-25 profile support; generic MCP audit comments are already covered by #899/#914/#917/#918/#921/#940 through #899 71326, #940 71320, and recheck sections 43 and 51.

5. Embeddings/search implementation planning:
   Return only for implementation or failing-test boundary work. The useful next work is a shared effective embedding contract/fingerprint and all-or-nothing embedding response validation across #995/#976/#979/#975/#682/#929. Generic provider-routing, dimension, batching, cache-lineage, transaction, and response-cardinality comments are already covered through #995 71477 and the related issue comments.

6. OAuth implementation planning:
   Return only after OAuth implementation or product answers land. The current first PR candidate is #1005 same-client introspection/revocation guard from comment 71419, because it can land before #917 resource/audience storage. Broader OAuth implementation order still needs product/engineering decision across #1003 profile/capability builder, #917 resource/audience storage, #924 scope subset/source-of-truth cleanup, #941 S256/public-client auth dispatch, and #944 DCR gating; generic OAuth audit comments are already covered.

7. Inbound connector implementation planning:
   Return only after an inbound implementation PR or product answer lands. Current first implementation concern is #988 comment 71424: changing future defaults is insufficient unless existing enabled rows are quarantined or explicitly grandfathered by profile. Generic config/secret/destination/event-admission and streaming-health identity findings are covered through #988 71424, #974 71386, and #988 71388.

8. Docker/native bundle implementation planning:
   Return only when implementation starts or product answers land for #989/#990/#992/#982/#937. The useful next work is rendered Compose preflight, loopback/default exposure profile, generated/operator-supplied DB secrets, image lock/manifest, and socket/autoheal profile decisions; generic host-bind/default-secret/image-pin/socket comments are covered through #989 71462, #990 70798/70547, #992 71070, and #982 70788.

Questions to ask the user as planning gaps emerge:
- Should hosted public docs be a static/manual product guide only, with generated OpenAPI/AsyncAPI always authenticated?
- If a public generated schema subset remains, should its source of truth be the #710 route/action inventory or separate OpenAPI annotations?
- Should operator full schema remain at /openapi.yaml behind auth, or move to an operator-only path such as /api/v1/operator/openapi.yaml?
- For API errors, should Fortemi first extend the current JSON shape to `{ error/code/message/request_id }`, or move hosted/new endpoints directly to RFC 9457 problem details?
- For bridge/proxy compatibility, should Fortemi support explicit Tollbooth destination profiles in docs/tests, or only document generic OpenAI-compatible proxy behavior with Tollbooth as one example?
- For temporary replay/cache sinks, should DSAR/export receipts disclose only metadata and TTL, or include recoverable cached payload content while it still exists?
- Should hosted restore remain available through HTTP API behind admin policy and confirmation, or be reserved for operator-local/console break-glass flows only?
- For hosted backup restore, should only `verified` artifacts from Fortemi-controlled producers be restorable, with uploaded/imported artifacts requiring an explicit operator verification/promotion ceremony?
- Should backup.conf remain shell-sourced operator code with strict ownership/mode checks, or be replaced by a strict key/value parser?
- Should scheduled backup readiness require only `pg_restore --list` plus manifests, or periodic disposable-database restore drills as an ops/release gate?
- Should default credential examples be allowed anywhere outside explicit local-dev/test fixtures, or should all docs use placeholders even for quickstart paths?
- Should Fortemi emit legacy `X-RateLimit-*` headers in parallel for SDK compatibility, or keep them out of the product contract unless a specific integration requires them?
- Should #1004's docs-contract runner be CI-blocking immediately once two rule packs are wired, or advisory first?
- Should docs-contract rule packs live under `scripts/ci/docs-contract-rules/` as CI-owned rules, under `docs/contracts/` as docs-owned contracts, or somewhere else?
- Should the docs-contract runner avoid `rg` for portability, or should CI/pre-commit explicitly require/install `ripgrep` since existing guards already assume it?
- Should #999 and #1001 be the first blocking rule packs, or should they run advisory until the docs cleanup pass removes existing known matches?
- Should the first docs-contract PR commit a tracked baseline file for known #999/#1001 matches, or should it stay advisory-only until cleanup makes the baseline unnecessary?
- Should SARIF or another machine-readable output be part of the initial docs-contract runner, or is deterministic plain text enough until the rule set stabilizes?
- For secret-like docs-contract findings, should CI/baseline/SARIF output always redact matched literals by default, or is a protected local debug mode enough for maintainers who need full snippets?
- Should OAuth hosted launch prefer CIMD/pre-registered clients first, with protected DCR only as a compatibility mode, or should DCR remain a first-class hosted path?
- Should hosted OAuth metadata have explicit deployment profiles such as local-dev, self-hosted-operator, hosted-strict, and hosted-compat, or should each feature flag independently shape discovery output?
- Should Fortemi implement a single `OAuthDeploymentProfile` / `OAuthCapabilities` source of truth before runtime fixes, or allow direct endpoint edits if contract tests are added later?
- For OAuth implementation order, should Fortemi land the #1003 profile/metadata fixture first as a failing guard, or immediately protect runtime behavior with #944 DCR gating and #941 S256/public-client auth dispatch?
- For remote MCP/ChatGPT public clients, should Fortemi support `token_endpoint_auth_method=none` first, `private_key_jwt` first, or only predefined confidential clients for hosted launch?
- For ChatGPT/remote MCP launch, should public-client `none` be implemented first, or should pre-registered confidential clients be used for first hosted launch while CIMD/private_key_jwt are deferred?
- Should OAuth `resource`/audience support initially be limited to the MCP protected resource, or generalized for API and MCP protected resources at the same time?
- For resource indicators, should the first storage support one canonical protected-resource URI per code/token, or multiple resources via array/table from day one?
- For OAuth introspection, should Fortemi allow only dedicated first-party/resource-server credentials, or should ordinary OAuth clients be able to introspect only their own tokens?
- For OAuth revocation, should client-owned revocation be strictly same-client only, with operator/resource-server revocation on a separate privileged path?
- Should `delete` be promoted to a real OAuth scope, or removed from metadata/docs until #710 AuthorizationPolicy actions exist?
- Should the `oauth_scope` seed table become the canonical source for advertised/accepted OAuth scopes, or should it be treated as migration/reference data behind a separate runtime profile builder?
- If `delete` stays seeded for existing installs but is not a hosted OAuth scope, should migrations mark it inactive/internal, remove it on upgrade, or leave it as historical data while hosted metadata excludes it?
- If browser EventSource query auth remains, should Fortemi issue a distinct short-lived `<STREAM_TOKEN>` class, or allow ordinary access/API tokens in the query string with strict redaction and TTL guidance?
- Should scanner-safe placeholders be centralized in a docs style guide and enforced everywhere, including MCP `get_documentation` strings and generated OpenAPI/AsyncAPI examples?
- Should hosted defaults strip external router response-cache controls such as `X-OpenRouter-Cache` unless tenant policy explicitly opts in?
- Should Fortemi treat provider/model "requires 24h prompt-cache retention" as a model eligibility constraint for ZDR/no-retention tenants?
- Should provider cache/sticky-routing controls be visible to bridge clients as coarse policy classes only, or also exposed in admin/operator diagnostics?
- Should bridge `/v1/models` remain strict OpenAI shape by default with Fortemi policy classes only on opt-in extension/admin endpoints, or should ordinary clients receive safe `x_fortemi_*` extension fields?
- Should models with unverifiable router downstream privacy/region/cache posture be omitted from the ordinary catalog for strict hosted tenants, or listed with a safe `requires_operator_review` / `unknown_fail_closed` class?
- For bridge model strings, should `foo:bar` always be treated as an unknown provider error in bridge mode, or should Fortemi maintain an explicit allowlist of colon-bearing bare model names for local/self-hosted compatibility?
- Should Fortemi implement a typed `RoutePlan`/`BridgeResolvedModel` layer before any bridge request route, or allow the first B1/B2 bridge routes to call the current provider registry directly with strict wrapper checks?
- For MCP tool results, should Fortemi remove generated curl commands entirely in hosted mode and return structured operation metadata instead, or keep placeholder curl examples for local/operator ergonomics?
- For inbound connectors, should hosted mode ban inline connector secrets entirely from day one and require `secret_ref`/secret-store integration, or allow inline secrets only in self-hosted/local profiles while redaction tests land first?
- Should tenant-created inbound connectors be allowed in hosted mode after admin approval, or should Redis/Kafka/SSE connectors be operator-managed infrastructure only for the first hosted release?
- For inbound connectors, should the first implementation PR stop at draft/disabled + redacted DTOs + Debug redaction, or also add validation-evidence columns and supervisor start gating immediately?
- For inbound connectors, should first activation require an event-admission schema/namespace profile, or can early local/self-hosted connectors emit arbitrary `external.<kind>.v1` payloads while hosted mode blocks activation?
- Should connector-derived events be allowed to map into internal event names such as `note.created` or `call_event` through an explicit transform, or should all connector events stay in an external namespace until a separate workflow consumes them?
- Should Kafka `extra` and arbitrary SSE headers be first-class hosted features behind operator approval, or local/self-hosted compatibility only for the first release?
- For MCP HTTP tests, should Fortemi extract an in-process Express app/session-manager factory, or keep `index.js` top-level and run a child-process MCP server against a mocked Fortemi API/introspection service?
- For hosted remote MCP, should legacy `/sse` + `/messages` be disabled by default before auth/origin work lands, or kept enabled behind a strict compatibility flag only after the full #914/#917/#921/#899 test set passes?
- Should the Docker bundle default be strictly local/workstation (`127.0.0.1` API/MCP bind, generated DB secret, mutable-image warnings allowed only for dev), with LAN/shared appliance exposure requiring an explicit profile?
- Should the Docker bundle preflight be a shell script around `docker compose config`, a small Rust/Node checker, or part of the future docs-contract runner?
- Should autoheal remain in the default bundle once Docker socket risk is modeled, or move behind an explicit `ops-autoheal` profile with a documented root-equivalent host-control warning?
- Should the effective embedding contract be persisted as one fingerprint used by embedding jobs, query embedding, cache keys, freshness checks, and restore/reindex decisions?
- Should semantic/hybrid search fail closed when query embedding generation fails, or explicitly fall back to FTS with a response flag and audit/telemetry evidence?
- Should embedding response cardinality/order/dimension validation live in the `EmbeddingBackend` trait contract for every provider, or inside each concrete backend plus defensive job-level checks?
```

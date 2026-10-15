# #1201 public archive authorization

Scope: public hardening and the policy hook only. The grant store and grant
policy remain `Fortemi-Enterprise/rbac#2`. No commit is made by this work.

## Verified issue citations

The issue text was read from the operator-provided `issue-1201.md` before code
inspection. Every cited location was checked against base `47642673`; the
v2026.10.5 line numbers in the issue are historical:

| Issue citation | Verified base location and finding |
| --- | --- |
| `matric-core/src/authorization.rs:435-466` | `crates/matric-core/src/authorization.rs:435`: `RoleBasedPolicy::authorize` checks authentication, tenant and scopes, without an archive visibility decision. |
| `matric-api/src/main.rs:3324-3333` | `crates/matric-api/src/main.rs:3435`: `authorization_policy_for_auth` hardcodes role-based vs allow-all selection. |
| `middleware/archive_routing.rs:327-349` | `crates/matric-api/src/middleware/archive_routing.rs:433`: the single-tenant header branch resolves an archive without a principal. Middleware starts at line 362. |
| `matric-db/src/archives.rs:1282-1297` | `crates/matric-db/src/archives.rs:1330`: name lookup is tenant-bound, without a per-user grant check. |
| `main.rs:23541` overview | `crates/matric-api/src/main.rs:23632`: overview reads the catalog and statistics; its route policy is `TenantObject` at `route_policy.rs:1067`. |
| `route_policy.rs:136` inventory | Still line 136: `/archives` uses `AdminOperator`. |
| `route_policy.rs:143-161` management | Lines 143, 150 and 157: named archive, clone and set-default use `TenantObject`, giving mutations the write scope. Memory aliases share these classes. |
| `external_oidc.rs:192` discarded groups | `crates/matric-api/src/external_oidc.rs:195` persists empty groups after scope mapping; line 208 provisions the stable application-user ID. That ID is absent from the base policy input. |
| `main.rs:11366-11398` archive environment | `crates/matric-api/src/main.rs:11569`: `apply_archive_context_to_policy_input` already carries resolver-derived archive metadata. |
| ADR-110 line 94 | `docs/architecture/adr/ADR-110-external-oidc-identity-pass-through.md:3` is Accepted; the current residual-risk statement is per-user isolation at line 102, rather than the old non-goal wording. |
| ADR-068 future permissions | `docs/architecture/adr/ADR-068-archive-isolation-routing.md:301` retains the future-per-archive-permissions note. |
| ADR-089 / ADR-095 proposed | Both retain Proposed at line 3. ADR-089 documents a partially implemented policy seam; ADR-095 lists `Fortemi-Enterprise/rbac` at line 69. Neither establishes a shipped grant policy. |

## Hosted decision

Hosted owners are not automatically administrators. `resolve_user_principal`
preserves the verified mapped scopes; the pinned `fortemi-auth` claim-policy
implementation (`cc273293`, `claim_policy.rs:233`) only selects mapped/token
scopes, applies client ceilings and filters vocabulary. Existing hosted router
fixtures use `read write mcp` without admin. Archive management and full overview
are still excluded by the hosted tenant-transaction migration gate.

As a compatibility decision, the management/overview admin rule is applied only in
external-OIDC mode. Hosted scope rules and existing migration gates are retained,
and the reader picker is explicitly qualified on a tenant-owned connection.
Local community management continues to use AllowAllPolicy.

## Round 2: independent audit remediation

Read the supplied `audit-1201a.r1.md` and inspected each cited source path in this
worktree, including worker selectors in `handlers/jobs.rs`, the Node MCP
`manage_jobs` forwarding branch, ingest token storage, all affected handlers,
archive middleware, route policies, event publishers and SSE delivery, the policy
selection hook, existing tests, authentication docs, CHANGELOG and OpenAPI source.
The earlier round's passing gates did not prove isolation of these bypasses;
its completeness claims are superseded by this remediation.

The consistent rule is documented in `docs/content/authentication.md`:
authorize the archive actually used at the point of use; global data and shared
namespaces require policy permission for every archive in the caller's tenant
(complete-catalog access), including the built-in public archive. The default
policy still permits every archive, preserving existing scope and behavior.

| Finding | Verified use and fix |
| --- | --- |
| 1: payload-selected jobs, including MCP | `create_job` now resolves the worker's effective schema through persisted tenant catalog metadata before note reads or enqueue. All 22 publicly accepted job types consume `payload.schema` with the historical public fallback; named `memory` hints are also checked. Unknown and denied targets share 404. MCP still forwards to this same REST gate, tested with the actual MCP process and restrictive production API router. |
| 2: job reads, global backup/restore | Job list SQL filters before pagination; summary counts use the same visible archive set. Hidden job IDs share missing-job 404, and unscoped jobs require complete-catalog access. Child batch statistics are fenced by the parent's schema. Whole-database backup/snapshots, global restore, global queue controls/statistics, and the shared backup file namespace require complete-catalog access before use. Named restore also requires complete-catalog access because its dump SQL is not schema-confined, then authorizes its target before checking backup files. |
| 3: ingest tokens | Redis token records capture the minting principal, verified app_user UUID and tenant. Admission resolves the stored schema against trusted metadata and applies the current policy to that identity. Denied/missing archives behave like invalid tokens. Ingest routing no longer uses an unrelated anonymous header/default selection as admission. Successful outbox attribution uses the stored resolved archive. No DB column or migration is necessary; migration immutability was checked with the repository script. |
| 4: query-token enumeration | Archive middleware defers all SSE selection to the handler when query authentication is needed. The handler authenticates before resolving query/header targets. Verified SSE is the only query-token admission path. Regression compares invalid-token requests for existing B and a missing name, including identical status, full body and relevant headers. |
| 5: metadata/name oracles | Archive creation and cloning require complete-catalog access before shared-name/catalog checks. `/memory/info` requires the same access because whole-database and shared blob sizes cannot be truthfully partitioned by archive. Both hidden and absent creation names receive the same 403. |
| 6: local events and legacy WebSocket | Live and replayed global queue/system frames require complete-catalog access even when attributed to A. This covers the production publisher's per-archive copies of global community queue counts. A restrictive local policy refuses legacy WebSocket at upgrade and delivery rechecks close an admitted socket before events from a newly denied archive. |
| 7: docs and OpenAPI | Authentication and CHANGELOG state the effective-target and complete-catalog rules and their boundaries. OpenAPI mutations and aliases document external-OIDC admin/403; SSE archive denial is 404; job submission/detail document their new 404s. Global gates advertise 403. OpenAPI and SHA-256 use the repository generator. |
| Test file size | Existing assertions remain intact, split through explicit Rust module wiring into shared fixtures, management, selection, job, stream and global-operation files, each under 500 lines. |

## Validation

The B-denied policy exercises every item 1–6. Tests include all public job types,
real MCP job forwarding, hidden job detail/count/pagination behavior, default
policy unscoped visibility, stored minting app_user identity, query-token
indistinguishability, creation namespace and storage gates, attributed global
SSE live/replay, and a real legacy WebSocket upgrade.

The final hosted-auth suite, repository migration immutability check and required
Redis verification command/results are recorded in the associated provenance
bundle. Existing tests are neither weakened nor newly skipped.

Round 2 final hosted-auth binary: 1308 passed, zero failed, 1 pre-existing ignored.
Required workspace verification: `binaries=121 passed=4077 failed=0`.

```text
GATES fmt=0 migrations=1 clippy=0 tests=0 docs=0
VERIFY PASS
```

## Round 3: pass-2 audit remediation

The pass-2 audit (`audit-1201b.r2.md`) supersedes Round 2's completeness claims.
Every cited source path was inspected directly. Findings 4 and 5 remain covered.

| Finding | Current fix |
| --- | --- |
| 1 | REST and real MCP submissions reject underscore keys recursively. Extraction strips legacy internal config before deriving trusted paths/checkpoints. Diarization downloads inline bytes in its schema transaction and retains the temporary file until consumed. Grep found no other unscoped worker attachment download; worker isolation tests cover same-ID public/archive attachments and legacy config. |
| 2 | Every SQL dump restore requires complete-catalog access before file lookup or writes, including named restore. No schema confinement is claimed. |
| 3 | Old-format identity-less Redis records retain stored schema/rate under default catalog access. Restrictive policy returns 401 with tokens both required and optional, without header/default fallthrough. Redis remains trusted storage. |
| 6 | A real admitted WebSocket receives an allowed event, then creation of a denied archive closes the socket before its event. Delivery checks the current policy/catalog for every event. |
| 8 | Outgoing webhook management, history and test sends require complete-catalog access. Additive migration stores creating app_user with tenant FK. Dispatch checks the current active user and catalog; NULL legacy owners retain default-policy delivery. |
| 9 | Shared embedding config reads/mutations and global inference config mutation/audit require complete-catalog access. Exports include only archive-declared/referenced configs; default export excludes B-only config and includes it for B. |
| 10 | Locked `npm ci --ignore-scripts --no-fund --no-audit` runs before hosted tests; no action/image references added. |
| 11 | Inventory SQL again orders by `created_at DESC`, tested for archives, memories and the picker. |
| 7 | Authentication, CHANGELOG and generated OpenAPI describe these exact gates and compatibility boundaries; hash generated by the repository script. |

No schema/profile version or consumer wire-contract change: this repairs export
visibility through the existing archive-local declarations and dependency closure.
Round 3 final validation (disposable PostgreSQL and Redis, required TMPDIR):

- Required hosted command at port 55481, features `hosted-auth,kms-vault`:
  `test result: ok. 56 passed; 0 failed; 0 ignored; 0 measured; 1257 filtered out; finished in 116.11s`
- Full `cargo test --locked -p matric-jobs -- --test-threads=1` through testdb
  at port 55482: 618 passed across 10 test binaries; zero failures; one existing
  ignored doc example. Both new worker isolation tests pass.
- Required `verify-redis.sh "$PWD" 55491`: `binaries=121 passed=4077 failed=0`.
- Migration immutability and OpenAPI checksum checks passed; no new unpinned
  workflow actions/images; every new file is below 500 lines; no commit made.

```text
GATES fmt=0 migrations=1 clippy=0 tests=0 docs=0
VERIFY PASS
```


## Round 4: structural isolation and pass-3 remediation

This round supersedes the earlier completeness claims above. Pass-3 source:
`/home/roctinam/.cache/claude-work/fortemi-srvfix/audit-1201c.r3.md`.
No schema/profile/version change or consumer conformance expansion is claimed;
ADR-102's archive-validation and atomic-apply contracts remain unchanged.

The following Round 4 claims are historical; the pass-4 audit identified the
runtime matching, backup, deduplication and ordering gaps corrected in Round 5.
Every inventoried path requires an `ArchiveIsolation` constructor argument.
The existing router inventory check plus a dedicated isolation coverage test
reject routes missing metadata. Missing inventory rows or request policy input
also fail closed through the same runtime gate. A sentinel handler regression
checks that a restricted caller cannot execute such an unclassified route,
and that OPTIONS handlers on CompleteCatalog paths are gated too. CORS
preflight responses remain handled by the existing outer CORS middleware.
The final inventory has 236 paths: 193 CompleteCatalog, 18 CatalogFiltered,
3 ArchiveScoped, and 22 Public. Central admission checks `CompleteCatalog`
before resource normalization, synchronization, handler extraction/execution,
including bearer-exempt routes. Unknown/denied named targets still conceal
existence with 404; an allowed target lacking full catalog authority gets 403.
The built-in policies explicitly declare universal tenant-catalog selection;
restrictive/custom policies default to checking actual catalog/grants on each
admission. Thus built-in admission and coarse-probe behavior are preserved.

Only verified paths receive `ArchiveScoped` or `CatalogFiltered`; remaining
paths are `CompleteCatalog`. A class covers all methods on a path; an
unverified method makes the whole path `CompleteCatalog`. The mixed note
list/create path and reconstruction remain conservatively gated because their
shared-configuration/reconstruction boundaries have not been qualified here.
`Public` is limited to no-archive protocol/identity/coarse surfaces. `/health`
is `CatalogFiltered`: restricted callers get only status/version/lifecycle,
before queue reads or metrics construction; full-catalog callers retain the
existing response. Detailed health is `CompleteCatalog`.

Pass-3 #2 (shared backup download/upload), #9 (embedding-set binding plus all
JSON/multipart shard imports including skip/merge), #12 (shared calls), #13
(set-default aliases, receiver/connector/document-type state), and #14 (health)
are covered by central admission and production-router regressions. The backup
regression uses actual tar/multipart bytes and asserts the shared file is
unchanged. Shared-config import tests use a real exported full-profile archive,
try every conflict strategy, and assert no reference is created in A. The call
regression seeds a real shared session/transcript and checks full access still
returns its content under the built-in community policy. External OIDC's
pre-existing normalization gate still refuses these unqualified named-resource
routes, including with complete-catalog selection; streaming health is the
positive full-catalog OIDC control. Malformed UUID/JSON requests prove admission precedes
handler extractors. Full-catalog streaming health supplies a positive control.

### Worker database sweep (pass-3 #1)

Inspected every database receiver/call chain and raw SQL executor in production
`crates/matric-api/src/handlers/jobs.rs`, its document-type helper, and all
`crates/matric-jobs/src` handlers/helpers (excluding test fixture SQL):

- API AI revision/contextual revision, embedding, title generation, linking,
  purge, context update, reference extraction, concept tagging/related-concept
  inference, metadata/EXIF/document-type inference, bulk re-embedding,
  refresh/analyze/build-index, and graph maintenance.
- Jobs extraction, speaker diarization/relabel, audio/chunk transcription,
  keyframe vision/character/setting/assembly, view vision/assembly, sprite,
  media optimization, attachment scanning, sidecar and usage helpers.
- Worker queue/follow-up/progress/completion/retry, hosted dispatch/execution/
  recovery, and inbound connector supervisor were separately checked as shared
  control-plane operations. They are not content reads using an archive default.

Fixed all unscoped content repository calls found: contextual similarity (both
plain and strict-filter), linking title search, both concept-boost strategies,
wiki-link creation, context-update linked-note fetch, related-concept relation
creation, purge set-stat refresh, and vector-index row-count collection now use
transactions begun by the job's schema context. New transaction-aware strict
similarity and bulk SKOS methods retain transaction scope on fallback paths.
Remaining content SQL and repository methods already use schema-context
transactions, including graph maintenance closures, provenance and stored bytes.

The vector-index administrative DDL boundary remains an explicit exception to
transaction execution: PostgreSQL `CREATE INDEX CONCURRENTLY` cannot run in a
transaction. Its existing database facade validates the running persisted job,
tenant/archive ownership, schema name and index shape, then uses qualified
identifiers. Shared embedding configurations are read through authorized local
set references. Queue/outbox and operational DDL are not reclassified as
archive content. No arbitrary payload is promoted to administrative authority.

The new worker regressions execute actual linking/context workers with matching
note UUIDs in A and public, inspect the generation request and saved revision,
and assert wiki links are written only to A. Concept boosting checks A's tag
map and resulting score. Existing adapter-input regressions continue to test
sanitized internal controls and A/public byte collisions without skip paths.

### CI and documentation (pass-3 #7, #15)

`archive_input_isolation_test` now runs explicitly alongside
`worker_integration_test` in integration, coverage, ci-builder, platform runner
and quick-start commands. OpenAPI derives `x-fortemi-archive-isolation` and
complete-catalog 403 responses from executable metadata. Authentication,
hosted qualification and CHANGELOG describe conservative admission, rather
than claiming every schema-capable route is available to restricted callers.
Validation results and final hashes are recorded after the required commands.

### Final Round 4 validation

All required commands used `TMPDIR=/home/roctinam/.cache/claude-work/fortemi-srvfix/tmp`.
The hosted run used disposable `redis:7-alpine` and port 55531; the jobs run
used 55541; the required Redis verifier used 55551. The hosted filter includes
the two actual worker regressions and all six new route regressions. Existing
assertions were retained and no tests were newly skipped. Jobs retain their
one pre-existing ignored documentation example. Initial new-fixture failures
(PostGIS, tenant-configured pool, column-order copying and positive-control
qualification/declaration setup) were corrected before these final results.

```text
test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 1258 filtered out; finished in 125.79s
test result: ok. 577 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.88s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.62s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.50s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.78s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.26s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.06s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
binaries=121 passed=4078 failed=0
GATES fmt=0 migrations=1 clippy=0 tests=0 docs=0
VERIFY PASS
```

OpenAPI and its hash were regenerated using `scripts/ci/openapi-contract.sh
generate` (also run by the required verifier). Workflow YAML and shell syntax,
`git diff --check`, provenance hashes and the fewer-than-500-lines limit pass.
Migration immutability passes; that script compares committed migration history,
so the untracked additive creator migration is additionally exercised by the
179-migration disposable bootstrap and the verifier's migration gate.
No commit, tracker write or external publication was performed.


## Round 5: pass-4 findings 1–6

Source: `/home/roctinam/.cache/claude-work/fortemi-srvfix/audit-1201d.r4.md`.
The changes remain uncommitted and preserve prior work in this worktree.

1. The named memory SQL-export handler checks the effective schema before
   credential setup or `pg_dump`. Public and schemas containing shared-control
   tables require complete-catalog access, using the existing shared-table
   registry rather than policy identifiers. Ordinary non-public archive exports
   remain archive-scoped. The regression exercises a custom policy admitting
   public and ordinary operations while denying B, plus a non-public schema
   containing a shared table and an ordinary non-public export positive case.
2. Both shared queue dedup predicates include trusted database tenant and the
   canonical worker-effective schema, treating missing/empty schemas as public.
   REST stores its resolved schema; PATCH pipelines and API/extraction follow-up
   producers retain effective schemas, including public. Regressions cover
   B-pending/A-created and A-pending/B-created note-less submissions, existing
   same-archive suppression, PATCH with a matching foreign-archive note UUID,
   running jobs, legacy public fallback, and both predicates across tenants
   using a superuser pool so RLS cannot mask a missing tenant predicate.
3. Archive routing begins with isolation admission before cache mutation or
   either community schema-sync branch. Read-only named-target resolution may
   precede admission to retain denied/missing 404 equivalence. The regression
   leaves a stale schema and empty default cache unchanged after explicit/default
   refusals. A per-request counter proves neither routing branch attempts sync,
   including a sync that would fail; positive requests exercise both hook paths.
   An explicit sync in a rolled-back owner transaction also proves the schema
   version would observably change.
4. Runtime isolation uses exact Axum MatchedPath template plus method; unknown
   templates/methods and missing MatchedPath require complete catalog. Inventory
   construction requires class/method metadata, while ordinary Router::route
   registration remains independent. Source scans include production modules;
   the production-router test probes scanned registrations and verifies actual
   MatchedPath identities against exact inventory entries. Swagger's trailing
   slash and catch-all registrations have explicit rows (238 paths total).
   A separate router regression refuses an extra static notes/private route
   beside notes/{id}, an unlisted method, and a fallback without MatchedPath.
5. Federated all search again uses public first followed by main's registry
   order, preserving memories_searched and equal-score ordering. The overview
   double-count correction remains and is explicitly documented as a bug fix;
   its regression checks one public note, one breakdown, and consistent storage
   and note totals.
6. Authentication documentation and CHANGELOG describe the exact runtime
   boundary and independent registration/inventory model. OpenAPI documents the
   conditional SQL-export 403 and is regenerated with its SHA-256 contract.

Final validation: hosted suite 71 passed, worker suite 619 passed, route-policy
suite 27 passed. Full verification: binaries=121 passed=4079 failed=0.
`GATES fmt=0 migrations=1 clippy=0 tests=0 docs=0`; `VERIFY PASS`. Exact result lines and log paths are recorded in
the provenance bundle. No tests are weakened or
newly skipped; the pre-existing ignored worker doc example remains unchanged.

## Round 6: pass-5 admission order and documentation

Source: `/home/roctinam/.cache/claude-work/fortemi-srvfix/audit-1201e.r5.md`.
Archive-create POSTs on both aliases now have `CompleteCatalog` inventory rows;
list GET/HEAD rows retain `CatalogFiltered`. Policy input and generated OpenAPI
select the method's row. Early isolation admission resolves a memory backup's
persisted target read-only and requires complete catalog for public/shared-table
schemas before routing, cache mutation or synchronization. Handler checks remain.

The sweep covered all 18 prior `CatalogFiltered` rows:

| Prior rows | Admission and remaining catalog decisions |
| --- | --- |
| Archive/memory list-create (2) | Create POSTs now require complete catalog early; lists filter visible archives. |
| Named resources/stats, available and overview (6) | Named targets resolve read-only before routing; inventory/statistics filter visible archives. No handler-only complete-catalog refusal. |
| Memory backup (1) | Early persisted-target/shared-schema decision, repeated by the handler before dump setup. |
| Jobs list-create/detail (2) | List checks select a schema filter. Detail visibility, including the unscoped complete-catalog decision, now runs read-only before routing and repeats in the handler. Submission checks its effective payload schema. |
| Archive pause/resume (2) | Explicit target authorization; no complete-catalog decision. |
| Events/token mint (2) | Archive selection and per-event filtering; complete-catalog checks filter shared/system frames at delivery. |
| Ingest stream (1) | Legacy-identity catalog admission precedes stream writes; routing already bypasses header/default selection and synchronization. |
| Federated search (1) | Explicit targets/result filtering; no complete-catalog refusal. |
| Health (1) | Catalog authority selects coarse vs detailed response; restricted callers receive the coarse response. |

The sync-counter regression covers explicit/default create refusals for both
aliases and unscoped-job detail (GET/HEAD and URL-encoded IDs), preserving its
observable-sync positive controls.
A public+A-allowed/B-denied policy checks GET/HEAD backups of public and a
non-public schema holding a shared table. Every refusal asserts zero sync
attempts, unchanged stale schema version and an untouched default cache.
Authentication docs and CHANGELOG distinguish early classification/conditional
admission from handler defence in depth and result/delivery filtering.
The production probe compares exact distinct template sets, with separate
method-identity uniqueness assertions. The exported-document test checks list
GET/create POST classifications for both aliases and preserves the OAuth wildcard's
Public classification. Generated OpenAPI changes only those two POST annotations.


Final Round 6 validation uses `TMPDIR=/home/roctinam/.cache/claude-work/fortemi-srvfix/tmp`, disposable redis:7-alpine,
and PostgreSQL ports 55651 (hosted suite) and 55661 (required verifier).

```text
test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 1259 filtered out; finished in 190.38s
binaries=121 passed=4079 failed=0
GATES fmt=0 migrations=1 clippy=0 tests=0 docs=0
VERIFY PASS
```

Logs: `/home/roctinam/.cache/claude-work/fortemi-srvfix/hosted-1201-r6-final.log` and `/home/roctinam/.cache/claude-work/fortemi-srvfix/verify-1201-r6.log`.
The final source snapshot stayed unchanged through both commands. No matric-jobs
files changed in this round. All new files remain below 500 lines; no tests were
weakened or newly skipped, and no commit was made.

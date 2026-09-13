# Typed Metadata Search Integration Impact

Status: implementation candidate for Fortemi#1091 / Core#405, not released.
Authority: Fortemi candidate predicates schema and shared SQL corpus in
`contracts/metadata-search/candidate/1.0.0/`. Suite authority/profile ADR and
ADR-102 remain controlling; no Knowledge Shard format or profile changes.

## Changed Boundary

Cycle93 adds the producer selected-memory context read candidate and tenant-
qualified default uniqueness. It addresses the dependency exposed by Cycle92,
not the legacy inventory handler migration. HotM adoption/authority pins and
actual-daemon client/SSE acceptance remain pending. See
`selected-memory-context.md`; Core runtime, event authority, named profiles,
full-lane scope and public readiness are unchanged.

Cycle91 adds typed database retries, bounded read-only note metadata coupled to
fenced completion, and schema-based hosted SSE filtering. The Cycle91 checkpoint
owns native/binary/parser evidence. No Core runtime, wire authority, schema/profile,
consumer pin or readiness promotion. HotM application archive-name/schema mapping,
remaining handlers/follow-ups/index/queue, full lifecycle, consumer/CI/delivery/
release gates and suite NO-GO remain. See hosted-worker-transactions.md for the
explicit completion transaction and event-delivery boundaries.

Cycle90 connects that handler to an explicit hosted daemon registry and bounded
committed claim drain. Progress/terminal/retry/cancellation retain attempt fences;
the event bridge uses claim-bound tenant/archive context without raw queue reads
or writes. Native worker tests cover pagination/fairness, pause, errors, panic,
timeout, deferred settlement rollback, shutdown/replacement and suspension.
Actual-binary and installed-Core evidence belongs to the Cycle90 checkpoint.
Remaining handlers, follow-up and note.updated events, scoped queue summaries,
live SSE/lifecycle/named-profile/consumer/CI/release gates remain. No capability,
wire authority, consumer pin or suite NO-GO promotion.

Cycle89 implements a pool-free hosted DocumentTypeInference handler with shared
detection precedence, scoped active configuration, locked content snapshots and
atomic assignment/access/provenance. A completed job-bound activity supports
retry replay without repeated content effects. Native membership queue/statistics
triggers now preserve tenant/archive routing without public-table fallback and
use native UUID generation. This is one handler implementation, not activated
hosted claim drain, complete follow-up execution or tenant event delivery.
Wire authorities/pins, Core runtime/package and public readiness are unchanged.
Final test and cleanup receipts are in the suite Cycle89 checkpoint; overall
acceptance remains NOT_PASS and suiteNO-GO.

Cycle88 adds claim-bound content transactions and an explicit hosted handler/
context contract. Native note writes under a typed fixture verify tenant/archive
selection, catalog guards, final scope/attempt checks and rollback on error,
revocation, suspension, cancellation, timeout and commit failure.46unit+11DB+3API
functions pass; actual-binary/unchanged installed Core220groups/293requests remain
PASS (278binary/15auxiliary), healthy startup and580durable audit rows. Production
handlers, claim drain, follow-ups and event routing are not migrated. Content and
terminal commits remain separate. Core runtime/package, wire authorities/pins,
readiness false and suiteNO-GO remain unchanged.

Cycle87 adds a bounded service-authorized claim dispatcher, explicit nonempty
handler allowlist and immutable post-commit claim capability with short fenced
settlement transactions.38worker+8queue unit and10DB test functions pass, including
commit-time failure rollback, independent-tenant progress, registry errors,
deadlines, concurrency and late callbacks. Recovery shares the tested registry
helper. This is not yet connected to production handler execution; content access,
pending counts, callbacks/events and full lifecycle remain migration work. Wire
authority/pins and Core runtime/package bytes remain unchanged; suiteNO-GO.

Cycle87's actual-binary/unchanged installed-Core regression remains PASS:
220groups/293requests,278binary/15auxiliary, all7startup health markers,
580durable audit rows and graceful exit. This does not activate the dispatcher
in production handlers or clear the remaining full worker qualification gates.

Cycle86 wires bounded tenant-registry selection and scoped stale recovery into
actual worker startup/periodic paths. The new recovery matrix and prior queue
regressions pass. The actual hosted binary and unchanged installed Core pass
the220group/293request matrix again, now with healthy startup recovery. Durable
audit, mandatory bootstrap dependencies and cleanup remain verified. The health
gate also rejects partial recovery failures. This repairs the Cycle84 recovery
finding in the candidate; unscoped claims, handlers, callbacks and tenant events
remain unqualified, as do full lifecycle/cache/EE/performance/consumer/release
gates. Wire authority/pins and Core runtime/package bytes are unchanged.

Cycle85 adds tenant-transaction queue claims/recovery and UUID-fenced progress,
completion/retry/failure in matric-db. The scoped matrix and legacy queue tests
pass under non-bypass RLS; Core runtime and all wire authorities/pins are unchanged.
This is not connected to production worker dispatch or handlers yet. Cycle84's
actual-binary worker-health failure is not cleared. See
[the worker migration impact](hosted-worker-transactions.md) for authority,
remaining control-plane/handler/event/lifecycle work and rollback constraints.

Cycle84 launches the actual hosted-auth,kms-vault binary with private PostgreSQL,
HTTPS issuer, Redis, ClamAV and OpenBao. The unchanged installed Core candidate
passes220check groups/293requests:278reach the binary;15remain explicit auxiliary
policy/TLS/cold-JWKS router controls. All five Redis stores initialize,94forced-RLS
table assertions pass, tenant audit writes persist and shutdown exits cleanly.
Transit fixture permission now matches the documented local-DEK encrypt/decrypt
contract with context/AAD, while provisioning and rotation remain denied.

Overall acceptance is NOT_PASS: the actual worker receives HostedUnscoped but
recovery queries tenant-RLS job tables without a tenant. Fresh hosted connections
reproduce42704; reused request connections with empty scope reproduce22P02.
The owned probe separately shows42501 under valid scope, then zero recovered jobs
after narrow job-state UPDATE grants. That probe scope is reset and never applied
to the binary. Required worker success remains a hard final assertion; do not
disable the worker, default hosted sessions to the personal tenant, weaken RLS,
or treat /readyz plus the HTTP matrix as full hosted-runtime qualification.

This cycle changes test harnesses and integration evidence, not product runtime,
schemas, migrations, Core package bytes or capability flags. Hosted search
deliberately bypasses its Redis result cache, so enabled cache startup is not TTL
or cached-authorization acceptance. Tenant-aware worker dispatch/recovery and
handler execution, lifecycle/performance/every-consumer/delivery gates remain.
See suite Cycle84 acceptance-summary.json and the linked producer/Core reports.

Cycle82 explicitly selects production ClerkProvider authentication for the
installed HTTP fixture, including real HTTPS and PgTenantStore. Sixteen negative
token forms exercise search/detail/resolution; private trust, issuer/key outages,
cached-key rotation and live tenant-status changes are separate acceptance cells.
Production code and dependencies are unchanged. The pinned600-second JWKS cache
does not refresh for an unknown kid while warm; old keys remain usable until
expiry or a different JWKS URI. This observed limit is not immediate revocation.
No production binary bootstrap, EE policy atomicity, TTL expiry, lifecycle-worker
or released-runtime claim follows. See the Cycle82 report and producer EVIDENCE.

Cycle81 adds a private-loopback installed-consumer test, with the real search,
note-detail and resolver handlers behind existing middleware and non-bypass RLS.
The unchanged Cycle80 Core package uses native fetch with no response mocking or
query rewrite. All four units, three modes, exact UTF-8, typed resolution scope,
tenant/archive/policy denial and stale/deleted state are covered. The owned role
needs access-counter UPDATE and access-log/membership INSERT for existing detail
side effects; content updates and RLS bypass remain ungranted. Native PostgreSQL
treats an initial BOM as part of the adjacent lexeme, so the fixture preserves it
on a separate word from the search term. No product SQL, migration, schema,
OpenAPI or Core runtime change is made. Fixture authentication and SQL deletion
do not qualify JWT/JWKS or lifecycle workers; delivery and full claims stay gated.

Cycle80 Core adoption adds the explicit remote resolution operation, a distinct
candidate receipt and the unchanged 21-case wire corpus. Serialized request
validation, no-store/redirect denial, bounded UTF-8 response checks, deadline and
caller abort preserve the producer boundary without fetching newer note detail.
Existing auth/archive headers supply transport context; arbitrary caller scope
is rejected. The GET search subset and old receipts remain unchanged. Source and
clean candidate gates do not qualify launched/released producer-to-consumer
acceptance, complete capability or the suite NO-GO. No producer runtime, schema,
corpus, generated OpenAPI or migration changes are required by this adoption.

Cycle79 adds current-storage producer resolution and a separate candidate POST
request/response schema with21 shared wire cases. It uses read route admission,
the existing normalized note-read authorization/audit gate and the same request
tenant/archive connection, never a second pool or search cache. A demonstrated
route-allow/note-deny bypass in the first candidate is fixed by the explicit
target-note policy check. Missing/stale/denied evidence shares one unavailable
response. SQL checks exact native unit/source identity and bounds raw text before
the pure digest/range check. No stored migration or historical receipt changes.

Scope is the installed policy plus tenant RLS, not a claim of full EE RBAC or
principal/share-grant implementation. Backing-note authorization precedes the
unit/source statement on one request transaction; concurrent external policy
revocation is not made atomic by text hashing. Core GET search adoption remains
historical and unchanged; the new resolution endpoint needs its own consumer
receipt and clean live/installed/released acceptance. Complete capability remains
false. Rollback removes the candidate endpoint/consumers together without
rewriting stored text, identities, source tuples, applied migrations or releases.

Cycle78 makes the candidate REST schema consumable by strict Ajv as well as
Rust without disabling compiler checks: object types and conditional property
declarations make existing implications explicit. Core registers the predicate
file location alongside its distinct canonical $id, keeping offline resolution
and all original authority bytes except this additive REST schema clarification.
The remote adapter validates the full result before projection/detail requests,
then retains semantic note/total/evidence checks. Its q/mode/limit/tags request
subset and100-hit bound are unchanged. A separate REST receipt preserves prior
evidence-only receipts; no negotiated capability or immutable pin is promoted.

Cycle77 defines the producer's complete candidate GET query and REST result
shape in `search-rest.schema.json`, with20 portable request/response vectors.
The API generator embeds and locally relocates all four search schemas without
discarding conditional/unevaluated-property constraints or reordering unrelated
YAML. Native serialization additionally checks finite scores, exact returned
totals, same-note chain identity and explicit degradation consistency. Finite
diversity and limit validation run before cache/inference/storage access.
Legacy mode fallback, nullable strict tag count and display-only chain fields
are preserved. Core's smaller request limit is not the server's 1000-hit limit.

The full REST artifact remains candidate-unpublished. The prior evidence-only
consumer receipts are unchanged and do not certify this new request/result
authority. Adopt all declared consumers, capture clean live HTTP, finish scoped
resolution/lifecycle/cache/JWT/JWKS/native-index and released-runtime gates before
pin or capability promotion. This search boundary is independent of ADR-102's
registered shard profiles and does not alter static-index or persistence formats.

Cycles72-74 add unpublished locator/envelope authorities and shared55/36case
corpora. Core's six actual citation failures now pass with ranking-statement
projection, fusion and current-storage resolution; this is not released parity.
Cycle75 adds validated producer SearchHit evidence with explicit-null rejection,
same-note input/output validation and content-free Debug. Actual scoped lexical
and vector queries bind whole matched title/current/completed-attachment units
and the selected stored embedding ID/index/text in one PostgreSQL snapshot.
Source identity hashes use the existing tenant/schema/namespace/key digest,
never raw external keys, and selected source predicates constrain projection.
RRF/RSF/dedup retain canonical sets; MMR preserves them. Missing legs and
unrepresentable text stay explicit omissions, with at most64 locators per hit.
Attachment-only eligibility/ranking now uses the same tenant/completion scope.
No stored schema, applied migration, advertised capability or published contract
changes. Full request/response authority, producer database resolution and the
complete hosted/cache/deletion/purge/installed/released matrix remain open.

Before Cycle77, OpenAPI's candidate evidence type referenced the exact unpublished JSON Schema
authority so its conditional constraints are not flattened to a permissive
object. Cycle77 replaces that unresolved API reference with a local schema bundle;
published authority and full consumer/live fixtures remain gates.
Legacy raw repository helpers emit no evidence. The chain-info zero-index/count
heuristics remain legacy display metadata, not citation authority; clients must
use the validated native unit locators. Do not promote either as full parity.

The candidate REST query adds `metadata_predicates`, a JSON string containing
the schema's bounded conjunction. Invalid or unindexed paths fail before query
embedding work. No slow-scan authorization or fallback is introduced. Exact
wire request/result schemas, generated OpenAPI and citation locators must be
completed before promoting this candidate directory to an advertised contract.

The request transaction is the execution boundary. Tag notation resolution,
embedding-set/profile lookup, lexical/vector retrieval and MMR use its tenant
and archive context. Hosted requests do not reuse the personal FTS cache or
notation-only tag cache. Per-archive raw search pools are removed from the REST
path. Personal mode retains its existing request-schema transaction helper.
Missing hosted transaction state fails closed before cache access or inference.
Cycle44 adds only GET `/api/v1/search` to the hosted readiness gate. The earlier
candidate handler was unreachable through authenticated production middleware.
POST search and other unmigrated routes remain unadmitted.

`SearchCandidateScope` applies author metadata, source-run identity, strict tags,
legacy tags/time/collection, embedding-set membership, archive flag and deletion
before lexical/vector top-k. Set-scoped vectors must belong to the same set as
the note membership constraint. Unified tag filters reuse the strict builder,
including string tags and unsatisfiable-filter semantics. Scope narrowing is not
authorization: verified authentication and database RLS remain required.

Only non-NULL vectors enter semantic top-k or the MMR vector lookup. Pending
embedding rows remain stored and may still contribute lexical content; they
cannot displace usable semantic matches through NULL-first score ordering.

Cycle45 consolidates the public `HybridSearch::search` and `search_filtered`
methods onto the same pipeline even when metadata predicates are absent.
Duplicate post-limit filtering and unbound MMR helpers are removed. The trait
and builder convenience methods acquire exactly one pool connection and retain
its existing tenant/archive context; they do not authenticate or install scope.
Hosted callers must still use the request-bound connection entry point. Raw
repository nearest-neighbor/keyword helpers are not the canonical filtered
search API and are not covered by this entry-point claim.

Retrieval/fusion/MMR counts and durations are emitted in the shared pipeline.
Instrumented spans explicitly omit raw query, filter, vector and configuration
values; only bounded classification, length, weights and count fields are named.

## Compatibility And Rollback

- Producer: Fortemi REST and in-process search. Declared consumer: Core#405
  PGlite/RecordStore plus capability-gated adapters. Existing Core type-coercing
  SQL and indexes on generated metadata remain incorrect and must be replaced.
- The REST field is additive. Invalid predicate input becomes a clear failure;
  legacy consumer behavior is not accepted as the target contract. No consumer
  pin, capability advertisement or skew window is promoted by these source edits.
- One forward migration adds bounded expression indexes and immutable versioned
  key functions to author metadata plus source-identity indexes. Data values are
  unchanged; exact comparisons recheck bounded keys. Do not modify key-function
  semantics in place once indexed. Older binaries can ignore the extra indexes;
  rollback does not require deleting data or rewriting applied migrations.
- No static-index, shard-transfer or live-persistence schema merger is implied.
  No schema/profile tuple changes or broad portability claims are authorized.
- The tenant embedding-name migration removes legacy global single-column
  uniqueness for set name/slug and configuration name, replacing it with
  tenant-qualified constraints. It covers registered archive copies and uses
  the archive-DDL advisory lock; future archives clone the corrected catalog.
  Rows, UUIDs and relationships are unchanged. There is no shard wire-format or
  profile change. Once separate tenants reuse names, the old global constraint
  cannot be restored without a separate data decision; do not delete or rename
  tenant data as rollback. Older unscoped hosted lookups are not safe rollback
  consumers. Personal single-tenant behavior and same-tenant uniqueness remain.

## Evidence And Remaining Gates

Cycle43 full native migrations (with deployment PostGIS prerequisite) and real
retrieval fixtures use only synthetic vectors in an isolated PostgreSQL18 cluster.
Seventy-five checks cover registered paths, modes, language strategies, stronger
excluded candidates, MMR and tenant exclusion. The separate Cycle42 SQL corpus
covers122 comparisons/scope cases and natural index plans. Neither establishes
hosted HTTP admission, multi-archive REST behavior or consumer compatibility.

Cycle44 adds the real auth/tenant/archive/authorization/handler stack with a
fixture HostedAuthenticator, one-connection non-bypass runtime pool and local
synthetic OpenAI-compatible embedding endpoint. Seventy-two successful requests
cover six paths, three modes, two tenants, archive switching, same set/profile
names, strict tag resolution, exclusions and NULL-vector/MMR cases. Three auth
negatives and two invalid/invisible before-inference cases pass. Role and local
endpoint cleanup complete. This is in-process HTTP router evidence, not launched
production HTTP, JWT signature/JWKS, enabled Redis or model-quality acceptance.
The migration test separately proves old catalog upgrade, row/ID preservation,
cross-tenant naming and same-tenant rejection in existing and future archives.
All ten focused regression groups pass, including17 route-policy cases and the
prior75 native retrieval checks. Earlier compiled-handler admission and NULL
ranking failures are retained, not reclassified as successful evidence.

Cycle45 reproduces the no-metadata trait FTS failure with stronger out-of-set
candidates. After consolidation, 54 additional native checks cover both trait
methods and the builder, three retrieval modes, four language strategies, strict
and unified tag precedence, legacy tags, set membership, deletion/archive flags,
NULL-vector MMR and tenant exclusion. A non-bypass single-connection pool has a
five-second checkout timeout. A separate real subscriber captures all three
instrumented entry-point spans with sensitive fixture values, including the
closed-pool error path and zero-limit connection path. It does not claim all
downstream SQL/inference logs are sanitized. Prior 75 metadata checks remain.

Before delivery: run the verified-claim router and cache matrix, add citation-safe
locators, qualify native pg_bigm where advertised, correct every declared Core
backend and its capability report, run the same authority corpus, update generated
contracts and SAD/ADR/receipts, then complete clean-destination and released-runtime
acceptance. The full Lane B release sweep and suite NO-GO remain unchanged.

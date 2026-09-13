# Hosted Worker Transaction Migration

Status: Cycle90 connects the explicit hosted handler registry, bounded committed
claim drain and attempt-fenced callbacks to the daemon. Document-type inference
is the first production registration. Native worker/event acceptance passes;
actual-binary acceptance is recorded separately in the Cycle90 checkpoint.
Other handlers, follow-up execution and complete lifecycle delivery remain open.
Authority: ADR-090 and HotM ADR-MOBILE-001 Decision 6. Recovery behavior follows
ADR-084. Linked acceptance owners: Fortemi#1091 and Core#405. Suite NO-GO remains.

## Implemented Queue Boundary

Cycle91 adds typed temporary-database failure classification, bounded read-only
note metadata in fenced completion, and canonical hosted SSE schema filtering.
Its source-bound checkpoint owns final acceptance; no readiness promotion.

Typed deadline/transport failures and reviewed PostgreSQL SQLSTATEs trigger
bounded retry. Authentication, configuration, constraints and unknown failures
remain permanent, independent of message text. The document-type handler applies
this to content work and both progress updates. A failure after content commit
replays the job-bound receipt without another access/provenance write.
DeadlineExceeded retains indeterminate-commit semantics.

Successful completion uses a service-owned transaction that locks and admits the
exact claim, reads active-note metadata, then settles queue/attempt/history under
the same fence. This is an explicit queue/content/queue transition, not permission
for arbitrary handler transaction mixing. A share lock retains the note through
settlement; one SQL snapshot reads title, flat tags, current AI-revision timestamp
and outgoing-link presence. No full note fetch, content/snippet load or access
write is used. Limits:8192 title bytes,1024 tags,1024 bytes per tag,65536 aggregate
tag bytes. Oversized metadata rejects rather than truncates. Missing/deleted notes
produce no update. Only acknowledged commit publishes job.completed followed by
the existing note.updated payload. Snapshot/commit failure publishes neither;
stale recovery retains settlement responsibility. This is not a durable outbox:
process loss after commit can lose a broadcast. Debug omits titles/tags/identities.

Hosted SSE authorizes archive names, then filters both live and replay envelopes
against the resolved schema and exact tenant. Missing memory scope is rejected
on hosted streams; personal behavior is unchanged. No wire catalog, payload or
schema/profile pin changes. The real-binary fixture uses unchanged HotM parsing
with an owned Node transport/name adapter and default server coalescing. Seeded
Last-Event-ID replay does not qualify automatic reconnect, released HotM or its
application archive selection. HotM's request-name versus event-schema input
remains a consumer gap; a fixture adapter is not a production fix. Full lifecycle,
all remaining handlers, follow-up/index/queue and consumer delivery gates remain.

Cycle90 HostedExecution owns an immutable nonempty, duplicate-free registry of
HostedJobHandler implementations. Legacy registrations never grant hosted claim
authority. Main registers the document-type handler only in hosted mode and
retains the existing personal handler registry separately. Recovery-only callers
cannot fall through to personal claims. Public hosted readiness remains false.

Each cost tier uses bounded dispatcher pages and finite concurrent batches;
empty unwrapped pages continue after a short yield instead of waiting for the
safety poll. Complete empty scans sleep. Failed/partial/timed-out dispatch passes
are logged as incomplete and back off, never reported as zero pending work.
Global/per-archive pause is rechecked before each claim. No shared-model warmup,
unload or sidecar lifecycle operation is used by hosted execution. Future GPU/BYO
handlers still require explicit resource and authorization qualification.

The executor retains the committed claim through handler timeout/panic, terminal
settlement and retry/exhaustion. It emits progress only after fenced persistence,
and emits terminal/retry events only on acknowledged settlement. Lost or
indeterminate settlement emits no success. Shutdown aborts and joins active
handler tasks, attempts fenced retries under a five-second settlement deadline,
then stops the periodic reaper and acknowledges worker exit. Remaining
indeterminate attempts belong to stale recovery, not a bare-ID retry callback.

Internal HostedWorkerEvent identity comes only from the claim and includes
tenant/archive/job/attempt/note/type. Debug omits raw identities and progress
text. Main subscribes before starting the worker. Its hosted event branch uses
the existing EventContext envelope without raw job reads or duplicate progress
writes. Existing started/progress/completed/failed wire payloads are unchanged;
there is no new retry wire event. Global queue.status is personal-only until its
tenant-scoped producer is implemented. Cycle91 adds note.updated and scoped SSE;
follow-up/index events, full SSE lifecycle, all remaining handlers and recovery/restore/
purge races are still required. No authority schema/profile or consumer pin is
promoted by this partial migration.

`ScopedJobRepository` borrows `TenantScopedConn`; no ambient pool or payload
tenant is authority. Every operation verifies the bound tenant is still active
and matches the transaction setting. Queue work uses a dedicated short
transaction with local `pg_catalog,public,pg_temp` search path. Commit/rollback
restores that path; an archive or temporary table cannot redirect queue access.
Only the service-owned admitted completion wrapper above combines queue and
content work; ordinary queue transactions must not accept archive content queries.

Claim and attempt insertion are atomic. Valid payload routing may select public
or a currently tenant-owned archive, never another tenant. Malformed routing is
ineligible. A public pause includes absent schema and empty-object payloads;
the legacy personal repository retains its documented no-schema exclusion rule.
Concurrent claims and stale sweeps use SKIP LOCKED. Recovery retains existing
backoff, attempt evidence and exhaustion behavior, restricted to one active tenant.

The private-field, non-serialized claim carries tenant, archive, job and attempt
UUID. Attempt number alone is insufficient: rollback/reclaim can reuse it.
Progress and completion/retry/failure require the same running attempt UUID,
tenant, current queue retry number and still-owned archive. Queue/attempt locks
serialize settlement with recovery; queue, attempt and terminal history writes
are one SQL statement under the caller's transaction. Lost claims return false
or None, not successful completion. Invalid tenant/input fails explicitly.
Recheck active tenant/archive ownership per statement; this is not a revocation
lease spanning inference or arbitrary external side effects.

No REST/AsyncAPI/shard contract, Core runtime, consumer pin or capability flag
changes. Cycle89 adds a forward SQL function migration for native membership
queue/statistics triggers. Existing pool-based JobRepository remains for personal
mode and has NOT acquired the new hosted fencing guarantee.

## Required Migration

1. Recovery now selects active non-nil tenants from the canonical registry in
   service bootstrap, with short scoped transactions and bounded batches. Carry
   this authority boundary into pending selection and claims, without payload
   tenant authority or globally tenant-seeded pools.
2. Wire pending selection and claim drain to HostedJobDispatcher after introducing
   a hosted-capable handler registry. Its explicit nonempty handler list must never
   fall back to the personal repository's empty-means-all rule. Claims commit
   before the dispatcher returns a capability. Queue insertion or recovery success alone
   is not worker readiness; failed counts must not silently become zero work.
3. Migrate every enabled handler to HostedJobHandler/HostedJobContext, backed by
   immutable committed claims and with_content transactions. There is no blanket
   adapter from legacy JobHandler. Remove raw pool fallback for hosted work.
   Revalidate scope before content writes, follow-up jobs and external effects;
   migrate progress, completion, failure and retry callbacks to fenced methods.
4. Qualify tenant-scoped event delivery, archive pause/resume, cancellation,
   suspension/deletion, crash/restart/reclaim and purge/restore. Do not expose a
   hosted handler before its complete path satisfies the boundary.
5. Rebuild the actual hosted binary and rerun clean-installed Core HTTP plus
   worker execution/lifecycle acceptance with mandatory bootstrap dependencies.
   Retain the hard recovery-health assertion, including partial-pass failures.
   /readyz and HTTP success
   alone do not qualify workers, cache TTL/revocation or EE policy atomicity.
6. Complete every declared consumer, CI, delivery and lane-closeout release
   sweep before promotion. Server delivery is direct; Core requires PR/green CI.

## Verification And Rollback

Cycle89 uses the actual pool-free hosted document-type handler, not a renamed
test handler. Community/scoped detectors share filename, MIME, extension and
content scoring; hosted plaintext fallback additionally requires active tenant
configuration. The handler locks note/original/revised rows, prefers nonempty
revised content, preserves the 1000-character preview, and locks the selected
active public registry entry until commit. It preserves an existing assignment.
Access accounting, native trigger side effects and completed revision-linked
provenance commit with the assignment. The activity uses the unique job ID as
its identity: re-execution returns the committed metadata without repeating
effects. An incomplete/conflicting receipt fails closed. This does not make
external effects exactly-once or merge content and terminal-settlement commits.

Native full auto-refresh sets exposed unqualified membership statistics and
queue function calls, including the obsolete gen_uuid_v7 helper. Migration
20260913000000 resolves tables by triggering schema and explicit tenant, uses
the shared public queue/native pg_catalog.uuidv7, and includes archive routing
in follow-up payloads and duplicate checks. Statistics update both old/new
parents on reparent and preserve the existing embeddings_current calculation.
No public-table fallback or disabled trigger/RLS guard is introduced.

Cycle89 final48unit+11DB+4API functions PASS (63focused Rust functions).
The matrix includes replacement-attempt replay, stale-attempt rejection, deferred
COMMIT rollback, source locks/cancellation, membership reparent/delete statistics,
inactive/filter/no-create guards, revision provenance and receipt conflicts.
Actual-binary/unchanged installed Core220groups/293requests also PASS, with
278binary/15auxiliary requests,7startup markers,580audit rows and graceful exit.

See suite lane-b-delivery-20260913-cycle89 for final source-bound test/cleanup
receipts, including preserved failed attempts. Native handler acceptance remains
distinct from daemon registration/claim-drain, follow-up embedding execution,
tenant events, full lifecycle and every-consumer/CI/delivery/release qualification.
Rollback must retain the scoped trigger behavior for any admitted hosted writes;
do not activate this handler through the legacy raw-pool registry.

Cycle88 passes46unit+11DB+3API Rust functions, total60. The new typed handler
fixture performs native note-title writes under a claimed archive, preserving
same-ID public/other-archive notes and excluding foreign-tenant rows. It verifies
inventory/FORCE-RLS checks, no implicit public-table fallback, temporary-shadow
denial, callback error/GUC/path drift rollback, deferred COMMIT failure, recovery
lock exclusion, in-flight archive revocation/tenant suspension, cancellation,
100ms deadline, lost-claim callback suppression and same-connection restoration.
The fixture contains no pool and is not a migrated production handler. No active
tenant embedding sets are seeded; trigger-driven follow-up pipelines remain
unqualified. Actual-binary/unchanged installed Core220groups/293requests pass,
278binary/15auxiliary, all7startup health markers,580durable audit rows and exit0.
See suite `lane-b-delivery-20260913-cycle88/` for final source and terminal receipts.

Cycle87 passes38worker+8queue unit tests and10database test functions. Its new
dispatcher matrix proves active registry authority, independent cost-tier cursors,
fairness/wrap/high-water, nil/inactive/foreign-routing exclusion, public pause,
explicit handler allowlists, cross-dispatcher unique claims and committed claim
visibility. Deferred trigger failure at COMMIT produces no claim capability and
rolls back queue/attempt writes while another tenant progresses. Registry failures
are errors, not successful empty pages. Pass overlap, pass and settlement-acquisition
deadlines are bounded. Capability progress/completion/failure/retry/exhaustion
preserve UUID fencing; late and suspended callbacks cannot mutate the queue.
The same runtime connection is reused without tenant-setting leakage. Recovery
startup/periodic and legacy personal regressions pass after sharing the registry
page helper. These are real dispatcher DB operations, not production handler
execution. See suite `lane-b-delivery-20260913-cycle87/third/`.

The final actual-binary/unchanged clean-installed Core regression also passes:
3API functions,220groups/293requests (278binary/15auxiliary), all7startup health
markers,580durable tenant audit rows and graceful exit. The combined focused
total is59Rust test functions. This preserves recovery and HTTP qualification
after the shared registry refactor; production claim/handler activation remains
unproven. One earlier controller-path failure ran no DB payload and is preserved.

Cycle86 passes37worker+8queue unit tests,9database test functions and3scoped API
test functions. The new recovery matrix covers bounds, oldest-first selection,
cursor high-water/wrap/new tenant, inactive/nil exclusion, transaction rollback
after attempt-lock failure, independent tenant progress, pass deadline,
connection reuse, actual JobWorker startup/periodic recovery and stopped events.
The unchanged installed Core package again passes220groups/293requests against
the actual hosted binary:278binary and15explicit auxiliary-router controls.
Durable audit580rows, all five Redis stores, healthy recovery and graceful
shutdown pass. The pure health regression rejects partial-recovery error markers.
This fixes the Cycle84 recovery finding in the candidate, not hosted execution.
See suite `lane-b-delivery-20260913-cycle86/` and linked Cycle86 reports.

Cycle85 uses full migrations, owned PostgreSQL18 and a runtime role with no
ownership, superuser or RLS bypass. Four legacy retry tests, one strict parser
test, eight queue unit tests and the new scoped matrix pass. The matrix covers
two tenant archives, invalid routing, inactive/missing tenants, same-connection
reuse, rollback, concurrent claim/reap, UUID replacement, late callbacks after
retry and reap/reclaim, settlement lock contention, history failure atomicity,
archive revocation, temporary shadow denial and local search-path restoration.
The one matrix function is not counted as separate test functions per assertion.

Evidence: suite `lane-b-delivery-20260912-cycle85/third/` and linked Cycle85 reports.
No model, GPU, shared inference, Docker or production vault access was needed.
Rollback preserves all earlier WIP, data, migration history, attempt evidence
and immutable releases. Removing recovery wiring restores the known broken
hosted recovery path, not a qualified fallback. Claims/handler activation still
needs coordinated draining and claim/attempt reconciliation; never fall back
from scoped execution to id-only completion.

## Recovery Control Plane

`HostedJobRecovery` is service-owned and constructed only after the runtime-role
assertion. It reads only IDs/status from public.tenant_registry using the existing
system-table SELECT authority. This is not an admin HTTP endpoint, payload input,
RLS bypass or replacement for SystemScopedConn on cross-tenant admin operations.
All tenant data updates use TenantScopedConn and active/GUC admission. The nil
personal tenant, suspended and soft-deleted tenants are excluded. Recovery does
not read tenant routing authority from queued payloads.

Defaults are32tenants/page,128jobs/tenant,2second statement timeout,100ms lock
timeout and10second overall pass deadline. Config construction caps these at
64tenants,256jobs,5seconds/statement and30seconds/pass. The worker uses defaults,
not a new permissive environment profile. A pass holds one process-local cursor
lock; overlapping calls fail. The cursor advances past failed tenants. An epoch
high-water UUID prevents onboarding from indefinitely postponing wraparound;
an empty page resets it. Restart discards this scheduling cursor, not job state.

Startup processes one bounded page, not the whole registry. Periodic passes
continue it. A success marker means one page completed; it does not mean every
tenant backlog is empty. Failed tenant transactions roll back independently;
completed earlier tenant transactions remain committed. Partial failures and
outer deadlines produce an incomplete report/error log, never successful zero
work. Cancellation during commit can leave an indeterminate per-pass count;
durable job/attempt state, not that count, governs later recovery. This is not
exactly-once external side effects or a tenant revocation lease.

## Committed Claim Dispatcher

HostedJobDispatcher uses the same service registry page implementation as recovery,
with separate process-local cursors for all six cost tiers. Defaults:32tenants/pass,
2s statement and10s pass; caps64/5s/30s. Each pass returns at most one committed
claim and advances past attempted tenants. No claim means this page had none,
not that the entire registry has no pending work. Keep page continuation separate
from the future GPU warmup/pending-count decision. Overlap on the same tier fails.
New IDs above the epoch high-water mark or behind the cursor wait for wrap; IDs
within the remaining range may join the traversal. This is not a registry snapshot.

The caller supplies only registered hosted-capable job types, with a nonempty list
bounded by JobType::ALL and at most256excluded archive names of63bytes each.
Service selection provides tenant authority; queued routing may narrow to an
owned archive but cannot supply another tenant. Nil/personal tenants are excluded.
No dispatcher API accepts a user-selected tenant, raw connection or ambient scope.

HostedClaim is minted only after COMMIT and can be shared as an Arc. Its fields
and source pool are private, with immutable job/tenant/archive/attempt accessors.
Each progress/complete/fail/retry call opens a fresh short scoped transaction,
rechecks active ownership and the original attempt UUID, and returns only after
commit. False/None means lost claim; database/registry/deadline failures remain
errors. The default2s total settlement deadline includes pool acquisition and
commit, with100ms lock timeout. Cancellation during commit is indeterminate and
must not be represented as guaranteed rollback or blindly retried external work.

Production JobWorker still uses legacy claim drain and JobContext. Do not pass a
bare cloned job from HostedClaim to those raw-Database handlers. Use the Cycle88
content/handler contract for per-handler migration, add committed event routing,
then connect claim drain and callback settlement. This addition does not
disable the worker or clear hosted readiness. No schema, wire authority or consumer
pin change; rollback retains recovery and durable queue/attempt/history state.

## Claim-Bound Content Work

HostedClaim.with_content and HostedJobContext.with_content open a fresh bounded
TenantScopedConn and lock current queue/attempt rows before invoking a trusted
database-only callback. The inventory follows ADR-090: public requires
TENANT_SCOPED_TABLES; archives exclude canonical SHARED_TABLES. Missing tables,
nullable/missing tenant columns, absent tenant policies, disabled RLS or missing
FORCE RLS fail before callback entry. This checks the established policy convention,
not a new authorization schema. The local path is pg_catalog, the quoted validated
claim schema, then pg_temp. Archives have no implicit public-table fallback;
shared repositories must explicitly qualify public.

The callback receives a borrowed TenantScopedConn, not a pool or transaction
ownership. It remains an internal trusted-SQL API, not a sandbox for arbitrary
SQL. Review fully qualified queries, shared functions and every follow-up operation.
The whole transaction uses the claim's statement duration as its deadline
(default2s/max5s), including pool acquisition and COMMIT; lock timeout100ms.
Check the path after the callback and tenant/archive/attempt again before commit.
Lost claim returns None without callback execution or without committing callback
writes; scope errors remain errors. SQLx rollback handles error/drop/cancellation.
Recovery/settlement cannot replace the attempt while its queue locks are held.
No tenant/registry write privilege or row lock is added.

Keep inference/network/external effects outside the callback. Split read/inference/
write stages and enforce handler-specific revision preconditions. Content commit
and terminal settlement are separate; crash/retry can repeat content work. Require
idempotent or otherwise retry-safe handlers before activation. Final checks are
not a revocation lease, immediate global abort, atomic external effect or exactly-
once guarantee. Cancellation during COMMIT can leave an indeterminate outcome.
Do not widen bounds to accommodate unbounded handler work.

HostedJobContext accepts only a committed HostedClaim. It exposes immutable
note/payload/tenant/archive accessors, bounded content work and fenced async
progress persistence. It has no global event sender or terminal settlement API.
The worker retains settlement; production adoption and tenant event routing remain.

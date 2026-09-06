# Proposal: complete the bounded Fortemi load qualification

Status: proposed, for review; no load execution or production approval implied.
Primary issue: [Fortemi #1141](https://git.integrolabs.net/Fortemi/fortemi/issues/1141).
Authority: [#1136](https://git.integrolabs.net/Fortemi/fortemi/issues/1136).
Code inspected: `4d2ccd5740b02737e67db68d3635d3fdcf33339d`.
Research retrieved: 2026-09-05 America/New_York.

## Recommendation

Complete one integrated qualification runner around the existing Node dataset
controller, scheduler, collectors and signed-evidence machinery. Use a dedicated
synthetic target, a separate load generator, and an independently controlled
observer/watchdog. Validate the generator's arrival accounting against a small
k6 reference scenario before relying on its measurements. Preserve every
operation, phase, boundary case and dependency already required by #1141.

Proceed through three explicit stages: (1) finish instrumentation and falsify
broken harness/verifier behavior, (2) obtain approval for bounded calibration on
the selected topology, and (3) freeze a complete numeric contract and run fresh
certification trials. Calibration measurements select a candidate envelope;
they cannot be relabeled as the final preapproved certification run.

The research supports measurement and verification patterns, not Fortemi-specific
capacity numbers. This proposal therefore supplies concrete design decisions,
implementation work and a calibration protocol without inventing production SLOs.
All three acceptance criteria remain incomplete; suite NO-GO remains.

## Research findings and changes to the plan

| Finding | Assessment | Evidence and consequence |
|---|---|---|
| Scheduled arrivals and explicit unsent work | PARTIAL; high confidence in documented mechanisms | k6 distinguishes open/closed execution and dropped iterations [S1,S2]. Keep scheduled arrival timestamps; retain offered, dispatched, completed, cancelled, timed-out and undispatched counts. Diagnose generator saturation separately from target overload. |
| Per-operation tails and reproducible workload state | PARTIAL; moderate transfer to Fortemi | Tail-at-Scale motivates tail-sensitive measurement; YCSB distinguishes workload mixes and state changes [S3,S4]. Cheap status calls must not conceal slow ingest/materialization. Record initial dataset state, size classes, tenant distribution, cache state and actual histories. |
| Independent state and evidence completeness | PARTIAL; moderate engineering confidence | Elle explicitly limits its checker model; artifact evaluation separates availability from independent reproduction [S5,S6]. Build a Fortemi-specific deterministic state oracle; a verified callback flag or successful HTTP response is insufficient. |
| Complete pool/queue/provider observations | PARTIAL; high confidence in metric definitions, not capacity | OTel separates pool usage, pending acquisition and timeouts; PostgreSQL distinguishes statistics scope and timing [S7,S8]. Instrument actual acquisition/provider boundaries and retain source identity/age. |
| Retry amplification and reserve-aware abort | PARTIAL; moderate engineering confidence | Google SRE and AWS describe retry budgets, overload and bounded retry behavior [S9,S10]. Count attempts and logical operations separately; retain reserve for drain/teardown. |
| Repeated runs and transparent uncertainty | PARTIAL; moderate methodological confidence | Corpus REF-475 and ACM artifact guidance support exact configuration, run counts and reproducible analysis [S6,S11]. Retain every trial and rejection; never publish only the best run. |

These are engineering recommendations inferred from multiple sources. No source
proves Fortemi's throughput, latency, isolation, cleanup, or shard compatibility.

## Corpus use and source-quality corrections

Queried the requested canonical `section9/research-papers`, pinned to commit
`126758e2776f742445a1d2703fbd42d27dffa0a9`. The local `roctinam` checkout was an
archived predecessor and was not used as current authority. The fresh checkout's
Fortemi static index was not materialized; explicit local-index fallback returned
mostly queue bookkeeping. Targeted reference/index reads then supplied the source
candidates. No source was silently represented as a successful semantic-index hit.

| Corpus reference | Use in this proposal | Boundary established by checking original sources |
|---|---|---|
| REF-320, Postgres LISTEN/NOTIFY | Include notification/commit contention in the measured wait inventory [S12] | The original article was updated May 8, 2026 to report a PostgreSQL fix and links the upstream change [S13]. Do not copy the summary's universal scaling prohibition or historical writer threshold. Verify whether the exact deployed build contains the relevant fix. |
| REF-324, Scaling PG queues | Observe retry-driven status growth, compaction and query plans [S14] | Current article supports context-specific partitioning and retry-storm lessons. Its throughput is a vendor case, not a Fortemi target. The summary's 128-event batch prescription was not corroborated in the retrieved article; exclude it. |
| REF-326, Multi-tenant queues | Test a bursty tenant alongside a control tenant; measure fairness as well as confidentiality [S15] | Queue fairness patterns are not proof of forced-RLS isolation. Do not replace Fortemi scheduling merely because a vendor used a different queue design. |
| REF-475, Reproducibility | Preserve environment, seeds, run counts, uncertainty and analysis inputs [S11] | ML research methodology transfers indirectly; it does not supply server performance thresholds. |
| REF-1518 / REF-1592, readiness and quality gates | Useful examples of artifact-backed release decisions and evidence coverage [S16,S17] | Single-author preprints; their weights, thresholds and model results do not transfer. #1141 requires every hard gate; no weighted score may compensate for missing correctness or cleanup. |

Corpus letter grades are retained as source metadata, not converted into proof.
For this spike, official docs have high authority for their own interface semantics;
peer-reviewed methodology has moderate indirect applicability; vendor case reports
and single-system preprints have low/very-low certainty for transferring numerical
effects to Fortemi. Historical sources outside the default 18-month window are
used for mechanism/methodology only, with reduced transfer confidence.

## Proposed architecture and implementation

### 1. Topology, roles and authority

Use three resource domains: generator; API/workers/PostgreSQL/queue/blob target;
and observer/watchdog/evidence store. Prefer a disposable dedicated target VM
with separate cgroup budgets for its services. Containers alone do not establish
separate host-failure domains. Select explicit CPU/memory/storage/IO/process limits,
network destinations, provider account policy and run/control namespaces.
The independent watchdog must remain operable when the generator dies. cgroup v2
provides resource and process controls, but remote jobs and storage still require
separate accounting and cleanup [S18,S9].

Keep four roles distinct: runtime principal, read-only state observer, narrowly
scoped cleanup actor, and authority approver/verifier signing roles. The load
process cannot declare its own independent verification or change trusted code,
public pins, approved thresholds, or the evidence ledger. Reuse the detached
candidate design; retain version 2.0.0 unchanged. Any incompatible complete-envelope
schema requires a new candidate version, named consumers, fixtures and receipts
under #1136 before adoption.

### 2. Arrival and operation model

Use one logical-operation identity with separate attempt IDs. Start independent
workflows on scheduled arrivals; preserve causal dependencies inside each workflow.
Keep the proposed 20/25/10/10/5/5/10/5/10 percent mix as a candidate until justified
against the intended use. Keep single-operation diagnosis supplemental to the full
mix in each required phase.

Record acknowledgement and independently observed completion separately. For
materialization, completion means expected derived state is observable. For
cancellation, exercise active work and the commit/cancel race. For retry, force an
actual retry and reconcile durable effects; cached local receipt retrieval is a
different observation. Export/import must verify the exact named profile and a
clean destination. Source lineage requires its declared semantic assertions,
not merely successful provenance/links routes.

A generator shortfall means MISSING coverage with its cause retained; a measured
threshold breach is FAIL. Keep PASS/FAIL/MISSING/UNSUPPORTED vocabulary and the
separate rejection verdict for unsupported cells. Do not introduce a fifth
status that silently changes candidate consumers. Invalid policy fails preflight.

### 3. Feasible sample and artifact budgets

Compile the complete plan before approval: for every operation/window/size/tenant
stratum, prove scheduled sample count, maximum concurrent work, duration, raw
artifact volume and teardown reserve fit the approved bounds. Select percentile
precision and sample floors before trials. Report uncertainty and temporal
clustering; repeated observations from one stalled interval are not independent
replications. Fixed seeds do not reproduce operating-system schedules [S6,S11].

A concrete feasibility example: a proposed 1,000 completed samples for an
operation with exactly 5% share needs at least 20,000 scheduled logical operations
per window, assuming no lost samples. One such window in each of five phases
already uses the evaluator's 100,000 schedule-entry ceiling per invocation.
The evaluator counts logical schedule entries; actual HTTP requests, polling and
retry attempts need a separate expanded budget. Three independent repetitions
contain at least 300,000 logical entries in total and may use three bounded
evaluator invocations, with complete trial inventory and aggregation. The 1,000
floor is an illustrative design choice, not a statistical guarantee. A richer
soak/window plan within one invocation needs bounded streaming chunks and an
authenticated complete chunk inventory, or a reviewed budget revision. Check
observed sample sufficiency separately from planned counts. Never drop windows,
lower floors after results, or accept a truncated artifact to make the test pass.
Keep existing byte/count limits until any required implementation and authority
change is reviewed.

### 4. Finish measurement at the actual source

| Surface | Required implementation / assertion |
|---|---|
| SQLx pool | Instrument acquisition start/end/error: used/configured, pending, wait distribution, timeout count and pool instance/reset identity. Cover every acquisition path; pg_stat_database.numbackends is not this metric. Pin OTel vocabulary because connection metrics remain Development [S7]. |
| Queue | Retain physical pending/running backlog and oldest creation age, supported-job counts, delayed/dead/incompatible work, plus per-tenant fairness observations. Reconcile populations explicitly; public.job_queue and API supported-type counts are different scopes. |
| PostgreSQL/WAL | Bind server/database/reset identities; keep retained and generated WAL separate; observe lock/wait events, not just CPU. Reject stale/incomplete statistics [S8,S12,S13]. |
| Blobs | Reconcile logical metadata, database-backed bytes, object-store objects, staging and failed-attempt objects. Retain object hashes/version IDs and scope inventory. Logical size alone cannot prove physical occupancy. |
| Freshness | Bind committed identity to independently observed expected derived state. Use observer monotonic elapsed time or bounded clock mappings; do not subtract unrelated host clocks as if synchronized. |
| Provider | Instrument every dispatched attempt, retries and abandoned streams. Reconcile durable usage and explicit missing/estimated billing dimensions; use exact decimal pricing with a pinned price revision. Reserve worst-case cost before dispatch and reconcile actual usage. No-op metering is not zero cost. |
| Safety | Collect generator health and target health separately. Missing/stale collectors stop new arrivals; raw observations include sample start/end, source age and reset identity. |

Every metric still needs its explicit approved operator/limit/unit. These source
contracts are the implementation proposal; research does not establish their
complete runtime coverage.

### 5. Independent oracle and cleanup

Create deterministic fixtures and a canonical before-state inventory under the
independent observer. Retain invocation/completion histories and compare actual
records, relationships, jobs, checkpoints, derived rows and blob objects against
expected effects. A timeout stays ambiguous until reconciliation. Elle is useful
as a design reference, but only use its checker if the actual datatype/history
satisfies its assumptions [S5,S6].

For every admission limit, pair a valid boundary fixture with boundary-plus-one
and prove the stable diagnostic plus zero prohibited durable mutation. Distinguish
admission rejection from a runtime timeout after work may have committed. Define
any audit-only side effects explicitly; do not exempt them silently. Current
controller limits include 500 records, 16 MiB aggregate canonical input, 4 MiB per
record, 120 seconds and one active execution per controller; that last limit is
not global capacity. Enumerate API/import/provider and cross-process limits too.

After stop: cancel/classify outstanding operations, enforce drain, perform only
owned-namespace cleanup, then independently verify residual resources and the
unchanged control namespace. Repeat the observation after the approved late-work
settling interval. Retain failed cleanup and residual inventory. Test the oracle
by deliberately omitting a known record/job/blob in disposable fixtures; test
watchdog independence by killing the generator. These are harness checks and
require their own bounded test approval before targeting services.

## Execution stages and review decisions

| Stage | Concrete deliverable | Exit criterion |
|---|---|---|
| A: Integrated rehearsal | Plan compiler, actual source instrumentation, independent oracle, one-operation proofs and generator-accounting comparison; synthetic local fixtures | Tests detect missed arrivals, missing telemetry, forged verifier success, late writes and incomplete cleanup. No production claim. |
| B: Bounded calibration | Separately approved target/limits; short fixed rate steps with no automatic search beyond the maximum; characterize cache state, generator headroom and metric variance | Select candidate rates, windows, sample counts, margins and resource/cost ceilings. Preserve all failed and successful calibration runs. |
| C: Frozen qualification | Signed complete topology/fixture/consumer tuple; fresh smoke/average/stress/spike/soak trials, boundary matrix, recovery and cleanup | Every required cell/window passes; complete independent evidence and receipt admission; exact revision validation. |

**Concrete starting proposal for stage B, subject to review:** one dedicated
Linux/PostgreSQL 18 target, two runtime tenants plus an untouched control
namespace, and separate generator/observer resource domains. Run fixed offered
steps of 0.5, 1 and 2 logical operations/second for 120 seconds each, with a
60-second maximum drain and 300-second maximum teardown. These are provisional
engineering choices for initial characterization, not capacity claims or
execution permission. Product/platform owners must fill host allocation,
per-operation deadlines, concurrency, resource reserves and provider caps before
these steps run. Stop after the first breach or evidence gap. A calibration cell
with insufficient percentile samples cannot become a certification pass.

For certification, set the representative rate from intended demand, the stress
maximum from the approved candidate envelope, the spike's rise/hold/recovery and
the soak duration from the mechanisms being tested. Freeze all numbers before
fresh trials. Retain repeats, variation and the worst failed window; do not
increase thresholds after a failing certification result.

## Issue ownership and delivery order

| Issue | Proposed next deliverable |
|---|---|
| [#1141](https://git.integrolabs.net/Fortemi/fortemi/issues/1141) | Own integrated runner, source coverage, feasible plan compilation and full phase/window/boundary matrix. |
| [#1136](https://git.integrolabs.net/Fortemi/fortemi/issues/1136) | Own complete-envelope authority/version decision, chunk inventory, disjoint trust roles, consumers and exact-cell admission. |
| [#1137](https://git.integrolabs.net/Fortemi/fortemi/issues/1137) | Provide runtime non-owner forced-RLS two-tenant evidence and observer/control-namespace contract; confidentiality and fairness remain distinct. |
| [#1138](https://git.integrolabs.net/Fortemi/fortemi/issues/1138) | Provide history/state reconciliation for commit ambiguity, active cancellation and exact/changed replay; independent cleanup after generator failure. |
| [#1072](https://git.integrolabs.net/Fortemi/fortemi/issues/1072) | Finish automated safe retention/monitoring and certify spare capacity for cleanup; shared CI is not a stress target. |
| [#1099](https://git.integrolabs.net/Fortemi/fortemi/issues/1099) | Supply provider retry/breaker/quota failure receipts and complete attempt/cost measurement with explicit recovery thresholds. |
| [AIWG #2242](https://git.integrolabs.net/roctinam/aiwg/issues/2242) | Bind exact producer/consumer/fixture tuple and live authorization under #2194. Include React #412 and HotM #231 when exercised. |

Deliver A first; B requires its limited run authorization; C requires accepted
prerequisite receipts and final approval. No issue is closed by this research
spike. AIWG static indexes, Knowledge Shard transfer and live persistence remain
separate; core-v1/full-v1/record-v1 claims still need their executable matrices.

## Alternatives and confidence

- **Keep Node orchestration, compare a small k6 reference (recommended):** preserves
  existing controller semantics and avoids introducing a second runtime contract.
  It still requires independent validation of schedule/accounting correctness.
- **Replace the entire harness with k6:** mature arrival mechanisms, but does not
  supply Fortemi lifecycle/oracle/receipt semantics; migration adds work without
  resolving the current evidence gaps.
- **Run existing wrappers directly on production:** does not satisfy the required
  authority, isolation, resource reserve or independent cleanup gates.

Overall recommendation confidence: moderate. Interface definitions are strong;
capacity and production efficacy remain unmeasured. No dissenting evidence was
found that justifies compensating for a correctness failure with a weighted score.
The k6-vs-Node choice is an engineering tradeoff, not experimentally established
superiority. Current source revisions and artifact availability do not prove
independent reproduction of their numerical claims.

## Verified public sources

All URLs were retrieved during this spike. Corpus reference numbers above refer
to the pinned internal warehouse; public recommendations are grounded in the
original public sources below. No private corpus prose or full source snapshots
are published in this report. Abstract-only sources support conceptual framing,
not detailed experimental estimates.

- **S1:** [k6 open and closed models](https://grafana.com/docs/k6/latest/using-k6/scenarios/concepts/open-vs-closed/), current official docs; high interface authority.
- **S2:** [k6 dropped iterations](https://grafana.com/docs/k6/latest/using-k6/scenarios/concepts/dropped-iterations/), current official docs; high interface authority.
- **S3:** [Dean and Barroso, The Tail at Scale](https://research.google/pubs/the-tail-at-scale/), CACM 2013, publisher abstract retrieved; historical, moderate indirect relevance.
- **S4:** [YCSB Core Workloads](https://github.com/brianfrankcooper/YCSB/wiki/Core-Workloads), project documentation; high authority for its workloads, indirect relevance to Fortemi workflows.
- **S5:** [Elle](https://github.com/jepsen-io/elle), author-maintained checker documentation; high authority for checker scope, not a Fortemi oracle.
- **S6:** [ACM SIGSIM PADS 2026 artifact evaluation](https://sigsim.acm.org/conf/pads/2026/blog/artifact-evaluation/), primary evaluation policy; methodological guidance.
- **S7:** [OpenTelemetry database metrics](https://opentelemetry.io/docs/specs/semconv/db/database-metrics/), semantic conventions 1.44.0 as retrieved; connection metrics marked Development.
- **S8:** [PostgreSQL 18 cumulative statistics](https://www.postgresql.org/docs/18/monitoring-stats.html), primary implementation documentation.
- **S9:** [Google SRE: Handling Overload](https://sre.google/sre-book/handling-overload/), primary historical operational guidance; numbers are workload-specific.
- **S10:** [AWS: Timeouts, retries and backoff with jitter](https://d1.awsstatic.com/builderslibrary/pdfs/timeouts-retries-and-backoff-with-jitter.pdf), 2019 primary guidance; PDF retrieved because the HTML redirect returned no article text.
- **S11:** [Pineau et al., Improving Reproducibility in Machine Learning Research](https://jmlr.org/papers/v22/20-303.html), JMLR 2021; REF-475. Retrospective study, indirect transfer to systems qualification.
- **S12:** [Recall.ai LISTEN/NOTIFY incident](https://www.recall.ai/blog/postgres-listen-notify-does-not-scale), updated May 8, 2026; REF-320. Vendor case report; no universal numerical threshold inferred.
- **S13:** [PostgreSQL LISTEN/NOTIFY optimization commit](https://github.com/postgres/postgres/commit/282b1cde9dedf456ecf02eb27caf086023a7bb71), primary upstream change linked by S12; deployed-version inclusion unverified.
- **S14:** [RudderStack queue scaling](https://www.rudderstack.com/blog/scaling-postgres-queue/), current page dated May 26, 2026; REF-324 metadata previously said 2024. Vendor experience, low/very-low quantitative transfer confidence.
- **S15:** [Hatchet multi-tenant queues](https://hatchet.run/blog/multi-tenant-queues), vendor design account; REF-326. Fairness motivation, not tenant isolation proof.
- **S16:** [Maiorano, LLM Readiness Harness](https://arxiv.org/abs/2603.27355), 2026 preprint abstract plus corpus analysis; REF-1518. Very-low certainty for transferred numerical effects.
- **S17:** [Maiorano, Automated Self-Testing as a Quality Gate](https://arxiv.org/abs/2603.15676), 2026 preprint abstract plus corpus analysis; REF-1592. Same author as S16, not independent replication.
- **S18:** [Linux cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html), current primary kernel documentation; controls do not cover remote side effects.

## Method and verification record

Best-Practices Audit and Research Query workflows; performance/testing/observability
focus with the detected SDLC research contributor. Citation threshold: two sources
per main finding; single-source interface details are identified as such. Read
corpus summaries as retrieval leads and checked their original sources; detected
source drift is explicitly retained above. GRADE-style certainty concerns transfer
of effects and is separate from a vendor's authority to define its own API.
No unsupported empirical improvement percentages or Fortemi capacity claims are
used. No paper induction, source-corpus edits, deployment or load execution was
performed. Corpus index lookup wrote only checkout-local activity bookkeeping.

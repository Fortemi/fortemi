# ADR-102: Canonical Knowledge Shard Contract and Conformance Profiles

**Status:** Accepted
**Date:** 2026-07-17
**Deciders:** Architecture team
**Implementation status:** The registered nine-cell `core-v1`, `record-v1`, and `full-v1` producer/consumer matrix is complete, including current receipt-bound AIWG-to-PGlite and AIWG-to-Fortemi `core-v1` cells. Versioned `core-v1` schemas through `2.0.0`, receipt-bound RecordStore self and Fortemi consumer cells, Fortemi `core-v1` and `full-v1` self-cells, PGlite `core-v1` self and bidirectional Fortemi/PGlite cells, a supported complete-inventory `full-v1` server route with production Ed25519 publisher signing, transactional rich-component apply, registered historical transitions, bounded preflight, identity-preserving import, signed-import verification, and disk-backed attachment-sidecar streaming are implemented. Matrix completion authorizes only registered-profile cell claims; unqualified suite compatibility, portability, complete backup, and parity remain blocked.
**Supersedes in part:** ADR-028, ADR-029

## Context

### Lane B Runtime Correction Delivery (#1147, 2026-09-11)

The cycle20 correction candidate preserves scoped replacement ownership and
retained identities while coordinating native and restore writers. Source-bound
server150/migration18/archive17/required-live tenant24 and28producer exports into
56clean installed consumers pass. React's native consumer candidate3739827 is
tracked by Fortemi/fortemi-react PR445 and issue #424. No authority schema/profile
tuple changes are introduced by these runtime corrections. Source delivery does
not replace exact-head CI, shared fixture/pin publication or released-runtime
qualification; those gates and the suite NO-GO remain in force. Historical matrix
receipts are not widened to include this unreleased behavior.

### Validated Tag Restore Correction (#1145)

Knowledge Shard note tags are governed by the selected wire schema. After full
archive preflight, the native restore path preserves those declared strings and
memberships; it does not apply the live authoring tag-path grammar, lowercase
values, or derive inline hashtags. Ordinary note writes continue to enforce the
live grammar and derive hashtags. `PgNoteRepository::restore_shard_note_tx`
makes this distinction explicit and is used only after validated shard apply.

The default product archive from Fortemi/fortemi-react#423 contains schema-valid
seeded `docs:...` tags. Released Fortemi 2026.9.9 validates/dry-runs that archive
but fails applying it through the live helper. The source correction has a
producer-owned fixture and clean import/re-export regression. This is a runtime
correction under unchanged schemas, not a live tag grammar or profile expansion.
Its release evidence must supplement the historical matrix before claiming the
new product path works with a released server. Suite `NO-GO` remains unchanged.

Fortemi is both a Knowledge Shard producer and consumer. The format is also
produced or consumed by sibling repositories, including `fortemi-react`,
`aiwg`, and HotM. Independent implementations have drifted in component
coverage, manifest fields, version interpretation, validation behavior, and
loss handling. In particular, a shard can contain data that the server's
default importer cannot restore.

A portable format cannot be described as lossless merely because its archive
can be parsed. Interoperability requires a single schema authority, explicit
capability profiles, deterministic validation, and preservation of the
semantics needed to reconstruct the source graph.

This ADR records the required target contract. The server currently emits
`core-v1` with the self-importable structured component set and enforces
profile, schema version, minimum reader, inventory, checksum, and count
preflight. It also validates collection/note/template/link relationships,
exports complete collection hierarchies, and preserves collection and template
identities and timestamps during import. Its REST route can optionally export
and restore verified attachment bytes as digest-addressed sidecars, while
missing entries remain reference-only. Import reads sidecar payloads in bounded
chunks into isolated preflight files instead of retaining them in the
in-memory component map. Any supplied sidecar that lacks a matching attachment
projection is rejected as an orphan. Missing sidecars remain explicit
reference-only attachments for `core-v1` and `record-v1`; the supported
`full-v1` policy requires bytes for every declared attachment digest. Export
streams filesystem sidecars through integrity verification into a
request-scoped disk-backed archive, validates the completed compressed artifact
before returning success, and streams that file to the client in bounded
chunks. The preferred multipart import spools bounded compressed chunks into a
request-scoped temporary file, and on-disk swap reads its existing file
directly into the same reader-based preflight. The legacy JSON request retains
its bounded encoded string, but base64 decoding streams through a fixed buffer
into a request-scoped temporary file before the same reader-based preflight.
Structured component files remain bounded-buffered, but transactional apply
deserializes JSONL notes and links one record at a time instead of retaining a
second component-sized typed vector. Schema/count and relationship preflight
also visit JSONL notes and links one record at a time, retaining only identity
and attachment-declaration sets needed for cross-record validation.
Schema/count preflight also validates and discards JSON-array component records
one at a time. Typed JSON-array relationship/apply data, historical migration,
and preflight raw buffers remain bounded-buffered. Export completes and
validates a bounded disk-backed artifact before streaming it to the client.

Current-version bytes complete checksum, schema/count, relationship, and
sidecar validation once before the explicit no-op migration result. Historical
migration output is a distinct representation and therefore repeats the full
checksum, schema/count, relationship, and sidecar validation before staging or
database mutation.

The current normative schema root for `1.2.0` / `core-v1` is
`contracts/knowledge-shard/1.2.0/core-v1/`. The immutable `1.0.0` and `1.1.0`
authorities remain at their original versioned roots. The machine-readable
receipt at `contracts/knowledge-shard/contract.json` records exact current and
historical digests, golden corpora, supported and reserved profiles, and
current limitations.

The supported `record-v1` root is
`contracts/knowledge-shard/1.2.0/record-v1/`. It is limited to notes,
collections, tags, note-to-note links, and attachment projections. Producers
must report every omitted or lossy source concept through their
machine-readable capability/loss result. The exact React producer artifact at
commit `df4762ad0c470ebd8ee460b56ba71be09b4f1616` passed the same bounded
validation and atomic apply path used for `core-v1`: dry-run and reserved
profile rejection wrote zero rows, two replace imports converged, Fortemi
re-exported the resulting state, and React validated and imported that return
archive while preserving IDs, bodies, the empty revision, relationships,
attachment reference, and tombstone instant. The durable receipt lives beside
the integration fixture.

Contract revision 19 supports the complete server `full-v1` route and
publishes a reproducible signed integrated fixture
that requires all 33 `full-v1` components, all 34 count fields, all 33
component checksums, and one mandatory content-addressed attachment sidecar
shared by two references. It unifies candidate embedding, note-revision,
provenance, SKOS, graph, and community boundaries under
`contracts/knowledge-shard/1.2.0/full-v1/`, with separately digest-pinned
corpora. The revision boundary covers current original state, original history,
the current revised snapshot, and the complete revision chain. The provenance
boundary preserves W3C-PROV edges and processing activities that reference
those exact note and revision identities. The spatial registry boundary
preserves named places, exact SRID-bearing PostGIS geometry, location
observations, and device agents. The unified boundary preserves note/attachment
targets, exact timestamp-range shape, registry references, extraction context,
AI context, and user corrections. The SKOS boundary preserves schemes,
concepts, labels, notes, semantic and mapping relations, memberships, note
tags, and ordered collections. The graph boundary preserves source lineage,
weighted note edges, nested communities, and assignments. These component
schemas are compiled by the server and exercised by bounded schema,
relationship, candidate-corpus, and negative-drift tests. The deterministic
generator normalizes the component families onto coherent note identities and
emits an immutable archive and adjacent receipt. Bounded archive preflight,
complete inventory and checksum validation, cross-component relationship
validation, typed revision serialization, mandatory-byte validation,
deduplication, and archive read/write/read equality all pass against those
exact bytes. A strict authority-owned signature-envelope schema and pinned
Ed25519 fixture signature authenticate the exact manifest SHA-256 and sorted
BLAKE3 sidecar inventory through the production import verifier. The
deterministic fixture key is public compatibility-test material and is never a
production trust anchor. The production `full-v1` exporter can load an
operator-provisioned Ed25519 seed from a private regular file, derive the
public identity, and emit the same canonical envelope. Signing-key bytes are
bounded, kept out of application responses and diagnostics, zeroized after
loading where the runtime representation permits, and never enter an archive.
Consumers load a public-key allowlist from a bounded file or the legacy inline
environment form; ambiguous dual configuration is rejected.
The `profile=full-v1` export selector always emits the exact complete component
set and forces attachment sidecars on. A database-backed route test imports the
signed fixture, exports it from an isolated schema, imports the live artifact
twice into a clean schema, and compares all component bytes, manifest counts,
checksums, and blob bytes after re-export. Persisted embeddings are constrained
to the server's 768-dimensional storage boundary. Replace mode reconciles
provisioned embedding-set name/slug conflicts and restores source tag
timestamps so clean-server and repeated imports converge.
Embedding records preserve a nullable contract fingerprint. Non-null values
must be exactly 64 lowercase hexadecimal characters, and schema validation
rejects malformed lineage before staging or database mutation. The registered
`1.1.0 -> 1.2.0` migration maps legacy absence to `null` without inventing a
producer identity. Embedding, revision, provenance, SKOS, graph, and community
apply paths run
inside the existing schema-scoped import transaction. Their database tests
prove exact source-field restoration, repeated replace convergence, skip and
dry-run accounting, and rollback after a late injected failure. Runtime
validation accepts only the complete `full-v1` manifest and rejects partial
profile selections. The canonical matrix currently has four passed self-cells
and five passed cross-repository cells under #1059. The reduced, lossy
`recordstore-record-v1-to-fortemi` cell binds the current signed React fixture
to independent clean Fortemi import, repeated convergence, semantic re-export,
version/malformed/resource rejection, and zero-mutation evidence. The two AIWG
`core-v1` cells separately bind the current v2 source authority and released
converter to clean PGlite and Fortemi destinations, hierarchy and lifecycle
state, repeated convergence, semantic re-export, compatibility-window handling,
negative inputs, resource limits, and zero mutation. These exact-cell receipts
do not establish suite-wide compatibility, portability, complete backup, or
parity, and they do not merge the AIWG static index, Knowledge Shard bridge, and
live Fortemi persistence planes.

Production signing does not change the archive schema: `signature.json` was
already an authority-owned `full-v1` entry in revisions 19 and 20. The
operator file shapes are separately published at
`contracts/knowledge-shard/operator/`; they configure producer and consumer
trust and are not part of the portable archive schema bundle.

## Decision

### 1. Contract ownership

The Fortemi server repository is the source of truth for the canonical
Knowledge Shard archive contract. It owns:

- the normative JSON Schema and component schemas;
- manifest and archive layout semantics;
- the schema-version and profile registries;
- the conformance fixture corpus;
- migration rules and compatibility policy; and
- the producer/consumer conformance matrix.

Sibling repositories may vendor generated schemas and fixtures for offline
builds, but must record the upstream revision and verify that vendored copies
match it. A sibling repository must not redefine a field, component, profile,
or compatibility rule independently.

The current authority bundle includes the manifest and record schemas for
notes, collections, tags, templates, and links. Fortemi import compiles and
applies these same schemas before component inventory/count validation and
before its normal write phase. Schema failures return a stable class without
echoing record content or validator diagnostics.

### 2. Schema version and producer identity

`manifest.version` is the Knowledge Shard **schema version** and uses Semantic
Versioning. `manifest.min_reader_version` is also a shard-schema SemVer value:
it is the minimum reader contract version required to interpret the shard
without undeclared loss. Neither field contains a Fortemi application release.

Producer identity is separate, informational metadata:

- `producer.name`
- `producer.version`
- `producer.revision` when available

Compatibility decisions use the schema version, profile, declared extensions,
and registered migrations. They must not infer compatibility from an
application CalVer or package version.

SemVer changes are classified as follows:

| Change | Required action |
|--------|-----------------|
| PATCH | Clarification or compatible validation fix that does not change accepted data |
| MINOR | Backward-compatible optional field, component, or registered extension |
| MAJOR | Required-field, type, meaning, archive-layout, or preservation change |

Adding a component to a required profile is a breaking profile change even if
the component schema itself is optional elsewhere.

### 3. Named conformance profiles

Every shard declares exactly one registered `manifest.profile`.

| Profile | Contract |
|---------|----------|
| `core-v1` | Shared structured records: notes and metadata, collections and hierarchy, tags, templates, links, timestamps, identities, tombstones, and attachment projections. Attachment bytes and rich analytical extensions are not guaranteed. Components outside the profile are neither implied nor silently discarded. |
| `full-v1` | Lossless Fortemi interchange. Extends `core-v1` with embeddings, SKOS, provenance, graph/community records, attachment byte sidecars, and the signature envelope. |
| `record-v1` | Explicit RecordStore transport subset. The producer must emit a machine-readable loss report for source concepts that the subset cannot represent. It must never be advertised as full parity. |

Profile identifiers are independent from `manifest.version`. Changing a
profile's required preservation behavior requires a new profile identifier or
a schema-major migration that leaves old profile semantics unambiguous.

The server's default export uses the richest profile that the same released
server can self-import. Until every `full-v1` gate passes, the default may be
`core-v1` with its limits reported explicitly. Once `full-v1` is supported, it
becomes the default backup profile. Every default export must be
self-importable by the same released server build and by every later compatible
server release.

### 4. Fail-closed validation before writes

Import is a two-phase operation:

1. Read, bound, and validate the complete archive without mutating persistent
   state.
2. Apply the validated import in one transaction or equivalent atomic unit.

Pre-write validation includes:

- archive path and resource-limit checks;
- manifest schema and registered profile checks;
- declared component inventory;
- file existence, media type, and checksum verification;
- exact manifest counts;
- component schema validation;
- referential integrity;
- schema-version compatibility and migration availability; and
- duplicate/conflict policy validation.

A corrupted, undeclared, unsupported, or incoherent required component fails
the import. Checksum mismatch is an error, not a warning. Malformed records are
not skipped during a conformant import. Unknown components are accepted only
through a registered extension whose compatibility behavior is defined.

Dry-run and real import execute the same validation and planning path. The
write phase either commits the entire plan or leaves the destination unchanged.

### 5. Preservation invariants

#### Ordinary Owner Selection (Unreleased #1147 Work)

Cycle20 reproduces two valid retained-concept coordinate swaps that preview but
fail during apply: URI exchange and primary-scheme exchange at the same notation.
Concept replacement now inspects incoming IDs, URIs and paired scheme/notation
coordinates. A collision with an unselected live identity rejects in preview and
apply. Changed coordinates belonging to selected retained IDs are vacated to NULL
before upsert; concept IDs, replaced_by references and unselected children are not
deleted or reassigned. Existing complete-archive uniqueness checks remain intact.

Apply obtains the migration208 concept statement guard via a zero-row UPDATE
before locking conflict rows in ID order, avoiding a row-lock-before-guard order.
Preview performs no write or row-lock staging. Existing transaction rollback
restores temporary coordinates and every selected/unselected snapshot. There is
no new schema revision, profile, migration or consumer source/package change.

The first implementation exposed a separate wipe-preview regression: destination
concept owners were checked even though explicit full wipe removes them before
apply. SKOS planning now receives the explicit-wipe flag and omits that destination
concept-coordinate check during wipe preview only. Complete-archive validation is
unchanged. The same independent owner rejects under ordinary replacement but
previews read-only and is removed by repeated explicit full wipe. Original wipe
RED and actual post-wipe exports are retained. This is not general partial-wipe
foreign-key preview qualification.

Handler regressions cover both wire orders from original state, repeat, Skip/Merge,
read-only preview, observable deferred rollback, RESTRICT-protected references and
unselected live-coordinate collisions. Native note-tagging tests additionally
exercise literary-warrant promotion on the third assignment: over-capacity commit
rejects and rolls back notes/assignments/lifecycle; with a balancing demotion,
SET CONSTRAINTS ALL IMMEDIATE validates the final state before test rollback.
The latter is explicit constraint validation, not a persisted positive native
commit/export receipt. Further native entry points, alternate-key permutations,
inverse consistency, performance, full Lane B acceptance and release qualification
remain open. Suite NO-GO; exact2.0.0/full-v1 candidate evidence only.

Cycle19 closes the concept-status omission in the cycle18 relation-only guard.
Two baseline handler/native regressions committed 201 approved narrower children
by promoting a candidate while leaving relations untouched. Shared read-only
planning now runs for selected concepts as well as relations. Unselected relation
rows remain in the projected graph; selecting concepts alone grants no relation
omission authority. Skip/Merge retained conflicts keep their destination status
and snapshot: the final restoration pass operates only on actually applied IDs.

Forward migration `20260911020800_skos_concept_status_constraints.sql` attaches
unconditional pre-statement concept INSERT/UPDATE/DELETE coordination to the same
tenant/archive guard. An unconditional deferred INSERT/UPDATE constraint checks
the current concept status and affected narrower parents in the triggering schema.
It does not trust queued row values or caller search_path/restore settings. Valid
promotion-before-demotion batches commit; over-limit final states reject. Existing
relation validators and coordination function bodies remain unchanged. This is a
native runtime correction, with no new wire field, authority revision or profile.

The mixed-writer regression covers48 combinations of public/registered archive,
three isolation levels, native/restore context, first-writer commit/rollback and
status-first/relation-first order. It observes backend blocking and checks final
capacity and unchanged logical guard values. Read Committed rejects excess breadth;
stale stronger-isolation transactions reject40001. Handler tests cover concept-only
preview/apply, retained RESTRICT references, Skip/Merge and valid batch replacement.
Source-local test evidence does not qualify released runtimes or the complete
original Lane B matrix. Partial ownership, narrower-only/inverse consistency,
alternate keys, performance, publication/pins, exact-head CI, delivery, cleanup and
release qualification remain. Suite NO-GO; exact2.0.0/full-v1 candidate only.

Clean archive cloning exposed an additional interaction: concept INSERT creates
the tenant-only coordination row before the clone reaches that table. The original
clone failed on its primary key. Clone now tolerates that one identical tenant key
in `skos_relation_write_guard`; data-table conflicts remain strict. The clone test
checks source/target guard values and both concept triggers. This does not turn
runtime lock ownership into portable shard state or suppress native constraints.

Cycle18 reproduces actual concurrent import write skew: two distinct, individually
valid relation-only imports both passed deferred validation and committed, leaving
four broader parents or a cycle. A test-only post-validation advisory barrier made
both competing snapshots observable before commit; original output and observations
are preserved. Forward migration
`20260911020700_skos_relation_writer_serialization.sql` serializes relation INSERT,
UPDATE and DELETE before row validation through an unconditional statement trigger.

Each archive has a forced-RLS `skos_relation_write_guard` table keyed only by tenant.
A physical no-op UPSERT retains the same logical tenant value while holding the row
until transaction end. Read Committed writers validate after the prior writer;
Repeatable Read/Serializable writers with stale snapshots abort with40001 and must
retry the whole transaction. Rollback releases the guard. Other tenants and archives
use distinct rows/tables. The coordination row is local runtime state, not a new
Knowledge Shard component or a portable record; all three integration planes remain
separate. Existing immediate and deferred validators and their function identities
remain unchanged. No historical migration, declared native record or profile changes.

Both actual handler races now show one waiting writer and exactly one successful
commit, retaining old relations/references and unselected snapshots. Winner replay
and loser preview rejection pass. The migration test executes24 combinations of
public/old-archive schema, three isolation levels, native/restore contexts and
first-writer commit/rollback, plus independent tenant/archive controls. Server146,
migration17, clean archive17 and required-live tenant23 pass. The restricted-runtime
case verifies guard visibility, forged-tenant rejection and rollback. Twenty-two
actual producer exports pass44 pristine installed consumers with exact component,
key-presence and mandatory-byte checks; cycle18 receipts bind this evidence.
This is not suite-wide concurrency acceptance: concept-status and other non-relation
writers do not use this guard. Partial owner/status changes, narrower-only/inverse
consistency, general alternate-key permutations, large-graph performance and all
original Lane B acceptance/publication/pins/CI/delivery/cleanup/releases remain.
Suite NO-GO; exact2.0.0/full-v1 candidate only.

PostgreSQL's [transaction isolation rules](https://www.postgresql.org/docs/current/transaction-iso.html)
explain why a lock alone does not refresh Repeatable Read snapshots and why a
physical update is needed for the stale-writer failure used here. The tests above,
not that documentation alone, establish the scoped implementation result.

Cycle17 reproduces two valid final relation batches that preview successfully but
fail during apply: an intermediate cycle and a two-owner move at the three-parent
limit where neither retained update can run first. Sorting cannot resolve that
capacity case. Forward migration `20260911020600_skos_atomic_relation_batches.sql`
keeps ordinary native writes' immediate broader/narrower guards and installs an
unconditional deferred final-state constraint in public and registered archives.
Restore skips only immediate validation; the deferred constraint cannot be disabled
by changing the restore flag and validates at transaction commit. Native authoring
effects remain guarded during restore as previously defined.

Final validation reads each queued ID's current row from the triggering schema,
not an obsolete event snapshot or the caller search path. Repeated updates and
deleted queued rows therefore do not validate transient state. Existing native
validator functions and OIDs remain unchanged; new archives clone the constraint.
Legitimate three-parent,200-approved-child and depth-five bounds remain active.
Invalid final cycles/cardinality abort the whole transaction. No retained identity
is deleted/recreated, no reciprocal record is synthesized, and unselected concept
snapshots remain exact. Historical migrations and rows are not rewritten.

Both handler cases cover preview, protected references, both input orders from the
old graph, repeated imports, whole-state rollback and a reached deferred failure.
The public/old-archive migration test covers idempotence, ordinary immediate errors,
unconditional final validation, repeated/deleted events, changed caller context and
invalid cycle/broader/narrower commits. Server144, migration16, clean archive17,
required-live tenant22 and API/DB Clippy/format/diff pass. Twenty actual producer
exports pass40 pristine installed-Core destinations with exact component contents,
key presence and mandatory bytes; cycle17 receipts bind this unreleased evidence.
General alternate-key permutations, partial owner/status changes, narrower-only/
inverse consistency, concurrent writers and large-graph performance remain open.
Transaction-local final validation is not proof against racing writers. Full
original Lane B acceptance, publication/pins, authenticated/worker/platform/released
checks, CI/delivery and cleanup/releases remain required. Suite NO-GO, unchanged
schema/profile authority and exact2.0.0/full-v1 candidate evidence only.

Cycle16 reproduced two invalid successful apply operations: closing a short
broader cycle and extending a root so an unselected descendant's actual depth
became six. Current scalar snapshots and the old pre-insert cycle predicate did
not validate the resulting graph. Forward migration
`20260911020500_skos_prospective_hierarchy.sql` replaces the broader trigger's
cached-depth/existing-cycle checks with traversal of the prospective graph,
excluding the retained ID and including its replacement. The existing function
OID and public/registered-archive bindings remain; narrower validation is unchanged.

Both native validation and read-only shard planning include affected descendants
and bound traversal at six edges. Deduplicated `(node, depth)` states terminate
even for cycles; reaching six rejects either a cycle or a path beyond native depth
five. This validates actual broader edges rather than trusting cached `depth`
snapshots. It does not recompute or rewrite unselected concept metadata. Related
edges remain outside the native broader hierarchy, and restore does not infer or
materialize undeclared inverse edges. Three-parent and approved-child limits stay.

Actual handler cases cover apply/preview rejection under replace/skip/merge,
ordinary native and restore-context rejection, exact native-table preservation,
protected references, valid retained single-edge reparenting/repeat, read-only
write-trigger detection and reached deferred-commit rollback. An idempotent public/
old-archive migration test checks bounded cycle/depth/self-loop rejection, valid
reparent/upsert, related-edge controls, binding identity and rollback. Server142,
migration15, pristine archive17 and required-live tenant22 pass with API/DB Clippy
and format/diff. Eighteen actual producer exports pass36 pristine installed-Core
destinations; the cycle16 receipt binds exact components, key presence and bytes.

These tests do not establish multi-edge final-plan application order, concurrency,
narrower-only/inverse-consistency semantics or all partial owner/status changes.
In particular, per-row native checks may encounter an invalid intermediate graph
while applying a valid final multi-edge plan; reproduce that next without deleting
retained identities or disabling final-state validation. No new failure is yet
confirmed for that case. No wire schema/profile or consumer source change, data
backfill, historical migration edit or release is claimed. Full original Lane B
acceptance/delivery/publication/pins/platform/cleanup/releases and suite NO-GO remain.

Cycle15 reproduces valid retained semantic-relation imports failing at the actual
native limits: three broader parents and200 approved narrower children. Later
February migrations supersede the original ten-child limit and count only approved
children. Forward migration `20260911020400_skos_retained_relation_limits.sql`
excludes the retained relation ID before counting its resulting state. Candidate
children do not consume approved-child capacity; retargeting one to an approved
child must count it. The numerical limits and existing broader depth/cycle calls
remain active, without a restore-context bypass. Shared function OIDs and public/
registered-archive trigger bindings are retained; no existing rows are rewritten.

The initial over-limit archive test also exposed dry-run accepting a plan that
native apply would reject. A shared read-only cardinality projection now accounts
for incoming IDs, retained/unselected relations, selected-owner omissions and
effective selected concept statuses. It applies the same bounds before writes in
preview and apply; skip/merge ignore retained conflicts but still reject genuinely
new over-limit edges. Native trigger checks remain the write-time enforcement.

Two handler cases cover repeated replay at the limits with protected RESTRICT
references, whole-native-table equality, over-limit replace/skip/merge rejection,
preview write-trigger detection, reached deferred-commit rollback and native
update/rejection controls. The idempotent public/old-archive migration test covers
approved/candidate capacity, retargeting, depth rejection, retained upsert/update,
function bindings and rollback. Server140, migration14, clean archive17 and
required-live tenant22 pass with API/DB Clippy and format/diff checks. Sixteen
actual producer exports pass32 pristine installed-Core destinations, including
both boundary graphs, with all-component/key-presence/mandatory-byte comparison.
The cycle15 receipt binds those exact source and candidate package identities.

This is not complete hierarchy qualification: recursive cycle formation, batch
reparenting/order at capacity, selected child/status changes and concurrent writers
remain separate acceptance gates. The migration preserves the existing cycle
predicate; it does not establish that the predicate checks prospective edges.
Wire schemas/profiles, consumer source/package and historical fixtures are unchanged.
Full original Lane B acceptance, shared publication/pins, exact-head CI, delivery,
platform/released checks and cleanup/releases remain open. Suite NO-GO remains.

Note-owned assignment restore likewise does not author the referenced concept's
lifecycle. Actual insertion and selected-note omission regressions reproduced
automatic candidate promotion and changed unselected concept count/timestamp
snapshots. Forward migration `20260911020300_restore_skos_note_lifecycle_guard.sql`
guards the existing note-count trigger in shard restore context. When concepts
are unselected, their stored snapshots remain destination-owned rather than
being recomputed by tagging rules. Selecting concepts restores their declared
source snapshots through the existing apply stage. Ordinary native tagging
retains count, first/last-use and literary-warrant promotion behavior.

Both handler regressions pass with read-only preview, reached deferred-commit
failure/whole-native-table rollback, repeat imports, independent assignment
references and native promotion controls. Public/registered-archive migration
is idempotent and preserves the original function and native insert/delete
events. No existing row, historical migration or schema/profile is rewritten.
This correction does not qualify all partial child-owner changes, broader/
narrower validator limits, concurrency or the original Lane B release gates.

The cycle13 clean-installed consumer run exposed a separate producer failure:
`notes,note_skos_tags` omission selects notes but not revision components, yet the
schema-2 apply path clears existing note revision export-presence flags. Exported
provenance still names those revisions, so Core rejects the artifact for missing
revision declarations. Eleven scenarios pass across22 clean destinations; this
twelfth scenario fails validation before import. The server136/migration13/
archive17/tenant22 suites do not clear that cross-runtime failure. Preserve the
failing omission artifact and correct partial-note history/current visibility and
native state without dropping provenance or weakening consumer validation.

The cycle14 correction separates schema-2 flat content restoration from the live
revision writer. Retained originals keep their declared hashes and metadata when
content is unchanged; retained current snapshots keep their pointers and metadata.
Existing unselected original/current/revision declarations remain visible. Only
new native scaffolding is hidden automatically; selecting original/current
components still permits authoritative omission, and selected revision omission
uses the existing reference-checked cleanup. No revision is authored on replay.

Flat note content and declared rich snapshots share native storage and must agree
under the existing relationship validator. A changed flat projection therefore
rejects in read-only preview and apply when its declared original/current snapshot
is unselected. Select the corresponding `note_originals` or `note_revised_current`
component to restore that change. Skip/merge retained owners stay unchanged.
Flat-only notes without declared rich snapshots can still change their content,
including empty revised content, without generating new revision history.

Actual-handler tests cover notes-only replay, both original/current conflict
directions, explicit selection, flat-only edits, exact six-table history/provenance
preservation, skipped owners, preview and reached deferred-commit rollback. Full
database snapshots are compared for preview and failed apply; successful repeated
imports compare history/provenance rather than local attachment scan timestamps,
which the existing scan policy refreshes. Server138, migration13, clean archive17,
required-live tenant22 and API/DB Clippy pass. All14 actual producer exports pass
28 pristine installed-Core destinations, including the former omission failure,
with all-component/key-presence/mandatory-byte comparisons. The cycle14 receipt
qualifies only those recorded exports and destinations, not a release.

This is a runtime correction under unchanged schemas, not a new profile or an
inferred repair of previously hidden history. Reapplying a trusted complete source
with the relevant rich components selected remains the explicit restoration path;
no existing rows, applied migrations or historical failure artifacts are rewritten.
Full original Lane B acceptance, shared publication/pins, CI, platform/released
qualification, cleanup and releases remain open. Suite NO-GO is unchanged.

Restoring semantic relations must not invoke native authoring effects. Two new
actual-handler regressions reproduced an undeclared reciprocal UUID during
explicit-relation import and changed unselected concept counts/depth/timestamps
during relation-only replay. Forward migration
`20260911020200_restore_skos_relation_authoring_guards.sql` guards only the native
hierarchy and reciprocal triggers while `app.shard_import` is active. It retains
their original functions and ordinary native behavior, including reciprocal
creation, counter maintenance and rollback. It updates public and registered
archives; future archives clone the guarded public triggers. Existing relation
rows are not inferred disposable or rewritten by this migration.

The two actual import/export cases pass, as do server134, migration12, clean
archive17 and required-live tenant22. Complete component records and mandatory
bytes pass comparison against the input and through four pristine installed-Core
destinations. Original fixture comparison treats omitted optional empty migration
history as empty; consumer-hop field presence remains exact. This is a runtime
correction under unchanged schema/profile authority, not publication or released
qualification. Other partial-selection/owner-change cases, note-count/lifecycle
effects, native validators, concurrency and the original Lane B gates remain.

The candidate SKOS correction preserves independent schemes, concepts and
collections. Child omission requires selection of both the child component and
its owning root: concepts own labels, notes, mappings, scheme memberships and
outgoing semantic relations; notes own assignments; SKOS collections own members.
Reference endpoints confer no omission authority. Incoming UUID identities and
complete composite keys are excluded from cleanup before in-place upserts.
Explicit wipe remains separate. The fixed seven consumer archive triples in
`tests/fixtures/shards/external/react-native-skos-retained-2026-09-11` pass native
reference, repeat, edit, skip, rollback and omission checks; an additional mixed
ownership test preserves independent roots and incoming references.

The initial three bootstrap/count regressions are corrected by forward migration
`20260911020000_skos_scheme_bootstrap_custody.sql` and explicit initializer custody.
Only a freshly executed public seed migration or an archive initializer/repair
that actually inserts a default scheme may record its tenant-qualified identity.
Existing default/system/unused rows are never backfilled as disposable. Edits,
imported updates and scheme references adopt live ownership transactionally;
deleting the last reference does not recreate custody. Export excludes untouched
scaffolding, but includes referenced schemes and every unmarked live identity.
Replacement locks conflicting roots, disposes only recorded unused scaffolding,
and rejects an unselected live identity conflict in apply and read-only preview.
Selected roots may exchange notation/URI keys without remapping identities.
Native references, payloads and rollback survive the exchange.

In-place collection updates exposed a further timestamp-trigger regression.
Forward migration `20260911020100_restore_skos_collection_timestamp_guard.sql`
preserves source timestamps only in restore context, retaining the original
function identity and normal native edit behavior. No applied migration was
rewritten. The former native-count test now asserts that untouched scaffolding
survives unchanged in the database; exact wire counts, identities and timestamps
remain checked separately, not relaxed to accept extra exported roots.

Current local server shard132, migration11, clean archive17 and required-live
tenant22 checks pass. Eight actual producer exports, including a natively edited
default scheme, pass16 pristine installed-consumer destinations. The live default
also crosses the real server export/import handlers into a clean server archive
with its original UUID and all component/byte contents intact. These results do
not qualify all partial-selection/native-writer/concurrency cases, released
artifacts or every platform. The wire schema/profile is unchanged; publication,
consumer pins and full original Lane B delivery remain open. Preserve custody
records during rollback planning; reverting to blanket root deletion or rewriting
applied migrations is not a supported rollback. Suite NO-GO remains.

Graph sources and community sets are independent roots. A graph source
reference does not make every referencing set part of that source's replacement
selection. A complete graph-family declaration is not authorization to delete
all destination graph roots. Replace reconciles edges of selected sources and
members/assignments of selected sets; retained roots and unrelated roots survive.
Skip retains children of skipped source/set owners; a new independent set may
reference an existing source. Merge remains the existing add-missing-records
strategy, distinct from skip. Empty selected families do not imply a wipe.
Destructive swap retains its separately explicit wipe policy.

Retained graph edges update by their complete source/from/to/kind primary key;
community assignments update by set/note. Selected-owner omission cleanup excludes
those incoming keys before native upserts, preserving references and creation
metadata. Opaque source/set/community IDs remain case-sensitive. Incoming
assignments move before omitted nested communities are deleted. A remaining native
assignment to an omitted community rejects the operation instead of cascading;
when assignments are not selected, read-only planning checks the same conflict
before apply. This does not simulate arbitrary external FK or trigger failures in
dry-run, reserve concurrent state, or widen selection ownership.

The fixed `tests/fixtures/shards/external/react-native-relationships-2026-09-11`
corpus is copied byte-for-byte from consumer #424's clean-installed checks.
Its five input/replacement/omission triples cover note/URL link controls, retained
graph edges, assignments and movement. Real native importer checks protect
references, detect reached deferred-commit rollback, and retain native export
values. These local checks do not publish the corpus or qualify released/platform
support; shared publication, consumer pins and delivery remain required. No wire
schema, profile or migration changes are introduced by this correction.

The skip apply plan resolves existing selected note, revision, concept and SKOS
collection owners inside the import transaction, after any explicit wipe. Owned
history, outgoing links, embeddings/memberships, concept children and provenance
records are skipped with those owners even when the child ID is new. Reference
endpoints alone do not transfer ownership: new templates and graph/community
roots may reference existing collections or notes. Merge retains its distinct
add-missing behavior. The complete original archive remains the validation and
manifest authority; owner filtering changes only the internal apply selection
and its skipped counts, without copying or rewriting an authority bundle.

Schema-2 note/history presence and collection snapshot counts are updated only
for notes/collections actually applied. Skip does not reset embedding-family
presence flags. Exact HTTP snapshots cover every archive-local native table plus
the shared embedding-config and job tables, including direct presence fields,
timestamps, derived columns, partial/empty selections and late rollback. The
changed-child fixture and positive independent-root fixture are deterministic
test variants of the pinned released React archive; consumer qualification of
those variants remains a separate gate.

The unreleased graph apply correction and its transaction-level tests exercise
these rules, including repeated omissions, retained-set reparenting, dry-run and
late-failure rollback. The public HTTP regression additionally exposed retained
note deletion cascading unrelated graph edges and assignments. Ordinary restore
now updates retained notes and attachments in place, preserving incoming graph
references, links and local note grants. It explicitly reconciles selected-note
tags and attachment omissions. Outgoing links belong to their source notes;
replacement removes omitted outgoing links only when those notes and the links
component are selected, without deleting incoming links or retained link IDs.

The local HTTP test covers dry-run, skip, repeated replacement, unrelated native
rows, attachment identity/refcounts and late-failure rollback. It does not qualify
released/authenticated artifacts or the complete cross-family preservation matrix.
Replacement revision-history identity, provenance/SKOS ownership, omitted
embedding/history families, the complete partial-selection matrix and shared
consumer fixtures/receipts still require
reconciliation before delivery. No release or widened compatibility claim follows
from this unreleased regression work.

Schema-2 configuration declarations now live in the archive-local
`shard_embedding_config_declaration` identity relation. Configuration values
remain in the shared live `embedding_config` registry, not a copied component
snapshot. Export includes explicitly declared roots plus configurations required
by included embedding sets. Future archives start without explicit config roots;
native configuration writes declare the identity in their active schema, while
ordinary shared-registry API writes use the public schema. Imported existing
identities are explicitly declared even for skip/merge without overwriting their
values under those strategies. The legacy global presence flag is not reset by
shard import and no longer controls schema-2 configuration export.

Migration20260910010000 preserves currently visible config roots in public and
every existing archive; it does not infer bootstrap ownership from user rows or
recover declarations lost before upgrade. The identity relation uses forced
tenant RLS and a tenant-qualified shared-config FK. Native-write and declaration
updates share a transaction; deleted configurations cascade their declarations.
The build now tracks the migrations directory so a newly added migration cannot
be omitted from a cached binary. Roll back failed imports transactionally and
retain pre-upgrade data; deployed migration corrections are forward changes,
not edits to applied migration history.

This removes one cross-archive presence mutation, not every shared-registry
hazard. Shard replacement now compares typed PostgreSQL configuration values,
locks existing identities, and rejects differing values declared or referenced
by another registered archive (including public). Identical reuse is accepted;
sole-owner changes remain allowed. Dry-run performs the same conflict check.
The guard covers live set references even without a shard declaration and does
not grant permission to rewrite shared roots merely because they were selected.
Concurrent first creation, archive DDL/native registry writer races and the
authenticated route remain separate qualification requirements.
Membership replacement no longer globally resets member presence. Omission
cleanup requires the member component and both endpoint scopes: the note and
set must each be explicitly selected. A set declaration can be a dependency of
a note-scoped export, not a complete list of that shared set's memberships.
Retained coordinates update in place; excluded-note/selected-set and
selected-note/excluded-set memberships survive. Missing or empty endpoint
selection grants no omission authority. Referenced notes are not deleted.
This corrects the earlier unreleased #1147 single-set ownership interpretation
and React #424's single-note cleanup. No wire/profile change or data migration
is introduced; producer/consumer fixture publication and exact released-runtime
qualification remain required. Explicit destructive wipe is separate.
Vector omission follows the same two-sided selection: both non-null note and set
must be selected. Null endpoints remain independent, and either excluded endpoint
prevents deletion. Retained vector IDs update in place, preserving external and
native token references. Changed incoming coordinates temporarily use a null set
inside the transaction, then receive final values by identity; immediate native
uniqueness remains enforced and unselected coordinate occupants cannot be removed
to resolve a collision. No migration or wire tuple change is introduced. React
#424 preserves native member vector pointers that are absent from the wire
record. Omission of a still-referenced vector rejects in that consumer. Complete
dry-run alternate-key equivalence, concurrency, shared fixture publication and
released/platform qualification remain open.
Ordinary replacement now uses the same live alternate-coordinate guard for
dry-run and actual apply. Existing incoming IDs may vacate coordinates, and
omittable selected-owner rows may release them; unselected occupants reject with
the same fixed validation error. Actual apply locks existing selected IDs and
coordinate occupants in ID order before checking. READ ONLY tests prove that
preview does not stage or delete rows. This is not first-insert/native-writer
concurrency proof, arbitrary-trigger/reference preview, or explicit-wipe preview.

Forward migration20260911010000 corrects native set statistics on same-tenant
vector reparenting. Both old and new non-null set coordinates are refreshed;
null-set transitions refresh the departed/arrived set. Trigger tables are
qualified by TG_TABLE_SCHEMA and counts are tenant-scoped, preserving sibling
schemas even when a qualified archive write uses the public search path.
Existing trigger function identity is retained; fresh and existing archive
triggers use the correction. It does not backfill historical snapshot counts.
Selected declared sets still restore their source snapshot after dependent apply.
This supersedes the no-migration statement for this additional native correction,
not the unchanged wire/profile tuple. Same-note cross-set/null-set import,
repeat, dry-run and failed-import rollback are checked. Broader consumer omission
fixtures and released qualification remain separate acceptance work.
The additional note-owner correction preserves native note-owned token rows,
IDs, payloads, owners and timestamps. Ordinary apply clears only token.chunk_id
pointers whose token.note_id differs from the incoming vector's note, including
a null incoming note. It does not move another note's tokens, delete referenced
token identities, or fabricate a replacement chunk association. Matching pointers
survive. Detached pointers are not reconstructed from absent wire fields on later
imports; native token generation can establish new associations independently.
Dry-run does not detach and failed apply restores pointers transactionally.
React #424 applies the same independent-note rule to member.embedding_id and
marks virtual materializations referencing vectors with changed note/set owners
stale, so existing live selectors reevaluate their criteria. This is not general
cache invalidation: newly matching uncached vectors, arbitrary native writers,
content/model changes and complete worker/released/platform coverage remain open.
These native adjuncts are not new shard fields. No schema/profile tuple or new
migration is introduced by this additional correction; suite NO-GO remains.
The external `react-native-membership-2026-09-10` corpus binds identical fixed
input/replacement/expected bytes used by clean-installed Core and PostgreSQL
tests. Local retained-reference, rollback, repeat and clean-destination membership
checks pass; these do not establish all-component or released-runtime parity.
Set presence is no longer reset globally. `shard_embedding_set_bootstrap` records
identity-only creation custody for fresh archive seeds and repair-created seeds.
The migration runner records a public seed only when that invocation actually
creates it in an empty database. The additive migration does not infer custody
for existing rows, even exact-looking Default/system rows. A failed or interrupted
initial migration that loses creation custody must preserve the unmarked seed,
not reconstruct deletion authority from its current shape.

Ordinary set edits, auto-memberships and embedding/attachment/coarse references
adopt bootstrap sets as live state. Transaction-local restore context suppresses
native auto-membership generation and implicit adoption during validated apply;
explicit imported set/member/embedding declarations adopt their parent identity.
The context is not an authorization boundary and never persists past the
transaction. Replacement hides only recorded bootstrap identities without live
embedding references. Name/slug collisions may retire only those recorded unused
bootstrap identities; collisions with unselected native/unmarked identities fail,
including dry-run. Selected embedding-set IDs and potential name/slug conflicts
are locked together in identity order before custody checks. Selected IDs may
exchange names/slugs or free a key for a new selected identity. Immediate unique
constraints are satisfied using transaction-local temporary keys on changed
selected rows, followed by in-place upserts; retained IDs and external references
are never deleted to accomplish a rename. Dry-run checks the batch without
staging keys. Any staging collision or later apply failure rolls back the import.
HTTP partial-selection/skip/repeat and duplicate-key rejection plus internal
three-way cycles, independent name/slug permutations, new-identity key reuse and
late-failure rollback are tested. The local HTTP harness injects archive context;
it does not qualify authenticated routing or arbitrary concurrent writers.
Migration20260910020100 preserves the native tombstone trigger predicate alongside
the restore-context check; already applied custody migration history is unchanged.

The custody table has forced tenant RLS and a tenant-qualified parent FK. Native
adoption and failed imports roll back with their transaction. Existing-state
export preservation, native CRUD after restore, populated upgrade, fresh creation
and repair custody have distinct test scopes. These do not qualify arbitrary
concurrent native writers, public-archive authenticated import, or published
consumer/platform matrices.

Explicit wipe clears the target archive's configuration declarations, never the
shared `embedding_config` registry. A target-only declaration is removed from
export even though its native shared value remains available to other archives.
The same shared-value conflict guard applies after wipe: selecting destructive
replacement does not authorize changing another archive's live configuration.
Internal tests cover repeated wipe, dry-run, sibling native/export preservation,
conflicting-value rejection and whole-database rollback after a late template
failure. The on-disk reader with swap's production regeneration option preserves
existing jobs and adds only target-schema jobs. This is not authenticated HTTP
swap or background-worker execution qualification; public-schema, concurrent and
released/platform matrices remain separate gates.

Existing-state/clean-destination,
upgrade/native-writer and cross-archive matrices remain separately required.
Consumer replacement
of community assignments must use the independent set-owner rule above, not treat
every referencing note as authorization to replace an unrelated set's members.

This is a runtime preservation correction under existing profile invariants, not
a schema/profile version change. Immutable authority bundles and historical
receipts remain unchanged. Fortemi owns the shared fixtures and policy; native
PGlite acceptance is linked through Fortemi/fortemi-react#424. RecordStore's
record-v1 does not carry graph components; AIWG and HotM wire production/consumption
retain their existing schemas. Producer/consumer runtime receipts and release
pins must be reconciled before widening the current named-profile claim.

History and provenance now update retained identities in place. Original-history
omission cleanup requires explicitly applied note owners and the history component;
revision omission cleanup additionally waits until retained current pointers,
activities and provenance records have been applied. A child-only or empty child
selection does not implicitly select its note owner. Existing current pointers
to retained revisions are not cleared by revision-only import. Omitted revisions
with retained activity references or unselected current pointers reject the apply
transaction instead of cascading or leaving live records dangling.

Provenance edges belong to selected revisions, not their source-note reference.
Activities belong to selected note owners. Unified records reconcile under
explicitly selected note/attachment owners, excluding retained record IDs so
reparented records survive. Activity omission cleanup follows unified-record apply;
an activity still referenced by retained provenance rejects the transaction.
Named locations, provenance locations and devices are independent roots: ordinary
replacement never globally deletes them. Existing references retain their identity
and are not nulled by delete/reinsert behavior. Explicit wipe remains separate.

Forward migrations20260910030000 and20260910030100 suppress automatic original
history snapshots and original/revision/location edit stamping only while the
transaction-local `app.shard_import` context is on. Native trigger predicates and
behavior outside restore remain intact. The first migration was already applied
before timestamp-trigger regressions were found; the second is a forward fix,
not a rewrite of applied history. Restore context is not an authorization boundary.

Local PostgreSQL regressions cover retained RESTRICT references, partial and
repeat restore, native edits after restore, independently owned provenance,
selected-owner omissions, retained activity reparenting before revision deletion,
empty history, and exact late-cleanup rollback. These checks do not qualify every
alternate-key coordinate exchange, concurrent writer, authenticated/public route,
or published consumer/platform matrix. Ordinary replace dry-run projects the
post-import activity/revision references without executing writes or triggers.
It accounts for selected-owner omissions, incoming reference changes and attachment
cascades, then reports the same retained-reference conflicts as actual cleanup.
Full and partial selections, unrelated current pointers, incoming activity moves,
and execution inside a PostgreSQL read-only transaction are tested. Actual apply
still rechecks references after its writes; preview does not reserve future state
or qualify concurrent writers, arbitrary trigger errors or explicit-wipe preview.
Consumer original-history/unified-record delete/reinsert and retained-revision
edge omission behavior still require alignment with this authority.

A conformant round trip preserves, subject only to a declared migration:

- stable entity identifiers;
- collection hierarchy and membership;
- template, tag, link, embedding-set, and other relationship identities;
- relationship endpoints and ordering where order is meaningful;
- the distinction among absent values, JSON `null`, empty values, and explicit
  tombstones;
- deletion/tombstone state and conflict metadata;
- source timestamps and their precision/time-zone meaning;
- attachment metadata, references, filenames, media types, sizes, and byte
  content;
- checksums for attachment bytes and all declared archive components; and
- extension data required by the declared profile.

Importers must not silently regenerate identifiers, attach children to a
different parent, turn `null` into an empty value, revive tombstones, replace
source timestamps with import time, or drop referenced bytes.

Migrations that cannot preserve an invariant must fail unless the selected
profile explicitly permits the loss and the archive carries the required
machine-readable loss report. `full-v1` never permits such loss.

### 6. Release and integration gates

A schema or profile release is blocked until all of the following pass:

1. The canonical schemas validate both positive and negative fixtures.
2. The server default profile imports into a clean instance with semantic
   equality; a `full-v1` claim additionally includes attachment byte checksums.
3. Export after import is canonically equivalent apart from documented
   non-semantic archive metadata.
4. The previous supported schema versions migrate through tested paths.
5. Each registered producer validates its emitted archive against the
   canonical schema and golden corpus.
6. Each registered consumer passes the same corpus for every profile it
   advertises.
7. Cross-repository integration tests pin the canonical contract revision and
   publish a producer/consumer compatibility result.
8. Negative tests prove that checksum, count, relationship, profile, and
   version failures perform zero writes.

No repository may claim support for a profile until its conformance result is
green. Contract changes require a Fortemi Gitea issue linked from every affected
consumer issue and pull request.

## Current-state gap

The runtime now separates producer identity from strict shard-schema SemVer,
declares a registered profile, rejects unsupported profiles and components,
and fails canonical manifest/record schema, checksum, count, inventory,
collection topology, and note/template/link reference validation before normal
import writes. Archive preflight also bounds compressed and expanded bytes,
entry count and size, manifest size, and component record count and size while
rejecting unsafe paths, duplicate names, and non-regular tar entries. Default
export includes roots and descendants. Import creates collections before
dependent notes and templates, preserves collection and template IDs and source
timestamps, restores source note timestamps after revised content, and uses
stable-ID conflict handling for repeated imports. Reference-only attachment
projections retain attachment IDs, display filenames, extraction state and
text, canonical digest metadata, and shared-blob deduplication; attachment IDs
and conflicting declarations are rejected during relationship preflight.
Shard imports retain the canonical extraction status and reason in validated
attachment metadata so subsequent exports preserve the exact projection state
after referenced bytes are promoted from reference-only to filesystem storage;
ordinary attachments without shard metadata continue to use deterministic
status, text, media-type, and size derivation.
Ordinary import applies all selected database components in one schema-scoped
transaction; a late database failure rolls back collections, notes, tags,
templates, links, attachments, and reference blobs together, and post-import
NLP jobs are queued only after commit. The current positive and negative corpus
is pinned by the schema receipt. Destructive shard swap validates before
mutation, then deletes the existing core-v1 entity families and applies the
validated shard within the same transaction. A late apply failure therefore
restores the pre-swap state.

Schema `1.1.0` adds an optional `deleted_at` note field. Current exports include
soft-deleted notes and always emit either explicit JSON `null` or the exact
deletion timestamp. Import restores that value in the same transaction as the
note. The registered `1.0.0 -> 1.1.0` migration validates source bytes and
records first, maps the legacy field absence to the documented `null`
active-state default, rebinds the component checksum and migration metadata,
then validates the migrated current representation before writes.

Schema `1.2.0` adds the optional nullable `contract_fingerprint` embedding
field. Current exports always emit the property as an exact 64-character
lowercase hexadecimal value or `null`; import restores it in the same
transaction as the vector. The registered `1.1.0 -> 1.2.0` migration records
legacy absence as `null`, recomputes the embedding component checksum, and
revalidates the migrated representation before writes.

The 1.x/default path retains gaps in complete absent-versus-null semantic
preservation across all accepted records, current-minus-two historical
migration coverage, and end-to-end streaming archive processing across the
legacy JSON request buffer, structured components, and single-pass live export
emission.
`record-v1` does not imply
preservation of templates, embeddings,
SKOS, provenance, graph/community data, URL-only links, signature guarantees,
or attachment bytes. Making tombstone-field presence mandatory in 1.x requires
a schema-major or new profile identifier. Those gaps block broad/default
claims; they do not negate the later exact receipt-bound
`2.0.0/full-v1` cells.

ADR-103 selects schema `2.0.0` with the existing profile identifiers and
direct JSON key-presence semantics. Revision 21 advertises only exact
`2.0.0/full-v1` as a receipt-bound opt-in after the runtime and paired
cross-repository receipts passed. The default remains `1.2.0/core-v1`, and
schema-2 `core-v1` and `record-v1` remain unadvertised. The receipt authorizes
only its named cells, not suite-wide compatibility, portability, complete
backup, or parity.

The filesystem backend provides a bounded-memory staging primitive that streams
bytes into an isolated `staging/shard-import/` namespace, verifies the declared
byte length and canonical BLAKE3 digest, rechecks integrity before atomic
promotion into `blobs/`, and supports receipt-bound compensation plus startup
cleanup of stale stages. Archive preflight now streams canonical sidecar tar
entries through a 64 KiB copy-and-hash buffer into request-scoped temporary
files. After the complete manifest, inventory, component, relationship, length,
and digest preflight succeeds, the HTTP route streams referenced files through
the storage staging primitive. Orphan sidecars fail before storage staging or
database mutation, while profile policy distinguishes optional `core-v1` and
`record-v1` bytes from mandatory `full-v1` bytes. It exports available
verified bytes when `include_blobs=true`: filesystem content is hashed directly
into a disk-backed archive, legacy database content is bounded to one blob at a
time, and the completed archive is size-checked before a bounded response
stream owns its temporary-file cleanup. For the 1.x default path, this
satisfies the opt-in `core-v1` attachment-byte transport prerequisites; it did
not by itself establish the later `2.0.0/full-v1` receipt.

The later `2.0.0/full-v1` runtime uses the same staging primitive with a
schema-scoped durable import journal to bridge the filesystem/database commit
boundary. Before promotion, the journal records each verified blob identity,
canonical digest, size, final storage path, and whether recovery owns a new
final link or must preserve pre-existing exact bytes. Journal writes use
file-sync, atomic rename, parent-directory sync, and private permissions. A
process-held lock excludes startup recovery while an importer is live. After
process death, startup obtains that lock, compares the journal with the
schema-scoped `attachment_blob` rows, verifies and preserves committed final
bytes, compensates only uncommitted links owned by the interrupted import,
discards remaining stages, and removes the journal and lock. This is runtime
crash consistency for attachment sidecars; it does not change the Knowledge
Shard wire schema, profile identity, or producer/consumer authority.

Fortemi #1093 exercises the two-sidecar promotion boundary with real child
process aborts after the first final link and before its promotion receipt,
after the first persisted receipt, and after both persisted receipts but before
database commit. A fourth real child abort runs immediately after successful
database commit. The pre-commit restart oracles prove zero committed rows,
the exact `pending`/`promoting`/`promoted` journal state, exact compensation
and staging cleanup, and clean retry convergence. The post-commit oracle proves
that startup reads the committed schema state, verifies and preserves both
final blobs, removes only journal/lock state, and converges under idempotent
retry. Deterministic tests also cover live-journal exclusion and interruption
immediately after temporary-record write, temporary-file sync, atomic rename,
and parent-directory sync. Before rename, startup consumes only the complete
prior record; after rename, it consumes only the complete new record. An
orphan temporary rewrite without a complete authority fails recovery without
deletion, and startup suppresses the stale sidecar-staging sweep after any
recovery failure. Parent-directory sync failures propagate on Unix; non-Unix
directory durability remains best effort.

The same route harness terminates a real child after exactly a 7-byte prefix of
the first sidecar has been written. The parent observes zero component rows,
exactly one 7-byte unverified `.blob.stage.tmp`, and no journal, verified stage,
or final blob. An explicit startup-equivalent sweep restores the exact
pre-abort filesystem baseline, and a normal retry converges to two notes, three
attachment references, two blobs, and refcount sum 3. This demonstrates process
death after a completed partial write, before integrity verification or journal
ownership; it is not evidence for death inside an unresolved read/write syscall.

The process-abort harness also terminates a real child immediately after the
initial journal temporary write, temporary-file sync, atomic rename, and
parent-directory sync. All four boundaries retain zero component rows, two
verified staged sidecars, one complete journal candidate, and the dead
process's lock. Post-rename restart recovery consumes the authoritative journal
directly. Pre-rename restart now acquires the dead operation's lock, parses and
validates the complete candidate identity and canonical staged-blob paths,
requires every entry to remain in the initial `pending` state, syncs the
candidate, atomically promotes it to the authoritative filename, syncs the
directory, and then runs normal recovery. A live lock, malformed candidate, or
candidate containing any post-promotion state remains fail-closed and
untouched. Every process-abort boundary returns to the exact pre-abort
filesystem baseline and converges under clean retry. This is evidence for
process death immediately after completed operations, not death in the middle
of a write or kernel syscall and not power-loss durability. Termination in the
middle of sidecar read/write syscalls, mid-operation journal persistence,
kernel-level write/fsync failure or power loss, an in-flight commit
acknowledgement ambiguity, and the complete platform/filesystem matrix remain
separate acceptance gates.

The Fortemi CI build now has a dedicated scoped AL-SYS04/05 receipt runner for
this runtime recovery boundary. It reruns the live HTTP/TUS restart and
concurrency tests, the two-sidecar process-abort matrix, journal recovery, and
filesystem refcount tests before emitting a sanitized machine receipt. The
receipt binds the exact clean commit and source digests, records headless
Linux/PostgreSQL/filesystem execution and explicit isolated no-auth mode, and
keeps authenticated operation, mid-syscall termination, kernel/fsync failure,
power loss, in-flight commit ambiguity, non-Unix durability, the platform
matrix, and suite-wide portability false. The receipt is runtime test evidence,
not a Knowledge Shard component or schema change. Its CI wiring does not become
immutable evidence until a successful Gitea artifact upload is observed.

ADR-104 adds a separate platform-qualified aggregate.
[Gitea run 6393](https://git.integrolabs.net/Fortemi/fortemi/actions/runs/6393)
passed the declared Fortemi authority-to-React/core-to-HotM contract surface on
Linux x86_64, Linux arm64, and macOS arm64 at exact revisions. Windows is the
only deferred operating system and is tracked by Fortemi #1096. That receipt
does not establish launched GUI/native-dialog coverage, universal portability,
complete backup, or one schema across the suite persistence planes. Fortemi
#1081 remains `NO-GO` pending independent audit.

For any tuple or broader claim whose release gates have not passed:

- user documentation must label lossless/full-profile behavior as a target;
- consumers must advertise only behavior demonstrated by tests; and
- default exports must not be represented as disaster-recovery complete.

## Consequences

### Positive

- One contract authority replaces incompatible local interpretations.
- Profiles make reduced transports explicit and testable.
- Atomic fail-closed import prevents partial or corrupt restores.
- Round-trip requirements cover semantic identity and binary content, not only
  JSON parsing.
- Release gates turn interoperability claims into reproducible evidence.

### Negative

- Existing producers and consumers require coordinated remediation.
- Strict imports will reject archives previously accepted with warnings.
- Golden corpus and cross-repository test maintenance become release work.
- `full-v1` archives may be larger because referenced attachment bytes must be
  portable.

## Alternatives considered

### Continue best-effort import

Rejected because silent skipping and warning-only checksum handling cannot
support backup, recovery, or lossless exchange claims.

### Let each repository own its local schema

Rejected because independently compatible-looking schemas have already drifted
in field names, component coverage, and version semantics.

### Use application versions for compatibility

Rejected because application releases and data-contract evolution are
independent. Package versions cannot substitute for a schema compatibility
contract.

### Define only one universal profile

Rejected because RecordStore and reduced clients have legitimate subset use
cases. Named profiles preserve those uses without misrepresenting them as full
fidelity.

## References

- [ADR-028: Shard and Archive Migration System](ADR-028-shard-archive-migration-system.md)
- [ADR-029: Shard Schema Versioning Specification](ADR-029-shard-schema-versioning.md)
- [Shard Migration Guide](../../content/shard-migration.md)
- [Semantic Versioning 2.0.0](https://semver.org/)

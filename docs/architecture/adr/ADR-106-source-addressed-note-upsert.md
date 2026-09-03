# ADR-106: Source-Addressed Atomic Note Upsert Contract

**Status:** Accepted
**Date:** 2026-09-03
**Decision owners:** Fortemi API, persistence, tenancy, and contract maintainers
**Producer tracking:** [Fortemi/fortemi#1090](https://gitea.fortemi.com/Fortemi/fortemi/issues/1090)
**Consumer tracking:** [Fortemi/fortemi-react#404](https://gitea.fortemi.com/Fortemi/fortemi-react/issues/404), [AIWG/aiwg#2194](https://gitea.fortemi.com/AIWG/aiwg/issues/2194)
**Suite coordination:** [Fortemi/fortemi#1081](https://gitea.fortemi.com/Fortemi/fortemi/issues/1081)
**Upstream policy authority:** `fortemi-suite/.aiwg/architecture/ADR-suite-contract-authority-and-profiles.md`
**Extends:** ADR-068, ADR-090, ADR-102, ADR-103, ADR-104

## Context

AIWG session import and other externally managed sources need a generic Core
operation that can address a note by a source namespace and external ID. The
ordinary create/update routes cannot distinguish an exact replay from changed
content and cannot commit a bounded import batch atomically. Reusing inbound
event connector issue #833 would also conflate provider delivery with generic
note persistence.

The suite has three distinct data planes: the AIWG static index, Knowledge
Shard state transfer, and live Fortemi persistence. Adding live source identity
to a Knowledge Shard profile without a versioned authority and every consumer
would violate that boundary. A route, vendored type, or local unit test is not
cross-repository compatibility evidence.

## Decision

Fortemi owns `source-note-upsert/1.0.0`, published under
`contracts/source-note-upsert`. It is an independent live persistence contract
and does not revise any Knowledge Shard schema or profile.

### Identity, authorization, and isolation

The canonical identity is `(tenant, memory, source_namespace, external_id)`.
Tenant and memory are derived from verified request state. They are not body
parameters and therefore cannot select or authorize another scope. PostgreSQL
forced RLS protects tenant rows, and each memory has its own source tables.
Existing archives are upgraded by the authority migration before the hosted
tenant catalog gate runs; new archives inherit the tables through the existing
deny-list clone path.

The API returns `external_id_hash`, computed over the scoped identity tuple,
instead of echoing the raw external ID. Optional `caller_stable_id` chooses the
note UUID only on first insert and cannot retarget an identity or occupy an
existing note.

### Batch and replay semantics

`POST /api/v1/notes/source-upsert` and MCP `upsert_external_notes` accept 1–500
items. One request-owned transaction covers note, original, revision, activity,
source identity, import-run, and import-batch writes. Database failure rolls
all of them back. The operation does not enqueue jobs, write blobs, or publish
outbox events.

An exact content replay is `unchanged`. Changed content follows an explicit
`version`, `replace`, or `conflict` policy, defaulting to `version`. Versioning
adds a revision; replacement changes managed original/current content without
adding a revision; conflict reports without mutation. Validation rejection and
dry-run make no writes.

Import journals store a request digest, bounded checkpoint, and redacted
receipt. If `batch_id` is omitted, the request digest supplies it. An exact
batch replay returns `duplicate` with material results converted to
`unchanged`; it adds no note, revision, activity, job, blob, outbox, or journal
row. Reusing an explicit batch ID with a different request is rejected.

### Non-echoing operational boundary

Request and item `Debug` implementations expose only lengths, counts, presence,
and enum values. Activity metadata contains a constant policy marker only.
Responses and journals contain note UUIDs, content digests, source identity
digests, counts, and stable reason codes; raw external keys and content remain
excluded from operation logs and receipts.

### Knowledge Shard loss

Source identities are outside `core-v1`, `full-v1`, and `record-v1`. Knowledge
Shard export does not serialize the new live tables. Instead it returns a
profile-qualified `X-Fortemi-Shard-Loss-Report` header with
`source-identity-outside-profile` and a count when identities are omitted. This
is typed loss evidence, not a portability or complete-backup claim.

### Compatibility evidence

The authority bundle includes request/response schemas and a shared executable
fixture. Server PostgreSQL, consumer PGlite, and consumer RecordStore must run
the same fixture and publish receipts containing exact repository revisions,
fixture hashes, test commands, and CI run URLs. AIWG may qualify the MCP tool
only after pinning the delivered Fortemi contract revision. The suite audit
remains `NO-GO` for broad parity, complete backup, and portability claims.

## Migration and rollback

The `1.0.0` migration is additive. Deploy the migration before enabling the
REST/MCP surface. Rollback first disables callers and the route. Tables may be
dropped only after operators have separately preserved any required live
identity/run state; dropping them is intentionally not an automatic downgrade.
The old ordinary note CRUD routes remain compatible throughout.

A breaking request, response, identity, replay, or loss-report change requires
a contract-major revision and coordinated producer/consumer migration. An
additive optional field requires a contract-minor revision. Receipt-only or
editorial changes may increment receipt revision without changing contract
SemVer.

## Consequences

- External importers gain deterministic, resumable, provider-neutral note
  persistence without coupling Core to provider discovery or AIWG taxonomy.
- Source identity state is deliberately not portable through current Knowledge
  Shard profiles; exporters report that fact explicitly.
- The database stores raw external keys because they are the lookup authority,
  while operational surfaces stay non-echoing.
- Cross-repository completion requires authority, server, both React stores,
  shared fixture hashes, receipts, documentation, and clean-destination CI.

## Derivation

@implements `contracts/source-note-upsert/README.md`
@implements `crates/matric-core/src/source_upsert.rs`
@implements `crates/matric-db/src/source_upsert.rs`
@implements `migrations/20260903010000_source_addressed_note_upsert.sql`
@tests `crates/matric-db/tests/source_upsert_contract_test.rs`
@depends `docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md`

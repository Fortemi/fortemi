# ADR-108: Previewable Lifecycle Purge and Content-Free Receipts

**Status:** Accepted  
**Date:** 2026-09-15  
**Authority:** `Fortemi/fortemi#1092`  
**Consumer:** `Fortemi/fortemi-react#406`  
**Extends:** ADR-102, ADR-103, ADR-106  
**Upstream policy authority:** `fortemi-suite/.aiwg/architecture/ADR-suite-contract-authority-and-profiles.md`

## Context

Soft deletion does not satisfy terminal erasure. A terminal purge must remove a
selected note graph, preserve shared blobs, clear search state, survive a crash
between relational and external deletion, and produce useful evidence without
retaining linkable content or target identifiers.

This contract applies to Fortemi live persistence. It does not merge the AIWG
static index, Knowledge Shard transfer schemas, or live persistence planes, and
it does not change any `core-v1`, `full-v1`, or `record-v1` profile.

## Decision

1. A caller first creates a 15-minute preview. Combined note and source
   selectors use intersection semantics. Execution re-locks the selected notes
   and rejects a preview when its selected IDs or ten-category counts changed.
   Creating a preview removes expired, unconsumed selector snapshots in the
   same tenant and memory.
2. Relational erasure is one transaction. Audit and provenance rows with
   `ON DELETE SET NULL`, shared job rows, and shared outbox rows are erased
   explicitly; normal graph children cascade from `note`.
3. A blob is purge-owned only when no surviving attachment references it as
   either a primary blob or preview blob. Filesystem paths stay in private,
   tenant-scoped cleanup state and must equal the canonical path derived from
   the blob UUID before deletion.
4. Filesystem and search-cache cleanup are durable, observable work. A retry
   repeats idempotent effects and issues exactly one terminal receipt only after
   both are acknowledged.
5. Public previews and receipts contain opaque IDs, counts, timestamps, outcome,
   and fixed policy metadata. They never contain selectors, source keys, target
   IDs, tenant/archive identifiers, raw paths, provider payloads, content, or
   content-derived/linkable digests.
6. Purged note IDs remain in private erasure-target state. Knowledge Shard
   restore re-erases a resurrected target in the import transaction before it
   can be queued for indexing, then repeats external cleanup before import
   reports success. A retry also resumes operations already left
   `cleanup_pending` by a crash after the restore transaction committed.
7. A whole-source purge removes source import checkpoints and idempotency
   journals only after no identity remains in that namespace. A single external
   ID does not claim ownership of a source-wide journal.

The executable vocabulary and negative controls are in
`contracts/lifecycle-purge/conformance/v1.json`. The OpenAPI authority exposes
preview, begin, status, and resume operations under `/api/v1/lifecycle-purge`.

## Consequences

- A begin response can be `cleanup_pending`; clients or workers call `resume`
  until a terminal receipt is present.
- Backups are beyond use while held. A later restore does not make a retained
  purge target live again.
- A valid receipt proves this operation reached terminal state. It is not proof
  of complete backup, suite parity, or an unqualified portability claim.

@depends `docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md`
@depends `contracts/lifecycle-purge/conformance/v1.json`
@issue `Fortemi/fortemi#1092`
@issue `Fortemi/fortemi-react#406`

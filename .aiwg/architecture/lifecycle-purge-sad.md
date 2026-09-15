# Lifecycle Purge Software Architecture Description

**Authority:** `docs/architecture/adr/ADR-108-previewable-lifecycle-purge.md`  
**Contract:** `contracts/lifecycle-purge/conformance/v1.json`  
**Scope:** live Fortemi persistence only

## Components

| Component | Responsibility |
|---|---|
| `matric-core::lifecycle_purge` | Versioned request, preview, count, status and receipt vocabulary |
| `PgLifecyclePurgeRepository` | Tenant/memory-scoped snapshot, lock, relational erasure, replay and finalization |
| Lifecycle purge migration | RLS-protected preview, operation, cleanup, re-erasure and receipt state |
| API lifecycle routes | Authenticated preview, begin, observable status and idempotent resume |
| Knowledge Shard importer | Re-erasure before indexing plus terminal external cleanup |
| Filesystem backend | Canonical-path-checked, idempotent sidecar deletion |

## State Model

`preview -> cleanup_pending -> completed`

- Preview consumption and relational graph deletion share one transaction.
- New preview creation removes expired unconsumed snapshots in the same tenant
  and memory; consumed snapshots are scrubbed immediately.
- Blob and search cleanup can be repeated after process failure.
- `completed` requires zero pending blob tasks and acknowledged search cleanup.
- Restore of a retained target returns the operation to `cleanup_pending`,
  removes the superseded terminal row, and reissues one receipt after cleanup.
- A restore retry surfaces already-pending operations even when their restored
  note was erased before the prior process stopped.

## Security Boundaries

- Tenant RLS and tenant-qualified foreign keys protect all operational state.
- Selector JSON, target IDs and paths are private state and are absent from API
  outputs, logs, debug formatting and terminal receipts.
- A persisted filesystem path is never trusted as authority; it must match the
  canonical path derived from the stored blob UUID.
- Shared inference services, OpenBao/GPG custody and Knowledge Shard profile
  claims are unchanged.

## Verification

The server conformance test covers exact preview counts, shared primary blob
survival, stale preview rejection, operation replay, crash-before-ack retry,
content-free receipt fields, one-receipt convergence and restore re-erasure.
Tenant/archive migration matrices and generated OpenAPI inventory remain release
gates.

@depends `docs/architecture/adr/ADR-108-previewable-lifecycle-purge.md`
@depends `.aiwg/architecture/ADR-suite-contract-authority-and-profiles.md`

# ADR-105: Manual Note-Link Mutation Contract

**Status:** Accepted
**Date:** 2026-09-01
**Decision owners:** Fortemi API, tenancy, and graph maintainers
**Producer tracking:** [Fortemi/fortemi#61](https://github.com/Fortemi/fortemi/issues/61),
[Fortemi/fortemi#62](https://github.com/Fortemi/fortemi/issues/62)
**Consumer tracking:** [Fortemi/HotM#10](https://github.com/Fortemi/HotM/issues/10)
**Originating review:** [Fortemi/fortemi#58](https://github.com/Fortemi/fortemi/pull/58)
**Extends:** ADR-012, ADR-037, ADR-068, ADR-071, ADR-104

## Context

Fortemi has long persisted server-generated note relations, while the public
`/api/v1/notes/{id}/links` contract was read-only. A proposed mutation route
treated the path identifier as a target, admitted user-authored `semantic`
links, returned a candidate UUID even when no row was inserted, and was not
available through the hosted tenant transaction gate. That behavior would
invert link direction, blur asserted and generated semantics, permit duplicate
rows under concurrent writers, and provide no authority artifact for HotM.

The operation is a tenant-data mutation. Its input and failure paths can carry
secret-like strings or identifiers, so validation and diagnostic behavior must
preserve Fortemi's established non-echoing boundary. A route, a local unit
test, or a consumer snapshot alone is not compatibility evidence.

## Decision

### Direction and writable taxonomy

`POST /api/v1/notes/{id}/links` implements `manual-note-link-v1`:

- `{id}` is the source note.
- `to_note_id` is the target note.
- `kind` is required and the only accepted user-writable value is `explicit`.
- `semantic` and `wiki` remain server-owned generated kinds.
- URL targets and URL-link identity are outside this contract and remain
  deferred.

The request contains no arbitrary metadata field. This keeps the mutation
surface bounded and prevents clients from storing unclassified sensitive
content through relation metadata.

### Score, self-links, and identity

`score` is optional, defaults to `1.0`, and must be finite and within the
inclusive range `0.0..=1.0`. Self-links are rejected.

The canonical note-link identity is:

```
(from_note_id, to_note_id, kind)
```

PostgreSQL enforces that identity through a partial unique index for rows with
`to_note_id IS NOT NULL`. The migration first refuses to proceed when existing
duplicates are present; it does not choose an arbitrary survivor.

The initial insert returns `201` and `created: true`. An exact repeat returns
`200`, `created: false`, and the original persisted ID and timestamp. A repeat
with a different score or any persisted metadata returns `409` without
changing the existing row. Repository create calls return the authoritative
persisted ID, never a discarded candidate ID. Concurrent transactions rely on
the database uniqueness boundary and must converge on one row and one ID.

### Visibility, tenancy, and authorization

Both source and target must be active: present, not soft-deleted, and not
archived. Missing, invisible, cross-tenant, archived, and deleted notes all
collapse to the same not-found response. The mutation never exposes whether a
target exists outside the request's visible tenant/archive scope.

The route remains a `TenantObject` action in the route-policy authority. POST
requires bearer authentication and write scope, binds the path `id` as the
source-note resource, and is admitted in hosted mode only through the verified
tenant request transaction. Community mode uses the same archive-scoped
transactional repository path. Missing hosted scope fails closed.

The route deliberately does not perform a separate backing-store lookup for
the source before policy evaluation. Authorization retains the opaque path ID
and verified tenant metadata, while the source and target visibility checks run
together inside the mutation transaction. This prevents the path ID from
becoming a `403` existence oracle while the body target receives the stable
non-enumerating not-found response.

Generated OpenAPI carries typed request, success, and Problem Details schemas,
bearer security, the required write scope, source resource metadata,
tenant-transaction requirement, target visibility check, and the
`manual-note-link-v1` supported disposition. The committed artifact must equal
the generated artifact byte for byte.

### Stable non-echoing failures

Malformed JSON, malformed identifiers, unsupported kinds, invalid scores,
self-links, unavailable notes, and attribute conflicts use constant problem
details. Submitted strings, identifiers, metadata, database errors, and
secret-like values are not interpolated into response or debug text. Request,
response, and repository debug implementations expose only presence, counts,
lengths, timestamps, numeric score, and created state.

### Event decision

This contract does not emit `NoteLinksUpdated` and does not change AsyncAPI.
Hosted transaction commit occurs after the handler returns, while the current
in-process event bus has no post-commit outbox handoff for this route. Emitting
before final commit could announce a row that is later rolled back. A future
event addition therefore requires a separately versioned post-commit/outbox
decision and coordinated consumer receipts.

### Producer/consumer and platform evidence

Fortemi is the producer authority. HotM may enable the mutation only after it
pins the exact delivered Fortemi commit and committed OpenAPI SHA-256, verifies
the operation disposition, generates or implements the typed client, and
records zero-dispatch behavior on compatibility denial. The HotM receipt must
name the exact consumer revision and the producer revision/digest.

ADR-104 remains the platform gate. Linux x86_64, Linux arm64, and macOS arm64
must execute the same declared producer/consumer surface before the release is
described as supported on those cells. Any unavailable cell must be explicitly
recorded as deferred; a build-only result is not behavioral evidence. This ADR
does not change the suite audit's `NO-GO` status and does not authorize broad
parity, complete-backup, or portability claims.

## Consequences

- Manual relations have one unambiguous direction and one authority-owned
  writable kind.
- Exact retries are safe and return stable persisted identity.
- Existing duplicates block migration and require an explicit operator
  reconciliation rather than silent deletion.
- Hosted source and target checks share one tenant-bound transaction.
- Mutation events remain unavailable until post-commit delivery can be proven.
- HotM stays fail-closed until it consumes the exact producer artifact.

## Alternatives Rejected

### Permit clients to write `semantic` or `wiki`

Rejected because those kinds describe generated evidence and would make
asserted and computed graph edges indistinguishable.

### Treat a different score as an update

Rejected because retry behavior would become order-dependent and a repeated
create would silently mutate persisted graph weight.

### Keep application-only duplicate detection

Rejected because `SELECT ... WHERE NOT EXISTS` does not serialize independent
writers and previously returned IDs for rows that were never inserted.

### Emit `NoteLinksUpdated` directly from the handler

Rejected because handler completion precedes hosted transaction commit.

## References

- @docs/architecture/adr/ADR-012-semantic-linking-threshold.md
- @docs/architecture/adr/ADR-037-unified-event-bus.md
- @docs/architecture/adr/ADR-068-archive-isolation-routing.md
- @docs/architecture/adr/ADR-071-auth-middleware.md
- @docs/architecture/adr/ADR-104-supported-platform-suite-conformance.md
- @contracts/openapi/openapi.yaml

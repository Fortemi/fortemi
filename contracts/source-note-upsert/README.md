# Source-Addressed Note Upsert Contract

Fortemi is the authority and server producer for the provider-neutral
`source-note-upsert/1.0.0` live persistence contract. The contract is exposed
as `POST /api/v1/notes/source-upsert` and the core MCP tool
`upsert_external_notes`.

This contract is deliberately separate from all Knowledge Shard profiles.
Source identities are live persistence data and are not silently added to
`core-v1`, `full-v1`, or `record-v1`. A Knowledge Shard export reports the
typed loss code `source-identity-outside-profile` in the
`X-Fortemi-Shard-Loss-Report` response header when the selected memory has
source identities.

## Identity and scope

The identity tuple is `(tenant, memory, source_namespace, external_id)`. The
server derives tenant and memory from the authenticated request and active
memory; neither is caller-authorized through the body. Receipts expose only an
opaque SHA-256 digest of that tuple. They never echo external IDs or content.

`caller_stable_id` is an optional UUID used only when first creating a note.
It cannot retarget an existing source identity or claim an occupied note ID.

## Atomic and replay behavior

Batches contain 1–500 items and run in one database transaction across notes,
originals, revisions, activity, source identities, and import journals. An
unexpected write failure rolls back every layer. Validation rejection and
`dry_run` perform no writes.

The default changed-content policy is `version`. `replace` updates the managed
content without adding a revision, while `conflict` reports the difference
without mutating the note. Exact content replays return `unchanged`. Exact
batch replays return the stored note IDs as `unchanged` with batch outcome
`duplicate`, without adding notes, revisions, jobs, activity, blobs, outbox
events, or journal rows.

If `batch_id` is absent, it is derived from the SHA-256 request digest. A
caller-supplied batch ID may be resumed only with the identical request digest.
`checkpoint` is an opaque bounded JSON value persisted with the batch receipt.

## Authority and evidence

- Current receipt: `contract.json`
- Request schema: `1.0.0/request.schema.json`
- Response schema: `1.0.0/response.schema.json`
- Shared executable fixture: `conformance/v1.json`
- Architecture decision: `docs/architecture/adr/ADR-106-source-addressed-note-upsert.md`
- Producer issue: `Fortemi/fortemi#1090`
- PGlite/RecordStore consumer issue: `Fortemi/fortemi-react#404`
- AIWG live qualifier: `AIWG/aiwg#2194`

The receipt is compatibility evidence only for the exact revisions and hashes
it names. It does not establish unqualified suite parity, complete backup, or
Knowledge Shard portability.

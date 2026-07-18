# Knowledge Shard Contract

This directory is the Fortemi-owned schema authority for Knowledge Shard
consumers. `contract.json` identifies the current schema/profile revision,
stable schema paths, exact file digests, golden corpus, and demonstrated
limitations.

The current contract revision supports Knowledge Shard schema `1.1.0` under
`core-v1` and publishes a conformance candidate for `record-v1`:

- `core-v1`: notes, collections, tags, templates, links, and attachment
  projections.
- `record-v1` candidate: notes, collections, tags, note-to-note links, and
  attachment projections.

Each profile has its own manifest and record schemas under
`contracts/knowledge-shard/1.1.0/<profile>/`. Fortemi import selects and
applies those schemas by version and profile before component inventory/count
validation and before normal persistent writes. Positive and negative fixtures
live under `tests/fixtures/shards`.

Schema `1.0.0` remains immutable under its original stable path and receipt
hashes. The registered `1.0.0 -> 1.1.0` migration adds `deleted_at: null` to
legacy note records, recording the legacy absence as the documented active
state. Current exports always emit `deleted_at` as either `null` or an exact
timestamp and include soft-deleted notes.

Consumers must pin an immutable Fortemi commit, verify every digest in
`contract.json`, and treat the schema files as upstream authority. Vendored
copies are receipts, not independent definitions.

`core-v1` includes attachment metadata/reference projections by default.
Attachment content identities use the server's canonical
`blake3:<64 lowercase hex>` form. The bounded REST route can optionally carry
verified digest-addressed sidecars, but that does not establish `full-v1`
conformance. The `record-v1` candidate is a deliberately reduced RecordStore transport:
producers must return a machine-readable report covering templates, embeddings,
SKOS, provenance, graph/community data, URL-only links, signature guarantees,
attachment-byte omissions, and every other source concept that is not preserved
by the selected export. It must not be advertised as supported until the pinned
React producer/server consumer receipt passes. `full-v1` remains reserved and unsupported. Complete
absent-versus-null preservation still requires a schema-major or new profile
identifier because `deleted_at` is optional during the 1.1 transition.

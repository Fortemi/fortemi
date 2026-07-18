# Knowledge Shard Contract

This directory is the Fortemi-owned schema authority for Knowledge Shard
consumers. `contract.json` identifies the current schema/profile revision,
stable schema paths, exact file digests, golden corpus, and demonstrated
limitations.

The current supported contract is Knowledge Shard schema `1.1.0`, profile
`core-v1`. It contains:

- `manifest.schema.json`
- `note.schema.json`
- `collection.schema.json`
- `tag.schema.json`
- `template.schema.json`
- `link.schema.json`

Fortemi import compiles and applies these same schemas before component
inventory/count validation and before normal persistent writes. Positive and
negative fixtures live under `tests/fixtures/shards`.

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
conformance. `full-v1` and `record-v1` remain reserved and unsupported.
Complete absent-versus-null preservation still requires a schema-major or new
profile identifier because `deleted_at` is optional during the 1.1 transition.

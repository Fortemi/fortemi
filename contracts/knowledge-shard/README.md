# Knowledge Shard Contract

This directory is the Fortemi-owned schema authority for Knowledge Shard
consumers. `contract.json` identifies the current schema/profile revision,
stable schema paths, exact file digests, golden corpus, and demonstrated
limitations.

The current supported contract is Knowledge Shard schema `1.0.0`, profile
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

Consumers must pin an immutable Fortemi commit, verify every digest in
`contract.json`, and treat the schema files as upstream authority. Vendored
copies are receipts, not independent definitions.

`core-v1` currently includes attachment metadata/reference projections but not
attachment bytes. `full-v1` and `record-v1` remain reserved and unsupported.
The presence of these schemas does not establish atomic recovery, historical
migration, or full-profile conformance.

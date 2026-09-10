# Remote Adapter Capture

`remote-adapter.json` contains actual HTTP request/response pairs captured by
`scripts/ci/capture-remote-adapter-fixture.mjs` for producer issue #1146 and
React consumer issues #417 through #421. Runtime identity is the immutable
2026.9.9 bundle, source `e91c595a896275f835cb7ed1aef173cb26056206`.

The capture requires a loopback, explicitly lane-owned disposable container
with zero native note rows, including tombstones. It creates synthetic notes,
an explicit directional link and seeded provenance. Seeding demonstrates
serialization, not actual AI execution. All fixture notes are soft-deleted
in a finally block; removing the owned tmpfs container completes cleanup.

The Rust `remote_adapter_fixture_tests` deserialize the captured list/detail,
links, search and provenance through producer models and reject malformed
required fields. The fixture does not replace the generated OpenAPI authority.

This first capture covers 25 cases. It is not complete acceptance for #1146:
auth denial/rate-limit/internal error fixtures, explicit degraded semantic
search, full advertised mutation coverage, consumer source/release pins and
published-package live execution remain pending. No native shard-restore,
general API parity or suite portability claim follows; suite NO-GO remains.

## Search and Mutation Extension

`remote-operations.json` is a second clean-destination capture from the same
released runtime. Its 37 cases retain the first capture's operations and add
bounded search limits, an AND-tag no-match, semantic/hybrid FTS degradation,
content/tag update, archive/unarchive, delete, absent-after-delete and restore.
The first fixture's bytes remain unchanged. Both notes are soft-deleted again
after restoration. The capture script now emits this expanded case set.

The producer tests validate search degradation and mutation inputs/outputs
through the actual Rust types, plus body-free deletion and restore identity.
This extends source evidence for React #419/#420; it is not successful vector
retrieval, auth denial/rate-limit/internal-error qualification or live published
React acceptance. Those remaining #1146 gates and suite NO-GO still apply.

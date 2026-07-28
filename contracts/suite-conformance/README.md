# Supported-Platform Suite Conformance

This directory is the Fortemi-owned execution contract for the bounded
authority-to-consumer platform claim in ADR-104.

`platform-matrix.json` pins the separate Knowledge Shard and server
compatibility authority revisions, consumer revisions,
`@fortemi/core` source, version, profile, published Linux `.tgz` digest, and
cross-platform tar payload digest, per-platform immutable sidecar digests, the
exact Linux x86_64 and macOS arm64 cells, required gates, deferred platforms,
and prohibited claims.
`platform-matrix.schema.json` defines its machine-readable shape.

The raw npm `.tgz` is retained as Linux release evidence. Cross-platform
package parity is bound to the SHA-256 of its decompressed tar bytes because
gzip streams can differ across platform zlib implementations even when every
tar header and file byte is identical. Both required cells must match the tar
payload digest; the Linux cell must additionally match the published `.tgz`.

The matrix does not redefine Knowledge Shard profiles, OpenAPI, AsyncAPI,
authentication, or compatibility discovery. It composes their existing
authority-owned receipts into a platform-qualified release gate.

Each platform run owns its database lifecycle: the Linux cell uses the pinned
test database image and the macOS cell uses native Homebrew PostgreSQL 18.
Both recreate the database, including the required extension baseline,
between authority tests, React/core, and HotM. No participant inherits another
participant's test data or background workers.

Run:

```bash
python3 scripts/ci/verify-suite-platform-matrix.py manifest
python3 scripts/ci/verify-suite-platform-matrix.py aggregate \
  --platform-receipt <linux-receipt.json> \
  --platform-receipt <macos-receipt.json> \
  --output <aggregate-receipt.json>
```

Only Linux x86_64 and macOS arm64 on `mutsu` are required. Other platforms are
explicitly deferred and are not part of a passing claim.

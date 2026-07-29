# Supported-Platform Suite Conformance

This directory is the Fortemi-owned execution contract for the bounded
authority-to-consumer platform claim in ADR-104.

`platform-matrix.json` pins the separate Knowledge Shard and server
compatibility authority revisions, consumer revisions,
`@fortemi/core` source, version, profile, published Linux `.tgz` digest, and
cross-platform tar payload digest, per-platform immutable sidecar digests, the
exact Linux x86_64, Linux arm64, and macOS arm64 cells, required gates, the
deferred Windows operating system, and prohibited claims.
`platform-matrix.schema.json` defines its machine-readable shape.

The raw npm `.tgz` is retained as Linux release evidence. Cross-platform
package parity is bound to the SHA-256 of its decompressed tar bytes because
gzip streams can differ across platform zlib implementations even when every
tar header and file byte is identical. All three required cells must match the
tar payload digest; both Linux cells must additionally match the published
`.tgz`.

The matrix does not redefine Knowledge Shard profiles, OpenAPI, AsyncAPI,
authentication, or compatibility discovery. It composes their existing
authority-owned receipts into a platform-qualified release gate.

Each platform run owns its database lifecycle: the Linux cells use the pinned
test database image and the macOS cell uses native Homebrew PostgreSQL 18. All
three recreate the database, including the required extension baseline,
between authority tests, React/core, and HotM. No participant inherits
another participant's test data or background workers.

Run:

```bash
python3 scripts/ci/verify-suite-platform-matrix.py manifest
python3 scripts/ci/verify-suite-platform-matrix.py aggregate \
  --platform-receipt <linux-x86_64-receipt.json> \
  --platform-receipt <linux-arm64-receipt.json> \
  --platform-receipt <macos-receipt.json> \
  --output <aggregate-receipt.json>
```

Linux x86_64, Linux arm64 on native Colima virtualization on `mutsu`, and
macOS arm64 on `mutsu` are required. Windows is the only deferred operating
system. Architectures outside the exact matrix are not separately claimed.

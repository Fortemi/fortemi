# Supported-Platform Suite Conformance

This directory is the Fortemi-owned execution contract for the bounded
authority-to-consumer platform claim in ADR-104.

`platform-matrix.json` pins the separate Knowledge Shard and server
compatibility authority revisions, consumer revisions,
`@fortemi/core` source, version, profile, and reproducible packed-tarball
digest, per-platform immutable sidecar digests, the exact Linux x86_64 and
macOS arm64 cells, required gates, deferred platforms, and prohibited claims.
`platform-matrix.schema.json` defines its machine-readable shape.

The matrix does not redefine Knowledge Shard profiles, OpenAPI, AsyncAPI,
authentication, or compatibility discovery. It composes their existing
authority-owned receipts into a platform-qualified release gate.

Each platform run owns its database container and recreates the database,
including the image-provisioned extension baseline, between authority tests,
React/core, and HotM. No participant inherits another participant's test data
or background workers.

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

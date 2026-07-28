# Supported-Platform Suite Conformance

This directory is the Fortemi-owned execution contract for the bounded
authority-to-consumer platform claim in ADR-104.

`platform-matrix.json` pins the authority and consumer revisions, the exact
Linux x86_64 and macOS arm64 cells, required gates, deferred platforms, and
prohibited claims. `platform-matrix.schema.json` defines its machine-readable
shape.

The matrix does not redefine Knowledge Shard profiles, OpenAPI, AsyncAPI,
authentication, or compatibility discovery. It composes their existing
authority-owned receipts into a platform-qualified release gate.

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

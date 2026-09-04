# Dataset execution contract

This directory publishes Fortemi's live MCP adapter for the upstream Fortemi
Core dataset contracts. It is a live persistence-plane contract. It is not the
AIWG static index and it is not a Knowledge Shard schema or parity claim.

`1.0.0/authority.json` pins the exact producer/consumer authority chain.
`request.schema.json` defines the canonical input accepted by
`manage_dataset_execution` for `preview` and `execute` actions.
`run-receipt.schema.json` defines redacted terminal and non-terminal evidence.

Canonical serialization recursively sorts object keys by UTF-16 code units,
preserves array order, rejects non-JSON/non-finite values, and encodes the
result as UTF-8 without insignificant whitespace. A digest is the lowercase
hexadecimal SHA-256 of those bytes prefixed with `sha256:`. The vector
`{"a":1,"b":2}` therefore has digest
`sha256:43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777`.

The server descriptor is alpha and names only the current bounded Community
surface. Enterprise tenant/RLS certification, failure injection, backup and
restore certification, production migration, and load testing remain outside
this contract. No broad portability or Knowledge Shard parity follows from a
passing dataset RunReceipt.

Related work: Fortemi #1128–#1131, Fortemi React #408–#411, and AIWG #2242.

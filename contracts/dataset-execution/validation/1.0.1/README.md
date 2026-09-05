# Dataset validation and request binding revision 1.0.1

Fortemi #1131 owns this revision; roctinam/aiwg#2242 owns the independent consumer.
The original `../../1.0.0` bundle is immutable. This directory supplies stricter
validation for its wire schema and corrected request binding, advertised through
`capabilities.receiptValidation`.

The two JSON Schemas reject undeclared nested fields, unsupported revisions,
invalid identifiers/digests/profiles, resource limits, and contradictory state
variants. Executable verification additionally enforces count arithmetic,
effect/count agreement, checkpoint scope/order, resource counts, and redundant
output, negotiation, and resource-envelope digests. JSON Schema alone does not
prove these relationships or that a live execution occurred.

Request binding revision 1.0.1 hashes the canonical object containing
`contractVersions`, `schemaVersions`, `negotiation`, `plan`, `batch`,
`resourceEnvelope`, `profiles`, `inputSchemaDigest`, and `outputSchemaDigest`.
The digest is also stored in each source-upsert item's dataset metadata so an
explicit batch key cannot bypass the durable request-conflict check after an
MCP restart. AIWG computes this digest independently before a mutating call.

This corrects omitted schema bindings, so request-derived idempotency keys and
receipt digests differ from the legacy algorithm. Existing receipts remain
readable. A caller must discover the current binding revision and obtain a new
preview before submitting work; this revision does not certify cross-release
replay of legacy runs. Explicit legacy batch-key conflicts fail closed at the
storage journal instead of silently substituting new bindings.

The packaged MCP schemas are byte-checked against this authority by
`node scripts/ci/verify-dataset-execution-contract.mjs`. The shared negative
receipt vectors recompute the checksum before validation: rejection must depend
on schema and semantic checks, not merely a stale checksum. The current receipt
fixture binds the corrected request digest, while the original fixture remains
available for historical reader verification.

The supported scope remains the alpha `live-remote-persistence` profile. This
revision makes no Knowledge Shard, backup/restore, tenant-isolation, or broader
suite parity claim. Consumer delivery and live qualification must be linked
before the parent integration issues close.

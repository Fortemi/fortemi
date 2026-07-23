# ADR-103: Lossless Knowledge Shard Presence Semantics

**Status:** Accepted (target)
**Date:** 2026-07-22
**Decision owners:** Fortemi schema authority maintainers
**Tracking:** [Fortemi #1083](https://git.integrolabs.net/Fortemi/fortemi/issues/1083)

## Context

Knowledge Shard `1.x` does not preserve every distinction between an omitted
JSON property and a property whose value is JSON `null`. In particular,
`notes.deleted_at`, `embeddings.contract_fingerprint`, and manifest
`migration_history` admit states that current Rust and TypeScript models
collapse. Other mappers also replace empty or unavailable values with defaults.
ADR-102 already requires absent, null, empty, and populated states to remain
distinct, but changing preservation behavior is a breaking contract change.

The Fortemi repository remains the contract authority. A schema publication is
not a runtime-support claim; profile advertisement remains gated by immutable
producer/consumer receipts.

## Decision

### Contract identity

Publish schema `2.0.0` and retain the profile identifiers `core-v1`,
`record-v1`, and `full-v1`. The complete authority identity is the tuple
`(manifest.version, manifest.profile)`. Thus `1.2.0/full-v1` remains immutable
and is not semantically interchangeable with `2.0.0/full-v1`.

Every 2.0 manifest declares `min_reader_version: 2.0.0`. Readers that do not
support the exact schema/profile tuple reject the archive before staging,
database writes, or blob mutation.

### Wire representation

Presence uses ordinary JSON object-key semantics. No wire-level bitmap,
sentinel, or shadow envelope is added.

| State | Canonical representation | Outcome |
|---|---|---|
| absent | Property key omitted | Preserve only when the schema makes the property optional; otherwise reject. |
| null | Property key present with JSON `null` | Preserve only when the property schema admits null; otherwise reject. |
| empty | Property key present with `""`, `[]`, or `{}` | Preserve only when the property's type and constraints admit that exact empty value. |
| value | Property key present with a schema-valid non-empty value | Preserve exact JSON type and value. |
| unsupported | Illegal state or a source concept the destination cannot represent | Reject before writes. A reduced profile may convert only with a deterministic machine-readable loss report. |

Required-nullable fields continue to reject absence. Optional-nullable fields
preserve all schema-valid distinctions. Empty is a value, not a default and
not an alias for null. Unknown structural properties remain invalid; arbitrary
metadata values preserve nested key presence and JSON types recursively.

The normative generated inventory is
`contracts/knowledge-shard/2.0.0/field-semantics.json`. It covers every
nullable or optional field in all three profiles, its allowed states, equality
rule, and the server, PGlite, RecordStore, and AIWG implementation owner.

### Runtime representation

Readers classify own-property presence before any normalization:

- Rust uses a presence-aware deserializer/serializer rather than plain
  `Option<T>` where omission is allowed.
- TypeScript uses own-property checks rather than nullish coalescing or
  truthiness.
- PGlite persists presence metadata transactionally beside typed values because
  SQL `NULL` alone cannot represent both absence and JSON null.
- RecordStore retains own-property state and versioned presence metadata.
- AIWG conversion maps the exact state or emits a field-specific loss; it does
  not invent defaults to claim completeness.

Storage metadata is an implementation detail. Export always reconstructs the
direct-key wire representation and validates it against the authority schema.

### Migration and downgrade

The `1.0.0`, `1.1.0`, and `1.2.0` directories remain byte-for-byte immutable.
A registered `1.2.0 -> 2.0.0` migration may preserve a state only when the
source bytes prove it. The documented 1.x transitions may map a missing
`deleted_at` or `contract_fingerprint` to null, but the migration receipt must
record that legacy default; it must not claim that null was present in the
source. All changed checksums are recomputed and the complete 2.0 archive is
validated again before writes.

A 2.0 archive is never silently downgraded. `full-v1` downgrade rejects if any
state cannot project exactly. Reduced profiles may emit a deterministic loss
entry containing component, record identity, JSON Pointer, source state,
destination capability, action, and reason. The destination is not mutated
until all losses and rejection conditions are known.

### Rollout and rollback

Schema publication starts in `specified-implementation-pending` state. The
server and consumers retain 1.x readers and defaults during rollout. A rollback
may disable 2.0 production and default selection, but must retain the 2.0
reader and stored presence metadata once a released build has accepted 2.0
archives. No profile advertises 2.0 support until its matrix cell binds the
authority commit, bundle digest, fixture digest, implementation commit, and
passing CI receipt.

## Conformance

The canonical fixture at
`tests/fixtures/shards/presence-semantics-v2.0.json` covers absent, null, empty,
value, and unsupported cases for the known 1.x collapse boundaries and
arbitrary metadata. Verification asserts schema validity, exact own-property
classification, and JSON serialize/parse equality. Downstream suites must
table-drive the full generated field inventory through validation, storage,
export, import, and re-export.

Negative cases must prove zero mutation for unsupported tuples, illegal
states, inconsistent presence metadata, and lossy strict-profile downgrade.
Semantic equality includes own-property presence, JSON type, exact scalar and
container values, meaningful array order, relationship identity, timestamps,
and attachment bytes/digests.

## Downstream work

- React [#379](https://git.integrolabs.net/Fortemi/fortemi-react/issues/379): shared presence model, PGlite/RecordStore storage, and mapper remediation.
- React [#380](https://git.integrolabs.net/Fortemi/fortemi-react/issues/380): complete `2.0.0/full-v1` PGlite persistence and receipt.
- React [#381](https://git.integrolabs.net/Fortemi/fortemi-react/issues/381): native AIWG conversion and explicit losses.
- HotM [#272](https://git.integrolabs.net/Fortemi/hotm/issues/272): exact-tuple recovery validation and capability display.
- Fortemi [#1082](https://git.integrolabs.net/Fortemi/fortemi/issues/1082): per-cell evidence gate and final cross-repository receipts.

## Alternatives considered

### Make every nullable field required

Rejected because it defines absence away rather than preserving it for fields
where omission is meaningful.

### Add a wire-level presence bitmap or envelope

Rejected because it duplicates object state, requires consistency validation,
and makes field evolution and signatures more complex. Storage backends may
use a named presence map internally without changing the wire contract.

### Introduce `core-v2`, `record-v2`, and `full-v2`

Rejected because the schema major already provides the breaking boundary and
profile identifiers independently describe component/preservation coverage.
Duplicating both axes would expand matrices and make existing full-v1 work
ambiguous without adding negotiation safety.

### Reuse schema 1.x with clarified behavior

Rejected because preservation semantics affect serialization, persistence,
migration, and compatibility. That is a schema-major change under ADR-102.

## Consequences

The direct representation stays readable and preserves exact JSON semantics,
but every storage adapter must carry presence information that nullable columns
or plain `Option<T>` values cannot express. Existing 1.x archives remain valid
under their immutable contracts. Full portability claims remain blocked until
all required 2.0 matrix cells pass.

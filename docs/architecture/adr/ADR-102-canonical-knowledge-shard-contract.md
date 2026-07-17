# ADR-102: Canonical Knowledge Shard Contract and Conformance Profiles

**Status:** Accepted
**Date:** 2026-07-17
**Deciders:** Architecture team
**Implementation status:** Target contract; runtime conformance is pending the release gates in this ADR
**Supersedes in part:** ADR-028, ADR-029

## Context

Fortemi is both a Knowledge Shard producer and consumer. The format is also
produced or consumed by sibling repositories, including `fortemi-react`,
`aiwg`, and HotM. Independent implementations have drifted in component
coverage, manifest fields, version interpretation, validation behavior, and
loss handling. In particular, a shard can contain data that the server's
default importer cannot restore.

A portable format cannot be described as lossless merely because its archive
can be parsed. Interoperability requires a single schema authority, explicit
capability profiles, deterministic validation, and preservation of the
semantics needed to reconstruct the source graph.

This ADR records the required target contract. It does not claim that the
current server or any sibling producer already conforms.

## Decision

### 1. Contract ownership

The Fortemi server repository is the source of truth for the canonical
Knowledge Shard archive contract. It owns:

- the normative JSON Schema and component schemas;
- manifest and archive layout semantics;
- the schema-version and profile registries;
- the conformance fixture corpus;
- migration rules and compatibility policy; and
- the producer/consumer conformance matrix.

Sibling repositories may vendor generated schemas and fixtures for offline
builds, but must record the upstream revision and verify that vendored copies
match it. A sibling repository must not redefine a field, component, profile,
or compatibility rule independently.

### 2. Schema version and producer identity

`manifest.version` is the Knowledge Shard **schema version** and uses Semantic
Versioning. `manifest.min_reader_version` is also a shard-schema SemVer value:
it is the minimum reader contract version required to interpret the shard
without undeclared loss. Neither field contains a Fortemi application release.

Producer identity is separate, informational metadata:

- `producer.name`
- `producer.version`
- `producer.revision` when available

Compatibility decisions use the schema version, profile, declared extensions,
and registered migrations. They must not infer compatibility from an
application CalVer or package version.

SemVer changes are classified as follows:

| Change | Required action |
|--------|-----------------|
| PATCH | Clarification or compatible validation fix that does not change accepted data |
| MINOR | Backward-compatible optional field, component, or registered extension |
| MAJOR | Required-field, type, meaning, archive-layout, or preservation change |

Adding a component to a required profile is a breaking profile change even if
the component schema itself is optional elsewhere.

### 3. Named conformance profiles

Every shard declares exactly one registered `manifest.profile`.

| Profile | Contract |
|---------|----------|
| `core-v1` | Shared structured records: notes and metadata, collections and hierarchy, tags, templates, links, timestamps, identities, tombstones, and attachment projections. Attachment bytes and rich analytical extensions are not guaranteed. Components outside the profile are neither implied nor silently discarded. |
| `full-v1` | Lossless Fortemi interchange. Extends `core-v1` with embeddings, SKOS, provenance, graph/community records, attachment byte sidecars, and the signature envelope. |
| `record-v1` | Explicit RecordStore transport subset. The producer must emit a machine-readable loss report for source concepts that the subset cannot represent. It must never be advertised as full parity. |

Profile identifiers are independent from `manifest.version`. Changing a
profile's required preservation behavior requires a new profile identifier or
a schema-major migration that leaves old profile semantics unambiguous.

The server's default export uses the richest profile that the same released
server can self-import. Until every `full-v1` gate passes, the default may be
`core-v1` with its limits reported explicitly. Once `full-v1` is supported, it
becomes the default backup profile. Every default export must be
self-importable by the same released server build and by every later compatible
server release.

### 4. Fail-closed validation before writes

Import is a two-phase operation:

1. Read, bound, and validate the complete archive without mutating persistent
   state.
2. Apply the validated import in one transaction or equivalent atomic unit.

Pre-write validation includes:

- archive path and resource-limit checks;
- manifest schema and registered profile checks;
- declared component inventory;
- file existence, media type, and checksum verification;
- exact manifest counts;
- component schema validation;
- referential integrity;
- schema-version compatibility and migration availability; and
- duplicate/conflict policy validation.

A corrupted, undeclared, unsupported, or incoherent required component fails
the import. Checksum mismatch is an error, not a warning. Malformed records are
not skipped during a conformant import. Unknown components are accepted only
through a registered extension whose compatibility behavior is defined.

Dry-run and real import execute the same validation and planning path. The
write phase either commits the entire plan or leaves the destination unchanged.

### 5. Preservation invariants

A conformant round trip preserves, subject only to a declared migration:

- stable entity identifiers;
- collection hierarchy and membership;
- template, tag, link, embedding-set, and other relationship identities;
- relationship endpoints and ordering where order is meaningful;
- the distinction among absent values, JSON `null`, empty values, and explicit
  tombstones;
- deletion/tombstone state and conflict metadata;
- source timestamps and their precision/time-zone meaning;
- attachment metadata, references, filenames, media types, sizes, and byte
  content;
- checksums for attachment bytes and all declared archive components; and
- extension data required by the declared profile.

Importers must not silently regenerate identifiers, attach children to a
different parent, turn `null` into an empty value, revive tombstones, replace
source timestamps with import time, or drop referenced bytes.

Migrations that cannot preserve an invariant must fail unless the selected
profile explicitly permits the loss and the archive carries the required
machine-readable loss report. `full-v1` never permits such loss.

### 6. Release and integration gates

A schema or profile release is blocked until all of the following pass:

1. The canonical schemas validate both positive and negative fixtures.
2. The server default profile imports into a clean instance with semantic
   equality; a `full-v1` claim additionally includes attachment byte checksums.
3. Export after import is canonically equivalent apart from documented
   non-semantic archive metadata.
4. The previous supported schema versions migrate through tested paths.
5. Each registered producer validates its emitted archive against the
   canonical schema and golden corpus.
6. Each registered consumer passes the same corpus for every profile it
   advertises.
7. Cross-repository integration tests pin the canonical contract revision and
   publish a producer/consumer compatibility result.
8. Negative tests prove that checksum, count, relationship, profile, and
   version failures perform zero writes.

No repository may claim support for a profile until its conformance result is
green. Contract changes require a Fortemi Gitea issue linked from every affected
consumer issue and pull request.

## Current-state gap

At acceptance time, existing shard code and documentation do not satisfy every
requirement above. Known gaps include best-effort record skipping, non-fatal
checksum handling, incomplete component import, and ambiguous reader-version
metadata. Those behaviors describe legacy implementation state, not the target
contract.

Until the release gates pass:

- user documentation must label lossless/full-profile behavior as a target;
- consumers must advertise only behavior demonstrated by tests; and
- default exports must not be represented as disaster-recovery complete.

## Consequences

### Positive

- One contract authority replaces incompatible local interpretations.
- Profiles make reduced transports explicit and testable.
- Atomic fail-closed import prevents partial or corrupt restores.
- Round-trip requirements cover semantic identity and binary content, not only
  JSON parsing.
- Release gates turn interoperability claims into reproducible evidence.

### Negative

- Existing producers and consumers require coordinated remediation.
- Strict imports will reject archives previously accepted with warnings.
- Golden corpus and cross-repository test maintenance become release work.
- `full-v1` archives may be larger because referenced attachment bytes must be
  portable.

## Alternatives considered

### Continue best-effort import

Rejected because silent skipping and warning-only checksum handling cannot
support backup, recovery, or lossless exchange claims.

### Let each repository own its local schema

Rejected because independently compatible-looking schemas have already drifted
in field names, component coverage, and version semantics.

### Use application versions for compatibility

Rejected because application releases and data-contract evolution are
independent. Package versions cannot substitute for a schema compatibility
contract.

### Define only one universal profile

Rejected because RecordStore and reduced clients have legitimate subset use
cases. Named profiles preserve those uses without misrepresenting them as full
fidelity.

## References

- [ADR-028: Shard and Archive Migration System](ADR-028-shard-archive-migration-system.md)
- [ADR-029: Shard Schema Versioning Specification](ADR-029-shard-schema-versioning.md)
- [Shard Migration Guide](../../content/shard-migration.md)
- [Semantic Versioning 2.0.0](https://semver.org/)

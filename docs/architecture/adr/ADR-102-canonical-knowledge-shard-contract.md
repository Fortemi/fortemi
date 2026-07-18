# ADR-102: Canonical Knowledge Shard Contract and Conformance Profiles

**Status:** Accepted
**Date:** 2026-07-17
**Deciders:** Architecture team
**Implementation status:** Versioned `core-v1` schemas through `1.1.0`, an authority-owned and cross-repository-proven `record-v1` profile, digest-pinned candidate `full-v1` embedding, note-revision, and revision-linked provenance component boundaries and transactional apply paths, a registered `1.0.0 -> 1.1.0` tombstone transition, bounded archive and relationship preflight, identity-preserving structured import, and disk-backed streaming preflight for opt-in verified attachment sidecars; `full-v1` conformance remains pending the release gates in this ADR
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

This ADR records the required target contract. The server currently emits
`core-v1` with the self-importable structured component set and enforces
profile, schema version, minimum reader, inventory, checksum, and count
preflight. It also validates collection/note/template/link relationships,
exports complete collection hierarchies, and preserves collection and template
identities and timestamps during import. Its REST route can optionally export
and restore verified attachment bytes as digest-addressed sidecars, while
missing entries remain reference-only. Import reads sidecar payloads in bounded
chunks into isolated preflight files instead of retaining them in the
in-memory component map. Any supplied sidecar that lacks a matching attachment
projection is rejected as an orphan. Missing sidecars remain explicit
reference-only attachments for `core-v1` and `record-v1`; the reserved
`full-v1` policy requires bytes for every declared attachment digest. Export
streams filesystem sidecars through integrity verification into a
request-scoped disk-backed archive, validates the completed compressed artifact
before returning success, and streams that file to the client in bounded
chunks. The preferred multipart import spools bounded compressed chunks into a
request-scoped temporary file, and on-disk swap reads its existing file
directly into the same reader-based preflight. The legacy JSON request retains
its bounded encoded string, but base64 decoding streams through a fixed buffer
into a request-scoped temporary file before the same reader-based preflight.
Structured component files remain bounded-buffered, but transactional apply
deserializes JSONL notes and links one record at a time instead of retaining a
second component-sized typed vector. Schema/count and relationship preflight
also visit JSONL notes and links one record at a time, retaining only identity
and attachment-declaration sets needed for cross-record validation.
Schema/count preflight also validates and discards JSON-array component records
one at a time. Typed JSON-array relationship/apply data, historical migration,
and preflight raw buffers remain bounded-buffered, and export is not
single-pass live emission, so the route does not constitute fully streaming or
`full-v1` profile conformance.

Current-version bytes complete checksum, schema/count, relationship, and
sidecar validation once before the explicit no-op migration result. Historical
migration output is a distinct representation and therefore repeats the full
checksum, schema/count, relationship, and sidecar validation before staging or
database mutation.

The current normative schema root for `1.1.0` / `core-v1` is
`contracts/knowledge-shard/1.1.0/core-v1/`. The immutable `1.0.0` authority
remains at `contracts/knowledge-shard/1.0.0/core-v1/`. The machine-readable
receipt at `contracts/knowledge-shard/contract.json` records exact current and
historical digests, golden corpora, supported and reserved profiles, and
current limitations.

The supported `record-v1` root is
`contracts/knowledge-shard/1.1.0/record-v1/`. It is limited to notes,
collections, tags, note-to-note links, and attachment projections. Producers
must report every omitted or lossy source concept through their
machine-readable capability/loss result. The exact React producer artifact at
commit `df4762ad0c470ebd8ee460b56ba71be09b4f1616` passed the same bounded
validation and atomic apply path used for `core-v1`: dry-run and reserved
profile rejection wrote zero rows, two replace imports converged, Fortemi
re-exported the resulting state, and React validated and imported that return
archive while preserving IDs, bodies, the empty revision, relationships,
attachment reference, and tombstone instant. The durable receipt lives beside
the integration fixture.

Contract revision 9 publishes candidate embedding, note-revision, and
revision-linked provenance component boundaries under
`contracts/knowledge-shard/1.1.0/full-v1/`, with separately digest-pinned
corpora. The revision boundary covers current original state, original history,
the current revised snapshot, and the complete revision chain. The provenance
boundary preserves W3C-PROV edges and processing activities that reference
those exact note and revision identities. These component schemas are compiled
by the server and exercised by bounded schema and relationship preflight
tests. Dormant embedding, revision, and revision-linked provenance apply paths
run inside the existing schema-scoped import transaction. Their database tests
prove exact source-field restoration, repeated replace convergence, skip and
dry-run accounting, and rollback after a late injected failure. The spatial
and unified provenance families, canonical `full-v1` manifest, complete
component inventory, end-to-end revision round-trip receipt, and profile
support remain pending; manifest validation continues to fail closed for
`full-v1`.

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

The current authority bundle includes the manifest and record schemas for
notes, collections, tags, templates, and links. Fortemi import compiles and
applies these same schemas before component inventory/count validation and
before its normal write phase. Schema failures return a stable class without
echoing record content or validator diagnostics.

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

The runtime now separates producer identity from strict shard-schema SemVer,
declares a registered profile, rejects unsupported profiles and components,
and fails canonical manifest/record schema, checksum, count, inventory,
collection topology, and note/template/link reference validation before normal
import writes. Archive preflight also bounds compressed and expanded bytes,
entry count and size, manifest size, and component record count and size while
rejecting unsafe paths, duplicate names, and non-regular tar entries. Default
export includes roots and descendants. Import creates collections before
dependent notes and templates, preserves collection and template IDs and source
timestamps, restores source note timestamps after revised content, and uses
stable-ID conflict handling for repeated imports. Reference-only attachment
projections retain attachment IDs, display filenames, extraction state and
text, canonical digest metadata, and shared-blob deduplication; attachment IDs
and conflicting declarations are rejected during relationship preflight.
Shard imports retain the canonical extraction status and reason in validated
attachment metadata so subsequent exports preserve the exact projection state
after referenced bytes are promoted from reference-only to filesystem storage;
ordinary attachments without shard metadata continue to use deterministic
status, text, media-type, and size derivation.
Ordinary import applies all selected database components in one schema-scoped
transaction; a late database failure rolls back collections, notes, tags,
templates, links, attachments, and reference blobs together, and post-import
NLP jobs are queued only after commit. The current positive and negative corpus
is pinned by the schema receipt. Destructive shard swap validates before
mutation, then deletes the existing core-v1 entity families and applies the
validated shard within the same transaction. A late apply failure therefore
restores the pre-swap state.

Schema `1.1.0` adds an optional `deleted_at` note field. Current exports include
soft-deleted notes and always emit either explicit JSON `null` or the exact
deletion timestamp. Import restores that value in the same transaction as the
note. The registered `1.0.0 -> 1.1.0` migration validates source bytes and
records first, maps the legacy field absence to the documented `null`
active-state default, rebinds the component checksum and migration metadata,
then validates the migrated current representation before writes.

Known gaps remain in complete absent-versus-null semantic preservation across
all accepted current records, `full-v1`, current-minus-two historical migration
coverage, and end-to-end streaming archive processing across the legacy JSON
request buffer, structured components, and single-pass live export emission.
`record-v1` does not imply
preservation of templates, embeddings,
SKOS, provenance, graph/community data, URL-only links, signature guarantees,
or attachment bytes. Making tombstone-field presence mandatory requires a
schema-major or new profile identifier. Those gaps remain tracked release
blockers, not implicit `core-v1` or `full-v1` claims.

The filesystem backend provides a bounded-memory staging primitive that streams
bytes into an isolated `staging/shard-import/` namespace, verifies the declared
byte length and canonical BLAKE3 digest, rechecks integrity before atomic
promotion into `blobs/`, and supports receipt-bound compensation plus startup
cleanup of stale stages. Archive preflight now streams canonical sidecar tar
entries through a 64 KiB copy-and-hash buffer into request-scoped temporary
files. After the complete manifest, inventory, component, relationship, length,
and digest preflight succeeds, the HTTP route streams referenced files through
the storage staging primitive. Orphan sidecars fail before storage staging or
database mutation, while profile policy distinguishes optional `core-v1` and
`record-v1` bytes from mandatory reserved `full-v1` bytes. It exports available
verified bytes when `include_blobs=true`: filesystem content is hashed directly
into a disk-backed archive, legacy database content is bounded to one blob at a
time, and the completed archive is size-checked before a bounded response
stream owns its temporary-file cleanup. This satisfies only the opt-in
`core-v1` attachment-byte transport prerequisites, not single-pass end-to-end
streaming or the `full-v1` attachment gate.

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

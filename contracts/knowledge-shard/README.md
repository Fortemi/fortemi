# Knowledge Shard Contract

This directory is the Fortemi-owned schema authority for Knowledge Shard
consumers. `contract.json` identifies the current schema/profile revision,
stable schema paths, exact file digests, golden corpus, and demonstrated
limitations.

The current contract revision supports Knowledge Shard schema `1.2.0` under
`core-v1`, `record-v1`, and `full-v1`:

- `core-v1`: notes, collections, tags, templates, links, and attachment
  projections.
- `record-v1`: notes, collections, tags, note-to-note links, and
  attachment projections.
- `full-v1`: the complete 33-component inventory and mandatory attachment
  bytes.

Revision 19 supports the complete `full-v1` server route and publishes a
reproducible signed integrated fixture
with the complete 33-component inventory, 34 count fields, 33 component
checksums, and one mandatory content-addressed attachment sidecar shared by
two references. The archive unifies the digest-pinned embedding, note-revision,
provenance, SKOS, graph, and community boundaries onto coherent note
identities. Embedding records carry a nullable, validated contract fingerprint
that preserves the provider/model/dimension/normalization/set identity used to
produce a vector without inventing lineage for legacy records. Its strict
Ed25519 envelope authenticates the exact manifest bytes
and sorted content-addressed blob inventory through the same verifier used by
server import. The deterministic fixture key is public and test-only; operators
must never add it to a production trust store.
`GET /api/v1/backup/knowledge-shard?profile=full-v1` always includes every
component and every referenced attachment byte. Database-backed tests import
the signed fixture, export it through that route, import it twice into a clean
schema, and prove exact component, checksum, attachment, and re-export
convergence. Cross-repository producer and consumer receipts remain pending.
The revision boundary covers current original state, original history, current
revised snapshots, and revision chains. The provenance boundary adds the
W3C-PROV edges and processing activities that reference those exact note and
revision identities. The spatial registry boundary adds named places, exact
PostGIS location observations, and device agents. The unified boundary adds
temporal ranges, spatial/device/activity references, extraction context, and
user-correction state for note and attachment targets. The SKOS boundary adds
schemes, concepts, labels, notes, semantic and mapping relations, memberships,
note tags, and ordered collections. The graph boundary adds source lineage,
weighted note edges, nested communities, and assignments. These boundaries
have bounded schema and relationship validation plus transactional apply paths
with convergence, dry-run, conflict-accounting, and late-failure rollback
tests. The integrated fixture passes bounded archive, complete inventory,
checksum, relationship, revision-chain, mandatory-byte, deduplication, and
archive read/write/read equality tests. The files and dormant paths are
reviewable authority inputs for the supported server profile. Runtime profile
validation requires the exact full inventory and rejects partial `full-v1`
exports before archive or database mutation.

Each profile has its own manifest and record schemas under
`contracts/knowledge-shard/1.2.0/<profile>/`. Fortemi import selects and
applies those schemas by version and profile before component inventory/count
validation and before normal persistent writes. Positive and negative fixtures
live under `tests/fixtures/shards`.

Schemas `1.0.0` and `1.1.0` remain immutable under their original stable paths
and receipt hashes. The registered `1.0.0 -> 1.1.0` migration adds
`deleted_at: null` to legacy note records, recording the legacy absence as the
documented active state. The registered `1.1.0 -> 1.2.0` migration adds
`contract_fingerprint: null` to legacy embedding records. Current exports
always emit `deleted_at` as either `null` or an exact timestamp, include
soft-deleted notes, and emit an exact 64-character lowercase hexadecimal
embedding contract fingerprint or `null`.

## Schema 2.0 presence authority

Schema `2.0.0` is published in `specified-implementation-pending` state under
`contracts/knowledge-shard/2.0.0/`. It retains the `core-v1`, `record-v1`, and
`full-v1` profile identifiers; consumers negotiate the complete
`(manifest.version, manifest.profile)` tuple. It is not the current server
default and no 2.0 profile is advertised yet.

[ADR-103](../../docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md)
defines direct JSON key-presence semantics for absent, null, empty, value, and
unsupported states. `2.0.0/field-semantics.json` inventories all nullable or
optional fields across the server, PGlite, RecordStore, and AIWG mappings.
`2.0.0/contract.json` pins the schema bundle and canonical presence corpus.
Run `python3 scripts/ci/verify-knowledge-shard-presence.py` to verify schemas,
digests, own-property state, and JSON round-trip equality.

Consumers must pin an immutable Fortemi commit, verify every digest in
`contract.json`, and treat the schema files as upstream authority. Vendored
copies are receipts, not independent definitions.

`core-v1` includes attachment metadata/reference projections by default.
Attachment content identities use the server's canonical
`blake3:<64 lowercase hex>` form. The bounded REST route can optionally carry
verified digest-addressed sidecars, but that does not establish `full-v1`
conformance. `record-v1` is a deliberately reduced RecordStore transport:
producers must return a machine-readable report covering templates, embeddings,
SKOS, provenance, graph/community data, URL-only links, signature guarantees,
attachment-byte omissions, and every other source concept that is not preserved
by the selected export. The pinned React producer/server consumer/React return
receipt is stored at
`tests/fixtures/shards/record-v1-fortemi-react-df4762a.shard.receipt.json`;
the exact producer archive is a permanent integration fixture. `full-v1`
supports those same transactional boundaries through its complete route;
cross-repository conformance remains tracked separately.
The 1.x runtime still has the documented absent-versus-null limitation. The
2.0 authority resolves the contract decision, but runtime and cross-repository
support remain blocked on Fortemi #1083, React #379-#381, HotM #272, and the
per-cell evidence gate in Fortemi #1082.

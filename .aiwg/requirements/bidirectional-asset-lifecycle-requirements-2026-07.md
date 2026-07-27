---
title: Bidirectional Asset Lifecycle Requirements
status: accepted-audit-baseline
date: 2026-07-27
scope: exact-2.0.0-full-v1-and-live-client-boundaries
derived_from:
  - "@docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md"
  - "@docs/architecture/adr/ADR-103-lossless-knowledge-shard-presence-semantics.md"
  - "@.aiwg/testing/knowledge-shard-profile-test-matrix.json"
---

# Bidirectional Asset Lifecycle Requirements

## Scope

An asset is attachment bytes plus the metadata and relationships needed to identify, authorize,
verify, restore, and retrieve those bytes. Byte portability is claimed only for the exact
`2.0.0/full-v1` tuple. `core-v1` and `record-v1` may carry projections or references; they are not
attachment-byte recovery profiles.

The required directions are:

1. **Filesystem-origin:** local file -> HotM browser or desktop upload -> Fortemi
   content-addressed filesystem persistence -> signed `full-v1` export -> clean Fortemi import ->
   downloaded local file with identical bytes.
2. **Server-origin:** existing Fortemi attachment -> clean local filesystem download -> clean
   server re-upload or signed `full-v1` recovery -> identical bytes, digest, length, metadata, and
   owning relationships.

A filesystem-hosted `.shard` is a transport package. It does not merge the AIWG static index,
Knowledge Shard bridge, and Fortemi live persistence planes.

## Functional Behaviors

| ID | Expected behavior | Acceptance oracle | Priority |
|---|---|---|---|
| AL-B01 | The client accepts a regular local file and preserves filename, media type, and exact length. | Upload metadata and final attachment response match the source. | P0 |
| AL-B02 | Desktop upload reads incrementally and uses TUS create/patch with exact acknowledged offsets. | Completion occurs only at the declared length; disconnect/resume does not corrupt bytes. | P0 |
| AL-B03 | Browser and desktop upload converge on the same normalized server contract. | Equivalent inputs produce equivalent attachment metadata and bytes. | P0 |
| AL-B04 | Fortemi persists exact bytes under a content-addressed identity and links the owning note/archive. | Stored length and BLAKE3 digest match independently computed source values. | P0 |
| AL-B05 | Identical bytes deduplicate while attachment references and blob refcounts remain correct. | One blob serves all references; deleting one reference preserves shared bytes. | P0 |
| AL-B06 | Removing the final reference removes only the orphaned blob metadata and bytes. | Cleanup occurs after the final committed reference, without unrelated data loss. | P0 |
| AL-B07 | Download remains denied until the configured scan policy permits release. | Pending, blocked, or failed scan states cannot disclose bytes. | P0 |
| AL-B08 | Authorized download returns exact bytes and safe metadata. | Digest and length equal the source; no storage path is disclosed. | P0 |
| AL-B09 | `2.0.0/full-v1` export declares all emitted components and required attachment sidecars. | Manifest components, counts, checksums, relationships, and sidecars validate. | P0 |
| AL-B10 | Sidecars use canonical content-addressed paths and exact server bytes. | Digest, declared length, and byte comparison pass independently. | P0 |
| AL-B11 | Signed export binds the manifest and attachment digest set to a trusted signer. | Required-signature import rejects unknown, revoked, malformed, or substituted signatures. | P0 |
| AL-B12 | Clean import restores all declared records, relationships, presence states, and bytes atomically. | A clean server can retrieve the same asset and semantic state. | P0 |
| AL-B13 | Re-export after clean import converges on equivalent records and identical sidecars. | Counts/checksums, normalized records, and every sidecar byte match. | P0 |
| AL-B14 | Repeated import is idempotent under the selected conflict policy. | No duplicate records, references, or refcount inflation. | P0 |
| AL-B15 | Missing, extra, duplicate, malformed, oversized, miscounted, checksum-invalid, or tampered input fails before mutation. | DB rows, promoted blobs, and pre-existing state remain unchanged. | P0 |
| AL-B16 | Late database failure rolls back writes and compensates staged or promoted sidecars. | No partial attachment, relationship, blob, or filesystem state remains. | P0 |
| AL-B17 | Unsupported profile, component, schema, or reader requirement fails closed. | Stable incompatibility result is returned before mutable queries or writes. | P0 |
| AL-B18 | Hierarchy, ownership, identity, timestamps, tombstones, presence states, and links survive the declared round trip. | Normalized source and destination snapshots are semantically equal. | P0 |
| AL-B19 | Server-origin download and clean re-upload/recovery preserve identity semantics. | Independent local and destination digests/lengths match the original server asset. | P0 |
| AL-B20 | HotM pass-through does not rewrite valid `full-v1` archives or buffer complete payloads unnecessarily. | Receipt-bound archive/component bytes remain unchanged. | P0 |
| AL-B21 | AIWG uses the released Core bridge and remains distinct from live persistence. | Current v2 source converts deterministically and reconstructs through advertised consumers. | P0 |
| AL-B22 | PGlite and Fortemi advertise only exact receipt-backed profiles and machine-readable losses. | Capability output matches executable profile matrices. | P0 |
| AL-B23 | Failures and diagnostics use stable reason classes without exposing paths, tokens, manifests, or payload bytes. | Response, log, debug, telemetry, and audit redaction tests pass. | P0 |
| AL-B24 | Each accepted cell binds authority, producer, consumer, fixture digest, command, and CI result. | Immutable receipt verification rejects commit or content drift. | P0 |

## Nonfunctional Requirements

| ID | Expected quality | Measurable acceptance criterion | Priority |
|---|---|---|---|
| AL-NFR01 | Cryptographic integrity | SHA-256 package/component and BLAKE3 blob checks pass; one-bit tampering fails before mutation. | P0 |
| AL-NFR02 | Atomicity | Deterministic late-write failure restores pre-operation DB and blob state. | P0 |
| AL-NFR03 | Idempotency | Retry and repeated import converge without duplicate state or refcount inflation. | P0 |
| AL-NFR04 | Determinism | Repeated source/options produce stable receipt-bound logical output. | P0 |
| AL-NFR05 | Resource bounds | Compressed, uncompressed, entry, record, count, decoded-body, and upload limits enforce boundary and limit-plus-one behavior. | P0 |
| AL-NFR06 | Streaming memory | Uploads/downloads/sidecars are incremental and meet an approved peak-RSS budget. | P0 |
| AL-NFR07 | Compatibility | Schema SemVer, application CalVer, profile, and minimum-reader matrices fail closed independently. | P0 |
| AL-NFR08 | Security | Path, scan, signature, content-type, authentication, and authorization gates precede disclosure/mutation. | P0 |
| AL-NFR09 | Confidentiality | Automated scans find no tokens, source/storage paths, raw bytes, or unbounded input in diagnostics. | P0 |
| AL-NFR10 | Durability | Committed metadata, refcounts, bytes, download, and re-export survive Fortemi restart. | P0 |
| AL-NFR11 | Crash recovery | Kill/restart at staging and promotion fault points cleans uncommitted state and preserves committed state. | P0 |
| AL-NFR12 | Concurrency | Same/different-byte upload, import, and delete races end in a serializable valid state. | P0 |
| AL-NFR13 | Performance | Approved p50/p95/p99 upload/export/import/download budgets pass at 1 MiB, 100 MiB, and maximum size. | P1 |
| AL-NFR14 | Scalability | Maximum size/count corpus completes within approved memory, time, and disk budgets without truncation. | P1 |
| AL-NFR15 | Resumability | TUS resumes from server-confirmed offsets and commits exactly one digest-correct attachment. | P0 |
| AL-NFR16 | Platform portability | Declared browser, desktop, OS, filesystem, and storage-mode matrix passes. | P1 |
| AL-NFR17 | Observability | Correlation IDs connect bounded audit events, failures, and receipts without protected content. | P1 |
| AL-NFR18 | Availability | Scan, extraction, and inference degraded modes follow a tested fail-open/fail-closed policy. | P1 |
| AL-NFR19 | Backup recovery | Approved RPO/RTO targets exist and a timed clean recovery meets them. | P1 |
| AL-NFR20 | Reproducibility | Clean checkout and lockfile install reproduce the focused evidence on exact commits. | P0 |
| AL-NFR21 | Traceability | Every P0 row maps to authority, code, test, receipt, and linked producer/consumer issues. | P0 |
| AL-NFR22 | Claim safety | Automated guards reject broad parity, complete-backup, universal-schema, and portability claims while #1081 remains open. | P0 |

## Claim Rule

Feature completeness may be claimed only for a named profile and producer/consumer cell whose P0
rows are green. Unqualified compatibility, complete backup, and bidirectional portability remain
`NO-GO` until the independent audit accepts the exit criteria and the live-system P0 gaps close or
are formally removed from scope.

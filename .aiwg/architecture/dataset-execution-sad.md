# Dataset execution architecture

## Scope and authority

This document covers the bounded alpha `live-remote-persistence` integration
tracked by Fortemi #1128–#1131 and consumer roctinam/aiwg#2242. ADR-107 names
upstream Core contracts; Fortemi projects those requirements into its own MCP
request and RunReceipt contracts. Core ingest receipts are distinct artifacts.
The static AIWG index and Knowledge Shard transfer planes remain separate.
The suite portability audit remains NO-GO.

The immutable historical authority is `contracts/dataset-execution/1.0.0`.
Strict validation and request binding revision 1.0.1 live under
`contracts/dataset-execution/validation/1.0.1`. The wire receipt schema remains
1.0.0. Input and output schema digests now participate in request identity;
legacy cross-revision idempotent replay is not qualified.

## Execution and verification

The AIWG consumer discovers `manage_dataset_execution`, negotiates the exact
validation/request binding revisions, independently computes the request digest,
and requires approval of that digest before execution. Fortemi previews schema,
capability, resource, and content constraints before storage calls. Source-upsert
metadata binds the full request digest to the durable storage fingerprint.

Fortemi verifies the storage response before advancing the checkpoint. Lost or
malformed responses remain ambiguous and expose no checkpoint. Exact retry
resolves the durable source journal. Archive rejects unresolved attempts.

Producer and consumer each validate receipt structure, digest bindings, effects
and counts, checkpoint scope, and resource bounds. AIWG implements its own
canonicalizer and semantic validator; it does not import producer code. Shared
canonical vectors exercise UTF-16 key ordering, numeric encoding, and escaping.
Negative fixtures recompute checksums so structural and semantic rejection cannot
be explained solely by a stale checksum.

## Qualification and limits

The validation bundle retains an actual PostgreSQL receipt accepted by both
implementations. A fresh installed AIWG package and clean MCP package exercised
execute, replay, checkpoint, resume, replay after an MCP restart, and repeated
archive. An unrelated sentinel row remained unchanged. The qualification report
records exact producer file digests and the installed consumer tarball digest.

This is local single-user qualification with inference unavailable. It does not
qualify hosted tenant isolation, Enterprise operation, full Core materialization,
backup/restore, Knowledge Shard profiles, or suite parity. Producer and consumer
CI and delivery remain necessary before issue closure.

## Deferred production qualification control plane

Fortemi #1136 owns the candidate qualification authority and admission tooling.
The decision record is
[ADR-dataset-qualification-admission](../../docs/architecture/adr/ADR-dataset-qualification-admission.md).
Its version 2.0.0 schemas describe signed qualification evidence; they do not
change the runtime request/RunReceipt contract above. No approved production
authority instance or independently qualified child matrix is currently retained.

Operator-managed trust pins authenticate a detached authority envelope. A
separate verifier signs receipts for exact declared producer/consumer cells.
The offline admission command checks signatures, bounded content-addressed
artifacts, summary bindings and replay history before atomically recording a
cell report in a protected local store. It cannot establish scenario coverage
or independently observe a database merely by accepting a signed receipt.

Child owners #1137–#1141 must supply actual isolation, fault, restore, version
skew and load evidence under approved synthetic environments and numeric
thresholds. The readiness graph separates those dependencies from epic closure.
A passing report is scoped to declared authority cells; suite NO-GO and the
existing bounded-alpha limits remain in force. See the
[acceptance audit](../reports/issue-1136-qualification-acceptance-audit-2026-09-05.md).

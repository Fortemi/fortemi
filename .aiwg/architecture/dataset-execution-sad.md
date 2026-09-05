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

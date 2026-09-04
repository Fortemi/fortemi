# ADR-107: Versioned MCP Dataset Execution and RunReceipt

**Status:** Accepted
**Date:** 2026-09-04
**Decision owners:** Fortemi MCP, live persistence, and contract maintainers
**Producer tracking:** [Fortemi/fortemi#1128](https://git.integrolabs.net/Fortemi/fortemi/issues/1128), [#1129](https://git.integrolabs.net/Fortemi/fortemi/issues/1129), [#1130](https://git.integrolabs.net/Fortemi/fortemi/issues/1130), [#1131](https://git.integrolabs.net/Fortemi/fortemi/issues/1131)
**Consumer tracking:** [roctinam/aiwg#2242](https://git.integrolabs.net/roctinam/aiwg/issues/2242)
**Core authority:** [Fortemi/fortemi-react#408](https://git.integrolabs.net/Fortemi/fortemi-react/issues/408), [#409](https://git.integrolabs.net/Fortemi/fortemi-react/issues/409), [#410](https://git.integrolabs.net/Fortemi/fortemi-react/issues/410), [#411](https://git.integrolabs.net/Fortemi/fortemi-react/issues/411)
**Upstream suite authority:** `fortemi-suite/.aiwg/architecture/ADR-suite-contract-authority-and-profiles.md`
**Extends:** ADR-102, ADR-106

## Context

The Fortemi server exposes storage primitives through MCP, including the
source-addressed atomic note upsert from ADR-106. Storage-tool presence is not
evidence that a caller can negotiate and execute a canonical Dataset
Intelligence plan. AIWG #2242 therefore correctly reports the live dataset
cell as pending against 2026.9.1.

Fortemi Core already owns the language-neutral capability, ingest/checkpoint,
lineage, and materialization semantics. Re-creating those semantics in this
repository would create two authorities. Treating the Knowledge Shard schema
or the AIWG static index as the common model would incorrectly merge three
separate data planes.

## Decision

Fortemi publishes `manage_dataset_execution` as one consolidated default-mode
MCP tool. MCP initialization advertises the versioned capability descriptor in
`capabilities.experimental.fortemiDatasetExecution`; tool discovery exposes
the same contract through the `capabilities` action.

The adapter pins the Core authorities and adds two server-owned envelopes:

- `fortemi.dataset-resource-envelope/v1` for record, byte, duration,
  concurrency, traversal, result, and outbound-network limits; and
- `fortemi.dataset-run-receipt/v1` for the redacted binding of the Core plan,
  schemas, negotiation, resource envelope, profile revisions, checkpoint,
  effects, counts, terminal state, verification state, and diagnostics.

The exact authority commits and schema digests are recorded in
`contracts/dataset-execution/1.0.0/authority.json`.

### Negotiation and preview

`capabilities` and `preview` are pure and make no REST request. Preview checks
all contract/schema majors, required and optional capabilities, the caller's
resource envelope, UUID dataset namespace, checkpoint scope/sequence, record
bounds, and record content digests. Required mismatches fail closed with stable
codes. Optional fallback is returned as a degradation with changed guarantees;
fallback is never silent.

The descriptor is for the concrete `fortemi-server-mcp` implementation and is
alpha. It reports only demonstrated Community behavior. The current execution
profile uses synchronous PostgreSQL lexical indexing, source-addressed record
identity/lineage, note-ID retrieval, bounded atomic upsert, and journaled
checkpoint/replay. It does not claim graph, community, rerank, or stable live
server maturity.

### Execution, replay, and checkpoint

`execute` requires caller-supplied run and dataset UUIDs. The dataset UUID is
mapped to the dedicated source namespace `dataset:<uuid>`. Tenant and memory
remain derived from authenticated server state; request data cannot choose a
different live isolation boundary.

After preview succeeds, one bounded batch is projected to ADR-106 and committed
inside the existing source-upsert transaction. That transaction atomically
writes note state, generated lexical index state, source identity/record
lineage, the caller checkpoint, and the source receipt. The MCP RunReceipt is a
deterministic redacted projection of that durable receipt and the already
approved request bindings. There is no second mutating step hidden behind a
successful receipt.

An exact run replay returns the same MCP receipt while the process is alive. An
exact replay after restart is resolved by the durable source journal and
reconstructed from its duplicate response. Reusing a run or batch identity
with different canonical content fails with `IDEMPOTENCY_CONFLICT` or the
ADR-106 stable conflict diagnostic.

`status` and `checkpoint` report the current MCP-process attempt. `retry`
reissues only the stored exact request. `cancel` aborts transport; because an
abort cannot prove whether PostgreSQL committed before the response was lost,
the attempt becomes `ambiguous`/`unverifiable`, never successful. Exact retry
then resolves the durable outcome without duplicate logical records.

### Receipt integrity and privacy

Canonical serialization recursively sorts object keys by UTF-16 code units,
preserves array order,
rejects non-JSON and non-finite values, and hashes UTF-8 bytes with SHA-256.
Receipts include digests rather than source content or raw logical IDs. The
`verify` action needs no server state and detects mutation of every bound field.

Dynamic time and host measurements are deliberately excluded from the
canonical receipt digest. The negotiated resource envelope is included. This
makes exact replay and cross-runtime test vectors stable while avoiding private
deployment data.

### Cleanup

`archive` soft-deletes only note UUIDs returned for the named run. It never
enumerates another namespace and does not permanently purge. A repeated
successful archive returns the same result. Partial cleanup returns hashed
unresolved identifiers; it is not reported as complete. Cleanup shares the
run's negotiated duration and the controller's single-operation concurrency
bound; expiry reports `ARCHIVE_TIMEOUT`. `resume` is an explicit alias for
exact-content `retry`.

## Compatibility and evidence

The repository fixture bundle covers positive preview/execution, visible
degradation, contract/schema drift, unsupported capability, resource overflow,
content tamper, replay/conflict, checkpoint, cancellation/ambiguous retry,
rejection, receipt tamper, and archive. CI verifies every manifest digest and
reproduces the canonical RunReceipt using only Node standard JSON/crypto as the
independent runtime.

AIWG #2242 remains the owner of bounded live cross-repository qualification.
This ADR and its repository-local fixtures do not by themselves advance that
live cell or establish product-wide parity.

## Consequences

- Dataset-aware clients can fail before mutation and can independently verify
  the evidence returned after an authorized bounded run.
- Existing storage MCP behavior, including `upsert_external_notes`, is unchanged.
- The default MCP surface grows from 44 to 45 tools; its serialized schema
  reduction remains explicitly gated at 58.3 percent or better.
- Process-local attempt status is not a replacement for the durable source
  journal. Recovery after MCP restart uses exact execute replay.
- No Enterprise tenant/RLS certification, recovery injection, complete backup,
  production migration, load certification, or Knowledge Shard parity is
  implied.

## Derivation

@implements `contracts/dataset-execution/1.0.0/request.schema.json`
@implements `contracts/dataset-execution/1.0.0/run-receipt.schema.json`
@implements `mcp-server/lib/dataset-execution.js`
@implements `mcp-server/index.js`
@tests `mcp-server/tests/dataset-execution.test.js`
@tests `scripts/ci/verify-dataset-execution-contract.mjs`
@depends `docs/architecture/adr/ADR-106-source-addressed-note-upsert.md`
@depends `docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md`

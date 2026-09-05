# Detached dataset qualification admission

Status: implemented candidate; operational approval and consumer adoption pending.
Owner: [Fortemi #1136](https://git.integrolabs.net/Fortemi/fortemi/issues/1136).

## Context

The bounded dataset runtime in [ADR-107](ADR-107-versioned-mcp-dataset-execution.md)
does not establish tenant isolation, fault recovery, named restore profiles,
version skew or bounded production load. The suite integration authority and
July transportability audit remain controlling; suite qualification is NO-GO.
Static AIWG indexing, Knowledge Shard transfer and live persistence remain
separate planes. Shard claims still require their named executable profile.

## Decision

Retain the 1.0.0 planning snapshots unchanged. Publish the incompatible
[2.0.0 candidate](../../../contracts/dataset-qualification/2.0.0/README.md)
with canonical JSON, SHA-256 bindings and detached DSSE envelopes. An authority
is finalized before approval; its approval envelope is finalized before a
receipt; a detached verifier envelope signs the complete receipt. Signatures
therefore do not participate in their own digest inventory. Runtime dataset
request and RunReceipt schemas are not changed by this candidate.

Operator-managed public-key pins are external trust anchors. Approvers and
independent verifiers must use disjoint accepted keys. The operator must select
reviewed verifier code, schemas, clock and protected store. In-bundle keys or
hashes cannot establish that trust. Tests use ephemeral keys and synthetic
artifacts; they do not establish operational custody or independent execution.

The offline admission tool verifies signatures, exact schema and revision
bindings, bounded immutable artifact bytes, summary consistency and attempt
replay history. A separate protected Linux store uses an exclusive lock and
atomic ledger replacement. See the normative candidate
[evidence policy](../../../contracts/dataset-qualification/2.0.0/evidence-policy.md)
and [admission behavior](../../../contracts/dataset-qualification/2.0.0/admission.md).
This is a local concurrency and replacement guarantee, not a power-loss
durability qualification or a distributed ledger.

Every declared cell receives PASS, FAIL, MISSING or UNSUPPORTED. Unsupported
cells retain a separate rejection-check verdict. A committed passing report
only covers declared cells. The admission tool does not prove that an authority
contains every required child scenario, nor independently interpret all raw
runtime observations. Child owners must demonstrate that coverage and provide
independently verified clean-destination evidence before closure.

## Adoption and consequences

The executable candidate consumer is Fortemi's qualification tooling.
[AIWG #2242](https://git.integrolabs.net/roctinam/aiwg/issues/2242),
[React #412](https://git.integrolabs.net/Fortemi/fortemi-react/issues/412) and
[HotM #231](https://git.integrolabs.net/Fortemi/HotM/issues/231) identify runtime
consumer coordination; listing them does not establish candidate adoption.
Every exercised consumer must pin its tuple and supply verified receipts and
clean-destination results. Old signatures cannot be relabeled as v2 approval.
Rollback must select an entire approved tuple under still-valid trust pins.

The readiness graph is acyclic by construction and regression checks; graph
validity does not satisfy its readiness nodes. Epic closure remains gated on
all five child matrices and their dependencies. The
[acceptance audit](../../../.aiwg/reports/issue-1136-qualification-acceptance-audit-2026-09-05.md)
records the outstanding approvals, receipts, adoption and CI checks.

---
title: HotM SNN Retention Safety Consumer Receipt
artifact_type: cross-repository-consumer-receipt
status: verified
date: 2026-08-28
producer_issue: "Fortemi/fortemi#1102"
consumer_issue: "Fortemi/HotM#287"
---

# HotM SNN Retention Safety Consumer Receipt

Fortemi's authoritative producer change is commit
`e09578e67732d2bd26cf7642ae9038a44a9b9e6c`, tracked by
`Fortemi/fortemi#1102`. The HotM contract implementation is commit
`029f047f865b087d399ca596e2f45d0690c6b89f`; its declared delivery head is
`c8389ceb889a0b7eef741f478e287211e7e96c44`, linked to the existing umbrella
consumer tracker `Fortemi/HotM#287`.

The consumer pins `contracts/openapi/openapi.yaml` byte-for-byte at SHA-256
`f585c0f07ac477159ae86d42ac318e059a42b51f02c8c8a70d767c9fd1c5c9a1` and
semantic fingerprint
`84f38ae783d1e6652e3e94062c98fee0dd680d96b81154ffb16badf2c5ada479`.
It accepts the typed SNN retention-policy `409`, applies its bounded JSON
reader and strict `SnnResult` decoder, exposes the explicit aggressive-pruning
override, and refreshes the executable 253-operation and 204-route
projections.

## Verification

- Fortemi producer workspace and documentation tests passed at the producer
  commit, including report-scale row-preservation and override cases.
- HotM's exact-byte and semantic OpenAPI verifier passed against the producer
  commit.
- HotM's full unit run accounted for 1,811 passing tests after refreshing the
  253-operation catalog fixture; focused clean-destination verification passed
  92 tests and TypeScript type checking.
- HotM's clean-destination route, operation, disposition, receipt, and
  projection-drift checks passed with no generated diff.
- Gitea checks on the consumer delivery head passed the OpenAPI consumer gate,
  SDLC route/contract gate, and container-image build.

## Evidence Boundary

This receipt covers only the named REST producer/consumer contract and its
mocked consumer verification. It is not a live deployment receipt and does not
establish complete API parity, backup completeness, portability, or any
Knowledge Shard `core-v1`, `full-v1`, or `record-v1` conformance cell. The suite
data-compatibility audit remains `NO-GO`.

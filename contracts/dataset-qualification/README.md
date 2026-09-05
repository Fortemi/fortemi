# Dataset production qualification

Tracking authority: [Fortemi #1136](https://git.integrolabs.net/Fortemi/fortemi/issues/1136).
The `1.0.0/readiness.json` graph records execution prerequisites separately from
epic closure. Authority publication permits preparation; it is not a qualification
receipt or permission to execute a drill. Hosted fault cells require tenant proof;
rollback requires restore proof; load requires isolation, recovery, telemetry and
an approved envelope.

Validate the graph with:

```sh
node scripts/ci/verify-dataset-qualification-graph.mjs
node --test scripts/ci/verify-dataset-qualification-graph.test.mjs
```

Graph validation proves only dependency structure. External issue closure must be
paired with delivered revision evidence. Qualification nodes require independently
verified receipts for every declared cell. A `MISSING` receipt cannot become
`PASS` because its issue closed. Unsupported tuples remain unsupported even when
their required rejection behavior passes.

The qualification authority schema, approved instances, receipt admission engine,
immutable evidence inventory and independent verifier integration are still pending.
No child qualification is asserted by this package. The suite audit remains
`NO-GO` for unqualified parity and transportability. Static index, Knowledge Shard
and live persistence claims remain separate; shard claims require the exact
`core-v1`, `record-v1` or `full-v1` executable matrix.

The suite planning authority is
`../.aiwg/architecture/sketch-dataset-execution-production-qualification.md`
(relative to the Fortemi repository root). Its companion use cases, issue plan,
test strategy and draft schemas define the remaining #1136 work. Consumer links
in the graph identify coordination issues, not completed compatibility evidence.

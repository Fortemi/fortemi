# Dataset production qualification

The [2.0.0 detached-attestation candidate](2.0.0/README.md) resolves the draft
signature/digest cycle and adds exact cell declarations and externally pinned
signature verification. Neither candidate admits production receipts yet.

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

The `1.0.0/schemas` directory snapshots the suite's proposed authority and receipt
schemas. `schema-sources.json` binds their original bytes and source paths. This
is a candidate contract, with no approved authority instance or qualification
claim. Schema changes require a new version and review of every declared consumer.
Approved instances, receipt admission, immutable evidence inventory and independent
verifier integration are still pending.
No child qualification is asserted by this package. The suite audit remains
`NO-GO` for unqualified parity and transportability. Static index, Knowledge Shard
and live persistence claims remain separate; shard claims require the exact
`core-v1`, `record-v1` or `full-v1` executable matrix.

The suite planning authority is
`../.aiwg/architecture/sketch-dataset-execution-production-qualification.md`
(relative to the Fortemi repository root). Its companion use cases, issue plan,
test strategy and draft schemas define the remaining #1136 work. Consumer links
in the graph identify coordination issues, not completed compatibility evidence.

## Candidate validation

```sh
node --test scripts/qualification/*.test.mjs
node scripts/qualification/inspect-authority.mjs path/to/authority.json
```

The tools reuse the locked AJV dependencies in `mcp-server`; install those with
`npm ci` in that directory. No runtime success logic is imported. Authority
inspection validates the schema, revision pins, ordered validity window, unique
metrics and nonnegative limits. Safety counts require `eq 0 count`; tuple coverage
requires `eq 1 ratio`; RPO/RTO require upper bounds in seconds. Every qualification
requires redaction and cleanup thresholds. These are necessary policy constraints,
not approval signatures or evidence of a suitable environment.

`inspectReceipt` additionally checks schema, canonical digests, authority/window,
producer/consumer/verifier references, fixture and approval references, exact
threshold coverage, measured outcomes and plane/profile consistency. Its `valid`
field describes internal consistency only; `admitted` is always false. An observed
failure can be a valid failure receipt. Signatures and digest references are not
trusted merely because their fields are present.

## Canonical bytes and compatibility

JSON digests use [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785.html) and SHA-256,
encoded as `sha256:` plus lowercase hexadecimal. JSON string inputs are serialized
as JSON strings; raw artifact hashes must be computed over original file bytes.
Persisted admission inputs must be exact canonical UTF-8 without a BOM or final
newline. The parser rejects duplicate properties, nonfinite numbers, unpaired
surrogates and noncanonical representations before they can become trusted input.
Object keys sort by UTF-16 code units and array order is preserved.

The 1.0.0 candidate receipt digest omits `receiptDigest` and the entire
`verifier.attestation` object as specified by the planning schema. Detached
attestation storage and its evidence-inventory relationship are corrected in the
2.0.0 candidate: an attestation cannot include its own digest in the payload it
signs. Version 1 remains inspection-only and is not eligible for admission.

Rollback must select an intact prior authority/schema/fixture/verifier tuple;
never overwrite signed evidence. An unknown schema version rejects; no broad
forward compatibility is implied. The separate readiness graph retains its own
version. Candidate tests use synthetic in-memory data, expired example windows,
and intentionally invalid signatures. They produce no qualification receipts.

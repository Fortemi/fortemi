# Full-v1 Relationship Conformance

`full-v1-reference-conformance.json` applies deterministic field replacements
or duplicate-row appends to the immutable integrated candidate named by its
SHA-256. Each case starts from a fresh copy. `index` replaces fields on one row;
`copy` appends a clone of that row before replacing fields. No other mutation
operations are supported.

The production Rust relationship validator must accept the four positive
controls and reject the other 93 cases. Every mutated component first passes
both the `1.2.0/full-v1` and `2.0.0/full-v1` record schemas. This isolates
relationship enforcement from schema, checksum, signature and count rejection.
UUID comparisons normalize case; opaque graph/community identities do not.
Finite timestamp ranges retain nanosecond ordering.

Run `cargo test -p matric-api --bin matric-api full_v1_shared_reference_conformance`.
The normal workspace test gate also includes this test.

Consumer coordination: [React #424](https://git.integrolabs.net/Fortemi/fortemi-react/issues/424)
and [Fortemi #1059](https://git.integrolabs.net/Fortemi/fortemi/issues/1059).
React consumes identical mutation bytes against its explicitly identified
schema-2 adaptation, recomputes checksums/counts, and asserts rejection before
database operations or blob writes. This corpus supplements existing fixtures;
it does not change a schema, profile, immutable archive or historical receipt.
It is not native-restore, clean-server, published-package or platform evidence.
Suite NO-GO remains unchanged.

# Knowledge Shard Conformance Matrix

`matrix.json` is the machine-readable producer/consumer inventory owned by
Fortemi issue #1059. It derives required cells from every declared producer
profile and every advertised consumer profile. A cell is not compatible merely
because its repository tests pass: `passed` requires immutable evidence, a
clean destination, semantic re-export, and zero-mutation failure evidence.
Every passed cell must independently cover the complete feature inventory for
its profile plus current-minus-two, current, next-major rejection,
malformed-input, and resource-limit behavior for its consumer. Coverage cannot
be borrowed from another producer, consumer, or cell, and a passed cell must
bind its exact coverage array to a digest-pinned JSON receipt. Pending cells may
record partial evidence, but their missing dimensions are emitted as false
`coverageOutcomes` and keep the registered-profile release gate blocked.

`scripts/ci/verify-knowledge-shard-matrix.py` validates the topology, pins the
Fortemi authority, hashes local evidence, and can clone sibling repositories at
exact commits to verify their declared inputs. The normal CI mode publishes a
per-cell result. Completing every cell sets only
`registeredProfileClaimsAllowed: true`; `claimsAllowed` remains false and the
unqualified compatibility, portability, backup, and parity claims remain
blocked. Tagged release publication invokes `--require-complete`, so a release
fails closed until every required registered-profile cell is genuinely passed.

Run the local checks with:

```bash
python3 -m unittest tests/test_verify_knowledge_shard_matrix.py
python3 scripts/ci/verify-knowledge-shard-matrix.py --verify-remotes
(cd tests/conformance/pglite && npm ci --ignore-scripts --min-release-age=0 && \
  node generate-core-v1-fixture.mjs \
    ../../fixtures/shards/pglite-core-v1-2026.7.11.shard --verify && \
  node generate-record-v1-fixture.mjs \
    ../../fixtures/shards/recordstore-record-v1-2026.7.11.shard --verify)
```

The stricter per-cell gate now records nine passed cells and no pending cells.
This completion is limited to the registered `core-v1`, `record-v1`, and
`full-v1` producer/consumer topology. It does not establish an unqualified
suite-wide compatibility, portability, complete-backup, or parity claim, and
it does not unify the AIWG static index, the explicit AIWG-to-Shard conversion
bridge, and live Fortemi persistence.

The current RecordStore `record-v1` self-cell and
`recordstore-record-v1-to-fortemi` cell are complete: `record-v1` began at
schema `1.1.0`, so their current-minus-two evidence explicitly proves that an
undefined `1.0.0` record-v1 archive is rejected without mutation while the
oldest defined `1.1.0` archive remains accepted by RecordStore. The
cross-repository receipt independently binds the signed current producer
fixture to a clean Fortemi destination, hierarchy, metadata values, explicit
nulls, tombstones, repeated import, semantic re-export, malformed and
next-major rejection, resource limits, and zero mutation. The profile remains
reduced and explicitly lossy. The Fortemi `core-v1` and `full-v1` self-cells
are also complete, each through its own immutable fixture, digest-pinned
receipt, clean destination, semantic re-export, failure rollback,
version-policy, malformed-input, and resource-limit evidence. The PGlite
`core-v1` self-cell independently binds the same nine required dimensions to
its current source fixture and package boundary. The
`pglite-core-v1-to-fortemi` receipt separately binds that fixture to a clean
Fortemi destination, semantic re-export, version and malformed rejection,
resource limits, and zero-mutation evidence. The
`fortemi-core-v1-to-pglite` receipt independently binds the current Fortemi
fixture to clean repeated PGlite import, all declared component and attachment
projections, semantic re-export, version and malformed rejection, resource
limits, and zero-mutation evidence. The AIWG `core-v1` producer now binds its v2
source-schema authority, released Core converter, collection hierarchy,
embedded records, lifecycle state, tombstone, compatibility window, malformed
and resource-limit rejection, repeated import, and semantic re-export to
independent PGlite and Fortemi consumer evidence. The exact AIWG receipt commit,
native Fortemi cell receipt, and clean-database CI jobs are immutable inputs for
those two cells.

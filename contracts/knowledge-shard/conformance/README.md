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
`coverageOutcomes` and keep suite claims blocked.

`scripts/ci/verify-knowledge-shard-matrix.py` validates the topology, pins the
Fortemi authority, hashes local evidence, and can clone sibling repositories at
exact commits to verify their declared inputs. The normal CI mode publishes a
per-cell result while pending cells keep compatibility, portability, backup,
and parity claims false. Tagged release publication invokes
`--require-complete`, so a release fails closed until every required cell is
genuinely passed.

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

The stricter per-cell gate currently records one passed cell and eight pending
cells against the published `@fortemi/core@2026.7.11` receipts. The
RecordStore self-cell is complete: `record-v1` began at schema `1.1.0`, so its
current-minus-two evidence explicitly proves that an undefined `1.0.0`
record-v1 archive is rejected without mutation while the oldest defined
`1.1.0` archive remains accepted. Every other cell remains present in the
required topology with an exact missing-evidence reason; none can inherit that
RecordStore receipt or another cell's semantic coverage.

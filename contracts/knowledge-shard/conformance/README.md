# Knowledge Shard Conformance Matrix

`matrix.json` is the machine-readable producer/consumer inventory owned by
Fortemi issue #1059. It derives required cells from every declared producer
profile and every advertised consumer profile. A cell is not compatible merely
because its repository tests pass: `passed` requires the complete coverage
corpus, a clean destination, semantic re-export, version behavior, and
zero-mutation failure evidence.

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
```

The matrix currently records verified authority and fixture inputs without
promoting incomplete cross-repository cells. Pending work stays linked to its
own repository issue.

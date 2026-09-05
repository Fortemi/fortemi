# Candidate matrix admission

`scripts/qualification/admit-matrix.mjs` combines detached-signature verification,
the [evidence reader](evidence-policy.md), exact-cell aggregation and a protected
local replay ledger. Tests use temporary synthetic files and ephemeral keys.
Actual qualification still needs the approved environment, independent scenario
execution and complete issue acceptance matrix. No production qualification,
durability or capacity claim follows from these tests.

## Operator request

After installing the locked MCP dependencies, run from the repository root:

```sh
node scripts/qualification/admit-matrix.mjs OPERATOR_REQUEST.json
```

The operator-controlled JSON request contains:

| Field | Value |
|---|---|
| `authorityEnvelope` | Complete approved v2 authority DSSE envelope |
| `receiptEnvelopes` | Complete v2 receipt envelopes for declared cells |
| `trust.authorityKeys` | Array of `{digest, publicKeyPem}` from external operator configuration |
| `trust.verifierKeys` | Separate verifier key pins in the same format |
| `trust.authorityThreshold`, `trust.verifierThreshold` | Optional distinct-signature thresholds, default 1 |
| `evidenceRoot` | Operator-selected immutable evidence directory |
| `storeRoot` | Existing protected store directory, separate from the evidence directory |
| `limits` | Positive `maxFileBytes`, `maxTotalBytes`, `maxFiles` bounds for the whole batch |
| `initialize` | `true` only for a deliberately provisioned first-use store; omitted thereafter |

Never derive trust pins or the history location from an untrusted receipt, issue
comment or artifact bundle. The API clock argument is for an operator-controlled
clock/test harness; the CLI uses the current host clock. Operators must retain
the authoritative ledger across invocations and recovery. Selecting a new store
does not preserve replay protection. This is a single-store protocol, not a
distributed consensus or anti-rollback service.

## Results and replay

Every authority cell is emitted. Supported cells have `PASS`, `FAIL` or `MISSING`
status. Unsupported cells retain `UNSUPPORTED` and a separate `checkVerdict`;
verified prewrite rejection can pass its check without making a tuple supported.
Multiple submissions for one cell fail it rather than selecting a favorable
receipt. Invalid signatures and out-of-authority receipts prevent an aggregate pass.

`allRequiredChecksPass` covers only this exact authority's cells. It does not
prove that the authority enumerates every child issue acceptance criterion and
never closes an issue. Reports retain `suiteClaim: NO-GO`. Authority/approval
and receipt/envelope digests locate immutable evidence. Standalone inspectors
still return `admitted: false`; a persisted matrix report has `committed: true`.

Missing/corrupt files produce separate diagnostics and do not consume a verified
attempt. Signed failure observations with verified evidence are recorded as
failures. Nonces and UUID attempts cannot repeat, even across cells; attempt
numbers increase for the same authority/cell. Prior attempts remain in history.
Reports evaluate the submitted batch, not a mixture of old passes and new
receipts. Repeating an admitted batch is a replay; read its persisted report
rather than resubmitting it as fresh evidence.

## Store transaction and recovery

An exclusive `admission.lock` protects canonical `ledger.json`. The lock records
PID, random ownership token and creation time for diagnosis. File presence alone
does not prove that its writer is running. Code never steals a lock. Before
recovering one, confirm the actual writer is terminal and inspect the ledger;
a timeout alone is insufficient.

Missing history fails unless explicit first-use initialization was requested.
Existing stores reject initialization. Invalid or inconsistent history fails
before replacement. Bounds are 10,000 attempts and 16 MiB; exceeding them needs
an approved history-preserving archival/migration procedure, not deletion/reset.

History and report are written together to a new file, fsynced, atomically
renamed over the old ledger and followed by directory fsync. Failure before
replacement preserves prior history; failure after replacement needs inspection
rather than blind retry. Cleanup affects only the current writer's files.
Tests cover injected pre-replacement failure and competing real processes;
they do not qualify power-loss or filesystem durability.

The protected store, clock, verifier code and pins remain trust anchors. Report
consumers must check trust digest, authority validity and revocations before
reusing historical claims. An old store backup cannot revive a revoked signer.

# Dataset execution follow-up audit, September 5, 2026

Scope: Fortemi #1128–#1131 and declared consumer roctinam/aiwg#2242.
Baseline: Fortemi `60530e0655be8d038e3324c275658d964913ecab`.

## Result

The four issues were reopened after executable negative controls contradicted
their completion claims. Historical v2026.9.3 live success evidence does not
cover these cases. The suite-wide portability audit remains outside this
bounded live-remote-persistence qualification and is not advanced here.

## Reproduced defects and runtime corrections

1. The receipt verifier accepted six newly checksummed invalid receipts:
   unknown root field, malformed bound digest, unknown minor schema revision,
   negative counts, contradictory effect/count totals, and cancelled/verified
   state. The corrected verifier rejects these and additional missing/null,
   unknown negotiated revision, running/verified, and redaction controls.
2. Preview accepted unimplemented v1 minor and patch revisions. Runtime checks
   now require exactly the implemented 1.0.0 revisions, including checkpoint
   revisions, and execution rejects these inputs before storage calls.
3. An empty storage response could produce a verified/degraded receipt. A
   generic transport failure was classified as failed even though submission
   could have committed. Responses must now bind the run, batch, checkpoint,
   item digests, note identities, outcomes, and counts; lost or unverifiable
   responses produce ambiguous/unverifiable attempts resolved by exact retry.
4. Checkpoint reads exposed the proposed after-checkpoint during uncertainty.
   They now expose it only after verified committed/degraded execution.
   Archive rejects unresolved attempts with `RUN_OUTCOME_UNRESOLVED` instead of
   reporting complete cleanup from an empty process-local list after lost transport.
5. The contract verification script imports the producer canonicalizer and
   receipt verifier. Its output and ADR incorrectly described this as independent
   runtime verification. The wording now states its actual producer-only scope.

These are enforcement corrections to existing requirements. They introduce no
new wire fields, schema revisions, profile revisions, or mutations to the
published 1.0.0 authority/fixture bundle. The existing successful receipt fixture
remains byte-equivalent. Tighter validation deliberately rejects previously
accepted invalid input. Connection failures now return an explicit unresolved
attempt instead of throwing and leaving misleading failed state.

## Verification

- Focused dataset suite: 20 tests pass, including the new negative controls.
- Existing server-independent MCP CI selection: 84 pass, one existing skip.
- Published contract manifest: all 11 file digests verified; producer receipt
  reproduction and existing negative request vectors pass.
- `git diff --check`: clean.

No new live qualification or production mutation was performed in this audit.

## Remaining acceptance gates

- Publish stricter versioned receipt schemas and language-neutral negative
  fixtures. The existing schema permits underconstrained nested objects; its
  structural checks alone cannot establish count arithmetic or digest equality.
  Document structural versus executable semantic validation explicitly.
- Validate all nested receipt fields and redundant bindings, not just the
  reproduced cases. Root checks and a matching checksum are insufficient.
- Update every declared consumer and the exact authority/manifest pins for any
  new schema revision; retain the published historical 1.0.0 bundle unchanged.
- AIWG origin/main `0410360e` still has a read-only preflight in
  `src/dataset/fortemi-live-qualification.ts` expecting `dataset_capabilities`
  and `dataset_execute`, whereas Fortemi provides `manage_dataset_execution`.
  It never invokes tools and emits its own discovery receipt, not a Fortemi
  RunReceipt. The actual consumer integration and independent validation remain
  necessary despite the historical tracker completion comments.
- Add clean-destination consumer tests and a retained bounded UUID live receipt
  accepted by both implementations. Link producer and consumer delivery/CI
  evidence before closing the issues.

The broader Enterprise, recovery injection, backup/restore, and production load
matrix remains deferred under #1136–#1141; none is claimed by these corrections.

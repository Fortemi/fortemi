# Detached qualification attestations, candidate 2.0.0

Owner: [Fortemi #1136](https://git.integrolabs.net/Fortemi/fortemi/issues/1136).
This candidate breaks compatibility with the 1.0.0 planning snapshots. It has
no approved environment, production key pins, or independently qualified cells.
The suite `NO-GO` remains in force.

## Signed documents and immutable storage

1. Finalize the authority payload with environment, schema hashes, thresholds,
   fixtures, exact producer/consumer revisions, verifier identity/key digest, and
   declared cells. No approval signature or approval-envelope digest is inside it.
2. Authorized operators sign those complete canonical bytes in a detached DSSE
   authority envelope. Its canonical file digest is the receipt's `approvalDigest`.
3. The independent verifier creates a receipt binding the approved authority,
   exact cell, evidence inventory and measured result. `receiptDigest` hashes
   canonical receipt bytes with only `receiptDigest` omitted.
4. The verifier signs the complete receipt, including its digest, in a separate
   DSSE receipt envelope. No signature or receipt-envelope digest is inside the
   receipt's own evidence inventory. The `attestation` evidence type is removed.

Store canonical payloads, approval envelopes, receipt envelopes and artifact
bytes under immutable SHA-256-addressed locations. A separate release inventory
may bind all completed files after signing; that inventory is not an input to
either signature. Never rewrite a prior payload or envelope to rotate a key.
The receipt inventory still includes the authority approval artifact, which
exists before receipt creation, so this ordering has no digest cycle.

## Signature profile and trust anchors

The implementation follows the [DSSE protocol](https://github.com/secure-systems-lab/dsse/blob/master/protocol.md)
using Ed25519 and application-specific payload types:

| Role | Authenticated payload type |
|---|---|
| Authority approver | `application/vnd.fortemi.dataset-qualification.authority.v2+json` |
| Independent verifier | `application/vnd.fortemi.dataset-qualification.receipt.v2+json` |

`verifyEnvelope` verifies the DSSE pre-authentication encoding and parses the
same authenticated canonical UTF-8 bytes. Base64 and base64url are accepted.
The unauthenticated `keyid` hint cannot establish trust. Payloads are limited to
8 MiB and signatures/keys to 32. Thresholds count distinct verified keys.

| Anchor | Control and protection | Rotation and compromise response |
|---|---|---|
| Authority approver public-key pins | Operator-managed configuration outside evidence; read-only to the evidence producer | Remove compromised pins, require newly approved authorities, reassess affected cells |
| Verifier public-key pins | Independently managed verifier configuration outside the runtime; authority binds its exact signer digest | Remove compromised pins; regenerate independent evidence with a new signer and authority tuple |
| Verifier code/schema bytes | Operator selects reviewed immutable code and schema hashes in a trusted execution environment | Select a reviewed replacement tuple and rerun admission; an in-bundle hash alone is insufficient |
| Admission clock | Operator's trusted environment | Reject expired/not-yet-valid authority or future verifier timestamps; investigate clock faults |

Public-key pins are SHA-256 of DER-encoded SPKI plus the corresponding public
SPKI PEM. Authority and verifier accepted signers must be disjoint. Private
signing keys are not supplied to these tools. Unit tests generate ephemeral test
keys in memory and exercise removal/rotation; they do not establish production
custody, operator identity, or an operational rotation drill. Those remain gates
before approving a real authority.

## Current implementation and remaining admission gates

`authenticateReceipt` checks both signature roles, current authority validity,
schema hashes, cell scope, approval-envelope digest, producer/consumer/verifier
bindings, and receipt consistency. `authenticated: true` proves these checks,
but `admitted` remains false. It does not establish that the environment was
synthetic, that evidence bytes exist, or that the independent verifier actually
observed the claimed state.

Artifact-byte verification, trusted prior-attempt/replay history, redaction and
cleanup evidence, complete child matrices, independent verifier execution, and
exact-cell aggregation are still required. Signature success cannot replace any
of them. Unsupported cells must declare rejection with zero mutation; an
authenticated rejection receipt does not make a tuple supported. Keep missing
evidence separate from failed observations and unsupported tuples.

## Migration, consumers and rollback

There is no automatic v1-to-v2 migration of approval evidence: an authorized
operator must approve a v2 authority and the verifier must issue a v2 receipt.
Old signatures cannot be relabeled. Unknown versions and mismatched
authority/receipt versions reject. The old candidate snapshots and their source
hashes remain unchanged for inspection. Runtime dataset schemas and Knowledge
Shard profiles are unchanged by this qualification-only contract.

Current executable consumer: Fortemi's qualification scripts. Future producer/
consumer coordination remains linked to [AIWG #2242](https://git.integrolabs.net/roctinam/aiwg/issues/2242),
[React #412](https://git.integrolabs.net/Fortemi/fortemi-react/issues/412), and
[HotM #231](https://git.integrolabs.net/Fortemi/HotM/issues/231) when exercised.
Their adoption, pinned receipts and clean-destination evidence are required
before claiming qualification for those consumers. Rollback selects a prior
complete approved tuple and its applicable trust policy; it cannot revive a
revoked signer or convert a candidate into an approved authority.

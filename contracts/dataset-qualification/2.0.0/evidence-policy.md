# Candidate evidence reader policy

`scripts/qualification/verify-evidence.mjs` authenticates the authority and
receipt before opening evidence. This is an offline, read-only Linux reader;
it does not provision or query production services. It is one stage of
admission, and always returns `admitted: false`; the [matrix command](admission.md)
owns replay checks and commits aggregate reports. Operator approval, signer custody and independent observation are the
trust requirements documented in [README.md](README.md).

## Files and resource limits

The operator supplies a trusted evidence-root directory and explicit positive
`maxFileBytes`, `maxTotalBytes`, and `maxFiles` limits. The implementation also
caps total bytes at 1 GiB and file count at 10,000. Unknown/unbounded budgets
reject. Production limits still require approval for the exact environment.

Each inventory entry uses a flat `sha256-<64 lowercase hex>` filename matching
its digest. Directories, traversal, symlinks and alternate path spellings reject.
The reader anchors an open directory descriptor through Linux `/proc/self/fd`,
opens child files with `O_NOFOLLOW`, requires regular files, reads only a bounded
length and checks for growth or metadata changes during the read. No writes occur.
The root is an operator-selected location; a receipt cannot select another root.
Missing files are reported separately from digest mismatches and invalid inputs.
The evidence area must be published as immutable for a qualification attempt;
the reader cannot certify storage durability after it returns.

The complete detached receipt envelope must also exist at its canonical-file
digest name, outside the receipt's own inventory. This preserves the signature
without a circular digest. It counts against the same file/byte budgets and its
digest is retained by the admission ledger.

Every referenced file is checked against its raw-byte SHA-256. Evidence revision
metadata must be an immutable Git SHA or SHA-256 reference. Fixture and detached
approval digests must appear under their corresponding roles. Clean-destination
provenance must also have a file in the inventory, usually as an additional
fixture. Unlisted files are not read and contribute no proof.

## Summary bindings

The inventory must contain fixture, approval, runtime-receipt, state-digest,
telemetry, redaction and cleanup evidence. Each summary role below is unique and
uses exact canonical UTF-8 JSON with no final newline:

| Role | Exact content |
|---|---|
| approval | Complete detached authority envelope |
| state-digest | Receipt `actual` object, including canonical state digest |
| telemetry | Receipt `measurements` object |
| redaction | `{attemptId, runNonce, verifierRevision, findings}`; findings matches `redactionFindings` |
| cleanup | `{attemptId, runNonce, verifierRevision, cleanDestinationProvenance, outOfScope}`; outOfScope matches `cleanupOutOfScope` |

All non-approval summaries name the pinned verifier revision in their inventory
entry. These summaries bind independent observations to the receipt; they do not
replace the verifier that computes canonical state, scans redaction, and checks
namespace cleanup. The signed verifier assertion is trusted only through the
operator's external pins. Fixture, runtime-receipt and provenance files remain
raw artifacts whose interpretation belongs to their respective scenario and
contract authorities; matching a hash does not certify their semantics.

An absent role/file yields missing evidence. Wrong hashes, replayed summary
identity, changed measurements, duplicate summaries or different approval bytes
are invalid evidence. A signed `PASS` cannot override these failures. Reader
tests use temporary synthetic files and ephemeral signing keys, and produce no
production qualification claim.

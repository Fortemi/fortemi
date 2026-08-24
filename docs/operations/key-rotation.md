# Key Rotation and DEK Rewrap

This runbook covers the #734 `KeyProvider` foundation. It does not make hosted KMS rotation operational by itself. Hosted execution requires a real KMS provider, audited application wiring, transactional persistence, and release evidence.

## Invariants

- Fail closed on unavailable, denied, disabled, context-mismatch, malformed, or unsupported key state. Never fall back to plaintext or `EnvKeyProvider` in hosted multi-tenant mode.
- Rewrap the existing DEK. Do not generate a new DEK unless the payload is also decrypted and re-encrypted in a separately designed data-key rotation.
- Reconstruct `KeyContext` from trusted tenant/user/resource records. Do not accept context fields from an API caller.
- Never log plaintext DEKs, master keys, wrapped DEKs, ciphertext, provider credentials, raw provider errors, KEK references, or context values. Audit only stable operation, failure class, counts, timestamps, and opaque job/row identifiers.
- Keep the old provider/key version available until every row is verified and the rollback window closes.

## Hosted KMS Procedure

1. Confirm provider health with a generate/wrap/decrypt canary using production-shaped context. Confirm audit and persistence transaction health.
2. Record the provider rotation receipt/current key-material version. AWS managed-key rotation normally keeps the same key ID; Vault Transit advances its key version.
3. Start a resumable background job. For each row, load the envelope and trusted context, unwrap with the recorded old version, wrap the same in-memory DEK with the current version, and atomically update only `WrappedKey` plus `rewrapped_at`.
4. Zeroize the plaintext DEK on success and every error path. A failed row remains unchanged and is retried only for retryable provider classes with bounded backoff.
5. Verify each batch through the normal read path. Track metadata-only totals for scanned, rewrapped, already-current, retryable-failed, and terminal-failed rows.
6. Finish only when a second scan reports no stale rows and all terminal failures are resolved. Retain the old version through the approved rollback window.

Rollback stops the job and keeps reads on both old and current versions. Restore prior `WrappedKey` values from transactional history only if the new wrapping is unusable; payload ciphertext does not change during rewrap.

## EnvKeyProvider Procedure

`EnvKeyProvider` is limited to explicit single-tenant desktop/development mode and does not implement `rotate()`. Plan downtime.

1. Back up the encrypted database/envelopes and verify restore before maintenance.
2. Keep the old 32-byte master only in the isolated rotation process. Construct the old provider with its exact KEK reference/version and a new provider with fresh CSPRNG-generated 32-byte material, a new non-secret KEK reference, and incremented version.
3. Stop all writers. Rewrap every row with `rewrap_between(old, new, wrapped_key, trusted_context)` inside bounded transactions.
4. Verify all rows through the new provider before cutover. Any failure leaves the old envelope unchanged and blocks cutover.
5. Update the secret source with standard-base64 `FORTEMI_MASTER_KEY`, `FORTEMI_ENV_KEK_REF`, and `FORTEMI_ENV_KEY_VERSION`; restart and run health plus representative decrypt checks.
6. Retain the old master in the approved recovery secret store for the rollback window, then destroy it under the deployment secret-destruction policy.

Do not place master material in shell arguments, tickets, logs, command history, this runbook, or rotation receipts.

## Current Acceptance Gaps

- No AWS KMS, Vault Transit, or GCP KMS implementation is present.
- No application-owned row enumeration, atomic update, checkpoint, audit-event, or startup enforcement is wired in this bounded scope.
- No LocalStack/OpenBao/live-KMS rotation evidence exists.
- `mlock`/non-dumpable process hardening is not implemented; zeroize-on-drop does not eliminate swap, coredump, allocator-copy, or provider-SDK exposure.
- Managed signing remains unsupported by `EnvKeyProvider`; the foundation does not substitute a symmetric MAC for a real KMS signing key.

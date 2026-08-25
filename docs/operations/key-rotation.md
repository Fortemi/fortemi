# Key Rotation and DEK Rewrap

This runbook covers the #734 `KeyProvider` foundation, AWS KMS provider, and the #730
`user_secrets` persistence consumer. Hosted builds include a resumable, leased, audited batch
rewrap worker. Production release still requires environment-specific KMS policy, outage, rotation,
rollback, and recovery receipts.

## AWS Provider Construction and Canary

Build `matric-crypto` with feature `kms-aws`. Construct the AWS SDK `Client` from the deployment's
central AWS configuration, wrap it in `AwsSdkKmsClient`, and construct `AwsKmsProvider` with
`FORTEMI_AWS_KMS_KEY_ID`. The key identifier may be a key ARN or alias accepted by KMS. Credentials
remain in the standard AWS credential chain and must not be copied into Fortemi configuration or
logs.

For LocalStack or another KMS-compatible emulator, set the endpoint on the AWS SDK configuration;
do not add emulator behavior to `AwsKmsProvider`. The injectable `AwsKmsClient` boundary is used by
unit tests and can also host a process-backed emulator adapter.

Hosted startup must call `health_check` with a production-shaped, non-secret `KeyContext`. The hook
executes `GenerateDataKey(AES_256)` followed by `Decrypt` using the exact versioned encryption
context. Any returned error or non-`Ready` status blocks hosted startup. Do not replace this with
`DescribeKey`, and never fall back to `EnvKeyProvider`; that provider refuses construction in
hosted multi-tenant mode.

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

## Fortemi Rewrap Worker

The worker is internal application state, not an HTTP route. It is absent from the Community
Edition path and starts only in hosted mode when both of these non-secret identifiers are present:

- `FORTEMI_USER_SECRET_REWRAP_TENANT_ID`
- `FORTEMI_USER_SECRET_REWRAP_JOB_ID`

`FORTEMI_USER_SECRET_REWRAP_BATCH_SIZE` defaults to `100` and is bounded to `1..=1000`. Generate a
fresh opaque job ID for each approved lifecycle. Reusing the same tenant/job pair resumes its
persisted cursor and aggregate counts. Each claim has a 120-second lease; retryable KMS failures are
recorded as `retryable`, release the lease, and use a bounded worker delay. Invalid envelopes,
context failures, access denial, and disabled/unavailable key versions stop the lifecycle according
to their stable failure class. A concurrent row change causes a compare-and-swap skip instead of
overwriting newer state.

The `user_secret_rewrap_job` row and `key_lifecycle` audit events contain only the opaque job ID,
status, cursor row identifiers/timestamps, aggregate counts, attempts, and stable reason codes.
They must never contain envelope JSON, wrapped keys, KEK references, contexts, provider errors, or
plaintext. Completion means enumeration reached the end for that job; operators must still verify
the normal decrypt path and approved rollback window before retiring old key material.

For an emulator receipt, run the same startup canary and worker against the emulator-backed
`AwsKmsClient`, force an outage, verify `provider_unavailable` is persisted as retryable and no row
changes, restore the emulator, rotate material, resume the same job, and verify ciphertext remains
unchanged while `wrapped_key` and `rewrapped_at` advance. Do not describe the injectable in-process
mock tests as LocalStack or live-KMS evidence.

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

- Vault Transit and GCP KMS implementations are not present.
- `user_secrets` has application-owned row enumeration, leased resumable checkpoints,
  compare-and-swap wrapped-key updates, and fail-closed lifecycle audit. The local PostgreSQL test
  receipt still requires a configured test DSN.
- AWS KMS has deterministic injectable-client outage and material-rotation receipts, but no
  LocalStack/OpenBao/live-KMS policy, outage, rotation, rollback, or recovery receipt exists.
- `mlock`/non-dumpable process hardening is not implemented; zeroize-on-drop does not eliminate swap, coredump, allocator-copy, or provider-SDK exposure.
- Managed signing remains unsupported by `EnvKeyProvider`; the foundation does not substitute a symmetric MAC for a real KMS signing key.

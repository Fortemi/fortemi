# Hosted User Credentials

This runbook covers the internal #730/#731 stored-provider-credential and
hosted inference preview. It is
available only in a binary built with `hosted-auth` and running with hosted
multi-tenant mode, a verified user-bound OAuth principal, forced PostgreSQL
RLS, the durable PostgreSQL audit sink, and a startup-validated KMS provider.
The community build does not mount these routes.

Stored credentials can be used by the existing inference route family. The
handlers load them through the tenant/user-scoped repository, decrypt through
`KeyProvider`, and route through the shared policy in
`docs/operations/inference-destination-policy.md`. Inline keys, request-owned
destinations, server-key fallback, and caller-selected models outside the
operator-approved profile defaults are denied in hosted mode.

## API Surface

The preview routes are hidden from generated public/operator OpenAPI output:

- `POST /api/v1/user/secrets` accepts `provider`, `name`, and `key`.
- `GET /api/v1/user/secrets` returns metadata for the authenticated user.
- `DELETE /api/v1/user/secrets/{id}` revokes an opaque row ID idempotently.
- `POST /api/v1/inference/complete` requires `provider_id`, `secret_id`, an
  approved `model`, and messages in hosted mode.
- `POST /api/v1/inference/stream` has the same hosted request contract and
  returns bounded-backpressure SSE. Its tenant transaction is explicitly
  committed before body delivery; key-use timestamp updates use a separate
  tenant-scoped transaction.
- `POST /api/v1/inference/embed` is hosted-only and requires `provider_id`,
  `secret_id`, approved `model`, `dimension`, and `input`.
- `GET /api/v1/inference/providers` returns only profiles backed by the
  caller's active stored credentials.
- `GET /api/v1/inference/catalog` is hosted-only and adds the
  operator-approved default generation and embedding models for those
  caller-available profiles.

The user-secret and embedding routes are compiled only with `hosted-auth` and
are hidden from Community Edition and generated public/operator OpenAPI. The
generation, streaming, and provider routes remain available in Community
Edition with its local/transient BYOK contract; in hosted mode the same
handlers switch to the stored-secret contract and reject inline credentials.

Provider names must resolve through the inference provider registry and must
identify a profile that uses an API key. Fortemi allocates the row ID before
encryption and derives the versioned key context from trusted tenant ID, user
ID, row ID, purpose, and table family. Callers cannot supply context, key
provider metadata, or an upstream destination.

Create and list responses include only row ID, provider, user-visible name,
`<provider>:configured`, and lifecycle timestamps. The mask is deliberately
not derived from a prefix, suffix, hash, or other credential material. Raw
keys, encrypted envelopes, wrapped DEKs, KMS references, and provider metadata
are never returned.

## Revocation And Support

Revocation sets `revoked_at`; active lookup excludes the row immediately.
Repeated DELETE requests return the same successful no-content result, and a
caller cannot use DELETE to distinguish a missing row from another user's or
tenant's row. This is local disablement only. It does not destroy an external
provider credential, provider account data, KMS key version, or audit history.

For a user support request:

1. Confirm only the provider, display name, row ID, and lifecycle timestamps.
2. Revoke the row when requested and confirm the metadata-only audit receipt.
3. Direct the user to rotate or delete the credential at the external provider.
4. Do not ask for or accept the raw key in tickets, chat, logs, or diagnostics.

Fortemi cannot recover the submitted plaintext after storage. If KMS context,
key access, or envelope validation fails, the operation fails closed with a
stable service error. Do not bypass KMS, copy envelope fields into a response,
or fall back to the community environment provider.

Plaintext keys are held in zeroizing buffers while the backend is constructed;
the OpenAI-compatible backend zeroizes its owned key on drop. Request, response,
provider, and audit debug output records only bounded metadata and stable reason
codes. The global authorization inventory, hosted Redis request quota gate, and
usage meter continue to wrap these routes.

This remains an internal enterprise surface, not an unqualified hosted
production approval. Generation, SSE, and embedding stored-secret calls share
a bounded provider/model/account circuit-breaker registry as documented in
`docs/operations/hosted-inference-resilience.md`. Live KMS/provider receipts and
UMG-compatible public protocol surfaces remain separate launch evidence.

## Rotation And Recovery

The hosted worker enumerates active and revoked rows in bounded batches,
rewraps the same DEK, compare-and-swap replaces only the envelope's
`wrapped_key`, sets `rewrapped_at`, and persists leased checkpoints plus
metadata-only lifecycle audit. The tests prove that payload ciphertext is
unchanged and remains decryptable. Environment-specific rollback and live
provider evidence remain required by `docs/operations/key-rotation.md`.

## Confirmed DSAR Erasure

The ordinary DELETE route remains non-enumerating revocation and is not hard
deletion. A confirmed DSAR workflow calls the internal erasure service after
identity, tenant, request authority, and audit availability have been
established. The service emits a fail-closed start receipt, hard-deletes all
active and revoked `user_secrets` rows for that tenant/user, then emits a
metadata-only completion receipt.

The erasure receipt distinguishes four outcomes: local encrypted secret rows
are deleted; aggregate rotation-job history is retained on a security basis;
durable audit history is retained on its compliance/security basis; and the
external provider account still requires provider-side user/operator action.
Neither local revocation nor hard deletion claims destruction of an upstream
credential, provider account, KMS version, backup copy governed by retention,
or audit record. DSAR tooling must surface these distinctions without exposing
envelopes, key references, contexts, or raw provider errors.

Encrypted `user_secrets` rows are not currently an authorized complete-backup
or portability profile. Recovery requires the matching provider/key versions,
the original trusted tenant/user/row context, and a separately verified
database restore. A database copy without KMS custody is not a recoverable
credential backup.

# OpenBao Transit for hosted Fortemi

The `kms-vault` build feature supplies the on-prem KeyProvider required by
[Enterprise KMS #1](https://git.integrolabs.net/Fortemi-Enterprise/kms/issues/1).
Build the server with `--features hosted-auth,kms-vault`. The ordinary published
2026.9.7 image does not contain this provider. Use an independently verified
internal image and its immutable digest from the Enterprise KMS delivery receipt.

## Configuration

```dotenv
FORTEMI_KEY_PROVIDER=vault-transit
FORTEMI_KEY_STRATEGY=per-purpose
FORTEMI_KEY_CONTEXT_VERSION=1
FORTEMI_VAULT_ADDR=https://vault.integrolabs.net
FORTEMI_VAULT_TRANSIT_MOUNT=transit
FORTEMI_VAULT_TRANSIT_KEY=fortemi-qualification
FORTEMI_VAULT_AUTH_METHOD=token-file
FORTEMI_VAULT_TOKEN_FILE=/run/fortemi-kms/token
FORTEMI_VAULT_CA_BUNDLE=/etc/fortemi/trust/openbao-ca.pem
FORTEMI_VAULT_TIMEOUT_SECONDS=10
```

`FORTEMI_VAULT_NAMESPACE` is optional. The address must be an HTTPS origin with
no credentials, query, fragment or path prefix. Mount/key names use ASCII letters,
digits, hyphens and underscores. Namespace components use the same characters,
separated by `/`. Timeout is 1–120 seconds; default 10.

`per-purpose` (default) resolves the configured base plus `-` plus the canonical
purpose: the current hosted secret consumer therefore uses
`fortemi-qualification-user_secret`. `shared-with-context` uses the configured
key directly. Both bind the tenant, user, resource, schema and purpose from
trusted server state. `per-tenant`, unsupported context versions and unsupported
auth methods fail configuration. They are not silently mapped to a weaker strategy.
The current server startup canary exercises `user_secret`; new hosted purposes
must add their own canary and provisioned key before becoming active.

An explicit `vault-transit` selection never falls back to AWS or local keys.
Absent `FORTEMI_KEY_PROVIDER` preserves the existing AWS selection for backwards
compatibility. AWS still needs `kms-aws` and its key/credential configuration;
its current supported strategy is one key with context binding
(`shared-with-context`). Local `env` is forbidden in hosted mode.

## Trust and credential lifecycle

`FORTEMI_VAULT_CA_BUNDLE` is independent of OIDC's `FORTEMI_AUTH_CA_BUNDLE`.
It contains public certificates only and adds trust to normal public roots.
Unset means public roots only. An explicitly empty, unreadable, malformed or
non-certificate bundle fails initialization. Certificate chain, hostname and
validity verification remain enabled; redirects and proxy discovery are disabled.
Mount the bundle read-only and restart after changing it.

The token file must be an absolute path to a regular file with a single hard link,
owned by root or the runtime UID, readable by the runtime, with mode `0400` or
`0600`. Group/other permissions, symlinks (including parent components), FIFOs,
empty and oversized files are rejected. Mount its protected parent directory
read-only in the API container so an external agent's atomic file replacement
becomes visible. A single-file bind mount can retain the old inode.

The API rereads the file for every request and has no last-known-token fallback.
It does **not** renew tokens or log into AppRole itself. A separately scoped Bao
Agent, using workload AppRole/TPM custody, owns login, renewal and replacement
of the token sink. Its policy permits lookup/renewal of its own token only;
these lifecycle paths grant no additional Transit or token administration. Denial/expiry fails the operation; replacing the sink with a
valid token permits recovery on the same provider instance. File reload is not
proof of renewal. Never put the token in command arguments, environment variables,
Compose values, logs, images or compatibility responses.

## Provisioning and least privilege

A separate operator provisions each key as `aes256-gcm96`, `derived=true`,
`convergent_encryption=false`, `exportable=false`, `allow_plaintext_backup=false`
and `deletion_allowed=false`. Runtime verifies these properties through key
metadata before operations. Provisioning, configuration, deletion and rotation
permissions do not belong to the API identity.

Example runtime policy for the key above:

```hcl
path "transit/keys/fortemi-qualification-user_secret" {
  capabilities = ["read"]
}
path "transit/encrypt/fortemi-qualification-user_secret" {
  capabilities = ["update"]
}
path "transit/decrypt/fortemi-qualification-user_secret" {
  capabilities = ["update"]
}
# The external Bao Agent can inspect and renew only its own runtime token.
path "auth/token/lookup-self" {
  capabilities = ["read"]
}
path "auth/token/renew-self" {
  capabilities = ["update"]
}
```

Do not grant encrypt `create`: that permits implicit key provisioning. Do not
expand this policy to wildcard keys. The runtime token needs neither PKI private
keys nor secret-store administration. Database migration and runtime credentials
remain separate from Transit credentials and qualification client readers.

## Context, failure and rotation behavior

The provider uses the existing canonical KeyContext byte encoding with a versioned
Transit wrapping domain. Those bytes bind the configured endpoint, namespace,
mount and resolved key and are sent as both derivation `context` and AEAD
`associated_data`. These have distinct roles in the
[OpenBao Transit API](https://openbao.org/docs/api/secret/transit/).
An endpoint/name change is therefore a wrapping identity change requiring rewrap,
even if the backend key material is copied. Payload AAD stays provider-neutral.

Fresh 256-bit data keys are generated from the OS CSPRNG for each operation and
wrapped through Transit encrypt; no DEK cache exists. OpenBao 2.3.1 does not
apply associated data on its datakey endpoint, so this implementation does not
use that endpoint or request its permission.
Hosted startup requires a real generate/decrypt canary. Unavailable/sealed service,
denial, bad context or invalid key configuration fails closed. Errors expose
stable classes, never raw provider bodies or secret payloads. Plaintext DEKs and
owned credential/response buffers are zeroized; locked memory and guarantees for
HTTP/TLS library internal buffers remain outside the v1 contract.

The operator rotates the Transit key. Old versions remain usable until the
consumer's resumable rewrap job is complete. Rewrap decrypts the old wrapped DEK
and encrypts the same DEK with current key material, then the existing consumer
atomically replaces only wrapped-key fields. It does not change payload ciphertext.
No native Transit rewrap permission is required. Runtime `rotate` is unsupported.
Do not raise minimum decryption versions or delete old material until verification
and rollback requirements are satisfied.

Qualification distinguishes deterministic provider tests, disposable real OpenBao
receipts, and the actual ITops environment. None alone closes the entire hosted
lane. ITops #662 and HotM #304 additionally require real API token, two-tenant and
realtime behavior. GCP remains follow-on; this provider makes no GCP parity claim.

---
title: Fortemi Auth Rust Consumer Smoke Receipt
status: passed
date: 2026-08-24
consumer: matric-api
authority_repository: https://git.integrolabs.net/Fortemi/fortemi-auth
authority_release: v2026.8.1
authority_tag_object: 7b4703a015357c347d4165ec591ec55e31b9e6b2
authority_commit: 1b6ddb1b58a12efc5b631386ad783cb12edec518
authority_contract_version: 1.1.0
authority_profile: rust-node-jwt-v1
manifest_sha256: 2df0a35edad67cc3e8869286183a4d098b1eb8fc2161432ed0b54ba69b17e242
release_policy_sha256: d70491c336a62508ef3c7937af709dd121a6ec4f421ceab66486af3f371de8db
related_issues:
  - Fortemi/fortemi#707
  - Fortemi/fortemi#728
  - Fortemi/fortemi#1081
  - Fortemi/fortemi-auth#1
---

# Fortemi Auth Rust Consumer Smoke Receipt

## Scope

This receipt proves that Fortemi's isolated consumer harness can consume the public
`fortemi-auth-core` and `fortemi-auth-axum` API from the signed immutable
`v2026.8.1` CalVer authority release above and that Fortemi independently executes the
canonical `rust-node-jwt-v1` corpus. It is a downstream release and corpus
smoke, not evidence that hosted middleware, RLS, `TenantScopedConn`, or
`SET LOCAL app.current_tenant` is complete.

## Executed controls

- Cargo resolved all four public/test crates at exact version `2026.8.1` from
  signed tag `v2026.8.1`; `Cargo.lock`
  records its peeled immutable commit.
- The locked harness runs in `tests/fortemi-auth-consumer` so its JWT dependency
  features cannot alter `matric-api` serialization or generated contracts.
- `cargo deny check` passed with unknown Git sources denied and only the public
  `Fortemi/fortemi-auth` authority URL explicitly allowed.
- The vendored manifest is byte-identical to the release authority at SHA-256
  `2df0a35edad67cc3e8869286183a4d098b1eb8fc2161432ed0b54ba69b17e242`.
- The vendored release policy is byte-identical at SHA-256
  `d70491c336a62508ef3c7937af709dd121a6ec4f421ceab66486af3f371de8db`.
- Fortemi executed all nine release-policy cases. The exact current CalVer
  tuple passed; the bootstrap predecessor, previous/next calendar trains,
  next calendar year, contract/profile drift, and manifest drift failed closed
  with stable policy errors.
- Fortemi executed all 22 canonical cases through `ClerkProvider`, including
  expiry, future and not-before timestamps, tampering, issuer/audience/algorithm
  rejection, tenant validation, exact scope rejection, malformed input, and
  signing-key rotation.
- A downstream `OAuthProvider` implementation produced the shared
  `AuthContext`.
- `fortemi_auth_axum::auth_layer` injected that context into a protected Axum
  handler.
- Missing and invalid bearer credentials failed closed with the stable,
  redacted `malformed_token` and `invalid_signature` codes.
- All five tenant-store cases passed: unavailable, timeout, and malformed
  responses map to redacted `tenant_store_unavailable` / HTTP 503; inactive and
  not-found tenants remain `unknown_tenant` / HTTP 403.
- The accepted request preserved tenant, principal, and exact scope values.
- `cargo test --locked` passed all six consumer tests, and `cargo deny check`
  passed advisories, bans, licenses, and source policy (with duplicate-version
  warnings only).
- Root `matric-api` hosted-auth tests passed for the redacted 503 mapping and
  exact compatibility tuple. The compatibility response remains `preview` and
  does not claim hosted readiness.

## Remaining gate

Issue #728 remains open for runtime router installation and transaction-bound
tenant enforcement after #726 and #727. This receipt must not be used to claim
hosted or multi-tenant readiness.

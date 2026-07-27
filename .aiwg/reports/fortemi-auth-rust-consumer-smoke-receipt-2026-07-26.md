---
title: Fortemi Auth Rust Consumer Smoke Receipt
status: passed
date: 2026-07-26
consumer: matric-api
authority_repository: https://git.integrolabs.net/Fortemi/fortemi-auth
authority_release: v0.1.0
authority_tag_object: a088993d6ed86ed3a0a7f2ac8a3e0ab03ecf6e49
authority_commit: e99f5378abdf416f443334d392e9d835a5a86212
authority_contract_version: 1.0.0
authority_profile: rust-node-jwt-v1
manifest_sha256: dbd7fff6370d8a0c55d2c7e4ad311d3ddd1796815e2caff6dc05501cdf417a38
related_issues:
  - Fortemi/fortemi#707
  - Fortemi/fortemi#728
  - Fortemi/fortemi#1081
  - Fortemi/fortemi-auth#1
---

# Fortemi Auth Rust Consumer Smoke Receipt

## Scope

This receipt proves that `matric-api` can consume the public
`fortemi-auth-core` and `fortemi-auth-axum` v0.1 API from the signed immutable
`v0.1.0` authority release above and that Fortemi independently executes the
canonical `rust-node-jwt-v1` corpus. It is a downstream release and corpus
smoke, not evidence that hosted middleware, RLS, `TenantScopedConn`, or
`SET LOCAL app.current_tenant` is complete.

## Executed controls

- Cargo resolved both public crates from signed tag `v0.1.0`; `Cargo.lock`
  records its peeled immutable commit.
- `cargo deny check` passed with unknown Git sources denied and only the public
  `Fortemi/fortemi-auth` authority URL explicitly allowed.
- The vendored manifest is byte-identical to the release authority at SHA-256
  `dbd7fff6370d8a0c55d2c7e4ad311d3ddd1796815e2caff6dc05501cdf417a38`.
- Fortemi executed all 13 canonical cases through `ClerkProvider`, including
  expiry, future and not-before timestamps, tampering, issuer/audience/algorithm
  rejection, tenant validation, exact scope rejection, malformed input, and
  signing-key rotation.
- A downstream `OAuthProvider` implementation produced the shared
  `AuthContext`.
- `fortemi_auth_axum::auth_layer` injected that context into a protected Axum
  handler.
- Missing and invalid bearer credentials failed closed with the stable,
  redacted `malformed_token` and `invalid_signature` codes.
- The accepted request preserved tenant, principal, and exact scope values.
- The focused consumer smoke and Cargo metadata contract test passed under
  `--locked`.

## Remaining gate

Issue #728 remains open for runtime router installation and transaction-bound
tenant enforcement after #726 and #727. This receipt must not be used to claim
hosted or multi-tenant readiness.

# ADR-094: Fail-Closed Authentication Default

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-071 (auth middleware), ADR-089 (authorization), ADR-090 (tenancy)
**Related docs:** `.aiwg/architecture/ce-ee-audit-2026-05.md` finding S-1 (CRITICAL)

## July 2026 checkpoint rebaseline

This ADR remains proposed for the default-auth inversion. The multi-tenant guard is partially implemented: `matric-api` rejects anonymous mode when `FORTEMI_MULTI_TENANT=true`. The general CE default change and explicit `I_UNDERSTAND_NO_AUTH` flow are still not complete.

- **Decision status:** Proposed; multi-tenant guard partially implemented.
- **Implementation phase:** Default-auth inversion awaiting a dedicated construction split.
- **Phase owner:** `Fortemi/fortemi#1017` until the construction issue is split.
- **Checkpoint decision date:** 2026-07-14.

## Context

ADR-071 added authentication middleware behind a `REQUIRE_AUTH` environment variable. The default is **off**, meaning:

- A fresh deployment of Fortemi has all `/api/v1/*` routes anonymous-accessible
- Operators must explicitly opt-in to enforcement
- The OAuth/JWT infrastructure is fully wired but inert by default

Source: `crates/matric-api/src/main.rs:1756` reads `REQUIRE_AUTH` from env, defaulting to `false`.

This is **fail-open** behavior. The CE/EE audit (finding S-1) classifies it as a CRITICAL severity issue. A misconfigured production deploy is unauthenticated by accident, and the standard "well, my JWT is valid" smoke-test gives a false sense of security because the unauthenticated path is never exercised.

Industry consensus (OWASP, NIST SP 800-53, CIS Controls) is that authentication should fail closed by default. Fortemi is the outlier.

## Decision

**Invert the default. `REQUIRE_AUTH` defaults to `true`. To run anonymously, operators must set `REQUIRE_AUTH=false` explicitly AND set a new `I_UNDERSTAND_NO_AUTH=true` flag in the same configuration.**

### Behavioral changes

| Configuration | Before | After |
|---|---|---|
| No env vars set | Anonymous | **403 on protected routes** (with clear startup error: "REQUIRE_AUTH=true but no OAuth issuer configured") |
| `REQUIRE_AUTH=true` + OAuth configured | Auth required | Auth required (unchanged) |
| `REQUIRE_AUTH=false` | Anonymous (silent) | **Refuses to start** unless `I_UNDERSTAND_NO_AUTH=true` is also set |
| `REQUIRE_AUTH=false` + `I_UNDERSTAND_NO_AUTH=true` | (n/a) | Anonymous, with prominent startup warning logged every 60s |

### Startup validation

On startup, the API binary:
1. Reads `REQUIRE_AUTH` (default `true`)
2. If `true`: verify at least one OAuth issuer is configured. If none, log a structured error and exit non-zero. **Do not start with auth required but no providers.**
3. If `false`: verify `I_UNDERSTAND_NO_AUTH=true`. If not, log error and exit non-zero.
4. If `false` + `I_UNDERSTAND_NO_AUTH=true`: log a Warn-level message on startup AND every 60 seconds containing the literal string "RUNNING WITHOUT AUTHENTICATION — THIS IS NOT A PRODUCTION CONFIGURATION".

### Multi-tenant builds

In `--features multi-tenant` builds (ADR-090), `I_UNDERSTAND_NO_AUTH=true` is **rejected at startup** regardless of `REQUIRE_AUTH`. Multi-tenant fortemi cannot run anonymous.

### Public routes (unchanged)

Health, OAuth discovery, OAuth endpoints (register/token/introspect/revoke), and the OpenAPI doc routes remain public regardless of `REQUIRE_AUTH`. These are explicitly allowlisted in the auth middleware.

### CI / test mode

Tests that need anonymous access use the `cfg(test)` switch or explicit fixture (`AppState::with_no_auth_for_tests()`). The runtime env-var dance is not required in unit/integration tests.

## Consequences

### Positive
- (+) Fortemi defaults to a safe posture
- (+) Operators must consciously decide to run insecure
- (+) Misconfigured deploys fail loudly at startup, not silently at runtime
- (+) Audit/compliance shows "auth enforced by default"
- (+) Cross-references industry standards (OWASP ASVS, NIST SP 800-53 IA-2)

### Negative
- (-) Breaking change for existing CE users who run without `REQUIRE_AUTH`. Mitigated by:
  - Clear CHANGELOG entry
  - Startup error includes the exact env vars to set for the previous behavior
  - One minor version of `WARN: REQUIRE_AUTH will default to true in 2026.7` ahead of the change
- (-) New deployments that don't set up OAuth provider can't even view the OpenAPI doc on a localhost test (public routes still work, but `/api/v1/*` returns 403)

### Neutral
- (~) HotM and other local-only consumers (which today rely on anonymous + localhost-bind) need to configure a local OAuth provider OR explicitly run with `I_UNDERSTAND_NO_AUTH=true`. Document in `HotM/agent-proxy/SECURITY.md`.

## Implementation

**Code location:** `crates/matric-api/src/main.rs` (the `require_auth` config block)

**Key changes:**
1. Change `require_auth` default from `false` to `true`
2. Add `i_understand_no_auth` config field
3. Add startup validation that returns `Err(...)` on bad config
4. Add periodic warning task when running anonymous
5. Update CHANGELOG with breaking-change notice and migration instructions

**Migration window:**
- Version N (release this ADR): default still `false`, log `WARN` "REQUIRE_AUTH will default to true in version N+1"
- Version N+1 (one minor later): default `true`, breaking change
- Version N+2: remove the deprecation warning

## References

- ADR-071 — Auth middleware
- ADR-089 — Authorization policy
- ADR-090 — Tenancy (forbids anonymous in multi-tenant)
- OWASP ASVS 4.0 §V2.1 (authentication enforcement)
- NIST SP 800-53 IA-2 (identification and authentication)
- CIS Controls v8 §5 (account management)

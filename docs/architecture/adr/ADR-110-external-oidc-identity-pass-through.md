# ADR-110: End-User Identity Through an External OIDC Provider for the API, MCP and CLI

**Status:** Accepted
**Date:** 2026-10-09
**Authority:** `Fortemi/fortemi#1188` (end-user OIDC identity epic)
**Related:** ADR-071 (auth middleware), ADR-089 (authorization policy trait), ADR-094 (fail-closed default), ADR-100 (MCP scope gate), `Fortemi/fortemi#1151`, `#1152`, `#1155`, `Fortemi/fortemi-auth#52`
**Supporting analysis:** `.aiwg/working/auth-oidc-design-brief.md`, `.aiwg/security/threat-model-oidc-identity.md` (T-01 to T-30, SR-1 to SR-63), `.aiwg/research/findings/auth-oidc-passthrough-research.md`

## Context

Organizations deploy Fortemi next to an existing OIDC identity provider. Their users expect to sign in once and then use the API from scripts, CLIs, agents and MCP clients (Claude Code, Claude Desktop, Cursor, VS Code). The server must know who each caller is.

A reference deployment showed what goes wrong today. Fortemi sat behind an oauth2-proxy cookie gate, and a member who completed OIDC login received **401 from Fortemi**: the proxy admitted the person but passed on no identity, and Fortemi still required its own token. Programmatic clients cannot carry a browser cookie at all.

Fortemi already validates IdP access tokens, but only in hosted multi-tenant mode. That mode also qualifies few routes (embedding jobs, attachments and provenance are excluded) and refuses Fortemi-issued tokens. Across modes:

- **No user model.** Identity stops at the tenant: `sub` is recorded only for audit, and `email` is never read.
- **Unowned API keys.** API keys have no owner.
- **Bypassable local OAuth server.** Fortemi's local OAuth server allows open dynamic registration and an identity-free consent by default, which bypasses any external IdP placed in front of it (T-22).
- **Unenforced scopes.** Single-tenant mode uses an allow-all authorization policy, so mapped scopes would not be enforced (T-07).
- **Weak redirect check.** The loopback redirect check matches by string prefix (T-19).

Current standards set clear expectations. As of MCP Authorization 2026-07-28:

- An MCP server is an OAuth 2.1 resource server and MUST publish RFC 9728 protected-resource metadata.
- It MUST validate that each token was issued for itself, per RFC 8707.
- It "MUST NOT accept or transit any other tokens".
- Clients should prefer pre-registration, then Client ID Metadata Documents, with Dynamic Client Registration deprecated.

On the CLI side, device authorization (RFC 8628) and loopback PKCE are the accepted flows. Keycloak 26.7 supports:

- the device grant;
- dynamic registration;
- standard token exchange (generally available since 26.2);
- CIMD and RFC 8707 resource indicators, behind experimental feature flags.

## Decision

1. **Fortemi is an OAuth resource server in every deployment mode.**
   - A new *external OIDC* mode validates IdP access tokens in single-tenant deployments, not only hosted multi-tenant ones. It checks issuer, audience, signature (RS256), lifetime and token type, and rejects ID tokens.
   - IdP groups or client roles map to Fortemi scopes through the existing claim policy, which becomes required, and are filtered to `read|write|admin|mcp`.
   - Scopes are **enforced** by the role-based policy. Startup refuses external OIDC combined with the allow-all policy.
   - In single-tenant deployments a configured default tenant replaces the tenant claim.
2. **API and MCP accept one user token; each has an exact resource identifier.**
   - RFC 9728 requires protected-resource metadata `resource` to match the URL the client calls, including any path.
   - The MCP resource is therefore `https://<host>/mcp`, and the API resource is `https://<host>`.
   - Fortemi accepts a configured **set** of audiences covering both, so a token a user obtained for the MCP resource is valid on the API, and vice versa when the IdP issues both audiences.
   - The MCP server **fully validates** each token before use: the API verifies it for an accepted audience and active status. It then forwards only the token presented on the current request. This preserves identity end to end, and it keeps the MCP rule that a server must not transit tokens issued for other resources.
   - When MCP runs as a separate service with its own audience, it must obtain an API-audience token through RFC 8693 token exchange, which requires a confidential MCP client at the IdP, and it fails closed without it.
   - MCP sessions are bound to the principal that opened them.
3. **Users are first-class principals.**
   - `app_user` is provisioned just-in-time from `(iss, sub)`, never from email. It stores display attributes, including `email_verified`, and the current effective scopes.
   - Requests carry the user principal.
   - Audit records capture issuer, subject, user id, credential class and client.
   - Content provenance (`created_by`/`updated_by`) and asynchronous jobs record the initiating user.
   - Service principals (client credentials) are recorded with `kind=service` and cannot act as users.
   - `GET /api/v1/me` returns the caller.
4. **Personal access tokens are bound to users.**
   - An OIDC-authenticated user can mint `mm_pat_` tokens with:
     - at least 256 bits of entropy and a detectable checksum;
     - a mandatory expiry with no sliding extension;
     - scopes that are a subset of the user's current scopes and are re-intersected on every use;
     - an HMAC-pepper hash at rest;
     - revocation and listing.
   - A PAT stops working when its user is disabled, and is suspended when the user has not signed in within a revalidation window.
   - PATs serve tools that cannot run an OAuth flow.
5. **Discovery is truthful.**
   - With an external IdP, protected-resource metadata names only the IdP issuer, and Fortemi no longer advertises its own authorization-server metadata.
   - The own-AS issuer and the external issuer are separate settings.
   - 401 and 403 responses carry RFC 6750 `WWW-Authenticate` headers with `resource_metadata` and, for insufficient scope, the required scopes.
6. **The local authorization server fails closed.**
   - When an external IdP is configured, Fortemi's own dynamic registration, identity-free consent and `mm_at_` issuance are disabled. They return only through an explicit opt-in with a real owner-authentication method.
   - Legacy `mm_at_`/`mm_key_` tokens are refused unless explicitly allowed, and then carry a scope ceiling.
   - Redirect URIs are validated with a URL parser.
7. **Client onboarding is documented and reproducible:**
   - **Browsers:** authorization code with PKCE at the IdP. A future first-party UI uses a backend-for-frontend, per the OAuth browser-based apps BCP (RFC 10017), rather than holding tokens in the browser.
   - **CLIs and agents:** loopback-redirect authorization code with PKCE as the primary interactive flow. The device grant is for headless hosts. Both use a public `fortemi-cli` client with a scope ceiling and rotated refresh tokens, and a `fortemi-login` helper keeps tokens in the OS keychain.
   - **MCP clients:** discovery through metadata. Known clients are pre-registered, CIMD is preferred where available, and locked-down DCR is the fallback.
   - **Services:** client credentials.
   - The repository ships a reference Keycloak realm and claim policy (linted in CI) and a connection guide with ready-to-paste client configuration.
8. **Front doors route bearer traffic to Fortemi.**
   - API and MCP paths are not placed behind cookie gates.
   - Gateways strip identity headers and may add JWT validation as defense in depth. With Istio, `RequestAuthentication` alone admits requests that carry no token, so it must be paired with an `AuthorizationPolicy` requiring a principal and the accepted audiences, and set `forwardOriginalToken: true` so the bearer still reaches Fortemi.
   - Hosted MCP clients (for example cloud-hosted assistants) reach both Fortemi and the IdP from public networks. Both endpoints must be publicly reachable over TLS.
   - Fortemi's own validation remains authoritative.
   - Authentication failures are rate-limited before verification.

## Consequences

- One identity reaches every layer: the same user is visible in audit, provenance and jobs, whether they arrive by browser, CLI, MCP client or PAT.
- Single-tenant deployments gain IdP integration without adopting the hosted multi-tenant stack and its route restrictions.
- Operators must supply a claim-policy file and a canonical resource URI. Misconfiguration fails at startup rather than at runtime.
- Existing `mm_` integrations keep working only with an explicit legacy flag, with a sunset decision recorded per deployment.
- Per-user data isolation inside a tenant is **not** provided. Any member with `read` sees all tenant content. This is stated in the connection guide (residual risk RR-8).
- IdP demotions reach PATs on the user's next sign-in or within the revalidation window (RR-2). Disabling a user in Fortemi revokes immediately.

## Alternatives considered

- **Gateway-injected identity headers** (oauth2-proxy `X-Forwarded-User`): rejected. They tie identity to network topology, are forgeable from any workload inside the trusted range, and don't serve non-browser clients.
- **Fortemi as the identity provider** (extending the local OAuth server with user accounts): rejected. It duplicates the organization's IdP, MFA and lifecycle, and is the source of T-22 today.
- **Hosted multi-tenant mode for every IdP deployment:** rejected for single-tenant use. It brings forced row-level security, Redis and KMS requirements, and excludes routes that single-tenant deployments need.
- **MCP as a separate resource with token exchange by default:** kept as a documented option. It is unnecessary when the API accepts both resource audiences.
- **Bare-origin single resource** (MCP at `/mcp` sharing the origin's identifier): rejected. Protected-resource metadata must match the called URL including its path, and client implementations enforce this.

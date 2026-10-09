# Design brief: end-user OIDC identity pass-through for the Fortemi API, MCP and CLI

Status: working brief (input to ADR-110, threat model, plan). Client-neutral.

## Problem (observed 2026-10-09)
A reference deployment puts Fortemi behind an oauth2-proxy cookie gate in front of a Keycloak realm.
1. A member completes OIDC login and then gets **401 from Fortemi**. The proxy admits the person but forwards no identity, and Fortemi still requires its own `mm_` bearer.
2. Programmatic clients (CLI, scripts, Claude/Cursor/VS Code MCP clients) can't use a browser cookie, so they can't get through the gate at all.
3. Fortemi never learns *who* the caller is.

## What Fortemi has today (main @ v2026.10.1)
- **External OIDC JWT validation exists only in hosted multi-tenant mode** (`FORTEMI_MULTI_TENANT=true` + `hosted-auth`):
  - RS256, issuer + audience, with JWKS from discovery;
  - a claim policy maps groups/roles to `read|write|admin|mcp`;
  - a tenant UUID claim is required;
  - realm-path issuers are accepted (#1155);
  - service clients via `azp` (fortemi-auth#52).
- Hosted mode qualifies few routes: embeddings jobs 0/27, with attachments, taxonomy and provenance excluded. It also refuses `mm_` tokens.
- Community/single-tenant mode accepts only `mm_at_` (Fortemi's own OAuth server) and `mm_key_` (API keys).
- **No user model.** The principal is `OAuthClient{client_id:"hosted-oidc", user_id:Some(sub)}`; `sub` is used for audit only. `email` is never read, and there is no users table.
- **API keys have no owner** (the `api_key` table has no owner column), and there's no personal-token minting after login.
- MCP is an RFC 9728 protected resource:
  - its `authorization_servers` is `ISSUER_URL`;
  - it forwards the user's bearer to the API per request (the same audience, `MCP_RESOURCE_URI` = `FORTEMI_AUTH_AUDIENCE`);
  - it checks for the `mcp` scope.
- The API's own `/.well-known/oauth-authorization-server` advertises `{ISSUER_URL}/oauth/*` endpoints, which don't exist on an external IdP.
- Fortemi's own OAuth server allows open DCR and an **identity-free consent** default (`FORTEMI_OAUTH_AUTHORIZE_OWNER_AUTH=none`) when self-hosted.
- No device authorization grant (RFC 8628).

## Standards baseline (2026-10)
- **MCP Authorization 2026-07-28:**
  - The MCP server is an OAuth 2.1 resource server and MUST publish RFC 9728 PRM.
  - 401 responses carry `WWW-Authenticate: Bearer resource_metadata=..., scope=...`.
  - The server MUST validate that the audience is itself (RFC 8707), and "MUST NOT accept or transit any other tokens".
  - Client registration preference: pre-registration, then CIMD (SHOULD), then DCR (deprecated, MAY).
  - Clients send `resource` and use PKCE.
  - Insufficient scope returns 403 `insufficient_scope` (step-up).
- **Keycloak 26.7 (our IdP):**
  - device grant (RFC 8628): supported;
  - standard token exchange (RFC 8693): GA since 26.2;
  - DCR: supported;
  - CIMD: experimental (`--features=cimd`);
  - RFC 8707 resource indicators: experimental (`--features=resource-indicators`). Without them, audience comes from an audience mapper on optional client scopes;
  - RFC 9207 `iss`: supported;
  - Claude Code and VS Code use localhost redirects; Claude Desktop uses a claude.ai redirect.
- **CLI/agents:** the device authorization grant against the IdP with a public client, plus refresh tokens kept in the OS keychain. No ROPC.
- **Services:** client credentials (machine principals).

## Decision direction
1. **Fortemi as a resource server in every mode.** A new "external OIDC" auth mode for community/single-tenant deployments validates IdP JWTs (issuer/audience/JWKS, claim policy for scopes) without requiring hosted multi-tenant mode. A default tenant applies when no tenant claim is configured. `mm_` tokens can stay enabled alongside it, controlled by a flag.
2. **One protected resource.** The API and MCP share one canonical resource URI and audience (e.g. `https://fortemi.example.org`, with MCP at `/mcp`). The same user token is valid at both, which complies with the MCP no-transit rule because it is the same resource. Token exchange (RFC 8693) is the documented alternative when MCP is deployed as a separate resource.
3. **A user principal.**
   - JIT-provision `app_user` from `(iss, sub)`, storing email, name, groups and last seen.
   - The request context carries the user. Audit records and content provenance (`created_by`/`updated_by`) record the user.
   - Add `GET /api/v1/me`.
   - Service principals are recorded the same way with `kind=service`.
4. **Personal access tokens bound to users.**
   - After an OIDC login, a user mints `mm_pat_` tokens bound to their user id: scoped (a subset of the user's scopes), expiring, revocable, with last used recorded and shown once.
   - Validation re-checks that the user is active.
   - They work in both modes, for API and MCP clients that can't do OAuth.
5. **Discovery correctness.**
   - With an external IdP, the PRM `authorization_servers` is the IdP issuer and `resource` is the canonical URI.
   - Fortemi stops advertising its own AS metadata.
   - The API and MCP return RFC 6750 `WWW-Authenticate` with `resource_metadata` and `scope`, and 403 `insufficient_scope`.
6. **Fail-closed own-AS.** When an external IdP is configured:
   - Fortemi's own OAuth DCR and the identity-free consent are disabled;
   - `mm_at_` issuance stops, unless explicitly re-enabled;
   - startup refuses an insecure combination.
7. **Client onboarding:**
   - **Browser/SPA:** auth code + PKCE directly against the IdP, then a bearer to Fortemi.
   - **CLI/agents:** device grant against the IdP, documented with a reference public client `fortemi-cli`, plus a small `fortemi-login` helper script that stores tokens in the keychain.
   - **MCP clients:** discover through PRM. Pre-registered clients for known tools, CIMD where enabled, DCR as fallback with locked-down policies.
   - **Services:** client credentials.
   - A reference Keycloak realm export plus claim policy, and a connection guide with ready-to-paste config.
8. **Deployment front door (reference deployment):**
   - Replace the cookie-only oauth2-proxy gate for API/MCP paths with direct bearer routing to Fortemi, which validates the JWT. The gateway can add JWT validation for defense in depth (Istio RequestAuthentication + AuthorizationPolicy requiring the issuer/audience).
   - Route `/mcp`.
   - The cookie gate remains only for future browser UI paths, or is removed.

## Non-goals (this release)
Per-user data isolation within a tenant (tenancy stays the isolation unit), SCIM provisioning, Fortemi as an IdP, a full browser UI.

# Threat Model: End-User OIDC Identity Pass-Through (API, MCP, CLI)

- **Status:** Draft for review (input to ADR-110 and the implementation plan)
- **Date:** 2026-10-09
- **Method:** STRIDE per component and per flow; risk = likelihood (1-5) x impact (1-5), max 25
- **Baseline:** `main` at v2026.10.1; `fortemi-auth` at tag `v2026.10.0` (pinned in `Cargo.toml:33-34`)
- **Scope source:** `.aiwg/working/auth-oidc-design-brief.md` (decision direction items 1-8)

## References

- @.aiwg/working/auth-oidc-design-brief.md - design brief; issue placeholders A1-A8 map to its "Decision direction" items 1-8
- @.aiwg/security/multi-tenant-threat-model.md - hosted tenancy threat model (tenancy remains the isolation unit)
- @docs/architecture/adr/ADR-071-auth-middleware.md, @docs/architecture/adr/ADR-089-authorization-policy-trait.md, @docs/architecture/adr/ADR-094-fail-closed-authentication-default.md, @docs/architecture/adr/ADR-100-mcp-scope-gate.md
- @docs/content/authentication.md - operator guide (hosted OIDC profile, claim policy, OAuth server)

### Issue placeholders

| ID | Brief decision | Short name |
|----|----------------|------------|
| A1 | 1 | External OIDC resource-server mode in every deployment mode (validation, claim policy, default tenant, legacy-token flag) |
| A2 | 2 | One protected resource: shared canonical resource URI/audience for API and MCP; MCP token handling |
| A3 | 3 | User principal: `app_user` JIT from `(iss, sub)`, request context, audit/provenance, `GET /api/v1/me`, service principals |
| A4 | 4 | Personal access tokens (`mm_pat_`) bound to users |
| A5 | 5 | Discovery correctness (PRM, AS metadata, `WWW-Authenticate`, `insufficient_scope`) |
| A6 | 6 | Fail-closed own authorization server when an external IdP is configured |
| A7 | 7 | Client onboarding (browser PKCE, device grant CLI, MCP registration, client credentials, reference realm) |
| A8 | 8 | Deployment front door (gateway bearer routing, oauth2-proxy scope, Istio JWT defense in depth) |

### Evidence conventions

- `path:line` citations below were read for this model. `fortemi-auth` paths are relative to the `v2026.10.0` checkout of `Fortemi/fortemi-auth` (`crates/...`).
- **[UNVERIFIED]** marks a claim inferred from design or third-party behavior that was not confirmed in code read for this model.
- **[CODE-READ FINDING]** marks a defect inferred from reading code that still needs a failing test to confirm.

---

## 1. Scope, assets and trust boundaries

### 1.1 In scope

Authentication and identity propagation for the Fortemi REST API (`matric-api`), the MCP server (`mcp-server`, HTTP transport), the CLI and agents, and service integrations. This covers a new "external OIDC" mode for community/single-tenant deployments and the existing hosted multi-tenant mode, the user principal, PATs, discovery metadata, the fate of Fortemi's own OAuth server, and the reference gateway. Per the brief, these are out of scope: per-user data isolation within a tenant, SCIM, Fortemi as an IdP, and a browser UI.

### 1.2 Assets

| ID | Asset | Why it matters |
|----|-------|----------------|
| AS-1 | Tenant knowledge content (notes, attachments, embeddings, provenance) | Primary confidentiality and integrity target |
| AS-2 | IdP-issued access tokens (JWT) and refresh tokens | Bearer credentials for API and MCP |
| AS-3 | Fortemi credentials: `mm_at_`, `mm_rt_`, `mm_key_`, future `mm_pat_` | Bearer credentials that bypass the IdP |
| AS-4 | `app_user` records (iss, sub, email, name, groups, status) | PII, and the anchor for PAT validity and attribution |
| AS-5 | Claim policy file and IdP role/group configuration | Decides who gets `read`, `write`, `admin` and `mcp` |
| AS-6 | JWKS and discovery metadata (IdP signing keys) | The root of trust for every JWT |
| AS-7 | Audit log and content provenance (`created_by`/`updated_by`) | Non-repudiation and incident response |
| AS-8 | MCP session state (session id to token map, active-memory selection) | Holds live bearer tokens in process memory |
| AS-9 | Service-principal client secrets | Machine access without a human |
| AS-10 | Availability of the auth path (JWKS fetch, DB token lookup) | Every request depends on it |

### 1.3 Components and trust boundaries

| ID | Component | Trust level | Boundary crossed to reach the API |
|----|-----------|-------------|-----------------------------------|
| C-BR | Browser / SPA (future UI, consent pages) | Untrusted | TB-1 internet to gateway |
| C-CLI | CLI / scripts / `fortemi-login` helper on user hosts | Semi-trusted (user device) | TB-1 |
| C-MC | MCP clients (desktop, IDE, CLI agents) | Untrusted third-party code acting for a user | TB-1 |
| C-IDP | External IdP (Keycloak realm) | Trusted for authentication only; *not* for Fortemi authorization semantics | TB-2 Fortemi to IdP (discovery/JWKS over HTTPS) |
| C-GW | Gateway / ingress / oauth2-proxy / Istio sidecars | Trusted infrastructure, but its headers are only as trustworthy as its configuration | TB-3 gateway to service network |
| C-API | `matric-api` (resource server, own OAuth AS, policy, audit) | Trusted | TB-4 API to DB |
| C-MCP | MCP server (Node, HTTP transport) | Trusted, but acts as a deputy for many users | TB-5 MCP to API (forwards user bearer) |
| C-DB | PostgreSQL (tokens, keys, users, audit, content) | Trusted | - |
| C-JW | Job worker (async embeddings, extraction) | Trusted, acts later on behalf of the job creator | TB-4 |
| C-SP | Service principals (ETL, integrations) using client credentials | Semi-trusted machine identities | TB-1 or TB-3 |

### 1.4 Data flows per client type

**DF-1 Browser auth code + PKCE.**
1. The SPA redirects to the IdP authorize endpoint with `code_challenge`, `resource` (when the IdP supports RFC 8707) and the requested scopes.
2. The IdP authenticates the user (MFA per realm) and redirects back with `code` and `iss` (RFC 9207).
3. The SPA exchanges the code at the IdP token endpoint with its verifier and gets an access token whose `aud` is the canonical Fortemi resource.
4. The SPA calls the API with `Authorization: Bearer <JWT>`. The API verifies the signature, issuer, audience, expiry, `nbf` and the claim policy.
5. The API JIT-provisions `app_user(iss, sub)`, applies the authorization policy, audits, and serves the request.

**DF-2 Device authorization grant (CLI/agents).**
1. `fortemi-login` (public client `fortemi-cli`) POSTs to the IdP device endpoint and receives `device_code`, `user_code` and `verification_uri`.
2. The user opens the URI in any browser, authenticates, enters or confirms the user code, and approves the requested scopes.
3. The CLI polls the token endpoint, receives access + refresh tokens, and stores the refresh token in the OS keychain.
4. The CLI calls the API or MCP with the access token and refreshes it as needed.

**DF-3 MCP PRM discovery.**
1. The MCP client sends a POST to `/mcp` with no token and gets 401 with `WWW-Authenticate: Bearer resource_metadata=".../.well-known/oauth-protected-resource"` (`mcp-server/index.js:5869-5873`).
2. The client fetches the PRM (`index.js:6147-6153`), reads `resource` and `authorization_servers`, then fetches the AS (IdP) metadata.
3. The client registers by pre-registration, CIMD, or DCR (fallback), then runs auth code + PKCE with `resource`, using a localhost or vendor redirect.
4. The client calls MCP with the bearer. MCP validates it through the API's `GET /api/v1/auth/token-info` (`mcp-server/lib/bearer-validation.js:53-73`) and then forwards the same bearer to the API on every tool call (`index.js:151-152`).

**DF-4 Personal access token.**
1. The user authenticates through DF-1 or DF-2 and calls a PAT mint endpoint with a fresh OIDC token, a name, the requested scope subset, and an expiry.
2. The API generates the random token, stores only its keyed hash with the `user_id`, and returns the value once.
3. A script or MCP client presents `Bearer mm_pat_...`. The API looks up the hash and checks that the PAT is unrevoked and unexpired and that `app_user.status = active`, then intersects the PAT scopes with the user's current scopes.

**DF-5 Client credentials (services).**
1. A service authenticates to the IdP token endpoint with its client secret or private-key JWT and receives an access token (`azp` = service client, `sub` = service account).
2. The service calls the API. The claim policy classifies the caller as `PrincipalKind::Service` because `azp` is in `clients.service` (`fortemi-auth crates/fortemi-auth-core/src/claim_policy.rs:215-233`).
3. The API records an `app_user` with `kind=service`.

**DF-L (legacy, existing) Fortemi's own OAuth server.** DCR at `/oauth/register` (`crates/matric-api/src/main.rs:23163-23209`), consent at `/oauth/authorize` (`oauth_consent`), and the token endpoint issuing `mm_at_`/`mm_rt_`. API keys `mm_key_` are created by admins. This flow bypasses the IdP entirely.

---

## 2. STRIDE analysis

Columns: **L** likelihood, **I** impact, **R** risk (L x I). "Existing control" cites code read for this model. "Required control" lists the SR numbers from section 3.

### 2.1 Token validation (C-API, TB-2): DF-1 to DF-5

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-01 | S | **Audience confusion.** A token minted for another relying party in the same realm is accepted, for example an OIDC ID token whose `aud` equals a client id the operator used as `FORTEMI_AUTH_AUDIENCE`, or a multi-audience access token. | 3 | 4 | 12 | Audience is mandatory (`hosted_auth.rs:50-52`) and the verifier is built with one audience (`fortemi-auth-clerk/src/lib.rs:674-678`). The header parser reads only `alg` and `kid` (`lib.rs:686-698`): **no `typ` check** for `at+jwt` (RFC 9068) and no ID-token marker check. Whether xjp-oidc rejects multi-valued `aud` that *contains* the audience is [UNVERIFIED]. | SR-1, SR-2, SR-3 | A1/A2 |
| T-02 | S/E | **Token passthrough between MCP and the API.** MCP forwards the user's bearer to the API (`index.js:151-152`). This is compliant only while MCP and the API are the same resource. MCP does no audience check itself and relies on the API (`bearer-validation.js:53-73`). A mismatch between `MCP_RESOURCE_URI` and the audience only logs a warning (`index.js:6155-6158`). `MCP_RESOURCE_URI` defaults to `http://localhost:<port>` (`index.js:98,101`). | 3 | 4 | 12 | API token-info runs the full hosted verification (`bearer-validation.js:1-5`, `main.rs:5190-5194`). External tokens that fail are always rejected (`bearer-validation.js:103-107`). | SR-4, SR-5, SR-6 | A2 |
| T-03 | S/R | **Issuer mix-up (RFC 9207) and conflated issuer config.** `ISSUER_URL` is both Fortemi's own AS issuer (`main.rs:2506-2532`, used in AS metadata `main.rs:23098-23111` and PRM `main.rs:23136-23143`) *and* the external IdP issuer the verifier trusts (`hosted_auth.rs:47-49`). The MCP PRM advertises `ISSUER_URL \|\| API_BASE` (`index.js:6150`), and MCP proxies the API's AS metadata, which lists `{ISSUER_URL}/oauth/*` endpoints that do not exist on the IdP (`index.js:6133-6141`). A client can be steered to the wrong AS or confuse codes between two AS. | 3 | 4 | 12 | Exact single-issuer map (`fortemi-auth-clerk/src/lib.rs:674-677`). HTTPS issuer without userinfo (`lib.rs:271-279`, tests `452-458`). The own AS metadata sets `authorization_response_iss_parameter_supported: true` (`main.rs:23121`); whether `/oauth/authorize` actually emits `iss` is [UNVERIFIED]. | SR-7, SR-8, SR-9 | A5/A6 |
| T-04 | S/D | **JWKS rotation and cache poisoning.** An attacker presents tokens with random `kid` values to force JWKS refetches, or poisons JWKS through DNS/TLS interception. A rotation gap rejects valid tokens. | 3 | 3 | 9 | HTTPS-only discovery and JWKS transport (`fortemi-auth-clerk/src/lib.rs:462-468`). An explicit CA bundle never falls back to default trust (`hosted_auth.rs:201-214`). Bounded TTL-honoring cache (`fortemi-auth-clerk/src/lib.rs:36-104`) with rotation-overlap test (`lib.rs:597`). JWKS outages are mapped to `JwksUnreachable`, not to invalid-token (`lib.rs:759-769`). Unknown-`kid` refetch throttling and single-flight in xjp-oidc 1.1.0 are [UNVERIFIED]. | SR-10, SR-11 | A1 |
| T-05 | S/E | **Algorithm confusion / `alg: none`.** HS256 with the public key as secret, `none`, or duplicate header members. | 1 | 5 | 5 | RS256 only, rejected before network (`fortemi-auth-clerk/src/lib.rs:1-5, 692-698`). Tests reject `none`, HS256, RS384, PS256, ES256, duplicate `alg`/`kid`, and a missing `kid` (`lib.rs:526-548`). JWK `kty`/`use` checks are [UNVERIFIED]. | SR-12 | A1 |
| T-06 | S | **Clock skew and long-lived tokens.** Expired tokens are accepted under large skew, or an IdP issues multi-hour access tokens that outlive deactivation. | 2 | 3 | 6 | Skew bounded 0-60 s, default 60 (`hosted_auth.rs:56-57`). `nbf` enforced (`fortemi-auth-clerk/src/lib.rs:740-753`, tests `556-576`). `exp` and future-`iat` are mapped from verifier errors (`lib.rs:771-774`). No maximum-lifetime check. | SR-13 | A1 |

### 2.2 Authorization and claim policy (C-API, C-IDP config)

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-07 | E | **Scopes are not enforced in community mode.** `authorization_policy_for_mode` returns `AllowAllPolicy` whenever `multi_tenant` is false (`main.rs:2859-2864`). If the new external-OIDC community mode reuses this, claim-mapped scopes decide nothing: a `read`-mapped IdP user can write and administer. ADR-089 records that many endpoints do not check scope themselves. | 4 | 5 | **20** | `RoleBasedPolicy` in hosted mode (`main.rs:2860-2861`). Operator docs routes always use `RoleBasedPolicy` (`main.rs:10037-10040`). | SR-14 | A1 |
| T-08 | E | **Claim-policy privilege escalation.** (a) With no policy file, `ScopeSource::Token` passes the token's `scope` claim through (`claim_policy.rs:138, 203-204`; `hosted_claim_policy.rs:17-20`). In Keycloak, any client allowed to request a client scope named `admin` or `write` yields that scope for any user. (b) `scope_source=union` merges token scopes with mapped scopes (`claim_policy.rs:205-206`), so it inherits (a). (c) The vocabulary filter applies only to mapping rules (`hosted_claim_policy.rs:40-50`), not to token-sourced scopes, so e.g. `system:*` in a token scope is not stripped (downstream effect [UNVERIFIED]). (d) Group names: Keycloak group mappers without "full path" emit bare names, so a nested group with a colliding name grants the same scope. | 3 | 5 | **15** | Mapping values are exact and case-sensitive (`claim_policy.rs:81, 243`). Missing claims yield no scopes (`claim_policy.rs:252-268`). Wrong-shape claims fail (`claim_policy.rs:216-219, 270-274`). Mapping to an unknown scope fails startup (`hosted_claim_policy.rs:40-50`). Policy errors do not echo values (tests `hosted_claim_policy.rs:97-124`). Client allow-list via `azp` (`claim_policy.rs:222-227`). | SR-15, SR-16, SR-17 | A1/A7 |
| T-09 | E/I | **Cross-tenant access via the tenant claim.** A user-editable IdP attribute feeds the tenant claim, or the new default-tenant mode silently accepts a token that carries a tenant claim for another tenant. | 2 | 5 | 10 | The tenant claim must be a string (`fortemi-auth-clerk/src/lib.rs:808-812`). The tenant registry lookup fails closed on unknown status (`hosted_auth.rs:153-161`). A verified tenant is injected and gates hosted routes (`main.rs:9839-9864`). A resource tenant is preserved to detect mismatch (`main.rs:10067-10072`). | SR-18, SR-19 | A1 |

### 2.3 User principal and JIT provisioning (C-API, C-DB): A3

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-10 | S | **JIT provisioning abuse and email spoofing across IdPs.** Users are keyed or linked by email. A brokered upstream IdP, self-registration, or an unverified email change in the realm yields the same email as a victim, which leads to account takeover or misattribution. Mass self-registration floods `app_user`. | 3 | 4 | 12 | None today: there is no users table and the principal is `client_id:"hosted-oidc", user_id:Some(sub)` (`main.rs:9972-9977`). `email` is never read (brief). | SR-20, SR-21, SR-22 | A3 |
| T-11 | S/E | **Service principal impersonating a user.** Any token whose `azp` is in `clients.service` is labeled Service (`claim_policy.rs:221, 228-232`), including human flows on that client. A service allowed RFC 8693 impersonation in Keycloak mints user tokens for the Fortemi audience. A service mints PATs. | 2 | 4 | 8 | Service classification is by an explicit list only (`claim_policy.rs:215-233`; test `hosted_claim_policy.rs:76-94`). | SR-23, SR-24 | A3/A7 |
| T-12 | R/T | **Job worker attribution.** Async jobs run later under worker privileges. `created_by` taken at execution time, or editable in the job row, misattributes content. | 2 | 3 | 6 | [UNVERIFIED]: job payload attribution was not reviewed. | SR-25 | A3 |

### 2.4 Personal access tokens (C-API, C-DB, C-CLI): A4

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-13 | S/I | **PAT theft** from shell history, CI logs, MCP client config files, or screen sharing. | 3 | 4 | 12 | Existing API keys are shown once (`docs/content/authentication.md:42`). | SR-26, SR-27 | A4 |
| T-14 | E | **PAT scope escalation beyond the user's grant.** A PAT minted with `admin` by a non-admin; a PAT used to mint more PATs; a user demoted at the IdP keeps old PAT scopes because Fortemi only sees group changes on the next OIDC request. | 3 | 4 | 12 | The analogous consent ceiling `Owner::covers` (`oauth_consent/owner.rs:25-35`) treats `admin` as a wildcard, and trusted-header owners have **no** ceiling (`owner.rs:21, 52-55`). Neither pattern is suitable for PATs as-is. | SR-28, SR-29, SR-30 | A4 |
| T-15 | E/R | **Revocation lag; a deactivated user's PAT still works.** The existing `mm_at_` validation slides expiry forward on every use (`main.rs:9922-9928`; `crates/matric-db/src/oauth.rs:539-550`), so an actively used token never expires. `api_key` has no owner column (`migrations/20260128000000_oauth.sql:78-93`), so disabling a person cannot revoke their keys. | 4 | 4 | **16** | Revoked and expired filters on lookup (`oauth.rs:610-612, 871-873`). | SR-31, SR-32, SR-33 | A4/A3 |
| T-16 | I/T | **PAT hashes at rest.** A DB read or backup leak; a DB write-capable attacker inserts a hash of a token they chose. | 2 | 4 | 8 | Existing secrets are stored as unsalted SHA-256 hex and looked up by hash (`oauth.rs:39-44, 600-618, 861-879`). `verify_secret` compares hex strings with `==` (`oauth.rs:47-48`, not constant-time; low impact for high-entropy values). | SR-34 | A4 |

### 2.5 MCP server (C-MCP, TB-5): A2

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-17 | E/S | **Confused deputy and session hijack in MCP.** (a) SSE `/messages` and Streamable `GET /` run tool calls with `session.token`, the token captured when the session opened (`index.js:5942, 5976, 6102`), not the token on the current request. Any caller who passes `validateToken` with their own token (or with none when `requireAuth` is false) and knows a session id acts with the session owner's token. (b) `POST /` on an existing session uses the caller's token but never checks that the caller is the session creator (`index.js:5994-5998, 6056`), so per-session state such as the active memory (`index.js:157-160`) leaks across users. (c) `apiRequest` falls back to the server's `FORTEMI_API_KEY` when no session token is present (`index.js:151-155`). With `requireAuth` false, an invalid Fortemi-format token is not rejected (`bearer-validation.js:103-107`), so the call runs as the deployment key. (d) Session ids are logged in clear (`index.js:5941, 5962, 5984, 6013`) and carried in the SSE query string (`index.js:5961`). | 4 | 4 | **16** | Session ids are random UUIDs (`index.js:6022`). Authentication is required by default and anonymous mode is refused in multi-tenant (`index.js:5901-5909`). | SR-35, SR-36, SR-37 | A2 |
| T-18 | E | **Inconsistent MCP admission.** Fortemi tokens are admitted with `mcp\|read\|admin` (`bearer-validation.js:47`), external tokens with `mcp\|admin` (`bearer-validation.js:69`). PATs (a new class) could be misclassified as "external" because the prefix list knows only `mm_at_`/`mm_key_` (`bearer-validation.js:7, 12-16`). | 3 | 2 | 6 | Refresh tokens are refused as bearers (`bearer-validation.js:13, 86`). Introspection type check (`bearer-validation.js:41-44`). | SR-38 | A2/A4 |

### 2.6 Client registration, device grant, CLI storage (C-IDP, C-CLI, C-MC, own AS): A6/A7

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-19 | S/I | **DCR/CIMD abuse and redirect URI hijack.** (a) Fortemi's own DCR defaults to `enabled` (open) outside hosted mode (`oauth_registration.rs:40-44`), and registration does not validate `redirect_uris` (`main.rs:23178-23201` checks grant types, scopes and auth method only). (b) **[CODE-READ FINDING]** `validate_redirect_uri` accepts any URI that *starts with* `http://localhost:` or `http://127.0.0.1:` whose path matches a registered loopback path (`main.rs:23663-23700`). `http://localhost:1234@attacker.example/cb` passes, because userinfo is not parsed and the host is `attacker.example`, so the authorization code is delivered off-host. (c) Keycloak anonymous DCR without client-registration policies lets anyone create a client with arbitrary redirects and scopes. (d) Loopback redirects let any local process on the user's host catch a code (accepted by RFC 8252 when PKCE is enforced). | 3 | 5 | **15** | Hosted mode refuses open DCR (`oauth_registration.rs:51-56`). The `admin` mode requires an admin bearer (`oauth_registration.rs:5-7`). Registration access tokens are not returned (`main.rs:23203-23206`). Exact match is tried first (`main.rs:23664-23667`). | SR-39, SR-40, SR-41 | A6/A7 |
| T-20 | S | **Device-code phishing.** An attacker starts a device flow with the public `fortemi-cli` client and socially engineers a member into approving the user code. The attacker receives access and refresh tokens. | 3 | 4 | 12 | None (no device grant today). | SR-42, SR-43 | A7 |
| T-21 | I | **Refresh-token storage on CLI hosts.** Plaintext token files on shared hosts, CI runners, or synced home directories; long-lived offline tokens. | 3 | 4 | 12 | None (no CLI login today). The docs give token-storage guidance (`docs/content/authentication.md:1140`). | SR-44, SR-45 | A7 |
| T-22 | S/E | **Identity-free consent of Fortemi's own OAuth server when an external IdP is enabled.** The owner-auth default is `none` (approve-only) outside hosted mode (`oauth_consent/config.rs:3-8, 124-127`; `mod.rs:3-7`). Combined with open DCR (T-19a), anyone who can reach `/oauth/authorize` obtains an `mm_at_` token, bypassing IdP authentication, MFA, the claim policy and deactivation. Trusted-header owners have no scope ceiling (`owner.rs:21, 52-55`). | 4 | 5 | **20** | Hosted mode forces `disabled` (`config.rs:124-134`). `trusted_header` requires trusted CIDRs (`config.rs:135-140`) and is honored only from trusted peers (`owner.rs:38-56`, tests `94-121`). `none` is logged as a warning at startup (`config.rs:4-6`). | SR-46, SR-47 | A6 |
| T-23 | E/S | **Downgrade to `mm_` tokens when both are enabled.** `mm_at_`/`mm_key_` are dispatched by prefix before any OIDC check (`main.rs:9916-9957`). A legacy key (ownerless) or an `mm_at_` minted via T-22 bypasses IdP controls. A `mm_key_` typed into the consent page can mint `mm_at_` tokens (`owner.rs:58-66`). | 3 | 4 | 12 | Hosted mode refuses `mm_` tokens (`main.rs:9917-9919, 9943-9945`). | SR-48, SR-49 | A1/A4/A6 |

### 2.7 Gateway and front door (C-GW, TB-1, TB-3): A8

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-24 | S | **Forged `X-Forwarded-*` or identity headers through the gateway.** In a mesh, the immediate peer is a sidecar or a shared ingress CIDR, so any workload in a trusted CIDR can forge `X-Forwarded-Email`/`X-Forwarded-User` (the `trusted_header` owner) or `X-Forwarded-Proto`/`Host` (which affects issued URLs). oauth2-proxy may pass client-supplied identity headers through if it is not configured to strip them. | 3 | 5 | **15** | Forwarded headers are honored only from `FORTEMI_TRUSTED_PROXY_CIDRS` peers (`trusted_proxy.rs:131-160`). Universal networks are rejected and the list is capped at 64 (`trusted_proxy.rs:10, 58-79`). RFC 7239 `Forwarded` is refused (`trusted_proxy.rs:150-154`). | SR-50, SR-51 | A8 |
| T-25 | S/T | **An oauth2-proxy cookie gate coexisting with bearer routes.** If the proxy converts the session cookie into an `Authorization` header on API/MCP paths (pass-access-token, set-authorization-header) and the proxy's client is also given the Fortemi audience, a cross-site request carrying the cookie becomes an authenticated bearer mutation (CSRF via the proxy). Path-split mistakes expose bearer routes without either control. MCP sets CORS `origin: '*'` with `Authorization` allowed (`index.js:5848-5853`). That is acceptable for header bearers but not if credentials are ever cookie-derived. | 3 | 4 | 12 | None in Fortemi (gateway-side). The API requires a bearer and does not read cookies for auth (`main.rs:9811-9825`). | SR-52, SR-53 | A8 |
| T-26 | D | **DoS on the auth path and JWKS fetch.** `auth_middleware` is the outermost layer (`main.rs:5262-5265`), so the rate limiter (`main.rs:5242-5245`) never sees rejected requests. Invalid-token floods are unlimited. The community limiter is global, not per client (`main.rs:9153-9162`). Each `mm_` token causes a DB SELECT + UPDATE (`oauth.rs:600-628, 881-891`). MCP calls the API token-info on every request with no cache (`bearer-validation.js:53-59`). | 3 | 3 | 9 | Hosted per-tenant quota gate (`main.rs:9174-9176, 9185-9200`). Bounded verifier HTTP timeout 1-30 s (`hosted_auth.rs:60-61`). MCP verify timeout of 5 s (`bearer-validation.js:9`). | SR-54, SR-55, SR-10 | A1/A8 |

### 2.8 Audit, logging and data at rest (C-API, C-DB, C-MCP): A3

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-27 | R/T | **Audit-log integrity and repudiation.** The community audit sink is `TracingSink`, which is not durable (`main.rs:2867-2873`). OAuth audit events are `BestEffort` (`main.rs:23069, 23092`). The hosted principal records `sub` without `iss` (`main.rs:9972-9977`), so subjects from different issuers could collide. No user or PAT lifecycle events exist. | 3 | 3 | 9 | Durable Postgres sink, health-checked at startup in hosted mode (`main.rs:2875-2879`). Claim-policy audit context applied in hosted mode (`main.rs:10026-10030`). Audit events are sanitized and record credential *length*, not value (`main.rs:23079-23094`). | SR-56, SR-57, SR-58 | A3 |
| T-28 | I | **Logging of tokens or PII.** New fields (email, name, groups) enter logs and metrics. MCP debug output logs session ids (`index.js:147-149`), which act as bearer handles under T-17. | 3 | 3 | 9 | Token values are never logged by MCP bearer validation (`bearer-validation.js:1-5`). The hosted authenticator logs only success booleans (`hosted_auth.rs:173-177`). CA and policy errors redact paths and values (`hosted_auth.rs:201-202`, tests `241-258`; `hosted_claim_policy.rs:3-6`). token-info does not echo the subject (`handlers/token_info.rs` test at line 144). | SR-59, SR-60 | A3/A2 |
| T-29 | I | **`app_user` PII and PAT hashes exposed via DB or backup.** | 2 | 3 | 6 | [UNVERIFIED]: DB encryption-at-rest and backup encryption were not reviewed for this model. | SR-61 | A3/A4 |

### 2.9 Discovery and error signaling (C-API, C-MCP): A5

| ID | STRIDE | Threat | L | I | R | Existing control (evidence) | Required control | Owner |
|----|--------|--------|---|---|---|-----------------------------|------------------|-------|
| T-30 | S/D | **Discovery steering.** The API publishes its own AS metadata and a PRM whose `resource` and `authorization_servers` are both `ISSUER_URL` (`main.rs:23125-23150`). With an external IdP this is wrong (see T-03), and clients cannot discover the IdP. The API emits no `WWW-Authenticate` header on 401/403 (`problem_response` calls with `None` at `main.rs:9874-9894`; no `WWW-Authenticate` occurrence in `main.rs`), so step-up via `insufficient_scope` is impossible against the API directly. | 3 | 3 | 9 | MCP already sends RFC 9728 `resource_metadata` on 401 and `insufficient_scope` on 403 (`index.js:5869-5882`). | SR-7, SR-62, SR-63 | A5 |

### 2.10 Risk ranking (top items)

| Rank | ID | Risk | Score |
|------|----|------|-------|
| 1 | T-07 | `AllowAllPolicy` in community mode makes claim-mapped scopes meaningless | 20 |
| 2 | T-22 | The own AS's identity-free consent plus open DCR bypasses the IdP | 20 |
| 3 | T-15 | No revocation path for a deactivated user; sliding `mm_at_` expiry; ownerless keys | 16 |
| 4 | T-17 | MCP confused deputy: session-token reuse, no session-to-principal binding, API-key fallback | 16 |
| 5 | T-08 | Claim-policy escalation (token scope pass-through, union, no vocabulary filter on token scopes) | 15 |
| 6 | T-19 | DCR redirect hijack, including the loopback userinfo bypass in `validate_redirect_uri` | 15 |
| 7 | T-24 | Forged forwarded and identity headers via shared trusted CIDRs | 15 |
| 8 | T-01/T-02 | Audience confusion and MCP-to-API token passthrough | 12 |

---

## 3. Security requirements

Each requirement is testable. "Verify by" names the negative test that must exist (see section 5).

### A1: External OIDC mode, validation and claim policy

- **SR-1** In every mode where external JWTs are accepted, `FORTEMI_AUTH_AUDIENCE` MUST be the canonical resource URI (absolute `https` URI). Startup MUST refuse an audience that equals any client id listed in the claim policy `clients.allowed`/`clients.service`. *Verify by:* config test fails startup for `audience = "fortemi-web"` when `fortemi-web` is an allowed client.
- **SR-2** The verifier MUST reject tokens that carry ID-token markers (`nonce`, `at_hash`, `c_hash`) and, when `typ` is present, any `typ` other than `at+jwt`/`JWT`. *Verify by:* signed fixtures of an ID token and of `typ: id+jwt` both return 401.
- **SR-3** The verifier MUST reject a token whose `aud` is an array containing values other than the canonical audience, unless `FORTEMI_AUTH_ALLOW_MULTI_AUDIENCE=true` is set explicitly. When it accepts one, `azp` MUST be present and allowed. *Verify by:* the fixture `aud: [fortemi, other-rs]` returns 401 by default.
- **SR-10** The JWKS refetch for an unknown `kid` MUST be single-flight and throttled (at most one fetch per issuer per 30 s). Unknown kids MUST be negative-cached for the throttle window. A JWKS document MUST be capped (for example 64 keys, 256 KiB). *Verify by:* 1,000 requests with random `kid` cause at most 1 outbound JWKS fetch in 30 s (mock HTTP counter).
- **SR-11** On a JWKS fetch failure, the verifier MUST keep using the last good JWKS for a bounded grace period (at most 1 h) and MUST return 503 (not 401) once the grace period has passed. *Verify by:* mock IdP outage test.
- **SR-12** Fortemi-level integration tests MUST assert that `none`, HS256 (signed with the RSA public key bytes), and a JWK with `use=enc` or `kty!=RSA` are rejected. *Verify by:* integration test through `auth_middleware`.
- **SR-13** The verifier MUST reject access tokens with `exp - iat` greater than `FORTEMI_AUTH_MAX_TOKEN_LIFETIME_SECONDS` (default 3600, max 86400). *Verify by:* fixture with a 24 h lifetime is rejected at the default.
- **SR-14** Any mode that admits external JWTs or PATs MUST use a scope-enforcing authorization policy (`RoleBasedPolicy` or a successor). Startup MUST refuse `AllowAllPolicy` combined with external OIDC or PATs. *Verify by:* a community external-mode test where a `read`-only principal gets 403 on `POST /api/v1/notes`, plus a startup refusal test.
- **SR-15** In external OIDC mode, `FORTEMI_AUTH_CLAIM_POLICY_FILE` MUST be required. `scope_source=token` and `scope_source=union` MUST additionally require `FORTEMI_AUTH_ALLOW_TOKEN_SCOPES=true`. *Verify by:* startup refusal tests for each case.
- **SR-16** The final scope set (after token, mapping or union evaluation) MUST be filtered to the mappable vocabulary (`read`, `write`, `admin`, `mcp`). Any other value MUST be dropped and counted in a metric. *Verify by:* a token with `scope: "read system:admin"` yields only `read`.
- **SR-17** The reference claim policy MUST map from client roles of the Fortemi client (or full group paths), never from bare group names. The connection guide MUST say why. *Verify by:* reference-realm lint test: the policy claim path is `resource_access.<client>.roles` or the group mapper has `full.path=true`.
- **SR-18** In default-tenant mode (no tenant claim configured), a token that *does* carry the configured tenant claim name MUST be rejected unless it equals the default tenant id. Default-tenant mode MUST be refused when `FORTEMI_MULTI_TENANT=true`. *Verify by:* fixture with a foreign tenant claim returns 401; startup refusal test.
- **SR-19** The reference realm MUST source the tenant claim from a protocol mapper on an admin-managed attribute or group, never from a user-editable attribute. *Verify by:* reference-realm lint test (user profile attribute has `edit: [admin]`).
- **SR-48** In external OIDC mode, `mm_at_` and `mm_key_` MUST be refused unless `FORTEMI_AUTH_ALLOW_LEGACY_TOKENS=true`. When they are allowed, they MUST be audited with `credential_class=legacy` and capped by `FORTEMI_AUTH_LEGACY_SCOPE_CEILING` (default `read mcp`). *Verify by:* an `mm_key_` with `admin` scope returns 401 by default and is capped when legacy tokens are enabled.

### A2: One protected resource, MCP token handling

- **SR-4** The MCP server MUST refuse to start in HTTP transport when `FORTEMI_AUTH_AUDIENCE` is set and differs from `MCP_RESOURCE_URI` (today this only warns at `index.js:6155-6158`), and MUST refuse a non-`https` `MCP_RESOURCE_URI` outside explicit local-dev mode. *Verify by:* startup test exits non-zero.
- **SR-5** MCP MUST forward to the API only tokens that the API's token-info reported as active for the canonical audience. MUST NOT forward any other inbound credential. *Verify by:* a token rejected by token-info never appears in an outbound API request (fetch spy).
- **SR-6** When MCP is deployed as a separate resource, it MUST use RFC 8693 token exchange to obtain an API-audience token and MUST NOT forward the inbound token. This MUST be a documented configuration that fails closed if exchange is unconfigured. *Verify by:* config test.
- **SR-35** Each MCP session MUST be bound at creation to a principal fingerprint (hash of `iss` + `sub`, or the PAT id). Every request on that session (POST, GET, DELETE, `/messages`) MUST present a token whose fingerprint matches, or receive 403. *Verify by:* user B's valid token with user A's session id returns 403 on all four routes.
- **SR-36** Tool calls MUST use the token presented on the *current* request, never a token stored at session creation. The HTTP transport MUST NOT fall back to `FORTEMI_API_KEY` for user requests. *Verify by:* a fetch spy shows the current request's bearer. With `FORTEMI_API_KEY` set and no request token, the outbound call carries no `Authorization` and the API returns 401.
- **SR-37** MCP session ids MUST NOT be logged in clear (log a truncated hash), and MUST NOT be accepted from the query string for new deployments (header only; keep the SSE legacy route behind a flag). *Verify by:* log-capture test contains no full session id.
- **SR-38** MCP admission MUST apply one rule to every credential class (`mcp` or `admin`), and MUST classify `mm_pat_` as a Fortemi credential. *Verify by:* table-driven test over `mm_at_`, `mm_key_`, `mm_pat_` and external with `read`-only scope, expecting 403 `insufficient_scope` for all of them.

### A3: User principal, audit, logging

- **SR-20** `app_user` MUST have a unique key `(iss, sub)` with `iss` stored as the exact issuer string. Email MUST be display-only: it is never used for lookup, linking, authorization or uniqueness. *Verify by:* two tokens from different issuers with the same email create two users. A token with the same `(iss, sub)` and a changed email updates the same user.
- **SR-21** JIT provisioning MUST occur only after full token validation and a claim-policy decision granting at least one scope. `sub` MUST be at most 255 bytes with no control characters. Creation MUST be rate-limited (default 60 new users/min per deployment). *Verify by:* a valid token with zero mapped scopes returns 403 and creates no row; a creation burst over the limit returns 429.
- **SR-22** `app_user` MUST store `email_verified`. If `email_verified` is not true, `/me` and the audit display MUST mark the email as unverified. *Verify by:* `/me` response test.
- **SR-23** `kind=service` MUST require both `azp` in `clients.service` and a token obtained via client credentials (Keycloak: `client_id` claim present and `preferred_username` with the `service-account-` prefix, or a configured marker claim). A human-flow token from a service-listed client MUST be rejected. *Verify by:* fixture of a human token with a service `azp` returns 401.
- **SR-24** Service principals MUST NOT mint PATs or create users, and the reference realm MUST disable token-exchange impersonation for the Fortemi audience. *Verify by:* a service token on the PAT mint endpoint returns 403; reference-realm lint.
- **SR-25** Async jobs MUST record the initiating `app_user.id` at enqueue time, and the worker MUST write provenance from that field, not from the worker identity. *Verify by:* a job created by user A is attributed to A in `created_by` after worker execution.
- **SR-56** Any mode with external OIDC or PATs MUST use a durable audit sink. Startup MUST refuse `TracingSink` in that mode. *Verify by:* startup refusal test.
- **SR-57** Audit records for authenticated requests MUST include `iss`, `sub`, `app_user.id`, `kind`, `azp`, `credential_class` (`oidc` or `pat` or `legacy`), the PAT id when applicable, and `jti` when present. User lifecycle and PAT mint/revoke events MUST use a fail-closed audit policy. *Verify by:* audit row assertions; the PAT mint returns 503 when the sink is unavailable.
- **SR-58** The audit table MUST be append-only for the application DB role (no UPDATE or DELETE grants). *Verify by:* a migration test asserts that the role privileges lack UPDATE/DELETE on the audit table.
- **SR-59** A log redaction layer MUST mask `mm_*` tokens and JWT-shaped strings (`eyJ...\.eyJ...\.`) in all log output. *Verify by:* a log-capture test that emits a sample token in an error path.
- **SR-60** Email, name and group values MUST NOT appear in logs, traces or metric labels. *Verify by:* a log/trace capture test during JIT provisioning.
- **SR-61** Operator docs MUST require encryption at rest for the DB volume and backups that contain `app_user` and token hashes. *Verify by:* doc checklist item in the release gate.

### A4: Personal access tokens

- **SR-26** A PAT MUST be `mm_pat_` + at least 256 bits of CSPRNG output with a checksum suffix that secret scanners can detect. It MUST be returned exactly once. *Verify by:* the format test, and a second GET of the PAT shows only its prefix/last4.
- **SR-27** A PAT MUST have a mandatory expiry (default 30 days, max 365 days). There MUST be no sliding extension on use. `last_used_at` and the last client IP MUST be recorded. *Verify by:* minting without an expiry gets the default; a PAT used repeatedly keeps its original `expires_at`.
- **SR-28** PAT minting MUST require an OIDC-authenticated request (not a PAT, `mm_at_` or `mm_key_`). The requested scopes MUST be a subset of the caller's current effective scopes. *Verify by:* minting with a PAT returns 403; a `read` user requesting `write` gets 403.
- **SR-29** At use time, the effective PAT scope MUST be `pat.scopes ∩ app_user.current_scopes`, where `current_scopes` is refreshed on every OIDC-authenticated request. *Verify by:* a user demoted (the next OIDC request carries only `read`) finds their `write` PAT limited to `read`.
- **SR-30** A PAT whose user has not been seen via OIDC for `FORTEMI_PAT_REVALIDATE_DAYS` (default 30) MUST be suspended until the user signs in again. *Verify by:* a clock-advanced test.
- **SR-31** PAT validation MUST check `app_user.status = active` on every request, with no cache or a cache of at most 60 s. *Verify by:* user disabled, then within 60 s their PAT returns 401.
- **SR-32** Admins MUST be able to disable a user, which revokes all of the user's PATs atomically and is audited. Users MUST be able to list and revoke their own PATs. *Verify by:* API tests.
- **SR-33** When legacy tokens are enabled in external mode, `mm_at_` sliding extension (`main.rs:9922-9928`) MUST be disabled or capped at an absolute maximum lifetime. *Verify by:* an `mm_at_` used continuously expires at its absolute cap.
- **SR-34** PAT hashes MUST be HMAC-SHA-256 with a server-side pepper held outside the DB (KMS or secret store; key derived with HKDF under a distinct `info` label, per the no-key-reuse rule). Lookup is by the HMAC value. *Verify by:* a row inserted with a plain SHA-256 of a known token fails validation.

### A5: Discovery

- **SR-7** With an external IdP configured, the PRM (API and MCP) MUST return `resource` = the canonical URI and `authorization_servers` = [the IdP issuer] only. The API's `/.well-known/oauth-authorization-server` MUST return 404, and MCP MUST NOT proxy it. *Verify by:* discovery contract tests in both services.
- **SR-8** The own-AS issuer and the external issuer MUST be separate settings (for example `FORTEMI_OAUTH_ISSUER` and `FORTEMI_AUTH_ISSUER`). Startup MUST refuse an own-AS issuer equal to the external issuer. *Verify by:* startup refusal test.
- **SR-9** If the own AS stays enabled, `/oauth/authorize` responses MUST include `iss` (RFC 9207) to match the advertised metadata. *Verify by:* redirect contains `iss=<own issuer>`.
- **SR-62** The API MUST return `WWW-Authenticate: Bearer resource_metadata="<PRM URL>"` on 401, and `error="insufficient_scope", scope="<needed>"` on scope-based 403. *Verify by:* header assertions on both statuses.
- **SR-63** `scopes_supported` in the PRM MUST list exactly the mappable vocabulary. *Verify by:* contract test.

### A6: Fail-closed own authorization server

- **SR-46** When an external IdP is configured, startup MUST force: own-AS owner auth `disabled`, DCR `disabled`, and no `mm_at_` issuance at `/oauth/token`. These are re-enabled only by `FORTEMI_OAUTH_ALLOW_LOCAL_AS=true` together with a non-`none` owner-auth method. *Verify by:* external mode plus unset settings yields a 403 or `access_denied` from register, authorize and token. Startup refuses `ALLOW_LOCAL_AS=true` with owner auth `none`.
- **SR-47** A `trusted_header` owner MUST carry a scope ceiling (`FORTEMI_OAUTH_OWNER_HEADER_SCOPES`, default `read mcp`), not unlimited. *Verify by:* a trusted-header owner requesting `admin` is denied.
- **SR-49** The consent page MUST NOT accept `mm_key_` as owner proof when an external IdP is configured. *Verify by:* owner auth with an API key is rejected in external mode.
- **SR-39** `validate_redirect_uri` MUST parse URIs with a URL parser. The loopback exception MUST apply only when the scheme is `http`, there is no userinfo, there is no fragment, and the host is exactly `localhost`, `127.0.0.1` or `[::1]`. Path and query MUST match the registered value. *Verify by:* `http://localhost:1234@attacker.example/cb`, `http://localhost.attacker.example:1/cb` and `http://127.0.0.1:1/cb#x` are all rejected against registered `http://localhost:3000/cb`.
- **SR-40** If own-AS DCR stays enabled, registration MUST validate redirect URIs: `https` or loopback per SR-39, or a private-use scheme; no wildcards; no userinfo. *Verify by:* registration with `http://attacker.example/cb` returns 400.

### A7: Client onboarding

- **SR-41** The reference realm MUST disable anonymous DCR, or restrict it with client-registration policies: trusted hosts, consent required, allowed client scopes excluding `admin`, max clients. Known MCP clients MUST be pre-registered. CIMD MUST be preferred where enabled. *Verify by:* reference-realm lint, and an anonymous registration requesting the `admin` scope is refused in the reference IdP integration test.
- **SR-42** The device grant MUST use a user-code lifespan of at most 600 s and a consent screen that shows the client name and scopes. The public `fortemi-cli` client MUST NOT be able to obtain `admin` (admin requires a separate confidential or step-up client). *Verify by:* reference-realm lint, and a policy test where `azp=fortemi-cli` with an admin role mapping yields no `admin` scope.
- **SR-43** The claim policy MUST support per-client scope ceilings (for example `clients.ceilings: {"fortemi-cli": ["read","write","mcp"]}`). *Verify by:* fortemi-auth unit test.
- **SR-44** `fortemi-login` MUST store refresh tokens in the OS keychain (macOS Keychain, Windows Credential Manager, Secret Service). A file fallback MUST require an explicit flag and write mode 0600 inside a 0700 directory. `logout` MUST revoke at the IdP. *Verify by:* helper tests check the file mode, refusal without the flag, and that the revoke call is issued.
- **SR-45** The reference realm MUST enable refresh-token rotation with reuse detection for public clients, MUST NOT grant `offline_access` by default, and MUST set an SSO idle timeout of at most 8 h. *Verify by:* reference-realm lint.

### A8: Front door

- **SR-50** In external OIDC mode, the API MUST NOT derive identity from any header other than `Authorization`. The gateway MUST strip inbound `X-Forwarded-User`, `X-Forwarded-Email`, `X-Auth-Request-*` and `X-Forwarded-Access-Token` on API and MCP routes. *Verify by:* a request with a forged `X-Forwarded-Email` and no bearer returns 401 at the API; a gateway conformance test shows the header removed upstream.
- **SR-51** `FORTEMI_TRUSTED_PROXY_CIDRS` in the reference deployment MUST list only the ingress or gateway pod addresses (no node or pod-wide CIDRs). Where identity headers are used at all, the mesh MUST restrict the source principal with an AuthorizationPolicy (mTLS identity). *Verify by:* deployment lint.
- **SR-52** API and MCP paths MUST NOT sit behind the cookie gate, and the proxy MUST NOT convert cookies into `Authorization` headers for those paths. *Verify by:* gateway test: a cookie-only request to `/api/v1/notes` returns 401 from Fortemi, not 200.
- **SR-53** The gateway SHOULD add Istio `RequestAuthentication` (issuer, JWKS) and an `AuthorizationPolicy` requiring the canonical audience on API and MCP routes as defense in depth. Fortemi validation remains authoritative. *Verify by:* a request with a wrong-audience token is rejected at the mesh and at Fortemi.
- **SR-54** Authentication failures MUST be rate-limited per client IP *before* token verification (a pre-auth limiter outside `auth_middleware`). The `Authorization` header MUST be capped at 8 KiB. *Verify by:* 200 invalid tokens/s from one IP return 429 after the limit; an oversized header returns 431/400 without a verification attempt.
- **SR-55** MCP MAY cache successful token-info results keyed by a hash of the token for `min(exp - now, 60 s)`. MUST NOT cache failures beyond 5 s. *Verify by:* cache test, and a revoked PAT stops working within 60 s through MCP.

**Total: 63 security requirements (SR-1 to SR-63).**

---

## 4. Residual risks and accepted-risk decisions required from the operator

| ID | Residual risk | Why it remains | Decision needed |
|----|---------------|----------------|-----------------|
| RR-1 | IdP compromise or a realm-admin error (role mapping, client scopes) grants Fortemi access. | Fortemi trusts the IdP for authentication by design. | Accept, with realm admin MFA and change audit at the IdP. **Owner sign-off required.** |
| RR-2 | IdP group or role demotion propagates only on the user's next OIDC request. PATs keep old scopes until then, up to `FORTEMI_PAT_REVALIDATE_DAYS` (SR-29, SR-30). | There is no SCIM or back-channel provisioning in this release (non-goal). | Accept a 30-day staleness bound, or choose a shorter value. Decide whether to subscribe to Keycloak admin events later. |
| RR-3 | IdP access tokens remain valid until `exp` after the user is disabled at the IdP (no introspection on JWT path). | Self-contained JWT validation. | Accept with a max lifetime of 1 h (SR-13), or require 5-minute tokens on the reference realm. |
| RR-4 | Device-code phishing cannot be fully prevented (SR-42 reduces it). | It is inherent to RFC 8628. | Accept, or disable the device grant and require browser PKCE with a loopback redirect for the CLI. |
| RR-5 | Loopback redirect interception by a malicious local process. | RFC 8252 design. PKCE makes a stolen code useless without the verifier, but a malicious local app can run its own flow. | Accept. |
| RR-6 | Legacy `mm_` tokens when `FORTEMI_AUTH_ALLOW_LEGACY_TOKENS=true` bypass IdP MFA and deactivation. | Migration period. | Decide a sunset date and the default scope ceiling (SR-48). |
| RR-7 | `scope_source=token`/`union` (if opted in via SR-15) delegates Fortemi authorization semantics to IdP client-scope configuration. | Operator flexibility. | Explicit opt-in per deployment, recorded in the deployment ADR. |
| RR-8 | Per-user data isolation inside a tenant is absent. Any member with `read` sees all tenant content. | Non-goal of this release. | Confirm acceptance and communicate it in the connection guide. |
| RR-9 | Third-party MCP clients hold user tokens in their own storage. | Outside Fortemi control. | Accept. Recommend short-lived tokens and per-client ceilings (SR-43). |
| RR-10 | Multi-issuer support (more than one IdP) is not covered. The single-issuer verifier is assumed (`fortemi-auth-clerk/src/lib.rs:674-677`). | Scope. | Confirm a single issuer per deployment for this release. |

---

## 5. Verification plan: negative tests required before release

All tests below MUST exist and pass in CI before the external OIDC mode is enabled for any deployment. They are grouped by suite. The SRs each test covers are given in parentheses.

### 5.1 `matric-api` integration (signed JWT fixtures, mock JWKS)

1. Wrong issuer, wrong audience, expired, `nbf` in the future beyond skew: each returns 401 (baseline; SR-1).
2. ID token (with `nonce`/`at_hash`) and `typ: id+jwt`: 401 (SR-2).
3. `aud: [canonical, other]` without the opt-in flag: 401 (SR-3).
4. `alg: none`, HS256 with RSA public key bytes, `use=enc` JWK: 401 (SR-12).
5. Lifetime `exp - iat` greater than the max: 401 (SR-13).
6. Random-`kid` flood: at most 1 JWKS fetch per 30 s; IdP outage beyond grace returns 503 (SR-10, SR-11).
7. Community external mode, a `read`-only principal on write and admin routes: 403 (SR-14).
8. Startup refusals: `AllowAllPolicy` with external mode; missing claim policy; `token`/`union` without opt-in; `TracingSink` with external mode; default-tenant mode with multi-tenant; own-AS issuer equal to external issuer; `ALLOW_LOCAL_AS` with owner auth `none`; audience equal to an allowed client id (SR-1, SR-8, SR-14, SR-15, SR-18, SR-46, SR-56).
9. A token scope containing `system:admin`: the effective scopes exclude it (SR-16).
10. Default-tenant mode, a token carrying a foreign tenant claim: 401 (SR-18).
11. Two issuers with the same email give two users; zero-scope token: 403 and no row; JIT burst: 429 (SR-20, SR-21).
12. A human token from a service-listed `azp`: 401. A service token minting a PAT: 403 (SR-23, SR-24).
13. Legacy `mm_key_`/`mm_at_` in external mode: 401 by default, capped when enabled (SR-48).
14. Forged `X-Forwarded-Email` without a bearer: 401 (SR-50).
15. 401 carries `resource_metadata`; scope 403 carries `insufficient_scope`; AS metadata is 404 in external mode (SR-7, SR-62).
16. Invalid-token flood from one IP: 429 before verification; oversized `Authorization`: rejected (SR-54).

### 5.2 PAT suite

17. Minting with a PAT, `mm_at_` or `mm_key_`: 403. A `read` user requesting `write`: 403 (SR-28).
18. A demoted user's `write` PAT is effective as `read` only (SR-29).
19. A user not seen via OIDC beyond the revalidate window: the PAT returns 401 (SR-30).
20. Disabled user: the PAT returns 401 within 60 s; admin disable revokes all PATs (SR-31, SR-32).
21. Repeated use does not extend `expires_at`; a mint with no expiry gets the default (SR-27).
22. A row with a plain SHA-256 of a known token does not validate (SR-34).
23. PAT mint with the audit sink down: 503 (SR-57).

### 5.3 Own AS / consent / DCR

24. External mode with defaults: `/oauth/register`, `/oauth/authorize` and `/oauth/token` (authorization_code) all refuse (SR-46).
25. `validate_redirect_uri` rejects `http://localhost:1234@attacker.example/cb`, `http://localhost.attacker.example:1/cb`, and a fragment variant (SR-39). This test must be written first and fail against the current `main.rs:23663-23700` to confirm the code-read finding.
26. DCR with a non-loopback `http` redirect: 400 (SR-40).
27. A trusted-header owner requesting `admin`: denied. `mm_key_` owner proof in external mode: denied (SR-47, SR-49).

### 5.4 MCP server (`mcp-server`)

28. User B's token with user A's session id on `POST /`, `GET /`, `DELETE /` and `POST /messages`: 403 (SR-35).
29. Outbound API calls carry the current request's bearer. With `FORTEMI_API_KEY` set and no request token, no API key is sent (SR-36).
30. Log capture contains no full session id and no token (SR-37, SR-59).
31. `mm_pat_` is classified as Fortemi. A `read`-only credential of every class yields 403 `insufficient_scope` (SR-38).
32. Startup exits when `MCP_RESOURCE_URI` does not equal `FORTEMI_AUTH_AUDIENCE` (SR-4).
33. A token rejected by token-info is never forwarded (SR-5). A revoked PAT is rejected within the 60 s cache window (SR-55).
34. In external mode, MCP does not proxy AS metadata, and its PRM lists only the IdP issuer (SR-7).

### 5.5 Reference realm, deployment and helper (lint/conformance)

35. Realm lint: claim path from client roles or full group paths; tenant attribute admin-only; anonymous DCR off or restricted; device user-code lifespan of at most 600 s; refresh rotation on; no default `offline_access`; impersonation off for the Fortemi audience; `fortemi-cli` ceiling excludes `admin` (SR-17, SR-19, SR-24, SR-41, SR-42, SR-45).
36. Gateway conformance: identity headers are stripped on API/MCP routes; a cookie-only request to API routes gets 401; a wrong-audience token is rejected at the mesh (SR-50, SR-52, SR-53). Deployment lint: trusted CIDRs are narrow (SR-51).
37. `fortemi-login`: keychain by default; the file fallback requires a flag and uses 0600/0700; logout revokes (SR-44).
38. Log and trace capture during JIT contain no email, name or groups (SR-60).

### 5.6 Release gate

The release is blocked until tests 1-38 pass, the residual-risk decisions RR-1 to RR-10 are recorded by the operator, and the code-read findings (T-17 parts a to c, and T-19 part b) are confirmed by failing tests and then fixed.

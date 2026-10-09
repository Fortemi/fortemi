---
title: "End-user OIDC identity pass-through for a self-hosted API + MCP server (Keycloak 26.7): best practices and evaluation of the design brief"
date: 2026-10-09
status: research-finding
input: .aiwg/working/auth-oidc-design-brief.md
sources:
  - url: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization
    verified: 2026-10-09
  - url: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
    verified: 2026-10-09
  - url: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery
    verified: 2026-10-09
  - url: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations
    verified: 2026-10-09
  - url: https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc9728.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc8707.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc9700.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc10017.html
    verified: 2026-10-09
  - url: https://datatracker.ietf.org/doc/draft-ietf-oauth-browser-based-apps/
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc8628.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc8693.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc9449.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc7591.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc8252.html
    verified: 2026-10-09
  - url: https://www.rfc-editor.org/rfc/rfc9068.html
    verified: 2026-10-09
  - url: https://datatracker.ietf.org/doc/draft-ietf-oauth-client-id-metadata-document/
    verified: 2026-10-09
  - url: https://www.keycloak.org/securing-apps/mcp-authz-server
    verified: 2026-10-09
    note: "Page currently renders the Nightly 26.8.0 edition; 26.7 behavior inferred, see section 4"
  - url: https://www.keycloak.org/docs/latest/release_notes/index.html
    verified: 2026-10-09
  - url: https://www.keycloak.org/securing-apps/token-exchange
    verified: 2026-10-09
  - url: https://www.keycloak.org/securing-apps/oidc-layers
    verified: 2026-10-09
  - url: https://www.keycloak.org/securing-apps/client-registration
    verified: 2026-10-09
  - url: https://github.com/keycloak/keycloak/pull/46763
    verified: 2026-10-09
  - url: https://skycloak.io/blog/keycloak-mcp-server-401-audience-rfc-8707/
    verified: 2026-10-09
    note: "Secondary source (vendor blog)"
  - url: https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/v1.x/src/shared/auth-utils.ts
    verified: 2026-10-09
  - url: https://cli.github.com/manual/gh_auth_login
    verified: 2026-10-09
  - url: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps
    verified: 2026-10-09
  - url: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens
    verified: 2026-10-09
  - url: https://github.blog/engineering/platform-security/behind-githubs-new-authentication-token-formats/
    verified: 2026-10-09
  - url: https://huggingface.co/docs/hub/security-tokens
    verified: 2026-10-09
  - url: https://code.claude.com/docs/en/mcp
    verified: 2026-10-09
  - url: https://www.claude.com/docs/connectors/custom/remote-mcp
    verified: 2026-10-09
  - url: https://www.claude.com/docs/connectors/building/authentication
    verified: 2026-10-09
  - url: https://developers.openai.com/apps-sdk/build/auth
    verified: 2026-10-09
  - url: https://developers.cloudflare.com/agents/model-context-protocol/protocol/authorization/
    verified: 2026-10-09
  - url: https://raw.githubusercontent.com/cloudflare/workers-oauth-provider/main/README.md
    verified: 2026-10-09
  - url: https://istio.io/latest/docs/reference/config/security/request_authentication/
    verified: 2026-10-09
  - url: https://oauth2-proxy.github.io/oauth2-proxy/configuration/overview
    verified: 2026-10-09
---

# End-user OIDC identity pass-through for the API, MCP server and CLI

**Scope.** This brief covers how browser apps, CLIs and agents, MCP clients and services should authenticate to a self-hosted API plus MCP server that sits behind an existing Keycloak 26.7 realm, with the end user's identity preserved all the way to the server. It evaluates `.aiwg/working/auth-oidc-design-brief.md`.

**Citation convention.** I fetched every inline link on 2026-10-09; the frontmatter lists them. Where I could not confirm a claim from a fetched primary source, the text says "unverified" or names it as a secondary source. Quotes are short excerpts.

---

## 1. Executive recommendation

1. **Make the server an OAuth resource server in every mode, validating IdP JWTs locally.** Check the signature (via JWKS), an exact `iss` match, `aud`, `exp`/`nbf`, `alg` not `none`, and the token type. [RFC 9068 §4](https://www.rfc-editor.org/rfc/rfc9068.html) and [RFC 9700 §2.3](https://www.rfc-editor.org/rfc/rfc9700.html) set these duties. The [MCP spec](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) makes audience validation a MUST. The brief's decision 1 matches this.
2. **Make the canonical resource identifier the exact MCP endpoint URL, not the bare origin.** [RFC 9728 §3.3](https://www.rfc-editor.org/rfc/rfc9728.html) says the PRM `resource` value "MUST be identical to the URL that the client used to make the request" when PRM was reached through `WWW-Authenticate`. [Anthropic's connector docs](https://www.claude.com/docs/connectors/building/authentication) require the PRM `resource` to "match your MCP server URL exactly, including any path". So use `https://fortemi.example.org/mcp` as the MCP resource. The API can then accept a configured *set* of audiences (the MCP URL plus the API URL) issued by the same audience mapper. **This is the most important correction to the brief.**
3. **Keep one IdP-issued user token for the API and MCP, but have the MCP server validate it fully before it forwards anything.** The spec says an MCP server "MUST NOT pass through the token it received" to upstream APIs ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)). Forwarding is defensible only if the API and MCP are documented as one protected resource, share the audience, and both validate. Better still, MCP calls the API in-process or over a trusted internal channel. Otherwise use token exchange.
4. **CLI and agents:** use auth code + PKCE with a loopback redirect when a browser is available, and the device grant ([RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html)) for headless use. Both run against one public client. Store refresh tokens in the OS credential store, the way `gh` does ([gh manual](https://cli.github.com/manual/gh_auth_login)), and rotate them, because [RFC 9700 §2.2.2](https://www.rfc-editor.org/rfc/rfc9700.html) says public-client refresh tokens "MUST be sender-constrained or use refresh token rotation". Never use ROPC ([RFC 9700 §2.4](https://www.rfc-editor.org/rfc/rfc9700.html); [Keycloak](https://www.keycloak.org/securing-apps/oidc-layers)).
5. **MCP clients:** use PRM discovery. Pre-register clients for known tools; enable CIMD where Keycloak's experimental feature is acceptable; fall back to DCR, which the spec deprecates ([client registration](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration)), under locked-down anonymous policies. **Without RFC 8707 support, Keycloak ignores `resource`.** The audience must come from a scope the client actually requests, so that scope has to appear in `scopes_supported` and in the `WWW-Authenticate` `scope` ([Keycloak MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server)).
6. **Browser UI (when it ships):** prefer a BFF. [RFC 10017](https://www.rfc-editor.org/rfc/rfc10017.html) (BCP 212, Aug 2026) "strongly recommend[s]" it for business apps and apps that handle personal data. A public SPA holding tokens is the weakest of its three patterns.
7. **Services:** use client credentials with `private_key_jwt` rather than shared secrets, recorded as `kind=service` principals.
8. **Personal access tokens:** use them only where OAuth is impossible. They should be user-bound, scoped to a subset of the user's rights, expire under a mandatory maximum lifetime, be stored hashed, be revocable, carry a distinctive prefix plus checksum for secret scanning ([GitHub token format](https://github.blog/engineering/platform-security/behind-githubs-new-authentication-token-formats/)), and be invalidated when the user is deprovisioned.
9. **Don't rely on trusted identity headers from a gateway** as the system of record. If a gateway also validates JWTs (defense in depth), configure it so the original bearer still reaches the server. [Istio's `forwardOriginalToken` defaults to false](https://istio.io/latest/docs/reference/config/security/request_authentication/).
10. **Plan for DPoP ([RFC 9449](https://www.rfc-editor.org/rfc/rfc9449.html)) as phase 2.** Keycloak lists DPoP as supported since 26.4 ([release notes](https://www.keycloak.org/docs/latest/release_notes/index.html)). It mitigates stolen-token replay for CLI and agent tokens, but not malicious code running inside the client (RFC 9449 §2, §11.4).

---

## 2. Patterns by client type

### 2.1 Browser apps

| Pattern | What the browser holds | RFC 10017 position |
|---|---|---|
| **BFF**: a server-side confidential client, cookie session, proxies API calls | Session cookie only | Recommended; "strongly recommended for business applications, sensitive applications, and applications that handle personal data" ([RFC 10017 via datatracker](https://datatracker.ietf.org/doc/draft-ietf-oauth-browser-based-apps/)) |
| **Token-mediating backend**: the backend gets tokens and hands the access token to the browser | Access token | Use only when a proxying BFF isn't feasible (same source) |
| **Browser-based OAuth client**: a public SPA with PKCE | Access + refresh tokens | Exposed to all the attack scenarios in the document (same source) |

The key reasoning in RFC 10017 is that injected JavaScript can run a fresh silent authorization flow in a hidden iframe. The document says "There are no practical security mechanisms for frontend applications that counter this attack scenario", and DPoP doesn't help because the attacker binds new tokens to its own key ([datatracker summary of RFC 10017](https://datatracker.ietf.org/doc/draft-ietf-oauth-browser-based-apps/)). For a knowledge base holding personal data, the BFF is the defensible default. A BFF still calls the server with an access token, so the server-side resource-server design doesn't change.

### 2.2 CLI and agents

- **Loopback + PKCE** (interactive desktop):
  - Use the system browser, never an embedded webview ([RFC 8252 §8.12](https://www.rfc-editor.org/rfc/rfc8252.html)).
  - The redirect should be `http://127.0.0.1:<any-port>/…`; RFC 8252 §8.3 says "the use of localhost is NOT RECOMMENDED", and §7.3 says the AS "MUST allow any port".
  - GitHub documents the same loopback-with-any-port pattern ([GitHub OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)).
- **Device authorization grant** (headless, SSH sessions, containers) per [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html):
  - Device clients "are generally incapable of maintaining the confidentiality of their credentials" (§5.6), so the client is public.
  - Rate-limit user-code attempts (§5.1).
  - Tell the user they are authorizing a device, as a defense against remote phishing (§5.4). Be careful with `verification_uri_complete` (§3.3.1, §5.4).
  - Honor `slow_down`, which adds 5 seconds to the interval (§3.5).
  - Keycloak lets public clients call the device endpoint `/realms/{realm}/protocol/openid-connect/auth/device` ([Keycloak OIDC layers](https://www.keycloak.org/securing-apps/oidc-layers)).
- **Reference implementation, `gh`:**
  - `gh auth login` uses a browser flow by default; a one-time code can be copied with `-c` ([gh manual](https://cli.github.com/manual/gh_auth_login)). GitHub's device flow, which it documents for headless CLI tools, is per-app opt-in, has 15-minute codes and caps users at 50 codes per hour per app ([GitHub docs](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)).
  - Tokens go to the system credential store, falling back to plain text only when that fails or `--insecure-storage` is passed.
  - `GH_TOKEN` in the environment is supported for automation.
  - The same shape works here: `fortemi login` stores tokens in the keychain, and `FORTEMI_TOKEN` serves CI.
- **Agents that call MCP servers through a client** (for example Claude Code) can use the client's own OAuth support. Claude Code can also run a `headersHelper` command that prints an `Authorization` header, "again after a 401 or 403" ([Claude Code MCP docs](https://code.claude.com/docs/en/mcp)). A `fortemi token print` helper backed by the keychain refresh token gives per-user identity even where interactive OAuth is awkward.

### 2.3 MCP clients

**Discovery and protected resource metadata**
- The MCP server MUST publish RFC 9728 PRM with at least one `authorization_servers` entry ([AS discovery](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery)).
- Clients try `WWW-Authenticate: resource_metadata=…` first, then path-inserted PRM (`/.well-known/oauth-protected-resource/mcp`), then the root PRM (same source).
- For an issuer with a path, which Keycloak's `/realms/<r>` is, clients try three AS-metadata URLs. The last is OIDC "path appending", and that is the form Keycloak serves. Clients MUST reject metadata whose `issuer` differs from the issuer they used (same source).

**Audience and resource**
- Clients MUST send `resource`, the canonical MCP URI, on both the authorize and token requests, "regardless of whether authorization servers support it" ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)).
- Servers MUST reject tokens that don't name them in `aud` ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)).
- Path matters:
  - RFC 9728 requires an exact `resource` match ([RFC 9728 §3.3](https://www.rfc-editor.org/rfc/rfc9728.html)).
  - Anthropic requires an exact match "including any path" ([Claude connector auth](https://www.claude.com/docs/connectors/building/authentication)).
  - OpenAI says ChatGPT "sends that exact value" from PRM `resource` and expects it copied into `aud` ([OpenAI Apps SDK auth](https://developers.openai.com/apps-sdk/build/auth)).
  - The reference TypeScript SDK v1.x client is more lenient. Its `checkResourceAllowed` accepts a PRM resource that is a path prefix of the server URL with the same origin ([auth-utils.ts](https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/v1.x/src/shared/auth-utils.ts)).
  - So an origin-only resource may work with some clients and fail with others. Designing for the strict interpretation is the safe choice.

**Client registration**
- **Preference order:** pre-registered, then CIMD (if the AS advertises `client_id_metadata_document_supported`), then DCR, then manual entry ([client registration](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration)). DCR is deprecated and kept for compatibility (same source).
- **CIMD** (draft-ietf-oauth-client-id-metadata-document-02, updated 2026-07-06, still an Internet-Draft per [datatracker](https://datatracker.ietf.org/doc/draft-ietf-oauth-client-id-metadata-document/)):
  - It requires the AS to block special-use IPs (SSRF), not auto-follow redirects, cap document size, and show the `client_id` hostname.
  - It "cannot prevent `localhost` URL impersonation by themselves"; the AS MUST clearly display the redirect host ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)).
  - Claude Code's CIMD is at `https://claude.ai/oauth/claude-code-client-metadata`. Claude uses CIMD only if AS metadata advertises CIMD support **and** `"none"` in `token_endpoint_auth_methods_supported`; otherwise it falls back to DCR ([Claude connector auth](https://www.claude.com/docs/connectors/building/authentication)).
- **DCR risks:** clients self-assert their name and logo, so "a rogue client might use the name and logo of a legitimate client" ([RFC 7591 §5](https://www.rfc-editor.org/rfc/rfc7591.html)). Claude also notes that DCR "registers a new client on each fresh connection" and recommends CIMD or pre-registered credentials for high-traffic servers ([Claude connector auth](https://www.claude.com/docs/connectors/building/authentication)).

**Redirects in practice**

| Client | Redirect URI |
|---|---|
| Claude hosted apps (claude.ai, Desktop, mobile) | `https://claude.ai/api/mcp/auth_callback` |
| Claude Code | `http://localhost:<ephemeral>/callback` |
| VS Code | `http://127.0.0.1:<port>/callback` |
| ChatGPT | `https://chatgpt.com/connector_platform_oauth_redirect` when the AS supports RFC 9207 |

Sources: [Claude connector auth](https://www.claude.com/docs/connectors/building/authentication), [Keycloak MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server), [OpenAI Apps SDK auth](https://developers.openai.com/apps-sdk/build/auth).

**Network origin:** hosted Claude reaches the MCP server **and** the AS from Anthropic's cloud (`160.79.104.0/21`). A WAF in front of the IdP can therefore break the flow ([Claude connector auth](https://www.claude.com/docs/connectors/building/authentication)). Claude Code and VS Code connect from the user's machine.

**Scopes and step-up**
- Return `401` with `WWW-Authenticate: Bearer resource_metadata=…, scope=…`.
- Return `403` with `error="insufficient_scope"` and *all* scopes the operation needs in one challenge.
- Respect scope hierarchies. Don't put `offline_access` in `scopes_supported` or challenges.
- Source for all of the above: [authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization).
- Keep `scopes_supported` minimal ([best practices: scope minimization](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices)).

**Issuer check.** Clients validate RFC 9207 `iss` when it is present or advertised ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)). Keycloak lists RFC 9207 as supported ([Keycloak MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server)).

**What major platforms do**

| Platform | Pattern |
|---|---|
| OpenAI ChatGPT apps | PRM; `resource` copied into `aud`; CIMD preferred, DCR fallback; the server "must assume the token is untrusted and perform the full set of resource-server checks" ([OpenAI](https://developers.openai.com/apps-sdk/build/auth)). Keycloak's guide says ChatGPT is **not** supported against Keycloak, because Keycloak rejects its metadata field ([Keycloak MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server)). |
| Anthropic Claude | DCR, CIMD ("Use Claude's published identity", recommended) or your own client ID ([remote MCP](https://www.claude.com/docs/connectors/custom/remote-mcp)). Refresh happens on 401 and up to 5 minutes before expiry; return `invalid_grant` for dead refresh tokens; rotate public-client refresh tokens ([Claude connector auth](https://www.claude.com/docs/connectors/building/authentication)). Org-level static bearer headers exist in beta but "shouldn't be used to identify individual users" (same source). |
| Cloudflare | Options are self-handled, third-party OAuth, Cloudflare Access, or a BYO provider. In the third-party pattern the Worker "generates and issues its own token to the MCP client", so the upstream token stays server-side, and identity reaches tools as props ([Cloudflare docs](https://developers.cloudflare.com/agents/model-context-protocol/protocol/authorization/)). The library stores "Tokens, codes and secrets … only as hashes" ([workers-oauth-provider README](https://raw.githubusercontent.com/cloudflare/workers-oauth-provider/main/README.md)). |

Cloudflare's pattern (MCP server acting as its own AS and wrapping an upstream IdP) is the alternative to "IdP is the AS". It costs an AS implementation and the confused-deputy controls the spec mandates for MCP proxies ([best practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices)). With a capable IdP like Keycloak, pointing PRM at the IdP is simpler.

### 2.4 Services

- Use the client credentials grant, which Keycloak documents for apps acting "on behalf of themselves". Keycloak supports secrets or key pairs ([Keycloak OIDC layers](https://www.keycloak.org/securing-apps/oidc-layers)). Prefer `private_key_jwt` so no shared secret exists. Keycloak 26.8 also promoted client-secret rotation via client policies to supported ([release notes](https://www.keycloak.org/docs/latest/release_notes/index.html)).
- Record a service as the principal, with `azp`/`client_id` and `kind=service`.
- If a service acts *for a user*, use token exchange so the user is the `sub` and the service appears as the actor (`act`), rather than reusing the user's raw token. See section 3.

### 2.5 Personal access tokens: when and how

**When.** Only for clients that can't do OAuth: scripts, CI, tools that accept only a static header. Hugging Face recommends "one access token per app or usage" and fine-grained tokens for production, and it replaces CI secrets with OIDC-to-short-lived-token exchange where possible ([HF tokens](https://huggingface.co/docs/hub/security-tokens)). For CI, prefer workload-identity exchange over PATs.

**How.**

| Property | Practice | Precedent |
|---|---|---|
| Bound to a user | The owner is the user; the token can't exceed the owner's current rights (evaluate the intersection at use time) | GitHub fine-grained PATs are tied to one resource owner ([GitHub](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)) |
| Scoped | Narrow permissions per resource | GitHub read/write/admin per permission; HF read/write/fine-grained |
| Expiring | Default 30 days, enforced maximum, admin policy | GitHub defaults to 30 days, allows infinite unless policy caps it (same source). Recommend *no* infinite option. |
| Hashed at rest | Store only a hash of a high-entropy random token | Cloudflare stores only hashes ([README](https://raw.githubusercontent.com/cloudflare/workers-oauth-provider/main/README.md)) |
| Detectable | Prefix plus checksum (e.g. `mm_pat_…` + CRC32) | GitHub `ghp_` with CRC32 "virtually eliminates false positives" ([GitHub blog](https://github.blog/engineering/platform-security/behind-githubs-new-authentication-token-formats/)) |
| Revocable | User and admin revoke; an unauthenticated leak-revocation endpoint | HF `POST /api/credentials/revoke` always returns 202 so it can't be used as an oracle ([HF](https://huggingface.co/docs/hub/security-tokens)) |
| Hygiene | Show once, record last-used, auto-expire unused tokens | GitHub removes PATs unused for a year ([GitHub](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)) |

Hashing note: a 256-bit random token needs no password KDF. A keyed HMAC or SHA-256 lookup hash is enough, because the input is high-entropy. That is my reasoning, consistent with the project's `no-adhoc-kdf` rule's entropy-class distinction, not a cited standard.

---

## 3. Identity propagation: one token vs token exchange vs gateway headers

| Approach | How identity reaches the server | Strengths | Weaknesses |
|---|---|---|---|
| **Same-resource token** (the API and MCP accept one IdP token with a shared audience set) | The user's JWT, validated by each component | Simplest; identity is cryptographically verified at each hop; no extra IdP round-trip | Widens the token's blast radius to both components. Spec-compliant only if they really are one protected resource, because the spec forbids passing through to an *upstream* API ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)). The MCP component must still validate, not just forward ([best practices: token passthrough](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices)). |
| **Token exchange** ([RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html)) | The MCP server exchanges the user token for one with `aud=api`, optionally with `act` for delegation | Clean audience separation; per-hop least privilege; `act` records "that delegation has occurred" (RFC 8693 §4.1) | In Keycloak the exchanging client must be confidential, and the subject token must name it in `aud`. The `resource` parameter isn't supported, and impersonation is "Not implemented yet". Delegation (`act`/`may_act`) is behind `token-exchange-delegation`: experimental in 26.7, preview in 26.8 ([Keycloak token exchange](https://www.keycloak.org/securing-apps/token-exchange); [release notes](https://www.keycloak.org/docs/latest/release_notes/index.html)). Adds latency and an IdP dependency per session. |
| **Gateway-injected trusted headers** (`X-Forwarded-User`, `X-Auth-Request-Email`, Istio `outputClaimToHeaders`) | The gateway validates and copies claims into headers | No token handling in the app | Weakest; see below |

**Why trusted headers are weaker.**
1. **No integrity.** Anything that reaches the app without passing the gateway can forge them. oauth2-proxy warns that "a client that can reach OAuth2 Proxy directly may be able to spoof forwarded headers" ([oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/configuration/overview)).
2. **The app can't check audience, expiry or scope itself**, so it inherits every gateway misconfiguration. For example, Istio `RequestAuthentication` alone *accepts* requests with no token, which then carry no identity, unless an `AuthorizationPolicy` requires one ([Istio](https://istio.io/latest/docs/reference/config/security/request_authentication/)).
3. **It breaks the MCP contract.** MCP requires the server to validate tokens and return RFC 6750 challenges itself ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)).
4. **It loses non-repudiable audit**, since a header can't be re-verified later.

Headers are acceptable only as an *extra* gate, never as the source of the principal.

**Recommendation.** Use the same-resource token for API+MCP in one deployment, with both components validating. Treat token exchange as the documented path when MCP becomes a separate resource or a separate trust domain. Don't use gateway headers for identity.

---

## 4. Keycloak 26.7 specifics and feature flags

**Version caveat.** The official MCP guide currently renders as **Nightly 26.8.0** ([Keycloak MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server)), and release notes list 26.8.0 as the latest ([release notes](https://www.keycloak.org/docs/latest/release_notes/index.html)). I couldn't fetch a 26.7-pinned copy of the guide. Treat the rows below as "26.8 docs, expected to apply to 26.7", and verify each flag on the deployed build with `kc.sh show-config` or the Admin console server info.

| Capability | Status / setting | Source |
|---|---|---|
| MCP 2026-07-28 conformance | "Experimental" (2025-03-26 is "Supported") | [MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server) |
| RFC 8707 resource indicators | Experimental, `--features=resource-indicators`. Without it, "Keycloak ignores the `resource` parameter". Initial support merged 2026-03-17 under `RESOURCE_INDICATORS` ([PR #46763](https://github.com/keycloak/keycloak/pull/46763)). Whether the flag ships in 26.7.0 is **unverified**: a vendor blog says the tracking issue was milestoned for 26.7.0 ([Skycloak](https://skycloak.io/blog/keycloak-mcp-server-401-audience-rfc-8707/), secondary). | [MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server) |
| Audience without RI | An optional client scope per MCP scope, with an **Audience mapper** whose *Included Custom Audience* is the MCP server URL. The `resource` value, the custom audience and the server URL "must match". | same |
| Audience with RI | Register the MCP server as a client with a `resource_url` attribute; an Audience mapper targets that client | same |
| CIMD | Experimental, `--features=cimd`. The 26.7.0 notes say the Claude Code and VS Code integrations use CIMD with PKCE public clients and localhost callbacks. Configure through client-policy executors: trusted domains, "Restrict same domain", "Only Allow Confidential Client", "Accept Public Client with Confidential-only Grant Types", and a resource-indicator allow-list that auto-creates the audience mapper. | [MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server); [release notes](https://www.keycloak.org/docs/latest/release_notes/index.html) |
| Claude Code vs Claude Desktop | Both use `claude.ai` client IDs and collide across separate policies. Use one policy with trusted domains `claude.ai`, `localhost`, `127.0.0.1`, Restrict-same-domain OFF and Accept-public-confidential-only-grants ON. | [MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server) |
| ChatGPT | Not supported (metadata field mismatch) | same |
| DCR (RFC 7591) | Supported. Anonymous registration is "de-facto disabled" until Trusted Hosts are set. Defaults: Consent Required, Full Scope Disabled, Max Clients 200, only realm-default client scopes. Keycloak says client **policies** are the recommended successor to registration policies. | [client registration](https://www.keycloak.org/securing-apps/client-registration) |
| Device grant | Device endpoint `/realms/{realm}/protocol/openid-connect/auth/device`, usable by public clients. The client must have the device grant capability enabled; the exact toggle label in 26.7 is **unverified** from fetched sources. | [OIDC layers](https://www.keycloak.org/securing-apps/oidc-layers) |
| Standard token exchange | "Supported Standard Token Exchange" since 26.2.0, enabled by default (`token-exchange-standard:v2`). Per-client switch; confidential requesters only; `audience` downscopes; `scope` can upscope unless `downscope-assertion-grant-enforcer` is applied; no `resource` parameter. | [token exchange](https://www.keycloak.org/securing-apps/token-exchange); [release notes](https://www.keycloak.org/docs/latest/release_notes/index.html) |
| Delegation (`act`) | `token-exchange-delegation`: experimental (26.7), preview (26.8) | [release notes](https://www.keycloak.org/docs/latest/release_notes/index.html) |
| RFC 9207 `iss` | Supported | [MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server) |
| DPoP | Supported since 26.4 | [release notes](https://www.keycloak.org/docs/latest/release_notes/index.html) |
| ROPC | "MUST NOT be used" | [OIDC layers](https://www.keycloak.org/securing-apps/oidc-layers) |

**Audience-mapper approach for this server (no experimental flags).**
- Create one optional client scope, e.g. `fortemi`, with an Audience mapper adding **both** `https://fortemi.example.org/mcp` and the API audience. Alternatively, have the API accept the MCP URL as a valid audience.
- Add that scope to every public and pre-registered client.
- List it in PRM `scopes_supported` and in the `WWW-Authenticate` `scope`. MCP clients request the challenged scopes, or all of `scopes_supported` ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)), and that request is what triggers the mapper.
- Map Fortemi permissions (`read`, `write`, `admin`, `mcp`) from either client scopes or realm roles/groups, and document which one is authoritative.

---

## 5. Pitfalls checklist

- [ ] **The PRM `resource` must equal the MCP URL used by clients, including `/mcp`** ([RFC 9728 §3.3](https://www.rfc-editor.org/rfc/rfc9728.html); [Claude](https://www.claude.com/docs/connectors/building/authentication)). Serve PRM at `/.well-known/oauth-protected-resource/mcp`.
- [ ] **The audience scope never requested, giving 401 "audience mismatch".** Put the audience-bearing scope in `scopes_supported` and in challenges ([Keycloak MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server)).
- [ ] **`authorization_servers` must equal Keycloak's exact `issuer`.** Mind the hostname: an internal service URL and the public `KC_HOSTNAME` produce different `iss`. Clients reject issuer mismatches ([AS discovery](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery)), and so must the server ([RFC 9068 §4](https://www.rfc-editor.org/rfc/rfc9068.html)).
- [ ] **Keycloak's AS metadata must include `code_challenge_methods_supported`**, or MCP clients "MUST refuse to proceed" ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)). Check the realm's discovery document.
- [ ] **Reject ID tokens presented as access tokens.** RFC 9068 uses `typ: at+jwt` for this ([RFC 9068](https://www.rfc-editor.org/rfc/rfc9068.html)). Whether Keycloak emits `at+jwt` by default is **unverified**. If it doesn't, rely on the `aud` check plus Keycloak's `typ` claim (`Bearer` vs `ID`; unverified naming) and test it.
- [ ] **Don't stop at the audience check:** also check `exp`/`nbf`, `alg` allow-list and JWKS rotation and caching ([RFC 9068 §4](https://www.rfc-editor.org/rfc/rfc9068.html); [OpenAI](https://developers.openai.com/apps-sdk/build/auth)).
- [ ] **Never accept tokens in query strings** ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)); never log raw tokens ([Cloudflare](https://developers.cloudflare.com/agents/model-context-protocol/protocol/authorization/)).
- [ ] **Public-client refresh tokens:** rotate them or sender-constrain them ([RFC 9700 §2.2.2](https://www.rfc-editor.org/rfc/rfc9700.html)). Return `invalid_grant` when they are dead ([Claude](https://www.claude.com/docs/connectors/building/authentication)).
- [ ] **Short access-token lifetimes.** Local JWT validation means revocation lags by up to the token lifetime. The AS "SHOULD issue short-lived access tokens" ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)).
- [ ] **DCR open to the world.** Configure Trusted Hosts, Max Clients and allowed scopes; prefer pre-registration and CIMD ([Keycloak client registration](https://www.keycloak.org/securing-apps/client-registration); [RFC 7591 §5](https://www.rfc-editor.org/rfc/rfc7591.html)).
- [ ] **CIMD and localhost impersonation:** show the redirect host and warn on localhost-only redirects ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)). Apply SSRF controls on the IdP's CIMD fetches ([CIMD draft](https://datatracker.ietf.org/doc/draft-ietf-oauth-client-id-metadata-document/)).
- [ ] **Hosted MCP clients connect from vendor cloud.** The IdP and MCP must be publicly reachable, and WAFs must allow the vendor's ranges ([Claude](https://www.claude.com/docs/connectors/building/authentication)).
- [ ] **Device-code phishing:** show "you are authorizing a device" text and rate-limit user codes ([RFC 8628 §5](https://www.rfc-editor.org/rfc/rfc8628.html)).
- [ ] **Gateway JWT validation:** `RequestAuthentication` needs an `AuthorizationPolicy` to reject missing tokens, and `forwardOriginalToken: true` so the server still gets the bearer ([Istio](https://istio.io/latest/docs/reference/config/security/request_authentication/)).
- [ ] **A cookie gate on `/mcp` or `/api` blocks every non-browser client.** Exempt bearer traffic, or use oauth2-proxy `--skip-jwt-bearer-tokens` with `--extra-jwt-issuers` if a cookie gate must stay ([oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/configuration/overview)).
- [ ] **Users keyed by email.** Key on `(iss, sub)`; email can change and be reassigned.
- [ ] **PATs outliving the user:** re-check owner status and the owner's current rights on every use.
- [ ] **Scope challenges that drip-feed** one scope at a time. Emit all required scopes in one challenge ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)).
- [ ] **`offline_access` in PRM `scopes_supported`:** the spec says SHOULD NOT (same source).
- [ ] **MCP state handles not bound to the user.** Key server-side state by the user ID from the verified token ([best practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices)).

---

## 6. Evaluation of the design brief

| # | Brief decision | Verdict | Notes and gaps |
|---|---|---|---|
| Problem / baseline | Diagnosis that the cookie gate forwards no identity and non-browser clients can't pass | **Agree** | Consistent with oauth2-proxy behavior (cookie session unless `--skip-jwt-bearer-tokens`) ([oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/configuration/overview)). |
| Standards baseline | MCP 2026-07-28 summary | **Agree, minor additions** | Accurate on PRM, `resource`, audience, no-transit, registration order, PKCE and 403 step-up. Add: RFC 9207 `iss` validation by clients; `code_challenge_methods_supported` required in AS metadata; exact-match PRM `resource`; refresh-token rotation for public clients ([authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization); [security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)). |
| Standards baseline | Keycloak 26.7 capability list | **Partly agree; verify** | Device grant, token exchange GA in 26.2, DCR, RFC 9207, and CIMD (experimental, `cimd`) are consistent with the sources. The **`resource-indicators` flag in 26.7.0 is unverified**: the official guide is the 26.8 nightly, and the RI code merged 2026-03-17 ([PR](https://github.com/keycloak/keycloak/pull/46763)). Add: Keycloak documents ChatGPT as unsupported, and the Claude Code/Desktop CIMD policy collision needs a combined policy ([MCP guide](https://www.keycloak.org/securing-apps/mcp-authz-server)). |
| 1 | Resource server in every mode, with a default tenant | **Agree** | Add an explicit validation contract: exact `iss`, `aud` set, `exp`/`nbf`, `alg` allow-list, ID-token rejection, JWKS cache/rotation, clock skew. Specify what happens when the claim policy yields no scopes (deny). Gate "`mm_` alongside" behind an explicit flag and log it at startup, consistent with ADR-094 fail-closed. |
| 2 | One protected resource, canonical URI `https://fortemi.example.org`, MCP at `/mcp` | **Disagree on the identifier; agree on one logical resource** | (a) An origin-only `resource` conflicts with RFC 9728 §3.3 exact match and with Anthropic's "including any path". Use `https://fortemi.example.org/mcp` as the MCP resource and give the token an audience set the API also accepts (via the mapper or API config). (b) "Complies with the no-transit rule because it is the same resource" holds only if the MCP server **validates** the token (today it reportedly checks only the `mcp` scope) and the deployment documents API+MCP as one resource. Otherwise the spec's "MUST NOT pass through the token it received" applies ([security considerations](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations)). (c) Token exchange as the alternative: note that Keycloak requires a **confidential** exchanging client, requires the subject token to name it in `aud`, and doesn't support `resource` ([token exchange](https://www.keycloak.org/securing-apps/token-exchange)), so the MCP server would need its own client credentials. |
| 3 | User principal: JIT `app_user` from `(iss, sub)`, `/me`, provenance | **Agree** | Gaps: (a) **deprovisioning** — with local JWT validation and PATs, a user disabled in Keycloak keeps access until expiry. Define "active" (e.g. last successful IdP-token use within N days, an admin disable, or a periodic check), and consider Keycloak's experimental Shared Signals Framework events later ([release notes](https://www.keycloak.org/docs/latest/release_notes/index.html)). (b) Treat email as display-only and require `email_verified` before showing it as identity. (c) Record `azp` (the client) alongside the user in audit, so "which tool acted for whom" is answerable. That gap is what the token-passthrough guidance warns about ([best practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices)). (d) Groups claim size: decide on a group/role filter. |
| 4 | User-bound `mm_pat_` tokens | **Agree, with additions** | Add: prefix plus checksum for secret scanning; hashed storage; a mandatory max lifetime with no "never expires"; effective rights = PAT scopes ∩ the owner's *current* rights; per-token audience (API-only vs MCP); an admin kill switch and unauthenticated leak revocation (HF pattern). Note that the MCP spec's client rule ("MUST NOT send tokens … other than ones issued by the MCP server's authorization server") means PATs at `/mcp` are an out-of-band, non-OAuth path. Document them as such, delivered via `headersHelper`/static header ([Claude Code MCP](https://code.claude.com/docs/en/mcp)). Clarify how a PAT is minted "after an OIDC login" when there is no browser UI: it should be an API call authenticated by an IdP token, with a recent `auth_time` required. |
| 5 | Discovery correctness | **Agree** | Add: PRM at the path-inserted location for `/mcp`; `authorization_servers` = the exact Keycloak issuer; `scopes_supported` minimal but including the audience scope; no `offline_access`; challenges list all required scopes. |
| 6 | Fail-closed own AS when an external IdP is configured | **Strongly agree** | Open DCR plus identity-free consent is exactly the setup the confused-deputy guidance targets ([best practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices)). Also stop serving `/.well-known/oauth-authorization-server` on the API in that mode so clients can't pick the wrong AS. |
| 7 | Onboarding: Browser/SPA with auth code + PKCE direct | **Disagree for the eventual UI** | [RFC 10017](https://www.rfc-editor.org/rfc/rfc10017.html) recommends a BFF for apps handling personal data. Fine to defer, since a UI is a non-goal, but record BFF as the target. |
| 7 | CLI: device grant + `fortemi-cli` public client + keychain | **Agree, extend** | Add loopback + PKCE (127.0.0.1, any port, system browser) as the primary interactive flow, with the device grant for headless use ([RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html)). Refresh-token rotation; an `FORTEMI_TOKEN` env override for CI (the `gh` pattern); a `token print` command for `headersHelper`. Device-grant anti-phishing text. |
| 7 | MCP: pre-registered → CIMD → DCR with locked-down policies | **Agree** | Add per-client notes: Claude Code CIMD needs `none` in `token_endpoint_auth_methods_supported`; hosted Claude/Desktop uses a claude.ai callback and connects from `160.79.104.0/21`; ChatGPT is unsupported on Keycloak per its guide. Mention the vendor-documented "use your own OAuth client" option for pre-registration ([remote MCP](https://www.claude.com/docs/connectors/custom/remote-mcp)). |
| 7 | Services: client credentials | **Agree** | Prefer `private_key_jwt`; use token exchange for user-delegated service calls. |
| 7 | Reference realm export plus connection guide | **Agree** | Include the audience-mapper scope, the combined CIMD policy, DCR anonymous policies, the device-grant client and the token lifetimes. Mark experimental flags as optional overlays, not the baseline. |
| 8 | Front door: drop the cookie gate for API/MCP; optional Istio JWT validation | **Agree, add config requirements** | `AuthorizationPolicy` is required to reject tokenless requests; `forwardOriginalToken: true`; audiences must include the MCP URL; never let the app trust `outputClaimToHeaders` output for identity ([Istio](https://istio.io/latest/docs/reference/config/security/request_authentication/)). Public reachability of the IdP and `/mcp` for hosted clients. |
| Non-goals | Per-user isolation, SCIM, IdP role | **Agree** | One caution: without SCIM or SSF, deprovisioning relies on token lifetimes and PAT checks (see row 3). Make that an explicit accepted risk in the threat model. |

**Gaps the brief does not address at all**
- **DPoP / sender-constrained tokens.** Long-lived CLI and agent refresh tokens are the highest-value theft target. Keycloak supports DPoP, but the server would have to validate proofs ([RFC 9449 §7.1](https://www.rfc-editor.org/rfc/rfc9449.html)). Suggest it as a later phase.
- **Token lifetime and revocation budget.** Set explicit access-token, refresh and idle lifetimes.
- **Clock skew and JWKS outage behavior** (fail closed, cached keys).
- **Rate limiting and abuse controls** on PAT validation and the leak-revocation endpoint.
- **A test plan with real clients** (Claude Code, Claude Desktop/web, VS Code, Cursor, MCP Inspector) against the reference realm, because client strictness on PRM `resource` differs ([TS SDK](https://raw.githubusercontent.com/modelcontextprotocol/typescript-sdk/v1.x/src/shared/auth-utils.ts) vs [Claude](https://www.claude.com/docs/connectors/building/authentication)). Cursor's OAuth behavior was not researched here and is unverified.

## References

- @.aiwg/working/auth-oidc-design-brief.md (the design brief evaluated here)
- All external sources are listed in the frontmatter with verification dates.

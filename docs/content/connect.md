# Connecting to Fortemi with Identity

Sign in once with your organization's identity provider, and your identity
reaches every Fortemi layer: the API, MCP tools, audit records, content
provenance and background jobs. Fortemi is an OAuth resource server: it
validates your provider-issued access token (issuer, audience, signature,
lifetime) and maps your provider roles to Fortemi scopes through the
claim policy. See [Authentication](#/security-authentication) for the
operator side (issuer, audiences, claim policy file).

Reference files for this guide:

- `deploy/identity/keycloak/realm-example.json` — importable Keycloak 26.7
  realm with a `fortemi` resource client, a public `fortemi-cli` client,
  pre-registered MCP clients, groups, and locked-down client registration.
- `deploy/identity/claim-policy.example.json` — the matching claim policy.
- `tools/fortemi-login/` — the `fortemi-login` sign-in helper.

Operator settings used below: `FORTEMI_AUTH_ISSUER`
(`https://idp.example.org/realms/example` in examples),
`FORTEMI_AUTH_AUDIENCES` (the API at `https://fortemi.example.org` and MCP
at `https://fortemi.example.org/mcp`), `FORTEMI_AUTH_CLAIM_POLICY_FILE`, and
`FORTEMI_AUTH_DEFAULT_TENANT` for single-tenant deployments.

## Browser apps

Use authorization code with PKCE (S256) against the provider, requesting the
`fortemi-api` scope so the access token carries the API audience. Never place
API or MCP paths behind a cookie gate: browsers can carry the cookie, but
programmatic clients cannot, and the server still needs its own Bearer token.

First-party browser UIs should use a backend-for-frontend (BFF): a server-side
confidential client holds tokens and proxies API calls with a cookie session,
instead of keeping tokens in the browser.

## CLI and agents

Use `fortemi-login` (see `tools/fortemi-login/README.md`):

```bash
fortemi-login --issuer https://idp.example.org/realms/example login
curl -H "Authorization: Bearer $(fortemi-login token)" \
  https://fortemi.example.org/api/v1/notes
fortemi-login whoami    # GET /api/v1/me
fortemi-login logout    # revokes at the provider, deletes local tokens
```

`login` uses a loopback redirect with PKCE by default and opens the browser;
`login --device` uses the device grant for headless hosts (SSH sessions,
containers). Refresh tokens live in the OS keychain; the file fallback needs
`--allow-file-store` and is permission-pinned (0600 in a 0700 directory).

`fortemi-login token` prints a fresh access token and is made for command
substitution and for agent `headersHelper` commands, giving per-user identity
where an interactive OAuth flow is awkward.

## MCP clients: Claude Code, Claude Desktop, Cursor, VS Code

Point the client at `https://fortemi.example.org/mcp`. Clients discover OAuth
from the protected-resource metadata there: no manual client id or secret is
needed. The realm pre-registers the known clients (`claude-code` and `vscode`
with loopback redirects, `claude-desktop` with
`https://claude.ai/api/mcp/auth_callback`); anything else falls back to
locked-down dynamic registration, or to Client ID Metadata Documents where the
provider advertises them. Ready-to-paste configuration:

```bash
fortemi-login mcp-config --client claude-code
fortemi-login mcp-config --client claude-desktop
fortemi-login mcp-config --client cursor
fortemi-login mcp-config --client vscode
```

Fallback without OAuth: mint a personal access token after signing in (see
[Authentication](#/security-authentication)) and present it as a static
`Authorization: Bearer <ACCESS_TOKEN>` header. OAuth sign-in stays preferred:
tokens from the provider expire within minutes and refresh automatically,
while a personal access token is a long-lived secret you must store and
rotate yourself.

Note: the `cursor` pre-registration is marked to confirm (see the `CONFIRM`
marker in `realm-example.json`): verify Cursor's current MCP OAuth redirect
against the vendor docs before relying on it.

## Scripts and services

Scripts that cannot run OAuth use a personal access token: one token per
script, the narrowest scopes it needs, a mandatory expiry, stored as a secret
and passed as `Authorization: Bearer <ACCESS_TOKEN>`. Prefer workload-identity
exchange over stored tokens in CI where the platform supports it.

Services use the client-credentials grant with a confidential client (copy
`fortemi-service-example`, one copy per service, fresh secret per copy).
Service tokens are recorded as machine principals, never as users: they cannot
mint personal access tokens, and the reference realm grants them no
token-exchange or impersonation rights.

## Scopes

| Scope | Grants |
|---|---|
| `read` | Read tenant content, search, and memory. |
| `write` | `read`, plus create, update, delete, imports and jobs. |
| `admin` | `read` and `write`, plus administration (tenants, users, keys). |
| `mcp` | Call MCP tools (combined with `read`/`write` as needed). |

Provider roles map to scopes in the claim policy: `fortemi-read`,
`fortemi-write`, `fortemi-admin`, `fortemi-mcp` client roles of the `fortemi`
resource client arrive in `resource_access.fortemi.roles`. Map from those
client roles (or full group paths), never from bare group names: a nested
group elsewhere in the tree can share a bare name and would silently grant the
same scope. The public `fortemi-cli` client carries a scope ceiling of
`read`, `write`, `mcp`: even a token that names the admin role yields no
`admin` scope through that client; administration needs a separate client.

Multi-tenant deployments read the tenant from the `fortemi_tenant_id` claim,
sourced from a provider attribute only realm admins can edit.
Single-tenant deployments ignore the claim and use
`FORTEMI_AUTH_DEFAULT_TENANT`.

## Audiences

The API resource is `https://fortemi.example.org`; the MCP resource is
`https://fortemi.example.org/mcp`. The server accepts both audiences, so one
user token works on both. Request the `fortemi-api` and `fortemi-mcp` scopes
so the provider embeds both audiences. Keycloak honors RFC 8707 `resource`
indicators only behind the experimental `--features=resource-indicators`
flag; without it, the audience scopes above are the mechanism, and clients
must request them (they appear in `scopes_supported` and in challenges).

## Network reachability

Hosted assistants (for example cloud-hosted Claude) reach both Fortemi and
the provider from vendor cloud networks. Both `https://fortemi.example.org`
(including `/mcp`) and the issuer must be publicly reachable over TLS; a WAF
in front of the provider can break the flow. Local clients (Claude Code,
VS Code) connect from the user's machine and need no inbound access.

## Data isolation

There is no per-user data isolation within a tenant: any member with `read`
sees all tenant content. Plan team boundaries as separate tenants if members
must not see each other's content.

## Troubleshooting

| Symptom | Meaning | Action |
|---|---|---|
| `401` without `WWW-Authenticate` details | No token, or the token is invalid, expired, or for the wrong audience/issuer. | Sign in again; confirm the token's audience includes the resource you called. |
| `403` with `error="insufficient_scope"` | Authenticated, but the token lacks a scope the operation needs. The challenge lists the required scopes. | Request the missing scope, or ask for the provider role that maps to it. An `admin` failure through `fortemi-cli` is by design: use the admin client. |
| `403` with `client_not_allowed` | The token's client is not in the claim policy allowlist. | Use a registered client (`fortemi-cli`, a pre-registered MCP client, or an approved service client). |
| `401` from a script after weeks of working | The personal access token expired or was revoked, or its owner was disabled. | Mint a fresh token after signing in. |
| MCP client loops on consent | The audience scope was never requested, so the token names no Fortemi audience. | Request the `fortemi-api`/`fortemi-mcp` scopes the challenge names. |

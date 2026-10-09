# Authentication Guide

Fortémi supports two authentication mechanisms: **API Keys** (simple, token-based) and **OAuth2** (full authorization flow with PKCE). Choose the method that best fits your use case.

The public Community Edition profile uses Fortemi's self-hosted OAuth/API-key
compatibility layer. Internal hosted deployments use a separate `hosted-auth`
build profile: external OIDC bearer tokens are verified against the configured
issuer and audience, a canonical tenant claim is required, and hosted startup
fails closed when identity configuration or tenant lookup is unavailable. The
hosted routes and requirements described below are not mounted by the public
Community Edition image.

## Table of Contents

- [Quick Start](#quick-start)
- [API Key Authentication](#api-key-authentication)
- [OAuth2 Authentication](#oauth2-authentication)
- [Scopes and Permissions](#scopes-and-permissions)
- [Rate Limiting](#rate-limiting)
- [Error Handling](#error-handling)
- [Security Best Practices](#security-best-practices)

---

## Quick Start

### For Simple Scripts and CLI Tools

Use API keys for straightforward authentication:

```bash
# Create an API key
curl -X POST http://localhost:3000/api/v1/api-keys \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Script",
    "description": "Script for daily note imports",
    "scope": "read write",
    "expires_in_days": 90
  }'

# Response (save the api_key value - shown only once!)
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "<API_KEY>",
  "key_prefix": "<API_KEY_PREFIX>",
  "name": "My Script",
  "scope": "read write",
  "expires_at": "2024-04-15T10:00:00Z",
  "created_at": "2024-01-15T10:00:00Z"
}
```

### For Applications with User Context

Use OAuth2 for applications that need user-scoped access:

```bash
# Register your application
curl -X POST http://localhost:3000/oauth/register \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "My App",
    "redirect_uris": ["http://localhost:3000/callback"],
    "grant_types": ["authorization_code", "refresh_token"],
    "scope": "read write"
  }'
```

### Internal hosted OIDC profile

The hosted profile requires all of the following at startup:

```text
FORTEMI_MULTI_TENANT=true
REQUIRE_AUTH=true
ISSUER_URL=https://fortemi.example.com
FORTEMI_AUTH_ISSUER=https://identity.example.com/
FORTEMI_AUTH_AUDIENCE=<DEPLOYMENT_AUDIENCE>
FORTEMI_AUTH_TENANT_CLAIM=fortemi:tenant_id
```

`FORTEMI_AUTH_CLOCK_SKEW_SECONDS` defaults to 60 and is bounded to `0..60`;
`FORTEMI_AUTH_JWKS_CACHE_CAPACITY` defaults to 128 and is bounded to
`1..4096`; `FORTEMI_AUTH_HTTP_TIMEOUT_SECONDS` defaults to 5 and is bounded to
`1..30`. The external issuer and audience are required and cannot be blank.
Values and identity-provider credentials must come from the hosted
configuration/secret authority, not committed examples. This profile is
distinct from Fortemi's self-hosted `/oauth/*` authorization-code and
client-credentials flows.

#### Issuer URL rules

`ISSUER_URL` names Fortemi's own local authorization server issuer and appears
in Fortemi OAuth discovery and RFC 9207 `iss` authorization responses.
`FORTEMI_AUTH_ISSUER` names the external identity provider whose JWTs Fortemi
verifies. Hosted deployments still fall back to `ISSUER_URL` as the external
issuer when `FORTEMI_AUTH_ISSUER` is unset, for compatibility with older
configuration, but new deployments should set both values separately. Startup
refuses equal explicit values.

Fortemi removes trailing slashes from issuer URLs and nothing else, and token
`iss` claims must equal the configured external issuer exactly
(case-sensitive). The verifier reads discovery metadata from
`<FORTEMI_AUTH_ISSUER>/.well-known/openid-configuration`.

| Accepted | Rejected |
|----------|----------|
| `https://idp.example.com` | `http://idp.example.com` (plain HTTP to a public host) |
| `https://idp.example.com/realms/acme` | `https://idp.example.com/realms/acme?x=1` (query) |
| `https://idp.example.com/auth/realms/acme` | `https://idp.example.com/realms/acme#x` (fragment) |
| `https://idp.example.com/realms/acme/` (stored without the slash) | `https://user@idp.example.com/realms/acme` (userinfo) |
| | `https://idp.example.com/realms//acme`, `/./`, `/../`, `%2F` (ambiguous path) |

Path-bearing issuers need no override. `FORTEMI_ALLOW_LOCAL_ISSUER` is a
separate control: it governs only local and private destinations (loopback,
RFC 1918 and IPv6 unique-local or link-local addresses, single-label host
names, and `.localhost`, `.local`, `.internal`, `.lan` or `.home.arpa`
names). It permits such a host over HTTP or HTTPS, and it never permits
plain HTTP to a public host. Leave it unset for hosted deployments.

If your IdP puts a trailing slash in `iss` (for example
`https://tenant.example.com/`), the exact match fails. Configure the IdP to
issue the slash-free form, or front it with an issuer URL that does not end
in a slash.

##### Worked example: Keycloak

A Keycloak realm named `acme` on `https://idp.example.com` issues tokens with
`"iss": "https://idp.example.com/realms/acme"`. Older Keycloak releases that
still serve under `/auth` issue `https://idp.example.com/auth/realms/acme`.
Copy the value from the realm's discovery document rather than typing it:

```bash
curl -s https://idp.example.com/realms/acme/.well-known/openid-configuration \
  | jq -r .issuer
# https://idp.example.com/realms/acme
```

```text
FORTEMI_MULTI_TENANT=true
REQUIRE_AUTH=true
ISSUER_URL=https://fortemi.example.com
FORTEMI_AUTH_ISSUER=https://idp.example.com/realms/acme
FORTEMI_AUTH_AUDIENCE=<DEPLOYMENT_AUDIENCE>
FORTEMI_AUTH_TENANT_CLAIM=fortemi:tenant_id
# FORTEMI_ALLOW_LOCAL_ISSUER stays unset.
```

In Keycloak, add an audience mapper so access tokens carry
`<DEPLOYMENT_AUDIENCE>` in `aud`, and a claim mapper that writes the tenant id
to `fortemi:tenant_id`.

#### MCP with an external issuer

The MCP server never parses or verifies external JWTs itself. For any bearer that
is not a Fortemi `mm_at_`/`mm_key_` token, it calls the API's hidden
`GET /api/v1/auth/token-info` route with that bearer. The request passes through
the same middleware as REST: issuer, audience, JWKS signature, expiry, tenant
claim and active tenant, plus the central authorization policy (which requires
`read`). The MCP server then requires `mcp` (or `admin`) in the returned scopes.

| Presented token | MCP result |
|---|---|
| Valid for `FORTEMI_AUTH_AUDIENCE`, active tenant, `mcp` and `read` scopes | Accepted |
| Wrong issuer, wrong audience, bad signature or expired | 401 `invalid_token` |
| Unknown or inactive tenant, or missing `read` | 403 `access_denied` (`WWW-Authenticate: Bearer realm="mcp", resource_metadata=...`, no `insufficient_scope`) |
| Verified but without `mcp` | 403 `insufficient_scope` (`WWW-Authenticate: ... error="insufficient_scope", scope="mcp"`) |
| Any refresh token (`mm_rt_…`, or introspection `token_type` other than `Bearer`) | 401 |
| Verifier unreachable | 503 `temporarily_unavailable` |

A presented external token that fails is always rejected; it never downgrades to
anonymous access. `FORTEMI_MULTI_TENANT=true` makes the MCP server require
authentication regardless of `REQUIRE_AUTH`. Fortemi-issued tokens keep the
self-hosted introspection path unchanged, apart from the refresh-token rejection.
Token values are never logged.

`scripts/test-mcp-external-issuer.sh` exercises this table end to end: it starts a
disposable PostgreSQL, Redis, a TLS OIDC fixture issuer with ephemeral keys, the
hosted API and the MCP server, then checks each row over MCP HTTP, the RFC 9728
metadata, and that no presented token appears in either log.

Configure the MCP container with the same Fortemi API URL/issuer metadata as the
API and set `MCP_RESOURCE_URI` to the API's `FORTEMI_AUTH_AUDIENCE`. RFC 9728 metadata at
`/.well-known/oauth-protected-resource` then advertises the external
authorization server and the audience clients must request. The server logs a
startup warning when the two values differ.

#### Tenant bootstrap

Admission requires the claim's tenant to exist as an `active` row in
`tenant_registry`; unknown, suspended and soft-deleted tenants are rejected.
Provision tenants with the idempotent bootstrap command instead of SQL:

```bash
MIGRATION_DATABASE_URL=<MIGRATION_DATABASE_URL> \
  matric-api admin bootstrap --slug acme --display-name "Acme" --json
```

The command prints the tenant id to emit in the configured tenant claim and
seeds the tenant's default memory. See
[Hosted tenant bootstrap](../deployment/hosted-bootstrap.md).

#### Mapping IdP groups and roles to scopes

External IdPs express authorization as groups or roles. Set
`FORTEMI_AUTH_CLAIM_POLICY_FILE` to a JSON policy, mounted read-only, to derive
Fortemi scopes from a verified claim. The file is read once at startup. A missing,
unreadable or invalid file stops the server. Mapped scopes must be `read`,
`write`, `admin` or `mcp`; `system:*` scopes cannot come from a directory group.
Matching is exact and case-sensitive. Unmatched values grant nothing, and a token
with no matching value receives no scopes (deny by default). Scopes are evaluated
by the central authorization policy exactly like token scopes. Authorization
decision audit records `principal_kind` and the ids of the matched rules in
`scope_grants`, never the raw group list.

Keycloak realm roles, with a service-account client for machine callers:

```json
{
  "scope_mapping": {
    "claim": "realm_access.roles",
    "rules": [
      {"id": "kc-readers", "value": "fortemi-read",  "scopes": ["read"]},
      {"id": "kc-writers", "value": "fortemi-write", "scopes": ["read", "write"]},
      {"id": "kc-agents",  "value": "fortemi-agent", "scopes": ["read", "mcp"]},
      {"id": "kc-admins",  "value": "fortemi-admin", "scopes": ["admin"]}
    ]
  },
  "clients": {"claim": "azp", "allowed": ["fortemi-web"], "service": ["fortemi-etl"]}
}
```

Keycloak client roles, using a JSON Pointer because the client id contains a dot:

```json
{"scope_mapping": {"claim": "/resource_access/fortemi.api/roles",
  "rules": [{"id": "api-read", "value": "reader", "scopes": ["read"]}]}}
```

A generic OIDC provider with a `groups` claim, keeping any `scope` the token
already carries:

```json
{"scope_source": "union",
 "scope_mapping": {"claim": "groups",
  "rules": [{"id": "analysts", "value": "/acme/analysts", "scopes": ["read"]}]}}
```

| Field | Meaning |
|---|---|
| `scope_source` | `mapping` (default when a mapping is set): mapped scopes only. `union`: token `scope` plus mapped scopes. `token`: ignore groups. |
| `scope_mapping.claim` | Dot path, or a JSON Pointer starting with `/`. The value must be an array of strings or one string; any other shape returns 403 `invalid_authorization_claim`. |
| `clients.claim` | Claim naming the OAuth client (default `azp`). |
| `clients.allowed` | Optional allowlist. Tokens from other clients, or without the claim, get 403 `client_not_allowed`. |
| `clients.service` | Clients whose tokens are service principals (`principal_kind: service`), for example a Keycloak client-credentials service account. The tenant claim is still required. |

Changing the mapping and restarting changes authorization for existing tokens;
no token format change is needed. Without the variable, token scopes pass through
unchanged.

#### Hosted note and event qualification

The migrated hosted routes include ordinary `POST /api/v1/notes`, note list,
owner detail/delete, and `GET /api/v1/events`. Creation resolves referenced
collections, document types, tags and queue writes through the same tenant
transaction. Missing tenant-visible configuration fails the request and rolls
back its writes. Use `"pipeline": []` for store-only qualification; enqueueing
NLP jobs does not establish tenant-worker execution readiness. Apply the
September 8 tenant tag migrations with the migration identity before upgrading
the API. They qualify default scheme notation and flat tag identity by tenant;
existing data and tenant-qualified foreign-key guards are preserved. Provision
any required default SKOS scheme or document types under the intended tenant,
not by sharing the personal tenant's seed rows; `matric-api admin bootstrap`
seeds the default scheme, embedding configuration and embedding set.

Hosted stale-running job recovery now enumerates active tenants through the
service control plane and updates each tenant in a short scoped transaction.
Startup performs one bounded page; periodic passes continue the cursor. Partial
failures and pass deadlines are reported as incomplete recovery, not empty
success. Recovery does not by itself establish tenant-worker execution readiness.

The candidate also includes a service-owned bounded claim dispatcher with an
explicit handler allowlist and post-commit, attempt-fenced settlement capability.
Cycle90 connects it to an explicit hosted-only registry and claim drain. The
document-type handler is registered; legacy personal handlers never receive
hosted claims. Global/per-archive pause is checked before claims and shutdown
retains attempt-fenced settlement. Public hosted readiness remains false.

HostedJobHandler/HostedJobContext expose bounded claim-bound content work and
fenced async progress. A pool-free document-type handler now implements scoped
detection, assignment/access/provenance and durable job-bound replay, including
native tenant/archive-addressed membership follow-ups. Its committed progress
and terminal events use tenant/archive context without raw bridge lookups/writes.
Cycle91 adds typed temporary-database retries and a bounded read-only note.updated
snapshot in fenced completion. Both success events follow acknowledged commit;
failures publish neither and leave settlement to recovery. Other handlers,
follow-up embedding execution, index events, scoped queue summaries and complete
live SSE lifecycle remain unqualified. Global queue
summaries are emitted only in personal mode. Content and terminal settlement
commit separately; no exactly-once external-effect guarantee is implied.

The legacy WebSocket (`GET /api/v1/ws`) and NDJSON ingest stream
(`POST /api/v1/ingest/stream`) are closed in hosted mode (#1163): they answer
`401` without a valid bearer and `503` with one, even if `REQUIRE_AUTH=false`,
because neither is tenant-bound yet. In community mode they keep their own
handling (the WebSocket is retired with `410` when auth is required; the ingest
stream validates its per-stream token).

Health endpoints are split by what they read (#1164). Liveness and readiness
(`/health`, `/livez`, `/readyz`) and the aggregate `/api/v1/health/streaming`
probe stay public in every mode. The knowledge diagnostics
(`/api/v1/health/knowledge`, `orphan-tags`, `stale-notes`, `unlinked-notes`,
`tag-cooccurrence`, `access-frequency`) read tenant notes, tags and links: they
stay public in community mode, and hosted mode answers `401` without a valid
bearer and `503` with one, because they do not yet run on the tenant
transaction.

Hosted SSE requires a bearer header with the canonical tenant and `mcp` scope.
Archive names are authorized first; resolved schemas filter live/replay events.
Missing tenant/memory attribution is rejected on hosted streams. Consumer request
names and canonical event schemas must be resolved separately; a test transport
adapter is not application acceptance.
Memory access also requires `read` scope. The default subscription is the caller's default memory, never a cross-tenant
monitoring stream. `memory=default` and `memory=public` select tenant-protected
public tables when no tenant archive exists. Other archive names must resolve
under that tenant's RLS transaction. Hosted archive selection does not share the
personal profile's process-wide default cache or attempt runtime migrations.

The request releases its database transaction before streaming. Live and replay
frames require an explicit matching envelope tenant; unattributed global events
are withheld. Note-created/deleted events publish only after commit. A hosted
stream closes at verified token expiry: obtain a replacement token, close the
old connection, reopen with its bearer header, and refresh the note list. Query
stream tokens remain a personal-profile feature and cannot replace hosted
canonical identity. Existing TLS, JWT, RLS and durable authorization-audit checks
remain enforced. These route gates do not enable `hosted_multi_tenant_ready` or
qualify unrelated routes or worker execution. The suite audit remains NO-GO.
The generated [hosted route qualification matrix](../deployment/hosted-route-qualification.md)
lists every operation as qualified, gap, excluded or public for a single-tenant
dedicated deployment; `/api/v1/system/compatibility` reports the same profile as
`deployment.hosted_profile`.


### Custom OIDC certificate trust

For a Keycloak or other supported issuer using a private CA, set
`FORTEMI_AUTH_CA_BUNDLE` to a PEM certificate file readable by the API process.
The hosted OIDC verifier adds every certificate in the bundle to its default
trust roots for both discovery and JWKS requests. HTTPS, hostname validation,
certificate validation, issuer/audience validation, and the redirect prohibition
remain enforced. The bundle does not change inbound server TLS or trust for
unrelated HTTP clients. No private keys belong in this file.

The variable is optional. When explicitly set, a blank path, unreadable or empty
file, malformed PEM/DER, or non-certificate PEM block prevents startup with a
`FORTEMI_AUTH_CA_BUNDLE` diagnostic. There is no fallback on configuration errors.
The file is read once during initialization; restart the API after changing it.
For CA rotation, deploy a bundle containing both old and new CA certificates,
restart, rotate the issuer certificate, then remove the retired CA and restart.

For a standalone process, use an absolute path:

```bash
export FORTEMI_AUTH_CA_BUNDLE=/etc/fortemi/trust/oidc-ca.pem
matric-api
```

For a container running the standalone API, add this to its Compose service
(alongside its existing hosted configuration):

```yaml
services:
  api:
    environment:
      FORTEMI_AUTH_CA_BUNDLE: /etc/fortemi/trust/oidc-ca.pem
    volumes:
      - ./trust/oidc-ca.pem:/etc/fortemi/trust/oidc-ca.pem:ro
```

For the bundle image, put the same environment variable and mount on the
bundle service when configuring a deployment that satisfies hosted admission:

```yaml
services:
  fortemi:
    environment:
      FORTEMI_AUTH_CA_BUNDLE: /etc/fortemi/trust/oidc-ca.pem
    volumes:
      - ./trust/oidc-ca.pem:/etc/fortemi/trust/oidc-ca.pem:ro
```

The host file must exist before starting either container and be readable by the
API runtime user. Recreate/restart the service after replacing the mounted file.
The standard API and bundle Dockerfiles compile the public `hosted-auth`
capability by default through `ARG FORTEMI_API_FEATURES=hosted-auth`. This does
not activate hosted mode or satisfy all hosted deployment prerequisites.
`FORTEMI_MULTI_TENANT` remains opt-in; hardened database roles, audit, quotas,
scanning, and key custody retain their existing admission checks. In particular,
these default images do not compile a KMS backend. Internal image builders select
`--build-arg FORTEMI_API_FEATURES=hosted-auth,kms-vault` for OpenBao or
`hosted-auth,kms-aws` for AWS. Runtime `FORTEMI_KEY_PROVIDER` selects the backend;
its credentials/configuration and every other hosted prerequisite remain required.
The Integro Labs environment requires [OpenBao Transit](#/security-openbao-kms). A bare Cargo
build continues to require explicit `--features hosted-auth` for this verifier.

Custom trust roots do not relax issuer validation. The hosted provider still
requires HTTPS and verifies TLS certificates and host names; a CA bundle does
not make HTTP or an untrusted certificate acceptable. Keycloak realm paths need
no override (see [Issuer URL rules](#issuer-url-rules)). Configure only the
intended issuer and trust roots for the qualification deployment.

---

## API Key Authentication

API keys are ideal for:
- Server-to-server integrations
- CLI tools and scripts
- Personal automation
- MCP server (stdio mode)

### Creating an API Key

**Endpoint:** `POST /api/v1/api-keys`

**Request:**
```json
{
  "name": "Production Integration",
  "description": "API access for production app",
  "scope": "read write",
  "expires_in_days": 365
}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "<API_KEY>",
  "key_prefix": "<API_KEY_PREFIX>",
  "name": "Production Integration",
  "scope": "read write",
  "expires_at": "2025-01-15T10:00:00Z",
  "created_at": "2024-01-15T10:00:00Z"
}
```

**Important:** The `api_key` field is only returned once. Store it securely immediately.

### Using an API Key

Include the key in the `Authorization` header with the `Bearer` scheme:

```bash
curl http://localhost:3000/api/v1/notes \
  -H "Authorization: Bearer <API_KEY>"
```

**Python Example:**
```python
import requests

API_BASE = "http://localhost:3000"
API_KEY = "<API_KEY>"

headers = {
    "Authorization": f"Bearer {API_KEY}",
    "Content-Type": "application/json"
}

# Create a note
response = requests.post(
    f"{API_BASE}/api/v1/notes",
    headers=headers,
    json={
        "content": "My new note",
        "tags": ["important"]
    }
)
print(response.json())
```

**JavaScript Example:**
```javascript
const API_BASE = "http://localhost:3000";
const API_KEY = "<API_KEY>";

async function createNote(content, tags = []) {
  const response = await fetch(`${API_BASE}/api/v1/notes`, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${API_KEY}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({ content, tags })
  });
  return response.json();
}

const result = await createNote("My new note", ["important"]);
console.log(result);
```

### Managing API Keys

**List All Keys (shows prefix only):**
```bash
GET /api/v1/api-keys
```

**Response:**
```json
{
  "api_keys": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "key_prefix": "<API_KEY_PREFIX>",
      "name": "Production Integration",
      "description": "API access for production app",
      "scope": "read write",
      "rate_limit_per_minute": 60,
      "rate_limit_per_hour": 1000,
      "last_used_at": "2024-01-15T14:30:00Z",
      "use_count": 1247,
      "is_active": true,
      "expires_at": "2025-01-15T10:00:00Z",
      "created_at": "2024-01-15T10:00:00Z"
    }
  ]
}
```

**Revoke a Key:**
```bash
DELETE /api/v1/api-keys/{id}
```

### API Key Format

- **Format:** `mm_key_{32_random_chars}`
- **Prefix:** First 12 characters (e.g., `<API_KEY_PREFIX>`) shown in listings
- **Storage:** SHA256 hash stored in database
- **Expiration:** Optional, defaults to no expiration

---

## OAuth2 Authentication

OAuth2 is ideal for:
- Web applications with user authentication
- Mobile applications
- Third-party integrations requiring user consent
- MCP server (HTTP mode)

Fortémi's current **self-hosted operator compatibility profile** implements OAuth 2.0
with:
- **Open Dynamic Client Registration** (RFC 7591 compatibility)
- **Authorization Code Flow** with PKCE (RFC 7636)
- **Client Credentials Grant**
- **Refresh Tokens** (30-day expiration)
- **Token Introspection** (RFC 7662)
- **Token Revocation** (RFC 7009)

This is not a hosted-strict launch profile. Hosted-strict OAuth remains unavailable
until its registration, client-type, authorization, and resource-binding owners
land. In particular, current open registration must not be exposed as a
hosted-safe default.

### Discovery Endpoint

OAuth2 server metadata is available at:

```bash
GET /.well-known/oauth-authorization-server
```

**Response:**
```json
{
  "issuer": "http://localhost:3000",
  "authorization_endpoint": "http://localhost:3000/oauth/authorize",
  "token_endpoint": "http://localhost:3000/oauth/token",
  "registration_endpoint": "http://localhost:3000/oauth/register",
  "introspection_endpoint": "http://localhost:3000/oauth/introspect",
  "revocation_endpoint": "http://localhost:3000/oauth/revoke",
  "response_types_supported": ["code"],
  "grant_types_supported": [
    "authorization_code",
    "client_credentials",
    "refresh_token"
  ],
  "token_endpoint_auth_methods_supported": [
    "client_secret_basic",
    "client_secret_post"
  ],
  "scopes_supported": ["read", "write", "admin", "mcp"],
  "code_challenge_methods_supported": ["S256"],
  "authorization_response_iss_parameter_supported": true
}
```

`registration_endpoint` appears only when dynamic registration is `enabled`
(see [Registration policy](#registration-policy)).

### 1. Register Your Application

**Endpoint:** `POST /oauth/register`

**Request:**
```json
{
  "client_name": "My Application",
  "client_uri": "https://myapp.example.com",
  "redirect_uris": ["https://myapp.example.com/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "scope": "read write",
  "contacts": ["admin@myapp.example.com"]
}
```

**Response:**
```json
{
  "client_id": "mm_AbCdEfGh12345678901234",
  "client_secret": "<MCP_CLIENT_SECRET>",
  "client_id_issued_at": 1705320000,
  "client_secret_expires_at": 0,
  "client_name": "My Application",
  "redirect_uris": ["https://myapp.example.com/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "scope": "read write",
  "token_endpoint_auth_method": "client_secret_basic"
}
```

Fortémi does not implement RFC 7592 client management routes, so the response
does not include `registration_access_token` or `registration_client_uri`
(#944). `token_endpoint_auth_method` must be `client_secret_basic` (default) or
`client_secret_post`; other values are rejected with `400`.

#### Registration policy

`FORTEMI_OAUTH_DYNAMIC_REGISTRATION` controls who may call `POST /oauth/register`:

| Mode | Behavior | Discovery `registration_endpoint` |
|------|----------|-----------------------------------|
| `enabled` | Open RFC 7591 registration. Default when `FORTEMI_MULTI_TENANT` is not `true`. | Advertised |
| `admin` | Requires `Authorization: Bearer <credential>` with the `admin` scope (an API key or access token): missing or invalid credential `401`, other scopes `403`. | Not advertised |
| `disabled` | Always `403` with detail `Dynamic client registration is disabled on this server.`; nothing is parsed or stored. Default when `FORTEMI_MULTI_TENANT=true`, where only `admin` and `disabled` are accepted. | Not advertised |

When registration is enabled, every redirect URI is validated before storage.
Allowed forms are HTTPS, RFC 8252 loopback HTTP (`localhost`, `127.0.0.1` or
`[::1]`, any port), or a reverse-DNS private-use scheme such as
`com.example.app:/cb`. Wildcards, userinfo, fragments and non-loopback HTTP
redirects are rejected with `400 invalid_redirect_uri`.

Operators provision first-party clients without the public endpoint:

```bash
matric-api admin oauth-client register --name "Operator tool" \
  --grant-types client_credentials --scope "read write" --json
```

The command writes to the database named by `DATABASE_URL` and prints the client
secret once. The Docker bundle uses it for the MCP introspection client.

**Important:** Save `client_id` and `client_secret` securely. The secret is only shown once.

### 2. Authorization Code Flow (with PKCE)

#### Step 1: Generate PKCE Parameters

```python
import secrets
import hashlib
import base64

# Generate code verifier (random 43-128 chars)
code_verifier = base64.urlsafe_b64encode(secrets.token_bytes(32)).decode('utf-8').rstrip('=')

# Generate code challenge (SHA256 hash)
code_challenge = base64.urlsafe_b64encode(
    hashlib.sha256(code_verifier.encode()).digest()
).decode('utf-8').rstrip('=')

print(f"Verifier: {code_verifier}")
print(f"Challenge: {code_challenge}")
```

#### Step 2: Redirect User to Authorization Endpoint

```
GET /oauth/authorize?
  response_type=code&
  client_id=mm_AbCdEfGh12345678901234&
  redirect_uri=https://myapp.example.com/callback&
  scope=read write&
  state=random_state_value&
  code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM&
  code_challenge_method=S256
```

Fortémi validates the request before showing anything (#943):

- An unknown or inactive `client_id`, or a `redirect_uri` that is not registered for
  the client, gets a local `400` error page. Fortémi never redirects to an
  unregistered URI.
- Other errors go back to the registered `redirect_uri` with `error`, `state` and
  `iss`: `unsupported_response_type`, `unauthorized_client` (the client is not
  registered for `authorization_code`), `invalid_scope` (every requested scope
  must be issued by the server and registered for the client; with no `scope`,
  the registered scope is requested) and `invalid_request` (PKCE must use `S256`
  with a 43-128 character challenge; `plain` is refused).

Whether approval requires an **authenticated resource owner** depends on
`FORTEMI_OAUTH_AUTHORIZE_OWNER_AUTH`:

| Value | Resource owner | Default |
|-------|----------------|---------|
| `none` | Approve-only: anyone who reaches the consent page can approve, and the code records no owner. This is the pre-#943 behavior, kept for compatibility; the API logs a warning at startup. **Less secure:** use `api_key`, or `trusted_header` behind a signing-in proxy such as oauth2-proxy. | Community |
| `api_key` | The person approving enters a Fortémi API key or access token on the consent page. It must hold every requested scope (`admin` holds all). Without one, approval never produces a code. The code records `api_key:<id>`, `oauth_client:<id>` or `oauth_user:<id>`. | |
| `trusted_header` | A reverse proxy that has already signed the user in (for example oauth2-proxy) sends the user in `FORTEMI_OAUTH_OWNER_HEADER`, such as `X-Forwarded-Email`. The header is honored only when the immediate peer is in `FORTEMI_TRUSTED_PROXY_CIDRS`; startup fails without them. The proxy's admission is the authorization decision. The code records `proxy:<value>`. | |
| `api_key,trusted_header` | Either. | |
| `disabled` | No browser authorization. Valid requests are answered with `error=access_denied`. | Hosted (`FORTEMI_MULTI_TENANT=true`), where it is the only accepted value |

`trusted_header` owners are limited by `FORTEMI_OAUTH_OWNER_HEADER_SCOPES`,
which defaults to `read mcp`. A proxy-authenticated owner cannot grant scopes
outside that ceiling unless the operator expands it.

When `FORTEMI_AUTH_ISSUER` is set, or when hosted mode is enabled, Fortemi
assumes an external IdP is authoritative and fails closed: local OAuth
registration, browser authorization and `/oauth/token` issuance are disabled.
They can be re-enabled only by setting `FORTEMI_OAUTH_ALLOW_LOCAL_AS=true` and
choosing a real owner-auth method (`api_key`, `trusted_header`, or both);
`none` is refused at startup. In this external-IdP mode, the consent page does
not accept `mm_key_` API keys as owner proof. Deployments that relied on
identity-free consent must either keep the external issuer unset for local-only
operation or migrate to authenticated owner consent before enabling an external
IdP.

In every mode, including `none`, the ceremony is protected on the server side:

- The GET stores the validated request in a server-side transaction. The form
  carries only an opaque transaction id and a CSRF token; a per-transaction
  `HttpOnly; SameSite=Strict` cookie (`Secure` when `ISSUER_URL` is HTTPS)
  binds it to the browser. Client, redirect, scope and PKCE values cannot be
  tampered with in the form.
- The POST fails with a local error page when the transaction is unknown,
  expired (10 minutes) or already used, when the CSRF token or cookie does not
  match, or when `Sec-Fetch-Site` or `Origin` shows a cross-site submission.
  Five failed sign-in attempts end the transaction.
- **Deny** is a server-side decision that redirects only to the transaction's
  registered `redirect_uri`, with `error=access_denied`, `state` and `iss`.
- Every response sends `Content-Security-Policy: frame-ancestors 'none'`,
  `X-Frame-Options: DENY` and `Cache-Control: no-store`. The page has no script.
- Transactions live in API process memory. Run one API replica, or route a
  browser's requests to the same replica.

This browser flow is for user consent. Machine access without a person uses the
client-credentials grant (step 5) or an API key; those tokens carry no resource
owner.

#### Step 3: Handle Redirect with Authorization Code

After approval, the user is redirected to:
```
https://myapp.example.com/callback?code=AUTH_CODE_HERE&state=random_state_value&iss=https%3A%2F%2Fmemory.example.com
```

`iss` (RFC 9207) equals the discovery `issuer`; discovery advertises
`authorization_response_iss_parameter_supported: true`. Clients should reject a
response whose `iss` does not match.

#### Step 4: Exchange Code for Tokens

**Endpoint:** `POST /oauth/token`

**Request (using client_secret_basic):**
```bash
curl -X POST http://localhost:3000/oauth/token \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=authorization_code" \
  -d "code=AUTH_CODE_HERE" \
  -d "redirect_uri=https://myapp.example.com/callback" \
  -d "code_verifier=VERIFIER_FROM_STEP1"
```

**Response:**
```json
{
  "access_token": "<ACCESS_TOKEN>",
  "token_type": "Bearer",
  "expires_in": 3600,
  "refresh_token": "<REFRESH_TOKEN>",
  "scope": "read write"
}
```

**Python Example:**
```python
import requests
import base64

client_id = "mm_AbCdEfGh12345678901234"
client_secret = "<MCP_CLIENT_SECRET>"
auth_code = "AUTH_CODE_FROM_REDIRECT"
redirect_uri = "https://myapp.example.com/callback"
code_verifier = "VERIFIER_FROM_STEP1"

# Basic auth header
credentials = f"{client_id}:{client_secret}"
auth_header = base64.b64encode(credentials.encode()).decode()

response = requests.post(
    "http://localhost:3000/oauth/token",
    headers={
        "Authorization": f"Basic {auth_header}",
        "Content-Type": "application/x-www-form-urlencoded"
    },
    data={
        "grant_type": "authorization_code",
        "code": auth_code,
        "redirect_uri": redirect_uri,
        "code_verifier": code_verifier
    }
)

tokens = response.json()
access_token = tokens["access_token"]
refresh_token = tokens["refresh_token"]
```

### 3. Using Access Tokens

Include the access token in the `Authorization` header:

```bash
curl http://localhost:3000/api/v1/notes \
  -H "Authorization: Bearer <ACCESS_TOKEN>"
```

### 4. Refreshing Tokens

Access tokens expire after 1 hour by default (configurable via `OAUTH_TOKEN_LIFETIME_SECS`). Use refresh tokens to obtain new access tokens without user interaction.

**Endpoint:** `POST /oauth/token`

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/token \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=refresh_token" \
  -d "refresh_token=<REFRESH_TOKEN>"
```

**Response:**
```json
{
  "access_token": "<ACCESS_TOKEN>",
  "token_type": "Bearer",
  "expires_in": 3600,
  "refresh_token": "<REFRESH_TOKEN>",
  "scope": "read write"
}
```

**Note:** Refresh tokens are single-use. Each refresh returns a new access token AND a new refresh token.

### 5. Client Credentials Grant

For machine-to-machine authentication without user context:

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/token \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "scope=read write"
```

**Response:**
```json
{
  "access_token": "<ACCESS_TOKEN>",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "read write"
}
```

### 6. Token Introspection

Check if a token is active and retrieve its metadata (requires client authentication).

**Endpoint:** `POST /oauth/introspect`

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/introspect \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=<ACCESS_TOKEN>"
```

**Response (active token):**
```json
{
  "active": true,
  "scope": "read write",
  "client_id": "mm_AbCdEfGh12345678901234",
  "token_type": "Bearer",
  "exp": 1705323600,
  "iat": 1705320000,
  "iss": "http://localhost:3000"
}
```

**Response (inactive token):**
```json
{
  "active": false
}
```

### 7. Token Revocation

Revoke access or refresh tokens when they're no longer needed.

**Endpoint:** `POST /oauth/revoke`

**Request:**
```bash
curl -X POST http://localhost:3000/oauth/revoke \
  -H "Authorization: Basic $(echo -n 'client_id:client_secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "token=<ACCESS_TOKEN>" \
  -d "token_type_hint=access_token"
```

**Response:** `200 OK` (always returns success per RFC 7009, even if token doesn't exist)

---

## Scopes and Permissions

Fortémi uses OAuth2 scopes to control access levels.

| Scope    | Description                                      | Permissions                                    |
|----------|--------------------------------------------------|------------------------------------------------|
| `read`   | Read-only access                                 | List/get notes, search, view tags/collections |
| `write`  | Create and update resources                      | `read` + create/update notes, tags            |
| `delete` | Delete resources                                 | `read` `write` + delete notes, purge          |
| `admin`  | Full administrative access                       | All permissions + API key management          |
| `mcp`    | MCP transport/session access                     | MCP-specific operations only                  |

### Scope Hierarchy

- `admin` includes all other scopes
- MCP transport scope is separate from REST `read`/`write`; grant explicit resource scopes for operations that read or mutate data.
- `delete` typically requires `write`
- Scopes can be combined with spaces: `"read write delete"`

### Checking Scopes in Code

```python
# The API validates scopes automatically.
# If your token lacks the required scope, you'll receive a 403 Forbidden response.

# Example: Creating a note requires 'write' scope
response = requests.post(
    "http://localhost:3000/api/v1/notes",
    headers={"Authorization": f"Bearer {token}"},
    json={"content": "New note"}
)

if response.status_code == 403:
    print("Insufficient permissions. 'write' scope required.")
```

---

## Rate Limiting

The CE limiter is process-wide. API-key-specific limit metadata is not enforced
by this limiter and must not be treated as tenant or principal isolation.
Hosted multi-tenant mode has a separate Redis-backed preview gate for
authenticated non-exempt API routes. It uses an opaque key over tenant,
principal, client, route class, and policy dimensions; it does not expose those
identifiers in response headers.

### Default Limits

- `RATE_LIMIT_ENABLED=true`
- `RATE_LIMIT_REQUESTS=100`
- `RATE_LIMIT_PERIOD_SECS=60`

### Rate Limit Headers

The CE global rate limiter does **not** emit quota-capacity headers. There are no
`X-RateLimit-Limit`, `X-RateLimit-Remaining`, or `X-RateLimit-Reset` headers, or
draft `RateLimit` / `RateLimit-Policy` fields. Its 429 response includes only
`Retry-After` with a whole-number delay in seconds. Clients cannot read a
remaining-quota value; wait at least the indicated delay before retrying.

The hosted preview emits bounded combined `RateLimit` and `RateLimit-Policy`
draft fields on admitted and denied authenticated routes. A hosted denial also
includes `Retry-After`; an unavailable Redis store or missing trusted request
context returns a non-cacheable 503 without capacity fields. These fields are
draft hints, not evidence of a billing balance or a complete tenant plan. The
remaining quota dimensions and policy work are tracked by ADR-098 and #714.

### Handling Rate Limits

**HTTP 429 Response:**
```json
{
  "type": "https://fortemi.com/problems/rate-limit-exceeded",
  "title": "Too Many Requests",
  "status": 429,
  "detail": "Rate limit or quota boundary reached.",
  "request_id": "018fd1a0-example"
}
```

**Best Practices:**
- Honor `Retry-After` and retain a bounded exponential-backoff policy
- Cache frequently accessed data
- Batch operations when possible (e.g., `bulk_create_notes`)
- In CE, back off on HTTP `429`; there is no remaining-quota header to monitor
- In hosted preview mode, treat `RateLimit` fields as bounded hints and honor `Retry-After`

**Python Example:**
```python
import time
import requests

def api_call_with_retry(url, headers, max_retries=3):
    for attempt in range(max_retries):
        response = requests.get(url, headers=headers)

        if response.status_code == 429:
            retry_after = int(response.headers.get("Retry-After", "1"))
            backoff = max(retry_after, 2 ** attempt)
            print(f"Rate limited. Waiting {backoff}s...")
            time.sleep(backoff)
            continue

        return response

    raise Exception("Max retries exceeded")
```

---

## Error Handling

### Authentication Errors

#### 401 Unauthorized

**Cause:** Missing, invalid, or expired token

```json
{
  "type": "https://fortemi.com/problems/unauthorized",
  "title": "Unauthorized",
  "status": 401,
  "detail": "Missing, malformed, expired, or invalid credentials.",
  "request_id": "018fd1a0-example"
}
```

**Resolution:**
- Verify token is included in `Authorization: Bearer {token}` header
- Check token hasn't expired (access tokens expire after 1 hour)
- Refresh token if expired (OAuth2) or generate new API key

#### 403 Forbidden

**Cause:** Valid token but insufficient permissions

```json
{
  "type": "https://fortemi.com/problems/forbidden",
  "title": "Forbidden",
  "status": 403,
  "detail": "Authenticated request denied by authorization policy or admin gate.",
  "request_id": "018fd1a0-example"
}
```

**Resolution:**
- Check token has required scope
- Request new token with broader scope
- For API keys, create new key with appropriate scope

### OAuth2 Errors

Token, introspection, and revocation endpoint errors use the Fortemi RFC 9457
problem contract. Authorization redirect denials still use RFC 6749 query
parameters on the registered redirect URI.

```json
{
  "type": "https://fortemi.com/problems/validation-error",
  "title": "Bad Request",
  "status": 400,
  "detail": "OAuth grant is invalid or expired.",
  "request_id": "018fd1a0-example"
}
```

The RFC 6749 error codes below are used **internally** to select the problem
`type`; they are not emitted as a body field. The response body carries
`type`, `title`, `status`, `detail`, and `request_id` — read those, not an
`error` field.

| RFC 6749 Code               | Description                           | Common Cause                          |
|-----------------------------|---------------------------------------|---------------------------------------|
| `invalid_request`           | Missing or malformed parameter        | Missing required field                |
| `invalid_client`            | Client authentication failed          | Wrong client_id or client_secret      |
| `invalid_grant`             | Authorization code/refresh token bad  | Expired or already used code          |
| `unauthorized_client`       | Client not authorized for grant type  | Requesting unsupported grant type     |
| `unsupported_grant_type`    | Grant type not supported              | Typo in grant_type parameter          |
| `invalid_scope`             | Requested scope invalid               | Non-existent or unauthorized scope    |

**Error Handling Example:**
```python
def exchange_code_for_token(auth_code):
    try:
        response = requests.post(
            "http://localhost:3000/oauth/token",
            headers={"Authorization": f"Basic {auth_header}"},
            data={
                "grant_type": "authorization_code",
                "code": auth_code,
                "redirect_uri": redirect_uri,
                "code_verifier": code_verifier
            }
        )

        if response.status_code >= 400:
            # Errors are RFC 9457 problem+json: read type/status/detail.
            problem = response.json()
            problem_type = problem.get("type", "")
            if problem_type.endswith("/validation-error"):
                print(f"Bad OAuth request: {problem.get('detail')}")
            elif problem_type.endswith("/unauthorized"):
                print("Client credentials invalid. Check client_id/secret.")
            else:
                print(f"OAuth error {problem.get('status')}: {problem.get('detail')}")

        response.raise_for_status()
        return response.json()

    except requests.exceptions.HTTPError as e:
        print(f"Token exchange failed: {e}")
        return None
```

---

## Security Best Practices

### Token Storage

**DO:**
- Store API keys and client secrets in environment variables or secure vaults
- Use encrypted storage for tokens on client devices
- Implement token rotation for long-lived applications
- Clear tokens on logout

**DON'T:**
- Commit tokens to version control
- Store tokens in localStorage for sensitive apps (use httpOnly cookies instead)
- Share tokens between users
- Log tokens in application logs

### PKCE for Public Clients

Always use PKCE (Proof Key for Code Exchange) for:
- Single-page applications (SPAs)
- Mobile applications
- Desktop applications
- Any client that cannot securely store secrets

### Secure Communication

- **Always use HTTPS** in production
- Validate SSL/TLS certificates
- Use certificate pinning for mobile apps

### Token Lifecycle

- **Access tokens:** 1 hour default expiration (use refresh tokens)
- **MCP access tokens:** 4 hour default expiration (longer to support interactive AI sessions)
- **Refresh tokens:** 30 days expiration (require re-authentication after)
- **API keys:** Optional expiration (recommend 90-365 days for rotation)
- **Authorization codes:** 10 minutes expiration (single-use)

#### Configurable Token Lifetimes

Token lifetimes can be tuned via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `OAUTH_TOKEN_LIFETIME_SECS` | `3600` (1 hour) | Standard access token lifetime |
| `OAUTH_MCP_TOKEN_LIFETIME_SECS` | `14400` (4 hours) | MCP access token lifetime |

**Tradeoffs:**
- **Shorter tokens** improve security posture but require more frequent re-authentication
- **Longer MCP tokens** reduce mid-session disconnects for interactive AI workflows
- Recommend not exceeding 24 hours for standard tokens or 48 hours for MCP tokens

### Scope Minimization

Request only the scopes you need:

```python
# Good: Minimal scope
scope = "read"

# Bad: Over-privileged
scope = "read write delete admin"
```

### Revocation

Revoke tokens immediately when:
- User logs out
- Security incident detected
- Token potentially compromised
- User revokes application access

### Environment-Specific Configuration

**Development:**
```bash
# .env.development
MATRIC_MEMORY_URL=http://localhost:3000
MATRIC_MEMORY_API_KEY=<API_KEY>
```

**Production:**
```bash
# .env.production (use secrets manager)
MATRIC_MEMORY_URL=http://localhost:3000
MATRIC_MEMORY_API_KEY=${VAULT_API_KEY}  # Loaded from vault
```

---

## MCP Server Authentication

The MCP server supports both authentication modes:

### Stdio Mode (API Keys)

```bash
# Set environment variable
export MATRIC_MEMORY_API_KEY="<API_KEY>"

# Run MCP server
cd mcp-server
node index.js
```

### HTTP Mode (OAuth2)

```bash
# Set transport mode
export MCP_TRANSPORT=http
export MCP_PORT=3001

# Run MCP server
cd mcp-server
node index.js
```

The MCP server will:
1. Use token introspection to validate OAuth2 access tokens
2. Store tokens per-session using AsyncLocalStorage
3. Automatically include tokens in API requests

---

## Complete Examples

### Python OAuth2 Client

```python
import requests
import secrets
import hashlib
import base64
from urllib.parse import urlencode

class MatricMemoryClient:
    def __init__(self, client_id, client_secret, redirect_uri):
        self.client_id = client_id
        self.client_secret = client_secret
        self.redirect_uri = redirect_uri
        self.base_url = "http://localhost:3000"
        self.access_token = None
        self.refresh_token = None

    def get_authorization_url(self):
        """Generate authorization URL with PKCE"""
        # Generate PKCE parameters
        self.code_verifier = base64.urlsafe_b64encode(
            secrets.token_bytes(32)
        ).decode('utf-8').rstrip('=')

        code_challenge = base64.urlsafe_b64encode(
            hashlib.sha256(self.code_verifier.encode()).digest()
        ).decode('utf-8').rstrip('=')

        self.state = secrets.token_urlsafe(32)

        params = {
            "response_type": "code",
            "client_id": self.client_id,
            "redirect_uri": self.redirect_uri,
            "scope": "read write",
            "state": self.state,
            "code_challenge": code_challenge,
            "code_challenge_method": "S256"
        }

        return f"{self.base_url}/oauth/authorize?{urlencode(params)}"

    def exchange_code(self, code):
        """Exchange authorization code for tokens"""
        credentials = f"{self.client_id}:{self.client_secret}"
        auth_header = base64.b64encode(credentials.encode()).decode()

        response = requests.post(
            f"{self.base_url}/oauth/token",
            headers={
                "Authorization": f"Basic {auth_header}",
                "Content-Type": "application/x-www-form-urlencoded"
            },
            data={
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": self.redirect_uri,
                "code_verifier": self.code_verifier
            }
        )
        response.raise_for_status()

        tokens = response.json()
        self.access_token = tokens["access_token"]
        self.refresh_token = tokens["refresh_token"]
        return tokens

    def refresh(self):
        """Refresh access token"""
        credentials = f"{self.client_id}:{self.client_secret}"
        auth_header = base64.b64encode(credentials.encode()).decode()

        response = requests.post(
            f"{self.base_url}/oauth/token",
            headers={
                "Authorization": f"Basic {auth_header}",
                "Content-Type": "application/x-www-form-urlencoded"
            },
            data={
                "grant_type": "refresh_token",
                "refresh_token": self.refresh_token
            }
        )
        response.raise_for_status()

        tokens = response.json()
        self.access_token = tokens["access_token"]
        self.refresh_token = tokens["refresh_token"]
        return tokens

    def request(self, method, path, **kwargs):
        """Make authenticated API request"""
        headers = kwargs.get("headers", {})
        headers["Authorization"] = f"Bearer {self.access_token}"
        kwargs["headers"] = headers

        response = requests.request(method, f"{self.base_url}{path}", **kwargs)

        # Auto-refresh on 401
        if response.status_code == 401 and self.refresh_token:
            self.refresh()
            headers["Authorization"] = f"Bearer {self.access_token}"
            response = requests.request(method, f"{self.base_url}{path}", **kwargs)

        response.raise_for_status()
        return response.json() if response.content else None

    def create_note(self, content, tags=None):
        """Create a new note"""
        return self.request(
            "POST",
            "/api/v1/notes",
            json={"content": content, "tags": tags or []}
        )

    def search_notes(self, query, limit=20):
        """Search notes"""
        return self.request(
            "GET",
            f"/api/v1/search?q={query}&limit={limit}"
        )

# Usage
if __name__ == "__main__":
    client = MatricMemoryClient(
        client_id="mm_AbCdEfGh12345678901234",
        client_secret="<MCP_CLIENT_SECRET>",
        redirect_uri="http://localhost:3000/callback"
    )

    # Step 1: Get authorization URL
    auth_url = client.get_authorization_url()
    print(f"Visit: {auth_url}")

    # Step 2: After redirect, exchange code
    code = input("Enter code from redirect: ")
    client.exchange_code(code)

    # Step 3: Use API
    result = client.create_note("Hello from OAuth2!", ["test"])
    print(f"Created note: {result}")
```

### Simple API Key Script

```python
#!/usr/bin/env python3
import os
import requests

API_BASE = "http://localhost:3000"
API_KEY = os.environ.get("MATRIC_MEMORY_API_KEY")

if not API_KEY:
    print("Error: MATRIC_MEMORY_API_KEY environment variable not set")
    exit(1)

headers = {
    "Authorization": f"Bearer {API_KEY}",
    "Content-Type": "application/json"
}

def create_note(content, tags=None):
    response = requests.post(
        f"{API_BASE}/api/v1/notes",
        headers=headers,
        json={"content": content, "tags": tags or []}
    )
    response.raise_for_status()
    return response.json()

def search_notes(query):
    response = requests.get(
        f"{API_BASE}/api/v1/search",
        headers=headers,
        params={"q": query}
    )
    response.raise_for_status()
    return response.json()

if __name__ == "__main__":
    # Create a note
    note = create_note("Daily standup notes", ["work", "meetings"])
    print(f"Created: {note}")

    # Search notes
    results = search_notes("standup")
    print(f"Found {results['total']} results")
```

---

## Additional Resources

- **Consumer API Reference:** [API Reference](#/developers-api)
- **Operator Swagger UI:** `/api/v1/operator/docs` (admin bearer required; request execution disabled)
- **Operator OpenAPI Spec:** `/api/v1/operator/openapi.yaml` (admin bearer required)
- **OAuth2 RFC 6749:** https://datatracker.ietf.org/doc/html/rfc6749
- **PKCE RFC 7636:** https://datatracker.ietf.org/doc/html/rfc7636
- **Token Introspection RFC 7662:** https://datatracker.ietf.org/doc/html/rfc7662

For questions or issues, please contact support or open an issue on the project repository.

## Selected memory context (producer candidate)

`GET /api/v1/memory/context` uses the authenticated hosted tenant transaction
and existing `read` scope. It returns only the currently authorized canonical
`name` and `schema_name`, with `Cache-Control: no-store`. The normal
`X-Fortemi-Memory` header selects a visible memory; an absent header resolves
the tenant default or public fallback. Later requests are independently
authorized; this response is not a continuing access grant.

Archive inventory remains admin-only and unmigrated. The context endpoint does
not expose global storage statistics or add management privileges. Missing
transaction/context and lookup failures fail closed. This Cycle93 producer
candidate is not yet consumed by HotM or qualified as a released hosted flow;
see `.aiwg/architecture/impact/selected-memory-context.md` and the suite receipt.

## Hosted OpenBao key custody

On-prem hosted deployments use the separately compiled OpenBao Transit provider.
See [OpenBao KMS](#/security-openbao-kms) for provider selection, independent TLS
trust, scoped token-file delivery, runtime policy, startup checks and rotation.
OIDC CA support alone does not supply the KMS backend.

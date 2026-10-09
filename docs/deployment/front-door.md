# Bearer front door for the API and MCP server

When Fortemi validates IdP access tokens (external OIDC), the gateway in
front of it must route OAuth bearer paths straight to Fortemi with no cookie
gate in between. This page describes that front door: why cookie gates break
API and MCP clients, the required topology, header handling, the optional
Istio JWT check, and how to verify the deployment. Normative requirements
are SR-50 to SR-55; the rationale is ADR-110 decision 8.

## Why not a cookie gate

A cookie gate (for example oauth2-proxy `auth-url`/`sign-in`) admits a
browser session but passes on no identity, so Fortemi still answers 401: the
person signed in, yet the API sees no token. Programmatic clients (CLIs,
agents, MCP clients) cannot carry a browser cookie at all. Worse, if the
proxy converts its session cookie into an `Authorization` header on API or
MCP paths, any cross-site request carrying the cookie becomes an
authenticated bearer call. Keep the two mechanisms apart: cookies stay on
browser UI routes, bearer tokens go straight to Fortemi.

Fortemi's own validation remains authoritative in every topology below. The
gateway checks are defense in depth only.

## Topology

```text
browser / CLI / agent / MCP client
        │  TLS, Authorization: Bearer <ACCESS_TOKEN>
        ▼
ingress or gateway (nginx / Gateway API / mesh ingress)
        │  no cookie gate on bearer paths, identity headers stripped
        ├─────────────────────────────► fortemi-api :3000
        │                                /api/*, /.well-known/oauth-protected-resource*
        └─────────────────────────────► fortemi-mcp :3001 (serves "/" internally)
                                         /mcp, prefix stripped at the edge

IdP (for example Keycloak)
        ▲  discovery + JWKS, reached by Fortemi
        │  token endpoint + authorize, reached by clients
```

Both Fortemi and the IdP must be publicly reachable over TLS: hosted MCP
clients reach them from public networks, and the API fetches discovery and
JWKS from the IdP over HTTPS.

## Bearer paths

| Public path | Upstream | Notes |
|---|---|---|
| `/api/*` | `fortemi-api` | REST API |
| `/mcp` | `fortemi-mcp` | Strip the `/mcp` prefix; the MCP server serves `/` internally |
| `/.well-known/oauth-protected-resource*` | `fortemi-api` | RFC 9728 metadata; MCP clients fetch it before registration |

The Helm chart routes this set in `ingress.yaml` (explicit
`/.well-known/oauth-protected-resource` entry on the API Ingress) and
`httproute.yaml` (first-match rule ahead of the generic prefixes), driven by
`frontDoor.bearerPaths`. The Kustomize equivalent is
`components/front-door` (Ingress pair). Never add authenticating annotations
to these routes, and never configure the proxy to mint `Authorization`
headers from cookies on them (SR-52).

## Strip identity headers (SR-50)

Fortemi derives identity only from the `Authorization` header, but anything
behind the edge that honors proxy headers must not see client-supplied ones.
Strip these on every bearer route: `X-Forwarded-User`, `X-Forwarded-Email`,
`X-Auth-Request-*`, `X-Forwarded-Access-Token`.

ingress-nginx (merge into the Ingress annotations):

```yaml
nginx.ingress.kubernetes.io/configuration-snippet: |
  proxy_set_header X-Forwarded-User "";
  proxy_set_header X-Forwarded-Email "";
  proxy_set_header X-Forwarded-Access-Token "";
  proxy_set_header X-Auth-Request-User "";
  proxy_set_header X-Auth-Request-Email "";
  proxy_set_header X-Auth-Request-Groups "";
  proxy_set_header X-Auth-Request-Preferred-Username "";
  proxy_set_header X-Auth-Request-Access-Token "";
```

Istio (remove the headers in the VirtualService ahead of the workloads):

```yaml
http:
  - match:
      - uri:
          prefix: /api/
    headers:
      request:
        remove:
          - x-forwarded-user
          - x-forwarded-email
          - x-forwarded-access-token
          - x-auth-request-user
          - x-auth-request-email
          - x-auth-request-groups
          - x-auth-request-preferred-username
          - x-auth-request-access-token
    route:
      - destination:
          host: fortemi-api
```

## Trust only the ingress pods (SR-51)

`FORTEMI_TRUSTED_PROXY_CIDRS` decides whose `X-Forwarded-*` metadata the API
may consume (for client IPs and external URLs). List only the
ingress/gateway pod addresses, for example from
`kubectl -n ingress-nginx get pods -o wide`. Never use node or pod-wide
CIDRs: in a mesh the immediate peer is a sidecar or a shared ingress CIDR,
and any workload inside a broad trusted range could forge identity headers.

## Optional Istio JWT check (SR-53)

Set `frontDoor.istio` in the chart (issuer, JWKS URI, accepted audiences), or
apply `deploy/kustomize/examples/front-door-istio`, to render a
`RequestAuthentication` (with `forwardOriginalToken: true` so the bearer
reaches Fortemi) paired with an `AuthorizationPolicy` that requires a
request principal and one of the accepted audiences on the bearer paths.
`RequestAuthentication` alone admits requests that carry no token, so the
pair is mandatory, not optional. A wrong-audience token is then rejected at
the mesh and, independently, at Fortemi.

## Pre-authentication rate limit (SR-54)

The API budgets authentication failures per client IP before verification
runs: `FORTEMI_AUTH_FAILURE_RATE_LIMIT` (default `20` failures per 60 s per
IP; `0` disables). A request that presents a credential while its IP is over
budget receives 429 with `Retry-After` and no verification attempt.
Requests without an `Authorization` header are never counted, so
unauthenticated discovery (protected-resource metadata) keeps working while
an IP is over budget. The client IP comes from the trusted-proxy policy
above: the first untrusted hop behind a configured proxy, else the socket
peer. The `Authorization` header value is capped at 8 KiB; larger values
receive 431 without a verification attempt.

## Verification checklist

Run these against the public host after each front-door change:

1. Cookie-only request to a bearer path returns 401 from Fortemi (no proxy
   sign-in redirect, no 200):
   `curl -s -o /dev/null -w '%{http_code}' --cookie 'session=abc' https://memory.example.com/api/v1/notes`
   expects `401`.
2. Forged identity header without a bearer returns 401:
   `curl -s -o /dev/null -w '%{http_code}' -H 'X-Forwarded-Email: admin@example.com' https://memory.example.com/api/v1/notes`
   expects `401`.
3. Valid bearer reaches the API:
   `curl -s -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer <ACCESS_TOKEN>' https://memory.example.com/api/v1/notes`
   expects `200` (or `403` for a scope-limited token, never `401`).
4. Protected-resource metadata is reachable without credentials:
   `curl -s https://memory.example.com/.well-known/oauth-protected-resource`
   returns the canonical resource and the IdP issuer.
5. Wrong-audience token is rejected (at the mesh when the Istio policies are
   applied, and at Fortemi regardless).
6. An invalid-credential flood from one IP turns into 429 with `Retry-After`
   after `FORTEMI_AUTH_FAILURE_RATE_LIMIT` failures; other IPs are unaffected.
7. An `Authorization` header larger than 8 KiB returns 431.

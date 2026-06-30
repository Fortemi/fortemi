# API Error Contract

Fortemi API errors use RFC 9457 Problem Details as the canonical HTTP error
format. The pre-GA compatibility break from the legacy `{ "error": "..." }`
shape is intentional: clients should parse `application/problem+json` bodies and
the stable `type` URI, not free-form diagnostic text.

## Response Shape

All non-stream HTTP API errors should use:

```http
Content-Type: application/problem+json
Cache-Control: no-store
x-request-id: <request id>
```

```json
{
  "type": "https://fortemi.com/problems/validation-error",
  "title": "Bad Request",
  "status": 400,
  "detail": "Invalid request.",
  "request_id": "018f4f5d-..."
}
```

`request_id` is copied from the `x-request-id` response header when request
context is available. Operators should use it to correlate client reports with
server logs, traces, and audit records.

Operational logs and diagnostics linked from `request_id` follow the hosted
telemetry classes in
`docs/architecture/hosted-telemetry-classification.md`; problem responses remain
public-safe interface errors, not a diagnostic sink.
Secret and credential classes that must not appear in problem responses are
defined in `docs/architecture/hosted-secret-inventory.md`.

## Stable Problem Types

The current public catalog is generated from the API `ProblemType` registry:

| Type URI | HTTP status | Title | Use |
|---|---:|---|---|
| `https://fortemi.com/problems/validation-error` | 400 | Bad Request | Malformed input, invalid parameters, unsupported request shapes. |
| `https://fortemi.com/problems/unauthorized` | 401 | Unauthorized | Missing, malformed, expired, or otherwise invalid credentials. |
| `https://fortemi.com/problems/forbidden` | 403 | Forbidden | Authenticated request denied by authorization policy or admin gate. |
| `https://fortemi.com/problems/not-found` | 404 | Not Found | Requested resource is not present or not visible to the caller. |
| `https://fortemi.com/problems/gone` | 410 | Gone | Previously valid cursor or token state is no longer usable. |
| `https://fortemi.com/problems/conflict` | 409 | Conflict | Duplicate resource or state conflict. |
| `https://fortemi.com/problems/rate-limit-exceeded` | 429 | Too Many Requests | Rate limit or quota boundary reached. The global limiter sends no headers; back off on 429. (Only the chat 503 capacity response carries `Retry-After`.) |
| `https://fortemi.com/problems/internal-error` | 500 | Internal Server Error | Unexpected internal failure. |
| `https://fortemi.com/problems/operation-failed` | 500 | Operation Failed | Backup, restore, command, or storage operation failed. |
| `https://fortemi.com/problems/provider-failure` | 502 | Provider Failure | AI, media, or inference provider failed. |
| `https://fortemi.com/problems/service-unavailable` | 503 | Service Unavailable | Required service or capacity is unavailable. |
| `https://fortemi.com/problems/blob-missing` | 404 | Blob Missing | Attachment metadata exists but the backing blob is missing. |

Clients should treat unknown Fortemi problem types as stable HTTP errors: use
`status` for control flow, surface `title`/`detail`, and include `request_id` in
support reports.

## Redaction Boundary

Problem `detail` is interface detail, not an implementation diagnostic channel.
Client responses must not contain raw SQL/database errors, filesystem paths,
command stderr, provider URLs, bearer/API tokens, secrets, private key material,
stack traces, request bodies, model/provider exception text, or backend-specific
configuration values.

Expected client behavior:

- Use `type` as the machine-readable error code.
- Use `status` for HTTP control flow.
- Use `Retry-After` or standard rate-limit headers when present.
- Include `request_id` when reporting failures.
- Do not depend on raw backend wording in `detail`.

Expected server behavior:

- Log internal diagnostics server-side with bounded, sanitized metadata.
- Keep detailed dependency, command, filesystem, provider, and database errors
  out of problem bodies.
- Use generic detail for internal, operation, and provider failures, such as
  `Check server logs for diagnostics.`
- Preserve safe validation messages only when they do not reveal internals.

## Security Headers and Proxies

Fortemi applies hosted-safe response headers by response class:

- API JSON and Problem Details responses receive `X-Content-Type-Options:
  nosniff`, `Referrer-Policy: no-referrer`, restrictive browser capability
  policy, and `Cache-Control: no-store` on errors.
- Browser-rendered docs and OAuth pages receive a document CSP with
  `frame-ancestors 'none'` for clickjacking protection.
- Download and media responses keep media-compatible headers and avoid document
  CSP that would break legitimate attachment rendering.

If TLS terminates at a reverse proxy, the proxy owns HSTS emission and must strip
or overwrite inbound `Forwarded` / `X-Forwarded-*` headers before forwarding to
Fortemi. Fortemi diagnostics and problem responses must not echo untrusted
forwarded host, proto, or client IP header values.

## Streaming Protocols

SSE and other streaming protocols may carry stream-frame error events after the
HTTP response has already started. Pre-stream HTTP failures still use Problem
Details. In-stream error frames should use generic, provider-independent text and
must not carry raw backend exceptions or secret-bearing diagnostics.

## Documentation Scope

Generated or public consumer documentation must describe the stable public
problem types above. Enterprise-only or operator-only problem types must remain
filterable from public docs in line with the docs exposure policy.

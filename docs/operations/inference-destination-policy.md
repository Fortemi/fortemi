# Inference Destination Policy

Fortemi authorizes inference URLs before attaching credentials or opening a
connection. The same policy protects completion, streaming, connection tests,
and global or archive-scoped configuration writes.

## Deployment Profiles

| Input | Community build | Hosted multi-tenant build |
|---|---|---|
| OpenAI and OpenRouter built-in URLs | HTTPS public addresses allowed | HTTPS public addresses allowed |
| Operator environment or stored config | HTTP/HTTPS local or public addresses allowed, except always-denied ranges | Exact allowlist entry, HTTPS, and public addresses required |
| Request `base_url` | Exact allowlist entry required | Denied |
| Request `api_key` | Supported for the current BYOK API | Denied; #731 must use hosted stored-secret lookup |

Set `FORTEMI_INFERENCE_ALLOWED_DESTINATIONS` to a comma-separated list of exact
`host` or `host:port` entries. A missing port means `443`. Entries are not URL
prefixes and do not allow subdomains.

```dotenv
FORTEMI_INFERENCE_ALLOWED_DESTINATIONS=models.example.com:8443,localhost:11434
```

The allowlist permits destination selection; it does not bypass address-class
or TLS rules. Hosted custom destinations must resolve entirely to public
addresses. Community operator-local destinations may resolve to private or
loopback addresses. Cloud metadata, link-local, unspecified, multicast,
documentation, benchmarking, carrier-grade NAT, and reserved addresses remain
denied in every mode, including IPv4-mapped and alternate numeric forms.

## Transport Controls

Authorization parses and normalizes the URL, resolves every DNS answer, rejects
mixed allowed/denied answers, and pins the approved answers into the HTTP
client. Clients do not inherit process proxy variables and do not follow HTTP
redirects. URLs containing user information, query strings, or fragments are
rejected so credentials cannot enter policy diagnostics.

Policy failures expose stable reason codes only in metadata logs. API responses
do not echo the rejected URL, resolved addresses, query data, or provider key.
An invalid allowlist prevents application startup.

## Future Consumers

Archive-aware runtime resolution (#666), hosted stored-secret proxying (#731),
and model-gateway or bridge routing (#864/#867) must call
`OutboundDestinationPolicy::authorize` and use the returned approved client.
Persisted validation alone is not runtime authorization evidence because DNS
and operator policy may change between write and use.

# Observability: OpenTelemetry Export

Fortemi can export traces and metrics to an OpenTelemetry Collector, or any
OTLP-compatible backend, over OTLP/HTTP (protobuf) or OTLP/gRPC. Logs stay on
stdout or in `LOG_FILE`; with `LOG_FORMAT=json` they carry the trace id of the
request or job that produced them (see [Log correlation](#log-correlation)).

Export is **off by default** and costs nothing until you turn it on.

## Build requirement

OTLP export is compiled in by the `otel` Cargo feature of `matric-api`:

```bash
cargo build --release -p matric-api --features otel
```

The published API and bundle images include the feature:
`FORTEMI_API_FEATURES` defaults to `hosted-auth,otel` in `Dockerfile` and
`Dockerfile.bundle`. If a binary built without the feature sees `OTEL_*`
variables that request export, it logs a warning at startup and exports nothing.

## Enabling export

Fortemi reads the standard OpenTelemetry environment variables. A signal is
exported only when one of these is true:

| Condition | Effect |
|---|---|
| `OTEL_SDK_DISABLED=true` | Everything off, whatever else is set. |
| `OTEL_TRACES_EXPORTER=otlp` / `OTEL_METRICS_EXPORTER=otlp` | That signal on. |
| Exporter variable unset and `OTEL_EXPORTER_OTLP_ENDPOINT` set | Traces and metrics on. |
| Exporter variable unset and `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` / `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` set | That signal on. |
| `OTEL_TRACES_EXPORTER=none` / `OTEL_METRICS_EXPORTER=none` | That signal off. |

With none of these set, nothing is exported. Setting only `OTEL_SERVICE_NAME`
does not turn export on. Fortemi refuses to start if an exporter variable
holds anything other than `otlp` or `none`, or if a protocol variable holds
anything other than `grpc` or `http/protobuf`.

Minimal configuration:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4318   # OTLP/HTTP
# or
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
OTEL_EXPORTER_OTLP_PROTOCOL=grpc
```

### Supported variables

| Variable | Read by | Notes |
|---|---|---|
| `OTEL_SDK_DISABLED` | Fortemi | `true` disables all export. |
| `OTEL_TRACES_EXPORTER`, `OTEL_METRICS_EXPORTER` | Fortemi | `otlp` or `none`. |
| `OTEL_EXPORTER_OTLP_PROTOCOL`, `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_PROTOCOL` | Fortemi | `http/protobuf` (default) or `grpc`. `http/json` is not supported. |
| `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_ENDPOINT` | OTLP exporter | HTTP adds `/v1/traces` and `/v1/metrics` to the generic endpoint. Use `https://` for TLS; gRPC TLS uses the WebPKI root store. |
| `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_HEADERS` | OTLP exporter | Collector credentials. Treat as secrets. |
| `OTEL_EXPORTER_OTLP_TIMEOUT` and the per-signal variants | OTLP exporter | Milliseconds. |
| `OTEL_SERVICE_NAME` | SDK | Default `fortemi-api`. |
| `OTEL_RESOURCE_ATTRIBUTES` | SDK | Operator-supplied resource attributes. Do not put tenant ids or secrets here. |
| `OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG` | SDK | Default `parentbased_always_on`. |
| `OTEL_BSP_SCHEDULE_DELAY`, `OTEL_BSP_MAX_QUEUE_SIZE`, `OTEL_BSP_MAX_EXPORT_BATCH_SIZE`, `OTEL_BSP_EXPORT_TIMEOUT` | SDK | Batch span processor. |
| `OTEL_METRIC_EXPORT_INTERVAL`, `OTEL_METRIC_EXPORT_TIMEOUT` | SDK | Default interval 60000 ms. |
| `FORTEMI_OTEL_TRACES_FILTER` | Fortemi | `tracing` filter directives for exported spans. Default `info`. This is separate from `RUST_LOG`, which now applies only to the stdout/file log sink. |
| `OTEL_LOGS_EXPORTER` | — | Not supported. Any value other than `none` logs a startup warning and is ignored. Use `LOG_FORMAT=json`. |

Fortemi logs which signals and protocols are active at startup
(`OpenTelemetry export configured`). It logs only an endpoint class
(`local_or_private`, `external`, `invalid_url`), never the endpoint value or
headers.

## Spans

| Span name | Kind | Attributes | Source |
|---|---|---|---|
| `{METHOD} {route template}`, e.g. `POST /api/v1/notes`, `GET /api/v1/notes/{id}` | server | `http.request.method`, `http.route`, `fortemi.route.class`, `http.response.status_code`; status `error` for 5xx | Every HTTP request |
| `job.execute` | consumer | `fortemi.job.type` | Every job the in-process worker runs |
| Existing `tracing` spans at INFO and above, such as the Ollama `embed_texts` / `generate_with_system` instrumented spans | internal | The span's own fields, already limited to lengths, counts, classes and codes (#974) | Inference, handlers, workers |

Unregistered paths use the route `unmatched` with route class `unmatched`.
Raw paths, query strings and path parameters are never recorded.

### Propagation

| Hop | Mechanism |
|---|---|
| Client → API | W3C `traceparent` / `tracestate` request headers continue the caller's trace. `baggage` is ignored. Without a header, each request starts a new trace. |
| API → job worker | When trace export is active, jobs queued during a traced request carry the request's `traceparent` in the reserved payload key `_fortemi_traceparent` (object payloads only). The worker validates the value, removes it before any handler sees the payload, and parents `job.execute` on it. Jobs queued while export is off, and jobs with `null` or non-object payloads, start their own trace. The key is visible in job payloads returned by the jobs API while export is on. |
| API/worker → inference providers | Ollama `/api/chat` and `/api/embed` requests and every OpenAI-compatible request (OpenAI, OpenRouter, llama.cpp, vLLM) carry `traceparent`. |
| MCP client → MCP server → API | The Node MCP server does not export telemetry. It forwards a validated inbound `traceparent`/`tracestate` unchanged on its API calls, taken from the tool call's `params._meta.traceparent` or, failing that, from the HTTP request headers of the MCP message. Malformed values and `baggage` are dropped. |

Gaps: background work that is not a queued job (startup tasks, schedulers,
periodic maintenance) starts its own traces. Sidecar calls (Whisper, pyannote,
GLiNER, vision, transcription) do not yet carry `traceparent`. The MCP server
has no spans of its own.

## Metrics

All attribute values come from closed sets. No metric carries tenant ids,
user ids, note content, raw paths or URLs.

| Metric | Type | Unit | Attributes |
|---|---|---|---|
| `http.server.request.duration` | Histogram | `s` | `http.request.method` (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `_OTHER`), `http.route` (route template or `unmatched`), `fortemi.route.class`, `http.response.status_code` |
| `http.server.active_requests` | UpDownCounter | `{request}` | `http.request.method`, `fortemi.route.class` |
| `fortemi.job.execution.duration` | Histogram | `s` | `fortemi.job.type` (job type, e.g. `embedding`), `fortemi.job.outcome` (`success`, `failed`, `retry`) |
| `fortemi.job.queue.depth` | Gauge | `{job}` | `fortemi.job.state` (`pending`, `delayed`, `processing`, `dead`, `incompatible`); sampled every 15 s |
| `fortemi.job.queue.oldest_pending_age` | Gauge | `s` | none; age of the oldest due pending job, 0 when idle; sampled every 15 s |
| `fortemi.inference.duration` | Histogram | `s` | `fortemi.inference.operation` (`embed`, `generate`, `generate_json`), `fortemi.inference.provider` (`ollama`, `openai_compatible`), `fortemi.outcome` (`ok`, `error`) |
| `fortemi.db.pool.connections` | Gauge | `{connection}` | `fortemi.db.pool.state` (`idle`, `used`) for the primary pool |
| `fortemi.db.pool.max_connections` | Gauge | `{connection}` | none |
| `fortemi.quota.admission.decisions` | Counter | `{decision}` | `fortemi.quota.decision` (`allowed`, `rejected`, `unavailable`, `invalid`); Redis request-quota gate |
| `fortemi.kms.operations` | Counter | `{operation}` | `fortemi.kms.operation` (`seal`, `unseal`, `rewrap`, `health_canary`, `startup_canary`; `sign`, `verify`, `rotate` when a consumer uses them), `fortemi.outcome` (`ok`, `error`), `fortemi.kms.failure_class` (`none` or a key failure class such as `throttled`, `access_denied`, `key_disabled`, `provider_unavailable`, `context_mismatch`), `fortemi.kms.retryability` (`none`, `retryable`, `terminal`) |
| `fortemi.kms.operation.duration` | Histogram | `s` | Same attributes as `fortemi.kms.operations` |

`fortemi.route.class` values: `public`, `public_inline_proof`,
`authenticated_read`, `authenticated_write`, `admin_operator`,
`tenant_object`, `system_health`, `oauth`, `realtime_transport`, `unmatched`.
They come from the route authorization inventory (`route_policy.rs`).

Error rate per route is the share of `http.server.request.duration` samples
with `http.response.status_code` ≥ 500. Request rate is the histogram count.

Key-provider metrics come from a decorator around the configured provider
(AWS KMS, OpenBao Transit or a local provider), so every backend reports the
same operations. `seal` covers data-key generation and wrapping, `unseal`
covers unwrapping, and `rewrap` covers the user-secret rewrap worker.
`retryable` marks throttling and provider-unavailable failures, where a
bounded retry may succeed; every other failure is `terminal`. Labels carry no
key ARN or reference, tenant, user or context value. The same outcomes feed
the cached key-provider health that `/readyz` reports (see the
[operator guide](#/operations-guide)).

Duration histograms share these bucket bounds, in seconds: 0.005, 0.01, 0.025,
0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1, 2.5, 5, 7.5, 10, 30, 60, 120, 300, 600.

## Redaction

Telemetry follows the classification in
`docs/architecture/hosted-telemetry-classification.md` (#974):

- Span names and metric attributes come from fixed vocabularies, such as route
  templates, route classes and job types.
- Every finished span passes through a redacting processor before export. It:
  - drops attributes whose key names a URL, query, statement, peer address,
    header, cookie, authorization, token, secret, password, credential,
    content, body, prompt, email, tenant or user id;
  - masks string values that look like credentials (`Bearer …`, `Basic …`),
    URLs, email addresses, or anything longer than 256 characters;
  - removes span events, so log messages are not copied into traces;
  - clears error status descriptions and link attributes.
- Keys ending in `_len`, `_count`, `_class`, `_code`, `_set`, `_secs` or `_ms`
  are kept. Existing diagnostics use these suffixes for derived, non-content
  values.

`crates/matric-api/src/otel/redaction_tests.rs` sends a request with a bearer
token, a query-string token, a tenant `baggage` entry and a note-body
sentinel. It asserts that none of them reaches an exported span.

## Log correlation

When trace export is active, the HTTP server span and `job.execute` carry the
fields `trace_id` and `span_id`. With `LOG_FORMAT=json`, every log line inside
a request or job includes them under `span` and `spans`:

```json
{"level":"INFO","fields":{"message":"..."},"span":{"trace_id":"0af7651916cd43dd8448eb211c80319c","span_id":"b9c7c989f97918e1","name":"http.server.request"}}
```

## Verifying against a collector

`scripts/test-otel-collector.sh` starts a disposable PostgreSQL and a
digest-pinned `otel/opentelemetry-collector` (see `ci/digests.txt`) with a file
exporter. It runs the API with `--features otel` over both protocols and
checks that:

- spans and metrics arrive;
- the inbound `traceparent` is continued;
- queued jobs carry the trace;
- JSON logs carry `trace_id`;
- no credential or content sentinel reaches the collector.

```bash
TESTDB_IMAGE=matric-testdb:local scripts/test-otel-collector.sh
OTEL_IT_PROTOCOLS=grpc scripts/test-otel-collector.sh
```

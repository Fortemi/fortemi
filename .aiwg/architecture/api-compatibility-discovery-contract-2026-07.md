# API Compatibility Discovery Contract - 2026-07

## Purpose

Define the Fortemi-side contract needed by HotM enterprise UX demos and future hosted clients before they enable advanced flows. This artifact supports `Fortemi/fortemi#1018`, `Fortemi/HotM#244`, and the July 2026 enterprise/backoffice checkpoint.

Existing `/health` responses expose useful operational status, but they are not a stable client compatibility contract. HotM needs a product-safe endpoint that separates API compatibility, deployment mode, auth posture, realtime support, premium/backoffice capability state, and implementation blockers without leaking tenant, license, KMS, or private repo details.

## Endpoint

```http
GET /api/v1/system/compatibility
```

Implementation status at checkpoint: initial Fortemi endpoint slice is implemented in `crates/matric-api/src/main.rs`. It intentionally reports conservative `preview`/`unavailable` states for enterprise/backoffice capabilities until RLS, KMS, audit, quota, MCP gate, and EE package gates close.

| Property | Contract |
|---|---|
| Authentication | Public-safe response without auth; optional authenticated augmentation may be added later under the same schema using coarse role/scope booleans only. |
| Stability | Additive-only for `schema_version: 1`; incompatible changes require `schema_version: 2`. |
| Cache | `Cache-Control: no-store` for hosted deployments; local sidecars may cache in-process but clients should refresh on connection change. |
| Content type | `application/json; charset=utf-8`. |
| Failure mode | If unavailable, HotM treats all enterprise capabilities as `unknown` and keeps local workflows available. |

## Response Shape

```json
{
  "schema_version": 1,
  "contract_revision": "2026-07-06",
  "api": {
    "name": "fortemi",
    "version": "2026.7.0",
    "minimum_hotm_enterprise_client": "0.0.0-checkpoint",
    "git_sha_present": true,
    "build_date_present": true
  },
  "deployment": {
    "mode": "local_sidecar",
    "edition": "community",
    "hosted_multi_tenant_ready": false
  },
  "auth": {
    "required": false,
    "mode": "anonymous_local",
    "oauth_issuer_configured": false,
    "tenant_context_available": false
  },
  "capabilities": {
    "core_notes": { "state": "available" },
    "search": { "state": "available" },
    "jobs": { "state": "available" },
    "realtime_activity": { "state": "available" },
    "hosted_auth": { "state": "unavailable", "reason_code": "hosted_auth_not_configured" },
    "premium_components": { "state": "preview", "reason_code": "capability_catalog_preview_only" },
    "backoffice_api": { "state": "unavailable", "reason_code": "contract_not_implemented" },
    "audit_posture": { "state": "preview", "reason_code": "hosted_audit_gate_open" },
    "quota_status": { "state": "unavailable", "reason_code": "quota_policy_not_implemented" },
    "kms_status": { "state": "unavailable", "reason_code": "key_provider_not_implemented" },
    "mcp_scope_gate": { "state": "preview", "reason_code": "enterprise_gate_not_implemented" }
  },
  "backoffice": {
    "contract_revision": "2026-07-10",
    "production_enabled": false,
    "preview_responses_enabled": false,
    "preview_gate": "FORTEMI_BACKOFFICE_PREVIEW=true",
    "surfaces": [
      {
        "key": "tenant_health",
        "state": "unavailable",
        "reason_code": "backoffice_contract_preview_disabled",
        "endpoint": "/api/v1/admin/tenant/health",
        "required_scopes": ["admin:tenant:read"],
        "actions": ["tenant.health.read"],
        "audit_events": ["backoffice.tenant_health.read"],
        "response_contract": "health_state, degraded_reasons, checked_at"
      },
      {
        "key": "audit_posture",
        "state": "preview",
        "reason_code": "hosted_audit_gate_open",
        "endpoint": "/api/v1/admin/audit/posture",
        "required_scopes": ["admin:audit:read"],
        "actions": ["audit.posture.read"],
        "audit_events": ["backoffice.audit_posture.read"],
        "response_contract": "coverage_state, sink_state, retention_state, missing_events"
      },
      {
        "key": "quota_status",
        "state": "unavailable",
        "reason_code": "quota_policy_not_implemented",
        "endpoint": "/api/v1/admin/quota/status",
        "required_scopes": ["admin:quota:read"],
        "actions": ["quota.status.read"],
        "audit_events": ["backoffice.quota_status.read"],
        "response_contract": "metering_state, period, limits, usage, reset_at"
      },
      {
        "key": "kms_status",
        "state": "unavailable",
        "reason_code": "key_provider_not_implemented",
        "endpoint": "/api/v1/admin/kms/status",
        "required_scopes": ["admin:kms:read"],
        "actions": ["kms.status.read"],
        "audit_events": ["backoffice.kms_status.read"],
        "response_contract": "provider_state, keyring_state, rotation_state, degraded_reasons"
      },
      {
        "key": "premium_components",
        "state": "preview",
        "reason_code": "capability_catalog_preview_only",
        "endpoint": "/api/v1/admin/premium/components",
        "required_scopes": ["admin:components:read"],
        "actions": ["premium.components.read"],
        "audit_events": ["backoffice.premium_components.read"],
        "response_contract": "component_key, state, reason_code, required_entitlements"
      },
      {
        "key": "support_diagnostics",
        "state": "unavailable",
        "reason_code": "support_diagnostics_not_implemented",
        "endpoint": "/api/v1/admin/support/diagnostics",
        "required_scopes": ["admin:support:read"],
        "actions": ["support.diagnostics.read"],
        "audit_events": ["backoffice.support_diagnostics.read"],
        "response_contract": "diagnostic_key, state, redacted_summary, collected_at"
      }
    ]
  },
  "links": {
    "openapi": "/openapi.yaml",
    "asyncapi": "/asyncapi.yaml",
    "health": "/health",
    "streaming_health": "/api/v1/health/streaming"
  }
}
```

## Enumerations

### `deployment.mode`

| Value | Meaning | HotM behavior |
|---|---|---|
| `local_sidecar` | Local/private sidecar or desktop-adjacent API. | Keep local workflows primary; disable hosted/backoffice production controls. |
| `single_tenant_server` | Server deployment for one operator or organization without hosted multi-tenancy. | Enable compatible core flows; treat tenant-admin backoffice as unavailable unless explicit capabilities say otherwise. |
| `hosted_multi_tenant` | Hosted deployment that serves multiple tenants. | Require auth, role/scope checks, RLS/KMS/audit gates, and compatibility flags before enterprise controls enable. |
| `unknown` | Server cannot classify itself. | Disable enterprise controls and show unknown compatibility state. |

### `deployment.edition`

| Value | Meaning |
|---|---|
| `community` | Open-BSL/community build. |
| `enterprise` | Enterprise distribution build. Must not be claimed until private distribution evidence exists. |
| `unknown` | Edition cannot be verified. |

### `auth.mode`

| Value | Meaning |
|---|---|
| `anonymous_local` | Local or dev mode allows anonymous API use. |
| `api_key` | API-key auth is active. |
| `oauth` | OAuth/JWT auth is active. |
| `hosted_oauth` | Hosted OAuth with tenant context is active. |
| `unknown` | Auth mode cannot be classified. |

### Capability `state`

| Value | Meaning | HotM enablement |
|---|---|---|
| `available` | Production-backed and compatible for this deployment. | May enable matching UI if role/scope also allows it. |
| `degraded` | Implemented but currently impaired. | Show UI with degraded warning; disable destructive/admin actions. |
| `preview` | Safe to display as non-production preview or fixture-backed surface. | Show preview labels; keep production actions disabled. |
| `unavailable` | Known unsupported or blocked. | Disable with reason text. |
| `unknown` | Metadata absent or incompatible. | Disable by default. |

## Required Capability Keys For HotM

Fortemi must include these keys even when unavailable so HotM does not branch on field presence:

| Key | Drives HotM surface | Related gate |
|---|---|---|
| `core_notes` | Local/private note workflows | Existing `/api/v1/notes` contract |
| `search` | Search and memory discovery | Existing `/api/v1/search` contract |
| `jobs` | Job/activity status | Existing `/api/v1/jobs` contract |
| `realtime_activity` | Realtime Activity Drawer | Streaming/SSE health |
| `hosted_auth` | Hosted Auth Onboarding | `Fortemi/fortemi-auth#25` |
| `premium_components` | Premium Components Catalog | `Fortemi/fortemi#1020`, EE repo gates |
| `backoffice_api` | Backoffice Console | `Fortemi/fortemi#1020` |
| `audit_posture` | Backoffice audit panel | ADR-091, `Fortemi-Enterprise/audit-sinks#2` |
| `quota_status` | Backoffice quota panel | ADR-092/098, `Fortemi-Enterprise/billing#1` |
| `kms_status` | Backoffice KMS panel | ADR-093, `Fortemi/fortemi#1019`, `Fortemi-Enterprise/kms#2` |
| `mcp_scope_gate` | MCP/admin capability status | ADR-100, `Fortemi-Enterprise/mcp-gate#2` |

## Public-Safety Rules

- Do not include raw tenant IDs, customer names, license keys, entitlement IDs, KMS key IDs, issuer secrets, provider URLs containing credentials, bearer tokens, API keys, private package names beyond coarse capability labels, or internal repository paths.
- Use booleans such as `git_sha_present` rather than exposing exact build provenance on public hosted responses unless an authenticated admin endpoint is used.
- Use stable `reason_code` values rather than raw backend errors.
- Unknown fields must be ignored by clients; missing required fields make the response incompatible.

## Initial Reason Codes

| Reason code | Meaning |
|---|---|
| `contract_not_implemented` | The backend endpoint/API contract does not exist yet. |
| `hosted_auth_not_configured` | Hosted OAuth/tenant context is not configured. |
| `rls_gate_open` | Hosted multi-tenant isolation evidence is incomplete. |
| `key_provider_not_implemented` | ADR-093 KeyProvider/KMS implementation is incomplete. |
| `hosted_audit_gate_open` | Mandatory hosted audit implementation or health evidence is incomplete. |
| `quota_policy_not_implemented` | Usage metering/quota policy is not implemented. |
| `enterprise_gate_not_implemented` | EE implementation belongs to private repo work that is not ready. |
| `capability_catalog_preview_only` | Capability is displayable as preview but not production-backed. |
| `backoffice_contract_preview_disabled` | The backoffice surface contract is discoverable, but preview/stub responses are not enabled for this deployment. |
| `support_diagnostics_not_implemented` | Support diagnostics API backing is not implemented. |
| `insufficient_role` | Authenticated principal lacks required role/scope. |
| `incompatible_api_version` | Server version is below the HotM-supported floor. |

## Backoffice Discovery Contract

`backoffice.surfaces[]` is the draft admin/backoffice contract for `Fortemi/fortemi#1020`. It must list every HotM enterprise panel surface even when unavailable: `tenant_health`, `audit_posture`, `quota_status`, `kms_status`, `premium_components`, and `support_diagnostics`.

Each surface declares:

- `endpoint`: the future authenticated `/api/v1/admin/...` route.
- `required_scopes`: OAuth/API-key scopes required before the panel may call the route.
- `actions`: authorization-policy action names used by Fortemi route policy.
- `audit_events`: audit event names emitted for reads or preview reads.
- `response_contract`: stable response summary, not raw tenant or provider data.

Stub or fixture responses are gated by `preview_responses_enabled`. Default CE and hosted demos must return `false`; preview responses are only allowed when the explicit gate named by `preview_gate` is configured, and they must remain non-mutating and labeled `state: "preview"`.

## Contract Tests

Minimum Fortemi tests for `Fortemi/fortemi#1018`:

- Public response validates against the schema and includes all required capability keys.
- Local sidecar/default CE response marks hosted/backoffice capabilities as unavailable or preview, never available.
- `FORTEMI_MULTI_TENANT=true` without RLS/KMS/backoffice gates cannot report `hosted_multi_tenant_ready: true`.
- Unknown/private implementation details do not appear in the JSON body.
- Additive extra capability keys do not break the schema test.
- Backoffice discovery includes required surfaces, scopes, authorization actions, audit events, and preview gating; no surface reports `available` while production backing is disabled.

Minimum HotM tests for `Fortemi/HotM#244`:

- Compatible response enables only allowed surfaces.
- Too-old or incompatible API response disables enterprise flows with `incompatible_api_version`.
- Missing/unknown capability metadata disables enterprise controls by default.
- Unreachable endpoint keeps local workflows available and shows an unavailable connection state.

## Implementation Notes

- `/health` and `/api/v1/health/streaming` remain operational health endpoints. They can feed this response, but they are not the compatibility contract.
- This contract can exist before hosted production readiness by returning `preview`, `unavailable`, and `unknown` states for gated surfaces.
- Backoffice-specific tenant health, audit, quota, KMS, and support diagnostics details must remain behind authenticated admin APIs. This public compatibility endpoint only decides whether HotM may show, hide, or disable coarse surfaces.

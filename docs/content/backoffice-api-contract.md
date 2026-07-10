# Backoffice API Contract

**Status:** Draft contract
**Applies to:** HotM enterprise UX, hosted demos, and future licensed-server admin panels

Fortemi exposes backoffice readiness through `GET /api/v1/system/compatibility`. The public compatibility response is the discovery surface; production admin data remains behind authenticated `/api/v1/admin/...` APIs.

The compatibility response includes a `backoffice` object with:

- `production_enabled`: `false` until hosted isolation, KMS, audit, quota, and enterprise package gates have production evidence.
- `preview_responses_enabled`: `false` by default.
- `preview_gate`: `FORTEMI_BACKOFFICE_PREVIEW=true`, the explicit opt-in required before fixture or stub responses may be served.
- `surfaces[]`: one entry per admin surface, including state, reason code, endpoint, required scopes, authorization actions, audit events, and response contract summary.

Clients must disable production backoffice flows unless `production_enabled` is true and the target surface reports `state: "available"`. A `preview` state may only drive labeled demo UI; it must not enable tenant mutation, support impersonation, key operations, quota changes, or audit export.

## Surface Matrix

| Surface | Endpoint | Required scope | Authorization action | Audit event |
|---|---|---|---|---|
| `tenant_health` | `/api/v1/admin/tenant/health` | `admin:tenant:read` | `tenant.health.read` | `backoffice.tenant_health.read` |
| `audit_posture` | `/api/v1/admin/audit/posture` | `admin:audit:read` | `audit.posture.read` | `backoffice.audit_posture.read` |
| `quota_status` | `/api/v1/admin/quota/status` | `admin:quota:read` | `quota.status.read` | `backoffice.quota_status.read` |
| `kms_status` | `/api/v1/admin/kms/status` | `admin:kms:read` | `kms.status.read` | `backoffice.kms_status.read` |
| `premium_components` | `/api/v1/admin/premium/components` | `admin:components:read` | `premium.components.read` | `backoffice.premium_components.read` |
| `support_diagnostics` | `/api/v1/admin/support/diagnostics` | `admin:support:read` | `support.diagnostics.read` | `backoffice.support_diagnostics.read` |

## Response Contracts

Backoffice responses must be tenant-safe and redacted. They use stable states and reason codes rather than backend errors, tenant names, customer identifiers, KMS key IDs, entitlement IDs, license keys, provider URLs, or private package paths.

Minimum response summaries by surface:

| Surface | Contract summary |
|---|---|
| `tenant_health` | `health_state`, `degraded_reasons`, `checked_at` |
| `audit_posture` | `coverage_state`, `sink_state`, `retention_state`, `missing_events` |
| `quota_status` | `metering_state`, `period`, `limits`, `usage`, `reset_at` |
| `kms_status` | `provider_state`, `keyring_state`, `rotation_state`, `degraded_reasons` |
| `premium_components` | `component_key`, `state`, `reason_code`, `required_entitlements` |
| `support_diagnostics` | `diagnostic_key`, `state`, `redacted_summary`, `collected_at` |

## Preview Gate

Stub, fixture, or preview responses are forbidden in production by default. A deployment may expose preview responses only when all of these are true:

1. `FORTEMI_BACKOFFICE_PREVIEW=true` is explicitly configured.
2. The response marks each preview-only surface with `state: "preview"`.
3. Mutating/admin operations remain disabled.
4. Every preview request emits the declared audit event with redacted subject, tenant-presence, action, and correlation metadata.

If the preview gate is not enabled, admin endpoints that do not have production backing must return an unavailable response or remain unregistered. The public compatibility endpoint still reports the draft surface contract so HotM can render disabled or preview-labeled panels without invoking incompatible APIs.

## Discovery Contract

`GET /api/v1/system/compatibility` is public-safe and returns `Cache-Control: no-store`. It must include every backoffice surface even when unavailable, so clients do not infer support from field presence. Unknown extra surfaces are additive; missing required surfaces make the backoffice contract incompatible for HotM enterprise UX.

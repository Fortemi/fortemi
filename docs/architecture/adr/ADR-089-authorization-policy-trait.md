# ADR-089: Authorization Policy Trait

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-071 (auth middleware), ADR-088 (plugin strategy), ADR-090 (tenancy), ADR-094 (fail-closed default), ADR-100 (MCP scope gate)
**Related docs:** `.aiwg/architecture/ce-ee-audit-2026-05.md` finding S-2, `.aiwg/security/multi-tenant-threat-model.md` §5

## July 2026 checkpoint rebaseline

The core authorization seam is partially implemented: `AuthorizationPolicy`, `AllowAllPolicy`, and `RoleBasedPolicy` exist in `matric-core`, and `matric-api` selects a policy at startup. This ADR is not yet an enterprise RBAC readiness claim because full route/tool coverage, backoffice discovery, and private EE RBAC implementations remain gated by `Fortemi/fortemi#1020` and `Fortemi-Enterprise/rbac#1`.

## Context

ADR-071 added authentication middleware: validates JWT/OAuth tokens, builds an `AuthPrincipal` enum (`OAuthClient { client_id, scope, user_id }` | `ApiKey { key_id, scope }` | `Anonymous`), and rejects requests when `REQUIRE_AUTH=true`.

There is **no authorization layer** between authentication and the route handlers. Today, an authenticated principal can call any endpoint in their declared OAuth scope, and many endpoints do not check scope explicitly. There is no `can(principal, action, resource, context) -> Decision` decision point.

For multi-tenant SaaS (the EE use case), this is a critical gap:
- A tenant admin and a tenant viewer must have different authority on the same resource
- Cross-tenant admin operations need explicit elevation, not implicit access
- Compliance (SOC2 CC6.1, ISO 27001 A.9.4.1) requires demonstrable access decisions per request

The CE/EE audit (`ce-ee-audit-2026-05.md` finding S-2) classifies this as **HIGH severity** and requires it before any multi-tenant deployment.

## Decision

**Introduce a pluggable `AuthorizationPolicy` trait in `matric-core` with a permissive `AllowAllPolicy` default for CE and EE-provided RBAC/ABAC implementations.**

### Trait definition

```rust
// crates/matric-core/src/authorization.rs

use async_trait::async_trait;

#[async_trait]
pub trait AuthorizationPolicy: Send + Sync {
    /// Decide whether `principal` may perform `action` on `resource` in `ctx`.
    ///
    /// MUST be deterministic for the same inputs within a config epoch.
    /// MUST be safe to call concurrently.
    /// SHOULD complete in <5ms for in-process policies.
    async fn authorize(
        &self,
        principal: &AuthPrincipal,
        action: &Action,
        resource: &Resource,
        ctx: &AuthContext,
    ) -> Result<Decision, AuthzError>;
}

pub enum Decision {
    Allow { obligations: Vec<Obligation> },
    Deny { reason: String },
    Indeterminate,  // Caller MAY retry; default core handling treats as Deny.
}

pub struct Action(pub Cow<'static, str>);   // e.g., "notes:read", "tenant:export"
pub struct Resource {
    pub kind: ResourceKind,                  // Note, Embedding, Archive, Tenant, ...
    pub id: Option<String>,
    pub tenant_id: Option<TenantId>,
    pub attrs: HashMap<String, serde_json::Value>,
}

pub enum Obligation {
    LogPii { fields: Vec<String> },
    RequireMfa,
    RecordReason { template: String },
    EnforceTtl { seconds: u64 },
}
```

### Default impl (CE)

```rust
pub struct AllowAllPolicy;

#[async_trait]
impl AuthorizationPolicy for AllowAllPolicy {
    async fn authorize(&self, _p: &AuthPrincipal, _a: &Action, _r: &Resource, _c: &AuthContext)
        -> Result<Decision, AuthzError>
    {
        Ok(Decision::Allow { obligations: vec![] })
    }
}
```

CE ships with `AllowAllPolicy` to preserve current behavior. This is documented explicitly in the CE `LICENSE.txt` and `SECURITY.md` so operators understand they are responsible for network-level access control if they do not install an EE policy plugin.

### Decision points (where authorize is called)

| Layer | When | What `Action` |
|---|---|---|
| Router middleware | Every authenticated request | `<verb>:<resource>` per route |
| MCP server | Every tool invocation | `mcp:<tool_name>` |
| Job dispatcher | Job creation | `job:create:<job_type>` |
| Plugin contract | Cross-tenant access | `system:tenant_admin` |
| OAuth scope upgrade | Token issuance with elevated scope | `auth:scope:<scope>` |

### Plugin integration

`AuthorizationPolicy` is a **Class A, B, or C plugin** per `.aiwg/architecture/plugin-contract-spec.md`. EE impls include:

- `fortemi-enterprise-rbac` — static role-based policy from configuration, consuming the standard role catalog
- `fortemi-enterprise-rbac-casbin` — Casbin engine, Class A
- `fortemi-enterprise-rbac-opa` — Open Policy Agent sidecar, Class B
- Vendor-specific cloud IAM bridges, Class C

### Concrete role and scope vocabulary

The starter role/scope catalog is defined in `.aiwg/architecture/roles-scopes-catalog.md` — six standard roles (`viewer`, `editor`, `power_user`, `archive_admin`, `tenant_admin`, `system_admin`), ~50 scopes across nine surfaces (content, archives, inference, MCP, tenant admin, identity, privacy, system), the role→scope matrix, and a custom-role overlay mechanism. The CE `RoleBasedPolicy` default impl consumes that catalog; EE `fortemi-enterprise-rbac` adds tenant-defined custom roles on top per ADR-090.

### Obligations

Decisions can return obligations that the core enforces alongside the Allow. Initial obligations:

- `LogPii { fields }` — audit event MUST list which PII fields were accessed
- `RequireMfa` — request rejected if `AuthContext` lacks recent MFA
- `RecordReason { template }` — request rejected if user did not provide a reason matching the template (for sensitive admin actions)
- `EnforceTtl { seconds }` — response cache/session limited

### Failure modes

| Outcome | Core behavior |
|---|---|
| `Allow` | Continue; emit `auth.decision` audit event |
| `Deny` | Return `403 Forbidden` with `reason` (sanitized); emit `auth.decision` audit event |
| `Indeterminate` | Treat as `Deny` (fail-closed per ADR-094); emit `auth.indeterminate` audit event with high severity |
| Policy plugin error | Treat as `Deny`; emit `auth.policy_error`; trip circuit breaker after N failures |
| Policy plugin timeout | Treat as `Deny` after configured timeout (default 100ms in-process, 500ms sidecar) |

## Consequences

### Positive
- (+) Single decision point — auditable, testable, swappable
- (+) CE users keep current behavior (no policy = AllowAll) explicitly
- (+) EE plugins layer in real policy without forking core
- (+) Obligations enable compliance patterns (PII logging, MFA gating) without per-handler code
- (+) Compatible with future MCP scope gate (ADR-100) and DSAR handling (ADR-099)

### Negative
- (-) Every authenticated route gains an authorization call (~µs in CE AllowAll; bounded by policy plugin for EE)
- (-) Resource construction (especially `attrs`) adds boilerplate; mitigate with macros
- (-) Out-of-process policy (Class B/C) adds 0.5-2 ms tail latency
- (-) Initial impl will require touching ~80 route handlers

### Neutral
- (~) Action vocabulary needs governance — `.aiwg/process/action-vocabulary.md` (forthcoming) defines naming and stability rules

## Implementation

**Code location:**
- Trait: `crates/matric-core/src/authorization.rs` (new)
- Middleware: `crates/matric-api/src/middleware/authorize.rs` (new)
- Default impl: `crates/matric-core/src/authorization/allow_all.rs`
- EE impls: separate `fortemi-enterprise-rbac*` crates

**Key changes:**
1. Define trait + `Action`/`Resource`/`Decision`/`Obligation` types in matric-core
2. Implement `AllowAllPolicy` and ship as CE default
3. Add `AuthorizePolicy` state to `AppState`; wire from config
4. Add `authorize_middleware` after `auth_middleware` in router stack (matric-api/src/main.rs ~line 2351)
5. Define action vocabulary in `.aiwg/process/action-vocabulary.md`
6. Implement first EE plugin (`fortemi-enterprise-rbac`) as static role config
7. Add audit events `auth.decision`, `auth.indeterminate`, `auth.policy_error`

**Migration:** rolling. New handlers use the middleware from day one; existing handlers are migrated in batches via tracking issues.

**Testing:**
- Policy fuzz testing (random principal × action × resource combinations)
- Performance: 95p < 5ms in-process, < 25ms sidecar
- Failure injection: policy plugin returns errors, times out

## References

- ADR-071 — Auth middleware (the layer this builds on)
- ADR-088 — Plugin strategy
- ADR-094 — Fail-closed authentication default (related fail-closed posture)
- ADR-100 — MCP scope gate (uses this trait)
- `.aiwg/security/multi-tenant-threat-model.md` §5
- OWASP ASVS 4.0 §4.1 (Access Control)
- NIST SP 800-162 (ABAC reference model)

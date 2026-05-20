# ADR-090: Multi-Tenancy Model — Schema-per-Tenant with Type-Enforced Scope

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-068 (archive isolation routing), ADR-071 (auth middleware), ADR-089 (authorization policy), ADR-094 (fail-closed)
**Related docs:** `.aiwg/security/multi-tenant-threat-model.md` (full STRIDE)

## Context

ADR-068 introduced archive isolation via PostgreSQL schemas with `SET LOCAL search_path TO {schema}, public` wrapped by a `SchemaContext`. Today this enables multiple parallel **archives within a single deployment** — same user, different memory contexts.

For the hosted multi-tenant SaaS (the EE deployment target), we need **tenant isolation** as a distinct concept from archive isolation:
- A tenant has 1..N archives
- A user belongs to 1..N tenants
- A request must be scoped to exactly one tenant
- Cross-tenant access requires explicit elevation (system admin)
- "Forgetting to scope" must be a compile error, not a runtime hope

The CE/EE audit (finding S-3, HIGH severity) and the multi-tenant threat model both flag this as a launch-blocking gap for hosted EE.

Three architectural options were analyzed in `.aiwg/security/multi-tenant-threat-model.md` §2:

| Option | Isolation | Scale ceiling | Ops complexity |
|---|---|---|---|
| (a) Schema-per-tenant | Strong (PG namespace) | ~500 tenants/cluster (practitioner-reported; not benchmarked here) | Moderate — pg_dump and planner stats degrade with many schemas |
| (b) Row-level via RLS + `tenant_id` everywhere | Strong (PG RLS) if policy correct | Very large | Moderate — every table needs RLS policy + every query needs context |
| (c) Hybrid: DB-per-large-tenant + schema-per-tenant for the long tail | Strongest for large tenants | Effectively unbounded | High — multi-cluster ops |

## Decision

**Adopt schema-per-tenant (option a) as the primary multi-tenancy model up to ~500 tenants per cluster, with a documented graduation path to the hybrid model (c). Enforce tenant scope at the type level via a `TenantScopedDb` newtype.**

### Tenancy primitives

```rust
// crates/matric-core/src/tenancy.rs

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TenantId(String);   // ULID-like opaque id, NOT mutable

impl TenantId {
    /// Schema name for this tenant. Format: "tenant_<lowercase_alphanum>"
    pub fn schema_name(&self) -> &str { /* validated at construction */ }
}

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub principal: AuthPrincipal,
    pub elevated: bool,           // True if user has system:tenant_admin scope
}
```

### Type-enforced database scope

```rust
// crates/matric-db/src/tenant_scoped_db.rs

/// The only database handle exposed to request handlers in multi-tenant mode.
/// All queries are automatically prefixed with `SET LOCAL search_path TO {tenant_schema}, public`.
pub struct TenantScopedDb<'a> {
    inner: &'a Database,
    tenant: &'a TenantContext,
}

impl<'a> TenantScopedDb<'a> {
    pub fn for_tenant(db: &'a Database, ctx: &'a TenantContext) -> Self { ... }

    /// Cross-tenant access. Requires `ctx.elevated == true`, emits an audit event.
    pub fn for_system(db: &'a Database, ctx: &'a TenantContext) -> Result<SystemScopedDb<'a>> {
        if !ctx.elevated {
            return Err(Error::Forbidden("system scope requires elevation"));
        }
        audit::emit(AuditEvent::SystemAccessGranted { principal: ctx.principal.id() });
        Ok(SystemScopedDb { inner: db, ctx })
    }
}
```

### Multi-tenant build feature

```toml
# fortemi/crates/matric-api/Cargo.toml
[features]
default = ["single-tenant"]
single-tenant = []
multi-tenant = []
```

In `multi-tenant` builds:
- `Database` does **not** implement methods that take raw queries without a `TenantScopedDb` wrapper
- Direct `state.db` access is compile-error
- `Database::raw()` is gated behind a separate feature with a loud constant `UNSAFE_RAW_DB_ACCESS = true`

In `single-tenant` (CE) builds:
- `Database` works as today
- `TenantScopedDb` exists as a thin wrapper that just forwards (so handlers can use one type across builds)

### Tenant context propagation

The auth middleware resolves the tenant from the JWT/API key (claim `tenant_id`) and attaches a `TenantContext` to the request extensions. Downstream extractors (analogous to `Auth` / `RequireAuth` extractors) provide `TenantScopedDb` directly to handlers.

```rust
async fn list_notes(
    db: TenantScopedDb,    // Axum extractor; refuses to construct without TenantContext
    Query(q): Query<ListNotesQuery>,
) -> Result<Json<ListNotesResponse>> {
    let notes = db.note_repo().list(&q).await?;
    Ok(Json(ListNotesResponse { notes }))
}
```

### Archive within tenant

Archives (ADR-068) become nested within tenants. The tenant schema contains an `archive_registry` table; archive schemas are named `tenant_<id>_archive_<name>`. The `for_archive` method on `TenantScopedDb` returns a further-scoped handle.

### Migration from single-tenant CE to multi-tenant EE

A CE deployment can be promoted to EE multi-tenant by:
1. Adding a `tenant_id` column to `archive_registry` (default tenant = `"default"`)
2. Renaming the public schema content to `tenant_default` via a migration
3. Rebuilding with `--features multi-tenant`

A separate migration ADR (forthcoming) will detail the data-migration plan.

### Scale ceiling and graduation path

| Tenant count | Strategy |
|---|---|
| 1..50 | Single cluster, schema-per-tenant, shared connection pool |
| 50..500 | Single cluster, schema-per-tenant, **per-tenant connection pool budgets**, planner-stats targeted vacuum |
| 500+ | **Hybrid**: dedicated cluster per "premium" tenant (revenue-justified); shared cluster pool for long-tail. New tenants land in least-loaded cluster. Routing via tenant→cluster map in a control-plane DB. |

The 500-tenant figure is practitioner-reported and not benchmarked by this work. A benchmarking subtask will land before EE GA to confirm or revise.

## Consequences

### Positive
- (+) Strong isolation via PostgreSQL namespace — verified by tooling (cannot accidentally SELECT from another schema without explicit qualification)
- (+) Compile-time enforcement via `TenantScopedDb` — "forgot to scope" is a build error in EE builds
- (+) Auditable: cross-tenant access requires explicit `for_system()` which audit-logs
- (+) Extends rather than rewrites ADR-068 — `SchemaContext` machinery is reused
- (+) Graduation path defined — not boxed in at 500 tenants

### Negative
- (-) PostgreSQL planner statistics and `pg_dump` degrade as schema count grows; mitigated by per-schema vacuum scheduling but real
- (-) Connection pool sizing becomes per-tenant or per-cluster, not single global; ops complexity rises
- (-) Per-tenant DDL changes during migrations require running each schema; mitigated by sqlx migrations runner extended to iterate tenants
- (-) CE single-tenant and EE multi-tenant builds diverge (feature flag); test matrix doubles
- (-) Cross-tenant analytics (e.g., usage dashboards across all tenants) require `for_system()` and explicit auditing

### Neutral
- (~) Backup/restore strategy shifts to per-tenant schema dumps; ops runbook in `.aiwg/operations/` needed
- (~) Tenant deletion is reversible (rename + retain) for N days before drop; soft-delete TTL policy needed

## Implementation

**Code location:**
- Tenancy primitives: `crates/matric-core/src/tenancy.rs` (new)
- Scoped DB: `crates/matric-db/src/tenant_scoped_db.rs` (new)
- Middleware: `crates/matric-api/src/middleware/tenant.rs` (new)
- Extractor: `crates/matric-api/src/extractors/tenant_scoped_db.rs` (new)

**Phases:**
1. (this ADR + issues) Land `TenantId`, `TenantContext`, `TenantScopedDb` shells with no-op semantics under `multi-tenant` feature
2. Migrate ADR-068 archive routing under the new scope type
3. Migrate handler population — batch 1 (read-only routes), batch 2 (write routes), batch 3 (admin)
4. Add `tenant_admin` and `system` scopes to OAuth/API key surface
5. Land tenant lifecycle endpoints (create, suspend, delete) — separate ADR for these endpoints
6. Benchmark schema-per-tenant scaling on representative workload before GA

**Testing:**
- Property-based test: random handler + random tenant context never returns data from another tenant
- Failure mode: handler that bypasses extractor and calls `Database::raw()` is a compile error in multi-tenant builds

## References

- ADR-068 — Archive isolation routing (the foundation)
- ADR-071 — Auth middleware (provides AuthPrincipal)
- ADR-089 — Authorization policy (uses TenantContext)
- `.aiwg/security/multi-tenant-threat-model.md` — Full STRIDE analysis
- PostgreSQL docs, "Schemas" — `https://www.postgresql.org/docs/current/ddl-schemas.html`
- OWASP ASVS 4.0 §1.11 (multi-tenancy controls)

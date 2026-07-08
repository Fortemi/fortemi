# ADR-090: Multi-Tenancy Model — Shared Schema + Postgres RLS

**Status:** Accepted (revised 2026-05-20 to align with HotM ADR-MOBILE-001 Decision 6)
**Date:** 2026-05-20
**Deciders:** roctinam
**Related:** ADR-068 (archive isolation), ADR-071 (auth middleware), ADR-089 (authorization), ADR-094 (fail-closed)
**Upstream:** HotM `.aiwg/architecture/adr-mobile-cloud-architecture.md` (ADR-MOBILE-001) — strategic source
**Related docs:** `.aiwg/security/multi-tenant-threat-model.md`
**Related issues:** Fortemi/fortemi#707 (matric-api auth + multi-tenancy epic)

## Revision history

| Rev | Date | Change |
|---|---|---|
| 0 | 2026-05-20 | Initial draft proposed schema-per-tenant with `TenantScopedDb` newtype |
| 1 | 2026-05-20 | **Revised** to match HotM ADR-MOBILE-001 Decision 6 and Gitea Fortemi/fortemi#707: shared-schema with Postgres RLS, `NOSUPERUSER NOBYPASSRLS` role, `SET LOCAL app.current_tenant` per request, FORCE RLS on every tenant-scoped table, CI gate via `pg_class`/`pg_policy`. Schema-per-tenant retained only as documented escalation trigger. |

## July 2026 checkpoint rebaseline

Accepted status means the target tenancy architecture is accepted; it does not mean the RLS implementation is complete. The July 2026 checkpoint found no complete migration/policy/test-gate evidence for every tenant-scoped table and no completed `TenantScopedConn` implementation in `crates/`. Hosted multi-tenant production remains blocked on `Fortemi/fortemi#1016`.

## Context

ADR-068 introduced archive isolation via PostgreSQL schemas with `SET LOCAL search_path TO {schema}, public` wrapped by a `SchemaContext`. That mechanism enables multiple parallel **archives within a single deployment** — same user, different memory contexts. It does **not** provide multi-tenant SaaS isolation.

For the hosted multi-tenant deployment (HotM mobile + HotM cloud-mode desktop), HotM ADR-MOBILE-001 Decision 6 selected the tenancy model after research into AWS, Crunchy, and Supabase guidance: **shared-schema with Postgres Row-Level Security**. The convergent recommendation across all three vendors is shared-schema + RLS for the operational scale HotM targets (well under 10k tenants at launch). Schema-per-tenant is documented as a future escalation path if a specific compliance regime (HIPAA / SOC2) requires per-tenant data separation or if a tenant grows beyond shared-schema's operational ceiling.

The audit (`ce-ee-audit-2026-05.md`) initially proposed schema-per-tenant before the upstream HotM decision was integrated. This ADR corrects that.

## Decision

**Adopt shared-schema with Postgres Row-Level Security as the multi-tenancy model. Make RLS load-bearing — not advisory — through six mandatory invariants below. Reserve schema-per-tenant and database-per-tenant as documented escalation triggers.**

### The model

Every user-data table carries:

```sql
CREATE TABLE notes (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    -- ... rest of columns
);

CREATE INDEX notes_tenant_id_idx ON notes(tenant_id);

ALTER TABLE notes ENABLE ROW LEVEL SECURITY;
ALTER TABLE notes FORCE ROW LEVEL SECURITY;

CREATE POLICY notes_tenant_isolation ON notes
    USING (tenant_id = current_setting('app.current_tenant')::uuid);
```

Every request handler wraps its query work in a transaction and sets the tenant scope before any tenant-scoped query runs:

```rust
let mut tx = pool.begin().await?;
sqlx::query("SET LOCAL app.current_tenant = $1")
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;
// ... handler queries inside this transaction ...
tx.commit().await?;
```

### Six mandatory invariants

These are what make RLS load-bearing instead of advisory. Each is enforced or asserted, not relied upon as discipline.

1. **Database role posture.** `matric-api` connects as a PostgreSQL role with `NOSUPERUSER NOBYPASSRLS`. Startup assertion fails-closed if either attribute is present.

   ```rust
   // crates/matric-db/src/role_assertion.rs (new)
   let row = sqlx::query!(
       "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user"
   )
   .fetch_one(pool).await?;
   if row.rolsuper || row.rolbypassrls {
       return Err(Error::Config(format!(
           "Refusing to start: DB role has rolsuper={} rolbypassrls={}",
           row.rolsuper, row.rolbypassrls
       )));
   }
   ```

2. **FORCE RLS** is set on every tenant-scoped table. Without `FORCE`, RLS does not apply to the table owner — and migrations frequently run as owner.

3. **CI gate via pg_catalog.** A CI step queries `pg_class` and `pg_policy` and fails the build when any table in the tenant-scoped schema lacks `rowsecurity = true` or has no policy referencing `current_setting('app.current_tenant')`. New table without RLS = failed build. See `.aiwg/security/multi-tenant-threat-model.md` §6 for the exact assertion query.

4. **Transaction-mode pooling only.** `SET LOCAL` is transaction-scoped. PgBouncer **session mode is unsafe** (transactions from different requests can share a connection and inherit each other's `app.current_tenant`). Production MUST use transaction-mode pooling (PgBouncer transaction mode, or no pooler at all). Documented in deployment runbook; asserted on startup by reading `application_name` or pool stats.

5. **Set-local-before-query discipline.** Every tenant-scoped query path begins with a transaction + `SET LOCAL app.current_tenant`. This is enforced at the type level by the `TenantScopedConn` newtype (below) and verified by integration tests.

6. **Type-enforced tenant scope (Rust).** The handler signature receives a `TenantScopedConn` extractor, not a raw `&mut PgConnection`. The extractor:
   - Opens the transaction
   - Sets `app.current_tenant`
   - Returns a guard that ensures the transaction commits or rolls back

   ```rust
   // crates/matric-api/src/extractors/tenant_scoped_conn.rs (new)
   pub struct TenantScopedConn<'a> {
       tx: sqlx::Transaction<'a, sqlx::Postgres>,
       tenant_id: TenantId,
   }

   #[async_trait]
   impl<'a> FromRequestParts<AppState> for TenantScopedConn<'a> {
       async fn from_request_parts(...) -> Result<Self> {
           let ctx: &AuthContext = parts.extensions.get().ok_or(Error::Unauthorized)?;
           let mut tx = state.pool.begin().await?;
           sqlx::query("SET LOCAL app.current_tenant = $1")
               .bind(ctx.tenant_id().0)
               .execute(&mut *tx).await?;
           Ok(TenantScopedConn { tx, tenant_id: ctx.tenant_id().clone() })
       }
   }
   ```

   Handlers that need cross-tenant access (admin endpoints) take a `SystemScopedConn` instead — its constructor requires `principal.has_scope("system:tenant_admin")` and emits a `system.cross_tenant_access` audit event (per ADR-091).

### Tenant ID semantics

```rust
// crates/matric-core/src/tenancy.rs (new)
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub uuid::Uuid);

// Source: extracted from the OAuth JWT claim "tenant_id" by fortemi-auth-core's
// AuthContext, or from API key metadata.
```

Tenant ID is the **tenant uuid from the OAuth/JWT claim**, not derived from the principal. A user can be a member of multiple tenants; the JWT/API key carries which tenant is active for the current session.

### Single-tenant mode (HotM desktop sidecar)

The HotM desktop local-install path runs `matric-api` in single-tenant mode. Configuration:

- `FORTEMI_MULTI_TENANT=false` (default for the desktop sidecar build)
- Database does not require RLS to be enabled (legacy schema acceptable)
- A synthetic `tenant_id = '00000000-0000-0000-0000-000000000000'` is used as a constant in all queries to keep code paths uniform

Multi-tenant mode (`FORTEMI_MULTI_TENANT=true`):
- Asserts the six invariants at startup
- Refuses to start if any assertion fails
- Required for the hosted deployment HotM mobile + HotM cloud-mode desktop consume

### Migration of existing data

The HotM desktop sidecar has existing notes without `tenant_id`. The migration plan (covered by Fortemi/fortemi#707 sub-item 1):

1. Add `tenant_id UUID NULL` column to every user-data table
2. Backfill all existing rows with the synthetic single-tenant UUID
3. `ALTER COLUMN tenant_id SET NOT NULL`
4. Add B-tree index `(tenant_id)` on every table
5. Add RLS policies + `FORCE ROW LEVEL SECURITY`
6. Test against a snapshot of representative data
7. Run on production with downtime if the snapshot test surfaces issues

### When schema-per-tenant becomes the right answer

Document the escalation triggers explicitly so the team does not re-litigate the tenancy model on every operational hiccup:

| Trigger | Action |
|---|---|
| HIPAA, SOC2 Type II, or PCI-DSS requires per-tenant data separation | Migrate the regulated tenants to schema-per-tenant. Shared-schema remains for unregulated tenants. |
| Single tenant grows beyond shared-schema ceiling (~10k tenants per cluster cited as the practitioner threshold across AWS/Crunchy/Supabase) | Migrate the largest tenants to dedicated schemas or dedicated databases |
| Tenant requires data residency (e.g., EU-only) | Region-pinned cluster per residency requirement; shared-schema within the region |
| Customer signs an MSA requiring "logical isolation guaranteed by separate schema" | Move that customer to schema-per-tenant; document in their contract addendum |

The schema-per-tenant escalation path is **not** the same as ADR-068's archive-per-user pattern — archives are within-tenant memory contexts, not tenant boundaries.

## Consequences

### Positive

- (+) Aligns with HotM ADR-MOBILE-001 Decision 6 and existing Fortemi/fortemi#707 scope
- (+) Operationally simple — one schema, one connection pool, one backup process
- (+) Standard Postgres feature; well-understood by ops/DBAs
- (+) Compatible with pgvector — HNSW/IVFFLAT indexes work; B-tree filter on `tenant_id` runs first
- (+) RLS provides a hard database-tier wall, not a developer-discipline wall
- (+) `TenantScopedConn` extractor makes "forgot to scope" a compile error in handler signatures
- (+) CI gate catches new tables without RLS at PR time, not in production
- (+) Migration from current single-tenant HotM sidecar to multi-tenant is well-scoped (Fortemi/fortemi#707 sub-item 1)

### Negative

- (-) Connection pooling is constrained to transaction mode (PgBouncer session mode forbidden); operational complexity in pool sizing
- (-) Every query must be inside a `SET LOCAL` transaction; raw `&Pool` use is forbidden in tenant-scoped paths
- (-) RLS adds a small per-query cost (planner has extra predicate); benchmark before launch confirms acceptable
- (-) Cross-tenant analytics (admin dashboards over all tenants) require explicit `SystemScopedConn` + audit emission
- (-) Migration to multi-tenant on a populated database requires a maintenance window
- (-) If RLS policy is missed on a new table, the CI gate is the only guard — the gate must never be bypassed

### Neutral

- (~) Tenant deletion is soft-delete + N-day retention before hard-delete; operational runbook in `.aiwg/operations/`
- (~) ADR-068 archive isolation pattern continues to work within tenants — archives are nested under tenants

## Implementation

**Code location:**
- Tenancy primitives: `crates/matric-core/src/tenancy.rs` (new — `TenantId`, `AuthContext::tenant_id()`)
- Role assertion: `crates/matric-db/src/role_assertion.rs` (new)
- TenantScopedConn extractor: `crates/matric-api/src/extractors/tenant_scoped_conn.rs` (new)
- SystemScopedConn extractor: `crates/matric-api/src/extractors/system_scoped_conn.rs` (new)
- Migration: `migrations/{date}_add_tenant_id_and_rls.sql` (new)
- CI gate: `ci/scripts/check-rls-coverage.sh` (new)

**Phases** (mirrors Fortemi/fortemi#707 scope):

1. Add `tenant_id` column + indexes + backfill to all user-data tables
2. Add RLS policies + FORCE RLS
3. Switch DB role to `NOSUPERUSER NOBYPASSRLS`; add startup assertion
4. Implement `TenantScopedConn` extractor; migrate read-only handlers
5. Migrate write handlers
6. Migrate admin handlers to `SystemScopedConn`
7. Land 10-case isolation test suite (per #707 sub-item 4)
8. Add CI gate (`pg_class` / `pg_policy` check)
9. Document operational runbook (tenant create/suspend/delete, backup-per-tenant)

**Testing — the 10 mandatory isolation tests** (verbatim from Fortemi/fortemi#707):

1. `test_user_b_cannot_list_user_a_notes`
2. `test_user_b_cannot_fetch_user_a_note_by_uuid` (returns 404, not 403)
3. `test_user_b_cannot_update_user_a_note`
4. `test_user_b_cannot_delete_user_a_note`
5. `test_user_b_cannot_insert_into_user_a_collection`
6. `test_same_connection_reused_between_users_isolates`
7. `test_sql_injection_in_search_string_cannot_bypass_rls`
8. `test_vector_similarity_search_filters_by_tenant_before_scoring` (pgvector-specific)
9. `test_new_table_without_rls_fails_ci` (meta-test via `pg_class`/`pg_policy`)
10. `test_role_lacks_bypassrls_and_superuser` (startup assertion)

## References

- HotM `.aiwg/architecture/adr-mobile-cloud-architecture.md` (ADR-MOBILE-001) — Decision 6 strategic source
- HotM `.aiwg/research/findings/mobile-multitenant-byo-llm.md` §1, §2 — research backing
- Fortemi/fortemi#707 — implementation epic
- ADR-068 — archive isolation (within-tenant; complementary)
- ADR-071 — auth middleware (provides AuthContext.tenant_id())
- ADR-089 — AuthorizationPolicy (composes on top of RLS row visibility)
- ADR-091 — AuditSink (consumes `system.cross_tenant_access` events)
- PostgreSQL docs, "Row Security Policies" — https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- AWS Database Blog, "Multi-tenant data isolation with PostgreSQL Row Level Security"
- Supabase docs, "Row Level Security"

# matric-api

`matric-api` is the Fortemi HTTP runtime. The default community build does not
compile the private hosted authentication adapter. Internal builds opt in with
the `hosted-auth` feature and a signed, pinned `fortemi-auth` release.

## Hosted tenant transaction invariant

`FORTEMI_MULTI_TENANT=true` requires canonical bearer authentication, an active
tenant registry record, a hardened PostgreSQL runtime role, and an unscoped
runtime pool. Tenant state is established only inside a request transaction:

```sql
SELECT set_config('app.current_tenant', $1, true);
```

Handlers that touch tenant data must use `TenantRequestScope::with_connection`
or an equivalent repository interface tied to that transaction. They must not
acquire a second connection from `AppState.db.pool`. This is compatible with
transaction-mode pooling; session-scoped tenant state is forbidden.

The transaction coordinator intentionally rejects streaming response bodies.
SSE, WebSocket, long-lived inference, and ingest streams need route-specific,
short transaction boundaries and must be covered by the #728/#729 isolation
matrix before hosted readiness can become true.

See `docs/deployment/hosted-postgresql-role.md` for credential separation,
grants, startup assertions, verification, and rollback.

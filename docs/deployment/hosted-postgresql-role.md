# Hosted PostgreSQL roles and tenant scope

This runbook applies only to the internal hosted build compiled with
`matric-api/hosted-auth`. Community and personal-server deployments retain the
synthetic local tenant and do not require these roles.

## Role split

Use different credentials for migrations and application runtime. The runtime
must be `NOSUPERUSER NOBYPASSRLS`, must not own application tables, and must not
have `CREATE` on the database or `public` schema.

The migration role may own objects and use `BYPASSRLS` because schema/data
migrations can require deployment-wide access after forced RLS is active. It
must never be supplied to `matric-api` as `DATABASE_URL`.

Run the following as the database administrator and replace the password
placeholders through the deployment secret manager:

```sql
CREATE ROLE fortemi_migrator
  LOGIN NOSUPERUSER BYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;

CREATE ROLE fortemi_runtime
  LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION;

GRANT CONNECT ON DATABASE fortemi TO fortemi_migrator, fortemi_runtime;
GRANT USAGE ON SCHEMA public TO fortemi_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO fortemi_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO fortemi_runtime;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO fortemi_runtime;

ALTER DEFAULT PRIVILEGES FOR ROLE fortemi_migrator IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO fortemi_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE fortemi_migrator IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO fortemi_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE fortemi_migrator IN SCHEMA public
  GRANT EXECUTE ON FUNCTIONS TO fortemi_runtime;

REVOKE CREATE ON DATABASE fortemi FROM fortemi_runtime;
REVOKE CREATE ON SCHEMA public FROM fortemi_runtime;
```

After migrations, inspect grants and remove write privileges from system-scoped
tables that the runtime only reads:

```sql
REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
  ON TABLE public.tenant_registry FROM fortemi_runtime;
```

Keep backup, restore, archive provisioning, and emergency maintenance on named
administrative roles. Do not grant those capabilities to `fortemi_runtime`.

## Application configuration

Hosted startup requires both URLs and rejects identical values:

```text
FORTEMI_MULTI_TENANT=true
REQUIRE_AUTH=true
MIGRATION_DATABASE_URL=postgres://fortemi_migrator:...@db/fortemi
DATABASE_URL=postgres://fortemi_runtime:...@db/fortemi
```

Startup runs migrations with `MIGRATION_DATABASE_URL`, closes that pool, then
opens `DATABASE_URL` in unscoped mode. It aborts when the runtime role is a
superuser, has `BYPASSRLS`, owns a tenant table, or the public/archive tenant
catalog is incomplete.

## Pooling invariant

Tenant state is transaction-local. After canonical authentication and active
tenant lookup, request code must begin a transaction and call:

```sql
SELECT set_config('app.current_tenant', $1, true);
```

All tenant queries for that request must use the same transaction. Never use
session `SET`, a connection initialization tenant, or a raw pool handle in a
hosted handler. Transaction-mode PgBouncer is compatible only when the complete
unit of work remains inside one database transaction. Streaming routes require
a separate bounded transaction policy because a response body can outlive the
request transaction.

## Verification

Run the catalog and isolation tests with a disposable database and the hardened
runtime login. A superuser test URL is setup-only and must fail the runtime-role
assertion. Hosted readiness must remain false until #728 and #729 prove every
repository, search, job, archive, backup, and streaming path observes the same
tenant boundary.

## Rollback

There is no destructive down migration that removes `tenant_id` or disables
RLS. For a failed rollout:

1. Stop writes and retain the failed database for evidence.
2. Restore the pre-migration snapshot to a separate destination.
3. Compare per-table row counts and tenant backfill receipts.
4. Start the prior community build against the restored destination only.
5. Repair forward and repeat clean-destination plus representative-snapshot
   migration tests before another hosted rollout.

Never "roll back" by granting `BYPASSRLS` to the runtime role or disabling a
policy in place.

# External OIDC canonical ownership upgrade

In external-OIDC single-tenant mode, `FORTEMI_AUTH_DEFAULT_TENANT` is the
canonical owner of identities, content, memories, defaults and background jobs.
The reserved local tenant (`00000000-0000-0000-0000-000000000000`, L) is used by
community localhost installations. The configured external owner is E.

## Upgrade from 2026.10.5

Archive-management authorization is hardened by #1201. Ensure the administrator's
IdP claim mapping grants `admin`: `write` alone no longer permits archive
update/delete/clone/set-default, including `/memories/{name}` aliases. The full
`GET /api/v1/memories/overview` also requires `admin`; reader clients should use
`GET /api/v1/memories/available` for names and default flags without statistics.
The default policy still permits every archive within the caller's tenant;
per-archive grants require the enterprise policy (`Fortemi-Enterprise/rbac#2`).
Archive visibility hooks apply to routing, inventories and live/replayed SSE;
missing and denied archives return the same 404.

1. Back up PostgreSQL and the attachment storage directory. Record the configured
   E UUID. Keep the existing PAT pepper and attachment storage paths.
2. Stop **all** API instances and workers that write this database, including
   old binaries. This is an offline deployment upgrade; rolling upgrades across
   the ownership boundary are unsupported.
3. Configure `FORTEMI_AUTH_MODE=external-oidc`, the existing issuer, claim policy
   and audience settings, and `FORTEMI_AUTH_DEFAULT_TENANT=E`.
   Set `MIGRATION_DATABASE_URL` to the table-owner migration role with BYPASSRLS,
   database `CREATE`, and ownership/grant authority over application schemas,
   tables, sequences and functions (or a superuser). `DATABASE_URL` **must** use a separate
   `NOSUPERUSER NOBYPASSRLS` runtime login that owns no tenant tables. Both URLs
   are required; missing migration credentials, identical URLs, privileged
   runtime roles, table ownership or an incomplete forced-RLS catalog abort
   startup before any API or worker starts.
4. Start one upgraded API instance. Before opening the HTTP listener or starting
   workers, startup applies schema migrations and runs the ownership transaction.
   Wait for readiness, then start the remaining upgraded instances and workers.
5. Verify `/api/v1/me`, `/api/v1/archives` and `/api/v1/memories`, select a memory
   with `X-Fortemi-Memory`, and read/edit an existing note and download an existing
   attachment. Verify pending jobs resume. The atomic completion receipt is:

   ```sql
   SELECT owner, completed_at FROM public.external_oidc_ownership;
   ```

The startup step is automatic because admitting requests under E before moving
L's graph would hide old content and strand pending jobs. It is guarded by the
same transaction advisory lock as archive DDL and locks the discovered tables
against writes. Every instance checks the deployment receipt; a repeated run
with the same E does nothing. A different E fails startup. Changing the receipt
manually is not a supported tenant rename procedure.

## Transaction and preservation

The transaction inventories tenant-owned tables from PostgreSQL catalogs in
public and registered deployment memories. It moves only L rows to E, including
notes, revisions, attachments/blobs, SKOS, tags, embeddings/memberships,
configuration declarations/custody, users/PATs, archive registry/defaults,
outbox/audit state and pending/completed job records. Existing E rows remain
owned by E. Runtime job payloads recursively replace L values under `tenant_id`
keys; other strings, including UUID-looking content, remain unchanged.

UUIDs, attachment bytes and storage keys, custom type prompts/chunk settings,
configuration references, user disabled status, and PAT hashes/status/expiry
are retained. Restrictive composite FKs are saved, dropped, recreated and
validated in the transaction. User triggers are disabled during the transform
and restored to their exact previous enabled/disabled/replica state. Old archive
identity references are redirected to shared public identities. The v2026.10.5
`app_user` and `personal_access_token` archive clones are then dropped with
`RESTRICT` inside the ownership transaction; unexpected dependencies abort it.
Hosted startup performs the same retirement without relabeling any tenant.
Catalog failures identify offending schema-qualified relations. Missing author
columns and guards are repaired, and `sync_archive_schema` runs on the owning
connection before any archive is served. Every selectable archive receives E's
embedding-set/configuration and SKOS defaults.
Active notes receive any missing canonical default-set membership. Membership
triggers refresh the cached document/embedding counts and embedding coverage flag
from the preserved rows; these derived values can change during the upgrade.

If old bootstrap created equivalent E defaults with new UUIDs, both identities
survive: L's public name becomes canonical and the duplicate E baseline is
renamed `external-previous-<UUID>`. References keep their UUIDs. Equivalence compares configuration values, including
those reached through `recommended_config_id` and `embedding_config_id`, rather
than the referenced UUIDs or configuration names. Model, dimensions, provider,
chunking and index tuning differences are conflicts. Default selection is
reconciled separately. Index timestamps/status/sizes, embedding coverage and
counters are derived operational state; they never cause a configuration
conflict. Existing index metadata is preserved, and membership triggers
recompute counts/coverage from preserved rows when memberships are added.
The old E default configuration loses its default flag when the equivalent L
default is adopted.
Distinct customized baselines, conflicting archive defaults, duplicate OIDC
`(issuer,subject)` identities, and other uniqueness/reference conflicts abort
startup. No user is silently merged or re-enabled. Resolve the reported conflict
offline, preserving the intended customization, then retry.

Any failure or interrupted connection rolls back the entire ownership
transaction, including its DDL and completion receipt. Ordinary schema migrations
have their own migration transactions; they may already be applied after an
ownership failure. Retry the upgraded binary after resolving the error. After
a successful upgrade, rollback to an old binary requires restoring the backup;
an old L-bound runtime cannot read E content.

Community mode and hosted multi-tenant mode never invoke this ownership upgrade.
Unrelated tenant rows are neither relabeled nor used as baseline templates.
Shard exports select embedding configurations for the current owner explicitly,
including when the database connection can bypass row security.
Knowledge Shard schemas, profile versions and portable identity contracts do
not change.

## Required runtime role and preview bundle configuration

The supported isolation boundary is PostgreSQL forced row-level security with a
non-bypass runtime login. Every legacy repository and worker pool binds E, but
that setting cannot constrain a superuser or BYPASSRLS connection. Requiring a
restricted role covers all of those paths; maintaining independent tenant
predicates in every legacy query would leave isolation dependent on each future
query. `startup::database` checks the same role/catalog contract as hosted
startup before exposing its returned runtime pool.

Prepare grants **before** the first upgraded start. Use the existing object
owner as the migration login (or arrange ownership explicitly); a fresh login
with BYPASSRLS alone cannot alter another role's objects. As a PostgreSQL
administrator, for a bundle whose existing owner/database are `matric`:

```sql
ALTER ROLE matric BYPASSRLS;
GRANT CREATE ON DATABASE matric TO matric;
GRANT USAGE,CREATE ON SCHEMA public TO matric;
CREATE ROLE fortemi_runtime LOGIN NOSUPERUSER NOBYPASSRLS
    NOCREATEDB NOCREATEROLE NOREPLICATION;
\password fortemi_runtime
GRANT CONNECT ON DATABASE matric TO fortemi_runtime;
REVOKE CREATE ON DATABASE matric FROM fortemi_runtime;
REVOKE CREATE ON SCHEMA public FROM PUBLIC,fortemi_runtime;
DO $$
DECLARE s RECORD;
BEGIN
    FOR s IN SELECT nspname FROM pg_namespace
        WHERE nspname='public' OR nspname LIKE 'archive\_%' ESCAPE '\' LOOP
        EXECUTE format('GRANT USAGE ON SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA %I TO fortemi_runtime',s.nspname);
        EXECUTE format('ALTER DEFAULT PRIVILEGES FOR ROLE matric IN SCHEMA %I GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO fortemi_runtime',s.nspname);
        EXECUTE format('ALTER DEFAULT PRIVILEGES FOR ROLE matric IN SCHEMA %I GRANT USAGE,SELECT ON SEQUENCES TO fortemi_runtime',s.nspname);
    END LOOP;
END $$;
```

After the first upgraded start has applied migrations, keep the runtime login
read-only on the tenant registry and append-only on audit events, as in the
[hosted role runbook](../deployment/hosted-postgresql-role.md):

```sql
REVOKE INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER
    ON TABLE public.tenant_registry FROM fortemi_runtime;
REVOKE UPDATE,DELETE,TRUNCATE ON TABLE public.audit_event FROM fortemi_runtime;
```

Keep migration credentials out of the runtime login's role memberships. The
administrative login must own existing application objects: BYPASSRLS and DML
grants alone cannot alter tables, create indexes, drop/rename indexes, or grant
access to their schemas. If your existing objects belong to several roles,
consolidate ownership offline or use the administrator credential. Startup checks
the administrative role's table ownership, BYPASSRLS and database CREATE.
Connect it directly to PostgreSQL (or use session pooling); concurrent index
creation cannot run inside a transaction or through transaction pooling with
session tenant settings.

The API retains `MIGRATION_DATABASE_URL` behind a narrow administrative executor.
After authorization, admins can create, clone and delete memories over HTTP;
authorized memory use synchronizes schema changes through that executor. New
or synchronized schemas automatically receive runtime USAGE, table DML,
sequence and function grants. No offline memory provisioning or restart is
needed. Ordinary queries, content writes, queue claiming, index-status updates
and job settlement continue using `DATABASE_URL`. Index DDL uses the
administrative executor only after checking the claimed job's persisted owner,
active tenant and archive ownership; a payload `tenant_id` is never authority.
Hosted mode uses the same executor and a fenced hosted index handler; its
unqualified HTTP archive-management routes retain their existing closed gates.
Whole-database backup/restore still uses its documented administrative procedure.

When `FORTEMI_AUTH_ALLOW_LEGACY_TOKENS=true`, authenticated `mm_key_` and `mm_at_`
credentials bind to the startup-established E owner. Their scopes are intersected
with `FORTEMI_AUTH_LEGACY_SCOPE_CEILING` before route authorization; archive
create/clone/delete still require effective `admin` scope. Authorized memory
synchronization and index work use the same owner and administrative executor.
Legacy credentials retain their service-principal identity and do not provision
an OIDC user. With legacy compatibility disabled, these credentials are rejected.

Archive pause/resume paths accept the registry's `schema_name` or display name.
An exact schema key wins when it collides with another archive's display name.
With memories `research` and `archive_research`, `research` and `archive_research`
both address the first memory; use `archive_archive_research` for the second.
Each request changes only the resolved memory's canonical persisted pause key.

Set both URLs explicitly from your secret manager. `MIGRATION_DATABASE_URL`
connects as the original owner role (`matric`) and `DATABASE_URL` as the
restricted runtime role (`fortemi_runtime`), both to the same database, with
URL-encoded passwords:

```text
MIGRATION_DATABASE_URL=<MIGRATION_DATABASE_URL>
DATABASE_URL=<DATABASE_URL>
```

With an IdP access token whose claim policy grants `admin`, validate provisioning
and first use (`FORTEMI_URL` and `TOKEN` are your deployment URL and access token):

```bash
curl --fail-with-body "$FORTEMI_URL/api/v1/archives" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"upgrade-check"}'
curl --fail-with-body "$FORTEMI_URL/api/v1/notes" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -H 'X-Fortemi-Memory: upgrade-check' \
  -d '{"content":"Upgrade first-use check","pipeline":[]}'
```

Enable workers for pending `build_set_index` jobs to resume. For a previously
failed build, enqueue a new build for the affected set after installing both
credentials (`SET_SLUG` is the set slug in `MEMORY`), then inspect the queue and
resulting index definitions:

```bash
curl --fail-with-body -X POST "$FORTEMI_URL/api/v1/embedding-sets/$SET_SLUG/build-index" \
  -H "Authorization: Bearer $TOKEN" -H "X-Fortemi-Memory: $MEMORY"
```

```sql
SELECT id,tenant_id,status,error_message FROM public.job_queue
WHERE job_type='build_set_index' ORDER BY created_at DESC;
SELECT schemaname,indexname,indexdef FROM pg_indexes
WHERE indexname LIKE 'idx_embedding_hnsw_%'
   OR indexname LIKE 'idx_attach_emb_hnsw_%';
```

Absent indexes are created concurrently. Changed HNSW settings build a
replacement concurrently, then drop and rename the old index. Do not grant
ownership or DDL rights to the runtime role to recover these jobs.

The requested preview manifest path
`data-edge/deploy/data-edge/preview/fortemi/statefulset.yaml` was absent in the
review workspace. The read-only copies at
`de-fortemi-1991/deploy/data-edge/preview/fortemi/statefulset.yaml` and
`de-2626/deploy/data-edge/preview/fortemi/statefulset.yaml` run the pinned
2026.10.5 bundle in external-OIDC mode against the Nexa realm, with a configured
E, claim-policy ConfigMap and disabled workers. They supply `POSTGRES_PASSWORD`
but neither database URL.

The all-in-one bundle provisions the runtime split itself (#1220). In
external-OIDC or hosted mode, when neither `DATABASE_URL` nor
`MIGRATION_DATABASE_URL` is set, the entrypoint keeps the `POSTGRES_USER` owner
as `MIGRATION_DATABASE_URL`, grants it BYPASSRLS and database `CREATE`, applies
migrations with it so every tenant's archives are migrated, and then, before the API
serves, creates `fortemi_runtime` with the grants above (once, marked as
bundle-managed) and runs the API as that login. Later migrations and memory
synchronization keep its grants current; a restart does not re-grant privileges
that were revoked. Every start applies the two revocations above, rotates the
runtime password and re-checks the role's attributes, and fails if the login can
reach the owner, a superuser or a BYPASSRLS role. Only a SCRAM verifier reaches
PostgreSQL, over the local socket; the password is not persisted, logged or passed
as an argument. An existing `fortemi_runtime` the bundle did not create is
refused. Do not share the role with other processes: each start invalidates the
previous password. `POSTGRES_USER` must not be `fortemi_runtime`. MCP client
auto-registration uses the runtime login and the configured tenant; hosted mode
skips the bundle MCP client, which would belong to no tenant. The preview
StatefulSet therefore needs only the image update for the offline ownership
upgrade: stop it and all writers, update the image, and validate the completion
receipt and HTTP reads before resuming other instances/workers.

Setting either URL explicitly turns this off; then supply both, as for any
non-bundle deployment, with `MIGRATION_DATABASE_URL` connecting to the original
owner with BYPASSRLS (or a superuser) and `DATABASE_URL` to `fortemi_runtime`,
using URL-encoded passwords. Older copies in `de-fortemi-asis` and
`data-edge-pr1870` use local auth, which retains its existing database behavior.
No preview manifest was modified in this change.

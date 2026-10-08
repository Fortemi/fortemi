# Hosted tenant bootstrap

## Purpose

`matric-api admin bootstrap` provisions a hosted tenant without hand-written
SQL. Hosted admission accepts a verified token only when the value of the
configured tenant claim (`FORTEMI_AUTH_TENANT_CLAIM`, default
`fortemi:tenant_id`) names an `active` row in `tenant_registry`. The command:

1. creates or updates that `tenant_registry` row (status `active`);
2. seeds the tenant's default memory baseline under the tenant's own scope:
   the default embedding configuration (copied from the deployment's local
   template when present), the `default` embedding set and the `default` SKOS
   concept scheme, with the same bootstrap custody records the migrations use;
3. prints the tenant id to configure as the identity provider's tenant claim.

It is idempotent: a second run with the same inputs reports `unchanged` and
writes nothing. It never prints database URLs, passwords or tokens.

The tenant's default memory is the tenant-scoped `public` memory that hosted
routing selects when no other memory is requested, so no archive schema is
created.

Warnings: run it with the migration role (`MIGRATION_DATABASE_URL`), never the
runtime role. By default it applies pending migrations first.

## System topology

| Item | Value |
|------|-------|
| Binary | `/app/matric-api` in the Fortemi image (`ghcr.io/fortemi/fortemi`) |
| Database role | migration/owner role from [hosted-postgresql-role.md](hosted-postgresql-role.md) |
| Tables written | `tenant_registry`, `embedding_config`, `embedding_set`, `skos_concept_scheme`, `shard_embedding_set_bootstrap`, `shard_skos_scheme_bootstrap` |
| Concurrency | serialized by a transaction advisory lock; safe to run from several replicas |

## Procedure

### Inputs

| Flag | Environment fallback | Required | Meaning |
|------|----------------------|----------|---------|
| `--slug <slug>` | `FORTEMI_BOOTSTRAP_TENANT_SLUG` | yes | Stable tenant handle: 1-63 of `a-z`, `0-9`, `-`; not `local` |
| `--tenant-id <uuid>` | `FORTEMI_BOOTSTRAP_TENANT_ID` | no | Explicit tenant id. When omitted it is derived from the slug |
| `--display-name <name>` | `FORTEMI_BOOTSTRAP_TENANT_DISPLAY_NAME` | no | Defaults to the slug. Changing it updates the row |
| `--reactivate` | | no | Return a `suspended` tenant to `active` |
| `--dry-run` | | no | Report the planned actions; nothing is written and migrations are skipped |
| `--json` | | no | Machine-readable report |
| `--skip-migrations` | | no | Do not apply pending migrations first |

`MIGRATION_DATABASE_URL` is required. `FORTEMI_AUTH_TENANT_CLAIM` only changes
the claim name shown in the report.

Tenant ids derived from a slug are UUIDv5 values under the fixed namespace
`6f0b6c2e-4f1d-5a8e-9c3b-2d7e1a4f0c59`, so the same slug always maps to the
same id in every environment. Use `--tenant-id` when the identity provider
already issues tenant ids; then pass the same id on every run. A slug is never
renamed: a run whose slug and id disagree with the existing row is refused.

### Steps

1. Plan the change:

   ```bash
   MIGRATION_DATABASE_URL="$MIGRATION_DATABASE_URL" \
     /app/matric-api admin bootstrap --slug acme --display-name "Acme Research" --dry-run
   ```

   Expected output (the id is the slug-derived value):

   ```text
   Fortemi tenant bootstrap (dry run, nothing written)
     tenant id:      e414b99d-9d42-5b13-98bb-d63935ce064a
     slug:           acme
     display name:   Acme Research
     status:         active
     default memory: public
     tenant_registry              create
     embedding_config:default     create
     embedding_set:default        create
     skos_concept_scheme:default  create
   Result: changed
   Configure the IdP to emit claim fortemi:tenant_id = e414b99d-9d42-5b13-98bb-d63935ce064a
   ```

2. Apply it by running the same command without `--dry-run`.
3. Configure the identity provider to emit the printed tenant id in the tenant
   claim for this tenant's users (for Keycloak, a hardcoded-claim mapper on the
   tenant's client or group; see [authentication](../content/authentication.md)).
4. The first operator signs in through the identity provider. No database step
   is needed: the token's tenant now passes admission.

### Automation (`--json`)

```json
{
  "tenant_id": "e414b99d-9d42-5b13-98bb-d63935ce064a",
  "slug": "acme",
  "display_name": "Acme Research",
  "status": "active",
  "default_memory": "public",
  "dry_run": false,
  "changed": false,
  "steps": [
    { "resource": "tenant_registry", "action": "none" },
    { "resource": "embedding_config:default", "action": "none" },
    { "resource": "embedding_set:default", "action": "none" },
    { "resource": "skos_concept_scheme:default", "action": "none" }
  ],
  "tenant_claim": { "name": "fortemi:tenant_id", "value": "e414b99d-9d42-5b13-98bb-d63935ce064a" }
}
```

`action` is `create`, `update` or `none`. Exit status is non-zero on any
refusal or error.

### Helm hook example

Run bootstrap as a post-install/post-upgrade Job with the migration secret.
Adapt names to your chart; this is an example, not part of a published chart.

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: fortemi-tenant-bootstrap-acme
  annotations:
    helm.sh/hook: post-install,post-upgrade
    helm.sh/hook-weight: "10"
    helm.sh/hook-delete-policy: before-hook-creation,hook-succeeded
spec:
  backoffLimit: 3
  template:
    spec:
      restartPolicy: OnFailure
      securityContext:
        runAsNonRoot: true
      containers:
        - name: bootstrap
          image: ghcr.io/fortemi/fortemi@sha256:<IMAGE_DIGEST>
          command: ["/app/matric-api", "admin", "bootstrap", "--json"]
          env:
            - name: FORTEMI_BOOTSTRAP_TENANT_SLUG
              value: acme
            - name: FORTEMI_BOOTSTRAP_TENANT_DISPLAY_NAME
              value: Acme Research
            - name: MIGRATION_DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: fortemi-migration-db
                  key: url
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop: ["ALL"]
```

Re-running the hook on every upgrade is safe; it reports `unchanged`.

## Verification

1. Re-run the command; it must be a no-op:

   ```bash
   MIGRATION_DATABASE_URL="$MIGRATION_DATABASE_URL" \
     /app/matric-api admin bootstrap --slug acme --display-name "Acme Research" --json | jq -r .changed
   ```

   Expected output:

   ```text
   false
   ```

2. Send an authenticated request with a token for the tenant:

   ```bash
   curl -s -o /dev/null -w "%{http_code}\n" -H "Authorization: Bearer $TOKEN" \
     https://fortemi.example.com/api/v1/notes
   ```

   Expected output: `200`. A token whose tenant claim names an unprovisioned,
   suspended or soft-deleted tenant still returns `403`.

## Troubleshooting

- **`admin bootstrap requires MIGRATION_DATABASE_URL`**: set the migration-role
  URL; the runtime `DATABASE_URL` is never used.
- **`could not connect with MIGRATION_DATABASE_URL`**: check network policy,
  credentials and TLS parameters for the migration role.
- **`the slug is already registered to a different tenant id`**: another tenant
  owns the slug. Pick a different slug, or pass that tenant's `--tenant-id`.
- **`the tenant id is registered under a different slug`**: pass the original
  slug; slugs are not renamed by bootstrap.
- **`the tenant is suspended; pass --reactivate`**: confirm the suspension is
  meant to end, then add `--reactivate`.
- **`the tenant is soft-deleted`**: bootstrap does not revive deleted tenants;
  follow the lifecycle purge/restore procedure.
- **`database error (SQLSTATE 42P01)`**: migrations have not been applied; run
  without `--skip-migrations` (and without `--dry-run`).

## House rules for agents

- DO run `--dry-run` first and show the planned actions.
- DO use the migration role and keep URLs in secrets; never echo them.
- DON'T insert into `tenant_registry` or seed tenant rows with SQL.
- DON'T pass `--reactivate` unless the operator asked to end a suspension.
- DON'T change the slug namespace; derived ids would change.

## What NOT to fix

- The tenant's default memory is `public`; bootstrap intentionally creates no
  archive schema. Additional named memories need tenant-qualified archive
  provisioning, which the archive repository does not yet provide under the
  `BYPASSRLS` migration role, and archive names are currently unique across the
  whole deployment.
- Document types are not seeded; hosted document-type inference works without
  them, and tenant-specific types are created through the API.
- The tenant's default SKOS scheme has no URI because scheme URIs are unique
  deployment-wide.

## Audit trail

| Date | Author | Change | Last verified | Hosts |
|------|--------|--------|---------------|-------|
| 2026-10-08 | Fortemi maintainers | Initial runbook (#1159) | 2026-10-08 against a disposable PostgreSQL 18 test database | hosted deployments |

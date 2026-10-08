# Kubernetes deployment with Helm

The chart at `deploy/helm/fortemi/` runs Fortémi as three separately scalable
Deployments on a PostgreSQL 18 server and Redis that you operate (managed
service, operator, or self-hosted). It does not deploy PostgreSQL, Redis,
extraction sidecars, or inference backends, and it never creates Secret values.

For a single-host install, the Docker bundle (`docker-compose.bundle.yml`) is
still the simpler option.

## What runs where

| Unit | Image | Process | Scaling |
|---|---|---|---|
| API | server image (`Dockerfile`) | `/app/matric-api` with `WORKER_ENABLED=false` | `api.replicaCount` or `api.autoscaling` |
| Worker | server image | `/app/matric-api` with `WORKER_ENABLED=true` | `worker.replicaCount` or `worker.autoscaling`, independent of the API |
| MCP | MCP image (`Dockerfile.mcp`) | `node index.js`, HTTP transport | `mcp.replicaCount` or `mcp.autoscaling` |
| Migrations | server image | `/app/matric-api --migrate-only` | Helm `pre-install,pre-upgrade` hook Job |

The API and worker use one image. The worker role is the same binary with the
job worker enabled; it still listens on its HTTP port so the kubelet can probe
`/livez` and `/readyz`, but no Service or Ingress routes to worker pods.

`--migrate-only` applies the SQLx migrations and exits. In hosted mode
(`FORTEMI_MULTI_TENANT=true`) it uses `MIGRATION_DATABASE_URL` and refuses to
run when that URL is missing or equal to `DATABASE_URL`, the same rule API
startup enforces. In Community Edition it uses `MIGRATION_DATABASE_URL` when set
and `DATABASE_URL` otherwise.

### Published images

Signed release tags publish these images to `ghcr.io` (public) and
`git.integrolabs.net` (internal). Each tag is a `linux/amd64` + `linux/arm64`
index, so arm64 nodes (for example AWS Graviton with Bottlerocket) pull the
native image. GHCR receives the internal indexes by digest, so both registries
serve the same index and per-platform digests.

| Profile | Image | Tags | Chart value / Kustomize image |
|---|---|---|---|
| API, worker, migrations (Community Edition) | `ghcr.io/fortemi/fortemi` | `<version>`, `latest` | `image.*` / `fortemi/server` |
| API, worker, migrations (hosted single-tenant, `FORTEMI_MULTI_TENANT=true`) | `ghcr.io/fortemi/fortemi` | `<version>-hosted`, `latest-hosted` | `image.*` / `fortemi/server` |
| MCP server | `ghcr.io/fortemi/fortemi-mcp` | `<version>`, `latest` | `mcp.image.*` / `fortemi/mcp` |
| All-in-one Docker bundle (not used by the chart) | `ghcr.io/fortemi/fortemi` | `bundle-<version>`, `bundle-latest` | — |

Pin the index digest (`image.digest`, or `digest:` in `images:`); the runtime
selects the platform member. The per-platform digests are in the release's
`container-release-evidence-*` artifact, and every digest is signed with SBOM
and SLSA provenance attestations ([image promotion](image-promotion.md)).
To build your own images, both Dockerfiles cross-compile for the target
platform:

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -t registry.example.com/fortemi/fortemi:2026.9.11 --push .
docker buildx build --platform linux/amd64,linux/arm64 -f Dockerfile.mcp \
  -t registry.example.com/fortemi/fortemi-mcp:2026.9.11 --push .
```

Hosted mode also needs a KMS backend compiled in: startup fails closed when
`FORTEMI_MULTI_TENANT=true` and no key provider is available. The `-hosted`
image is the same `Dockerfile` built with
`FORTEMI_API_FEATURES=hosted-auth,otel,kms-aws,kms-vault`, so one image serves
`FORTEMI_KEY_PROVIDER=aws-kms` and `vault-transit`. The plain `<version>` image
builds `hosted-auth,otel` only. Release CI starts the published `-hosted`
image for both platforms in hosted mode against a disposable OpenBao Transit
server and requires `/readyz` to report `key_provider` ready
(`scripts/ci/smoke-hosted-image.sh`). To build it yourself, add
`--build-arg FORTEMI_API_FEATURES=hosted-auth,otel,kms-aws,kms-vault` to the
build above.

## Prerequisites

- Kubernetes 1.27 or later, Helm 3.
- PostgreSQL 18 meeting [managed PostgreSQL compatibility](managed-postgres-compatibility.md),
  with `vector` and `postgis` created before the first install.
- For hosted mode, distinct migration and runtime roles as described in
  [hosted PostgreSQL roles](hosted-postgresql-role.md).
- Redis reachable from the namespace when you enable the cache or hosted quota.

## Secrets

Create Secrets before installing. Values only name the Secret and key.

| Secret (default name) | Key | Used for | Values setting |
|---|---|---|---|
| `fortemi-database` | `DATABASE_URL` | Runtime connection | `database.existingSecret`, `database.urlKey` |
| *(optional)* | `MIGRATION_DATABASE_URL` | Migration role; required for hosted | `database.migration.existingSecret` |
| *(optional)* | `REDIS_URL` | Cache (`REDIS_ENABLED=true`) | `redis.enabled`, `redis.existingSecret` |
| *(optional)* | `FORTEMI_QUOTA_REDIS_URL` | Hosted shared admission; required for hosted | `redis.quota.existingSecret` |
| *(optional)* | `MCP_CLIENT_ID`, `MCP_CLIENT_SECRET` | MCP token introspection | `mcp.oauthClient.existingSecret` |

```bash
kubectl create namespace fortemi
kubectl -n fortemi create secret generic fortemi-database \
  --from-literal=DATABASE_URL="$FORTEMI_DATABASE_URL"
```

Load the URL from your secret manager into the environment variable first; do
not type credentials on the command line or commit them to values files. The
External Secrets Operator or the Secrets Store CSI driver can create the same
Secrets instead.

Unlike the Docker bundle, the chart does not auto-register an MCP OAuth client.
Register one once (`POST /oauth/register` with
`{"grant_types":["client_credentials"],"scope":"mcp read write"}`), store the
returned credentials in a Secret, and set `mcp.oauthClient.existingSecret`.

## Install

```bash
helm upgrade --install fortemi deploy/helm/fortemi \
  --namespace fortemi \
  --set image.tag=2026.9.11 \
  --set env.ISSUER_URL=https://memory.example.com \
  --set mcp.baseUrl=https://memory.example.com/mcp \
  --set ingress.enabled=true \
  --set ingress.className=nginx \
  --set ingress.host=memory.example.com \
  --wait
```

Plain configuration goes under `env:` using the variable names in
[configuration](../content/configuration.md). Secret-backed variables go under
`extraEnv:` with `valueFrom.secretKeyRef`.

### Hosted single-tenant profile

`values-hosted-single-tenant.yaml` configures one tenant per release:
`FORTEMI_MULTI_TENANT=true`, external OIDC (`ISSUER_URL`,
`FORTEMI_AUTH_AUDIENCE`, `FORTEMI_AUTH_TENANT_CLAIM`), `FORTEMI_KEY_PROVIDER`,
the shared quota Redis, and `FORTEMI_ATTACHMENTS_ENABLED=false` for deployments
that ingest only notes and facts. Hosted mode always uses the durable
PostgreSQL audit sink ([operations](../ops/postgresql-audit-sink.md)); there is
no separate setting.

```bash
helm upgrade --install fortemi deploy/helm/fortemi -n fortemi \
  -f deploy/helm/fortemi/values-hosted-single-tenant.yaml \
  --set image.repository=registry.example.com/fortemi/fortemi \
  --set image.digest=sha256:<server-image-digest> \
  --set env.ISSUER_URL=https://memory.customer.example \
  --set env.FORTEMI_AUTH_AUDIENCE=fortemi-api
```

The chart refuses to render hosted values without `env.ISSUER_URL`,
`database.migration.existingSecret`, or `redis.quota.existingSecret`.

- AWS KMS: add `FORTEMI_AWS_KMS_KEY_ID` through `extraEnv`.
- OpenBao Transit: set `FORTEMI_KEY_PROVIDER=vault-transit` and the
  `FORTEMI_VAULT_*` settings from [OpenBao KMS](../content/openbao-kms.md).
  Mount the token file's parent directory read-only with `extraVolumes` and
  `extraVolumeMounts`, not as a single-file mount, so token rotation is visible.
- Private OIDC CA: create a ConfigMap with the PEM bundle and set
  `authCaBundle.configMap`. The chart mounts it read-only and sets
  `FORTEMI_AUTH_CA_BUNDLE`.

With attachments enabled, set `MATRIC_ATTACHMENT_SCAN_MODE=required` (hosted
needs a clamd scanner, `MATRIC_ATTACHMENT_CLAMD_ADDR`) and a ReadWriteMany
claim in `fileStorage.existingClaim`. API and worker pods must share it; the
default `emptyDir` is per pod.

## Migration Job behavior

On every `helm install` and `helm upgrade`, Helm runs
`<release>-migrate` before it changes the Deployments:

1. A hook-scoped ServiceAccount (`<release>-migrate`) is created first, because
   pre-install hooks run before the chart's regular resources exist. Its
   annotations default to `serviceAccount.annotations`.
2. The Job runs `/app/matric-api --migrate-only` with the migration
   connection and exits.
3. If the Job fails (`migrations.backoffLimit`, default 1 retry), the install
   or upgrade fails and the previous ReplicaSets keep serving.
4. On success the Deployments roll. The API rolls one surge pod at a time with
   `maxUnavailable: 0`, gated on `/readyz`.

API and worker startup still run the same migration step. After the hook it
finds nothing to apply. Hosted startup therefore still needs
`MIGRATION_DATABASE_URL` in API and worker pods; the chart provides it while
`database.migration.exposeToRuntime` is `true`.

Jobs are kept for `migrations.ttlSecondsAfterFinished` (one day) for log
inspection:

```bash
kubectl -n fortemi logs job/fortemi-migrate
```

Set `migrations.enabled=false` only if migrations run elsewhere.

## Probes

| Container | Startup | Liveness | Readiness |
|---|---|---|---|
| API, worker | `GET /livez` (5 s period, 60 failures) | `GET /livez` | `GET /readyz` |
| MCP | — | `GET /health` | `GET /health` |

`/readyz` checks PostgreSQL; in hosted mode it also checks the audit sink,
the quota Redis and cached key-provider health. A throttled key provider
stays ready; a disabled, deleted or denied current key fails readiness (a missing historical
key version on unseal or rewrap does not) until a later
operation or the health canary (`FORTEMI_KMS_HEALTH_CANARY_SECS`) succeeds.

## Scaling the worker independently

The worker has its own replica count, HorizontalPodAutoscaler, and
PodDisruptionBudget:

```bash
kubectl -n fortemi scale deployment/fortemi-worker --replicas=3
# or, persistently:
helm upgrade fortemi deploy/helm/fortemi -n fortemi --reuse-values \
  --set worker.autoscaling.enabled=true --set worker.autoscaling.maxReplicas=6
```

Workers claim jobs from PostgreSQL, so more replicas raise job throughput
without changing the API. `worker.terminationGracePeriodSeconds` (120 s)
gives running jobs time to finish on scale-down. HPAs need metrics-server.

## Routing

- Ingress: one Ingress sends `/` to the API, a second sends `/mcp` to MCP. The
  MCP server serves at `/` internally, so the proxy must strip `/mcp`. Put the
  controller-specific rewrite in `ingress.mcpAnnotations` (for ingress-nginx,
  `rewrite-target` with a regex `ingress.mcpPath`).
- Gateway API: `httpRoute.enabled=true` with `httpRoute.parentRefs` renders an
  `HTTPRoute` that rewrites `/mcp` to `/` with `ReplacePrefixMatch`.

Set `mcp.baseUrl` to the public MCP URL so OAuth protected-resource metadata is
correct.

## Workload identity

`serviceAccount.annotations` applies to the API, worker, and MCP
ServiceAccount, and by default to the migration ServiceAccount.

- IRSA: `eks.amazonaws.com/role-arn: arn:aws:iam::<account>:role/<role>`.
- EKS Pod Identity: create the association for
  `<namespace>/<release>` (and `<release>-migrate` if the migration needs AWS
  access); no annotation is needed.

Service account tokens are not automounted (`automountServiceAccountToken:
false`). Projected tokens used by IRSA and Pod Identity are injected by their
webhooks regardless.

## NetworkPolicy

`networkPolicy.enabled=true` renders an example:

- API: ingress from the namespace matching
  `networkPolicy.ingressControllerNamespaceSelector` and from MCP pods.
- MCP: ingress from the ingress controller namespace.
- Worker and migration pods: no ingress.
- Egress: unrestricted unless `networkPolicy.egress` lists rules. When set,
  DNS is allowed automatically, and MCP may reach the API. Add PostgreSQL,
  Redis, OIDC, KMS, and inference endpoints yourself.

## Upgrades and rollback

`helm upgrade` runs the migration Job, then rolls the Deployments. To roll
back the workloads:

```bash
helm -n fortemi rollback fortemi <revision>
```

Rollback restores the previous images and configuration. It does not run the
migration hook and does not reverse migrations. Fortémi migrations are
forward-only, and older binaries are not guaranteed to work against a newer
schema. Before upgrades that include migrations, take a database snapshot. If a
release must be abandoned after its migrations ran, restore the snapshot as
described in [hosted PostgreSQL roles](hosted-postgresql-role.md) rather than
pointing an old release at the new schema.

## Kustomize for GitOps platforms

On platforms where Argo CD reconciles plain manifests rendered by Kustomize,
use `deploy/kustomize/` instead of the chart. It has the same API, worker,
MCP and migration split and the same object names as a release named
`fortemi`, including the immutable Deployment selectors.

| Path | Contents |
|---|---|
| `base/` | Deployments, Services, ServiceAccounts, PodDisruptionBudgets, migration Job, default-deny NetworkPolicies, ConfigMaps for non-secret env |
| `components/` | `hosted-single-tenant`, `mcp-oauth-client`, `autoscaling`, `attachments-disabled`, `otel`, `without-mcp` |
| `examples/single-tenant/` | Overlay equal to `values-hosted-single-tenant.yaml` in namespace `fortemi` |

Reference the base and components from your overlay pinned to a commit, and
pin images by digest in `images:`. The base sets no namespace and names
images without a version (`fortemi/server:pinned`, `fortemi/mcp:pinned`), so
`images:` is the only place a version lives and the only field an image
updater has to write:

```yaml
resources:
  - https://git.integrolabs.net/Fortemi/fortemi//deploy/kustomize/base?ref=<commit-sha>
components:
  - https://git.integrolabs.net/Fortemi/fortemi//deploy/kustomize/components/hosted-single-tenant?ref=<commit-sha>
images:
  - name: fortemi/server
    newName: registry.example.com/fortemi/fortemi
    digest: sha256:<server-image-digest>
```

Add or override non-secret settings with a `configMapGenerator` entry for
`fortemi-server-env` (or `fortemi-mcp-env`) using `behavior: merge`. The
ConfigMaps keep their content-hash suffix, so a config change rolls the pods.
Secrets are referenced by name only, as in the [Secrets](#secrets) table. The
hosted component adds `fortemi-database-migrate` and `fortemi-quota-redis`,
and `mcp-oauth-client` adds `fortemi-mcp-oauth`. Create them with External
Secrets at a sync wave earlier than 1.

Differences from the chart:

- The migration Job is an Argo CD `Sync` hook with `BeforeHookCreation` at
  sync wave 1. Each sync runs it once before the Deployments at wave 2. A
  `PreSync` hook would run before the ExternalSecrets that create its
  credentials.
- Pods run with `readOnlyRootFilesystem: true` and an `emptyDir` at `/tmp`.
  The server image passed boot, migrations and `/readyz` with a read-only
  root in the API and worker roles. The extraction tools were not exercised.
  If one needs another writable path, mount an `emptyDir` there.
- NetworkPolicy is default-deny with one allow per decision. Overlays patch
  the ingress controller namespace label and add `to:` blocks to
  `fortemi-server-egress`.

## Validation

CI renders and validates the chart for the default and hosted values:

```bash
bash scripts/ci/lint-helm-chart.sh deploy/helm/fortemi
```

The script runs `helm lint --strict`, `helm template`, and `kubeconform`, and
checks that the chart version equals the workspace version.

`scripts/ci/kustomize-helm-parity.sh` builds every kustomization and validates
it with `kubeconform`. It then compares the base, the base without MCP and the
example overlay with the matching `helm template` render. The comparison covers
object kinds and names, containers, commands, effective env var names, ports
and probes, and the CI job fails on any difference.

## Chart version

`version` and `appVersion` in `Chart.yaml` equal the workspace CalVer in
`Cargo.toml` (`YYYY.M.PATCH`, no leading zeros) and change with each release.

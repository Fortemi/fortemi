# Hosted route qualification: single-tenant dedicated

## Purpose

This page states which API routes a **single-tenant dedicated** hosted
deployment can rely on, which are excluded, and which are gaps for a
graph/knowledge workload. It is not multi-tenant launch readiness:
`deployment.hosted_multi_tenant_ready` stays `false` and the suite audit
remains NO-GO.

The matrix below is generated from the route inventory in
`crates/matric-api/src/route_policy.rs`. Each registered router operation is
classified by `hosted_tenant_transaction_ready`, the same gate
`auth_middleware` applies in hosted mode. The unit test
`route_policy::tests::hosted_route_qualification_matrix_doc_is_current` fails
when the gate, the inventory, or the router changes without this page.

Warnings:

- In hosted mode an unqualified bearer route returns `503` with
  `This hosted route has not completed tenant transaction migration.` Do not
  build a dedicated-deployment client on any route the matrix does not mark
  `qualified`.
- Auth-exempt routes (`enforcement: auth_exempt`) never receive a tenant
  binding. Only the `public` rows among them are safe protocol, probe or
  inline-proof surfaces.
- `GET /api/v1/ws`, `POST /api/v1/ingest/stream` and the `knowledge_health`
  diagnostics are auth-exempt only in community mode. Hosted mode reports them as `hosted_503`: `401` without a
  valid bearer, `503` with one, regardless of `REQUIRE_AUTH`.

## System topology

### Profile definition

A single-tenant dedicated deployment is one hosted `matric-api` process (or
replica set) serving exactly one pre-provisioned tenant:

| Prerequisite | Applies | Notes |
|---|---|---|
| Binary built with `hosted-auth` and `kms-vault` or `kms-aws` | yes | Standard Dockerfiles include `hosted-auth`. |
| `FORTEMI_MULTI_TENANT=true`, `REQUIRE_AUTH=true` | yes | The hosted gate and tenant transaction run only in this mode. |
| External OIDC (`ISSUER_URL`, `FORTEMI_AUTH_AUDIENCE`, `FORTEMI_AUTH_TENANT_CLAIM`) | yes | See [authentication](../content/authentication.md#internal-hosted-oidc-profile). |
| One tenant provisioned with `matric-api admin bootstrap` | yes | The claim value is the printed tenant id. See [hosted-bootstrap.md](hosted-bootstrap.md). |
| Distinct `MIGRATION_DATABASE_URL` and non-owner runtime `DATABASE_URL` | yes | See [hosted-postgresql-role.md](hosted-postgresql-role.md). |
| KMS startup canary (`FORTEMI_KEY_PROVIDER`) | yes | Fails closed at startup. |
| Durable PostgreSQL audit sink | yes | Authorization decisions fail closed with `503` when the audit write fails. |
| Redis admission (`FORTEMI_QUOTA_REDIS_URL`) | yes | Startup `PING` fails closed. |
| Attachment scanner and shared attachment storage | relaxed | Set `FORTEMI_ATTACHMENTS_ENABLED=false`; the `attachments` class is excluded on this profile. |
| Outbound inference destination policy | only when `inference` routes are used | The `inference` class is outside the graph/knowledge workload. |
| Hosted job worker for embeddings/NLP | relaxed | `jobs_embeddings` is a gap; create notes with `"pipeline": []` for store-only qualification. |

A dedicated tenant still runs under row security. Nothing in this profile
relaxes tenant binding; it only narrows the routes a client may depend on.

### Status vocabulary

| Field | Value | Meaning |
|---|---|---|
| `status` | `qualified` | Admitted by the hosted gate; runs on the verified tenant's transaction-bound connection. |
| `status` | `gap` | Needed by the graph/knowledge workload but not tenant-bound in hosted mode. Tracked below. |
| `status` | `excluded` | Not offered to tenants of a dedicated hosted deployment. |
| `status` | `public` | Protocol, probe or inline-proof route carrying no tenant data. |
| `enforcement` | `tenant_transaction` / `hosted_503` / `auth_exempt` | What the hosted request path does. `hosted_503` answers `401` when no valid bearer is presented. |
| class status | `qualified` / `partial` / `gap` / `excluded` / `public` | Roll-up of the class rows; `partial` mixes qualified and unready rows. |

Workload-required classes are `search`, `notes`, `links_graph`,
`archives_memories`, `export`, `jobs_embeddings`, `realtime_mcp` and
`collections`. MCP tools call these REST routes over the `realtime_mcp`
transport, so an MCP tool is qualified only when every REST operation it calls
is qualified.

## Route matrix

<!-- BEGIN GENERATED hosted-route-qualification -->

| Route class | Workload required | Class status | Qualified | Gap | Excluded | Public |
|---|---|---|---:|---:|---:|---:|
| `search` | yes | partial | 4 | 2 | 0 | 0 |
| `notes` | yes | partial | 13 | 12 | 0 | 0 |
| `links_graph` | yes | partial | 1 | 14 | 0 | 0 |
| `archives_memories` | yes | partial | 1 | 18 | 0 | 0 |
| `export` | yes | partial | 2 | 9 | 0 | 0 |
| `jobs_embeddings` | yes | gap | 0 | 31 | 0 | 0 |
| `realtime_mcp` | yes | partial | 1 | 2 | 0 | 0 |
| `collections` | yes | partial | 6 | 1 | 0 | 0 |
| `taxonomy` | no | excluded | 0 | 0 | 38 | 0 |
| `inference` | no | partial | 5 | 0 | 7 | 0 |
| `attachments` | no | excluded | 0 | 0 | 17 | 0 |
| `provenance` | no | excluded | 0 | 0 | 7 | 0 |
| `templates` | no | excluded | 0 | 0 | 6 | 0 |
| `account` | no | partial | 4 | 0 | 7 | 0 |
| `voice_calls` | no | excluded | 0 | 0 | 1 | 1 |
| `knowledge_health` | no | excluded | 0 | 0 | 6 | 0 |
| `operator` | no | excluded | 0 | 0 | 55 | 2 |
| `public_protocol` | no | public | 0 | 0 | 0 | 18 |
| **total** | | | 37 | 89 | 144 | 21 |

```json
{
  "profile": "single_tenant_dedicated",
  "operations": [
    {"method":"GET","path":"/api/v1/entities/similar","class":"search","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/memories/search","class":"search","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/similar","class":"search","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/search","class":"search","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/search/evidence/resolve","class":"search","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/search/federated","class":"search","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/lifecycle-purge","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/lifecycle-purge/preview","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/lifecycle-purge/{operation_id}","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/lifecycle-purge/{operation_id}/resume","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/notes","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/activity","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/bulk","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/source-upsert","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/timeline","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/notes/{id}","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/{id}","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"PATCH","path":"/api/v1/notes/{id}","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/full","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/purge","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/restore","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/notes/{id}/status","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/{id}/tags","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"PUT","path":"/api/v1/notes/{id}/tags","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/{id}/versions","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/versions/diff","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/notes/{id}/versions/{version}","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/versions/{version}","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/versions/{version}/restore","class":"notes","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/tags","class":"notes","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/graph/cold-spots","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/graph/community/coarse","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/graph/diagnostics","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/graph/diagnostics/compare","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/graph/diagnostics/history","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/graph/diagnostics/snapshot","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/graph/maintenance","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/graph/pfnet/sparsify","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/graph/snn/recompute","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/graph/topology/stats","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/graph/{id}","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/backlinks","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/links","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/links","class":"links_graph","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/{id}/related","class":"links_graph","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/archives","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/archives","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/archives/{name}","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/archives/{name}","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/archives/{name}","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/archives/{name}/clone","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/archives/{name}/set-default","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/archives/{name}/stats","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/memories","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/memories","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/memories/overview","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/memories/{name}","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/memories/{name}","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/memories/{name}","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/memories/{name}/clone","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/memories/{name}/set-default","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/memories/{name}/stats","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/memory/context","class":"archives_memories","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/memory/info","class":"archives_memories","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/export","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/import","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/knowledge-archive","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/knowledge-archive/{filename}","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/knowledge-shard","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/knowledge-shard/import","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/knowledge-shard/upload","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/memory/{name}","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/collections/{id}/export","class":"export","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/memory/export","class":"export","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/notes/{id}/export","class":"export","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-configs","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/embedding-configs","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-configs/default","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/embedding-configs/{id}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-configs/{id}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/embedding-configs/{id}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-sets","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/embedding-sets","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/embedding-sets/{slug}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-sets/{slug}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/embedding-sets/{slug}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/embedding-sets/{slug}/build-index","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-sets/{slug}/members","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/embedding-sets/{slug}/members","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/embedding-sets/{slug}/members/{note_id}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/embedding-sets/{slug}/refresh","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-sets/{slug}/runs","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/embedding-sets/{slug}/runs","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/embedding-sets/{slug}/runs/{run_id}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/jobs","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/jobs","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/jobs/pause","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/jobs/pause/{archive}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/jobs/pending","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/jobs/resume","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/jobs/resume/{archive}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/jobs/stats","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/jobs/status","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/jobs/{id}","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/reprocess","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/reprocess","class":"jobs_embeddings","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/events","class":"realtime_mcp","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/ingest/stream","class":"realtime_mcp","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/ws","class":"realtime_mcp","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/collections","class":"collections","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/collections","class":"collections","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"DELETE","path":"/api/v1/collections/{id}","class":"collections","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/collections/{id}","class":"collections","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"PATCH","path":"/api/v1/collections/{id}","class":"collections","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/collections/{id}/notes","class":"collections","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/notes/{id}/move","class":"collections","status":"gap","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/autocomplete","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/collections","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts/collections","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/collections/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/collections/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/concepts/collections/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"PUT","path":"/api/v1/concepts/collections/{id}/members","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/collections/{id}/members/{concept_id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts/collections/{id}/members/{concept_id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/governance","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/schemes","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts/schemes","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/schemes/export/turtle","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/schemes/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/schemes/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/concepts/schemes/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/schemes/{id}/export/turtle","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/schemes/{id}/top-concepts","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/concepts/{id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}/ancestors","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}/broader","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts/{id}/broader","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/{id}/broader/{target_id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}/descendants","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}/full","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}/narrower","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts/{id}/narrower","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/{id}/narrower/{target_id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/concepts/{id}/related","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/concepts/{id}/related","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/concepts/{id}/related/{target_id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/concepts","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/concepts","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/notes/{id}/concepts/{concept_id}","class":"taxonomy","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/audio/transcribe","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/chat","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/chat/models","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/chat/stream","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/document-types/detect","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/inference/catalog","class":"inference","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/inference/complete","class":"inference","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/inference/embed","class":"inference","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/inference/providers","class":"inference","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/inference/stream","class":"inference","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/models","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/vision/describe","class":"inference","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/attachments/{attachment_id}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments/{attachment_id}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments/{attachment_id}/download","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments/{attachment_id}/sprites/{sprite_index}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments/{attachment_id}/subtitles","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments/{attachment_id}/thumbnail","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/attachments/{attachment_id}/thumbnails.vtt","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/attachments","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/attachments","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"OPTIONS","path":"/api/v1/notes/{id}/attachments/tus","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/attachments/tus","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/notes/{id}/attachments/tus/{upload_id}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/attachments/tus/{upload_id}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"HEAD","path":"/api/v1/notes/{id}/attachments/tus/{upload_id}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/notes/{id}/attachments/tus/{upload_id}","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/notes/{id}/attachments/upload","class":"attachments","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/memory-provenance","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/notes/{id}/provenance","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/provenance/devices","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/provenance/files","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/provenance/locations","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/provenance/named-locations","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/provenance/notes","class":"provenance","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/templates","class":"templates","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/templates","class":"templates","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/templates/{id}","class":"templates","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/templates/{id}","class":"templates","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/templates/{id}","class":"templates","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/templates/{id}/instantiate","class":"templates","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/auth/token-info","class":"account","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/pke/address","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/pke/decrypt","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/pke/encrypt","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/pke/keygen","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/pke/recipients","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/pke/verify/{address}","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/rate-limit/status","class":"account","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/user/secrets","class":"account","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"POST","path":"/api/v1/user/secrets","class":"account","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"DELETE","path":"/api/v1/user/secrets/{id}","class":"account","status":"qualified","enforcement":"tenant_transaction"},
    {"method":"GET","path":"/api/v1/calls/{id}","class":"voice_calls","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/realtime/twilio/{provider_call_id}","class":"voice_calls","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/api/v1/health/access-frequency","class":"knowledge_health","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/health/knowledge","class":"knowledge_health","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/health/orphan-tags","class":"knowledge_health","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/health/stale-notes","class":"knowledge_health","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/health/tag-cooccurrence","class":"knowledge_health","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/health/unlinked-notes","class":"knowledge_health","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/api-keys","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/api-keys","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/api-keys/{id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/database","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/database/restore","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/database/snapshot","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/database/upload","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/download","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/list","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/list/{filename}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/metadata/{filename}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"PUT","path":"/api/v1/backup/metadata/{filename}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/backup/status","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/swap","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/backup/trigger","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/document-types","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/document-types","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/document-types/{name}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/document-types/{name}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/document-types/{name}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/events/tokens","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/events/tokens/{token_id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/extraction/stats","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/inbound-sources","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/inbound-sources","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/inbound-sources/{name}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/inference/config","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/inference/config","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/inference/config","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/inference/config/audit","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/inference/test-connection","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/ingest/tokens","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/ingest/tokens/{token_id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/operator/asyncapi.yaml","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/operator/docs","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/operator/openapi.yaml","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/pke/keysets","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/pke/keysets","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/pke/keysets/active","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/pke/keysets/import","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"DELETE","path":"/api/v1/pke/keysets/{name_or_id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"PUT","path":"/api/v1/pke/keysets/{name_or_id}/active","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/pke/keysets/{name_or_id}/export","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/webhooks","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/webhooks","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/webhooks/incoming","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/webhooks/incoming","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/webhooks/incoming/validate","class":"operator","status":"public","enforcement":"auth_exempt"},
    {"method":"DELETE","path":"/api/v1/webhooks/incoming/{slug}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/webhooks/incoming/{slug}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/webhooks/incoming/{slug}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/webhooks/incoming/{slug}","class":"operator","status":"public","enforcement":"auth_exempt"},
    {"method":"DELETE","path":"/api/v1/webhooks/{id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/webhooks/{id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"PATCH","path":"/api/v1/webhooks/{id}","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/api/v1/webhooks/{id}/deliveries","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"POST","path":"/api/v1/webhooks/{id}/test","class":"operator","status":"excluded","enforcement":"hosted_503"},
    {"method":"GET","path":"/.well-known/oauth-authorization-server","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/.well-known/oauth-protected-resource","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/api/v1/health/streaming","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/api/v1/problem-contract-status/{status}","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/api/v1/problem-contract-test","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/api/v1/system/compatibility","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/health","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"POST","path":"/health","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/health/live","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/livez","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/oauth/authorize","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"POST","path":"/oauth/authorize","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"POST","path":"/oauth/introspect","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"POST","path":"/oauth/register","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"POST","path":"/oauth/revoke","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"POST","path":"/oauth/token","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/readyz","class":"public_protocol","status":"public","enforcement":"auth_exempt"},
    {"method":"GET","path":"/recording.wav","class":"public_protocol","status":"public","enforcement":"auth_exempt"}
  ]
}
```

<!-- END GENERATED hosted-route-qualification -->

## Evidence per qualified route class

All listed tests run in `.gitea/workflows/test.yml` against the disposable CI
database: the `--features hosted-auth,kms-vault` fast-lane commands
(`hosted_create_note_tests` with `FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL`,
`scoped_search_tests` with `DATABASE_URL`, and the named hosted unit tests) and
the `cargo test --workspace --tests` integration lane (manual note links, user
secret storage, and the matrix check itself).

| Class | Qualified operations | Hosted-mode evidence |
|---|---|---|
| `search` | `GET /api/v1/search` (text `fts`, `semantic`, `hybrid`), `POST /api/v1/search/evidence/resolve` | `scoped_search_tests::hosted_search_preserves_authorization_archive_and_candidate_scope` (real `auth_middleware`, fixture OIDC identities, two tenants, archive scope) |
| `notes` | create, list, get, delete, status, tags, source upsert, lifecycle purge | `hosted_create_note_tests::hosted_creation_postgres_atomicity_and_tenant_isolation`; `hosted_create_note_tests::bootstrap_request::hosted_creation_succeeds_for_bootstrapped_tenant_without_manual_sql`; unit `route_policy::tests::lifecycle_purge_operation_is_tenant_scoped_without_note_normalization` |
| `links_graph` | `POST /api/v1/notes/{id}/links` | `tests::hosted_manual_note_link_enforces_scope_and_tenant_transaction_visibility` |
| `archives_memories` | `GET /api/v1/memory/context` | `scoped_search_tests::memory_context::hosted_memory_context_is_tenant_bound_and_read_only` |
| `export` | `GET /api/v1/collections/{id}/export` | `hosted_create_note_tests::collection_export::hosted_collections_and_export_are_tenant_bound_for_bootstrapped_tenant` |
| `collections` | collection list/create/get/update/delete, collection notes | `hosted_create_note_tests::collection_export::hosted_collections_and_export_are_tenant_bound_for_bootstrapped_tenant` |
| `realtime_mcp` | `GET /api/v1/events` (SSE, bearer + `mcp` scope) | `hosted_create_note_tests::hosted_creation_postgres_atomicity_and_tenant_isolation` (replay/live tenant filtering, transaction released before streaming); unit `hosted_sse_filters_foreign_and_unattributed_live_and_replay_events` |
| `inference` | catalog, providers, complete, stream, embed | unit `handlers::inference_complete::tests::hosted_model_policy_is_profile_and_capability_specific`; outside the graph/knowledge workload |
| `account` | token info, user secrets | unit `handlers::token_info::tests::hosted_contexts_report_hosted_class_expiry_and_tenant_binding`; `route_policy::tests::hosted_user_secret_routes_are_hidden_tenant_objects_without_enumeration_lookup`; `tests/user_secret_storage_test.rs` |

`route_policy::tests::hosted_tenant_transaction_gate_admits_only_migrated_methods`
proves representative unready operations are refused by the gate, and the
collection/export test proves an unqualified route (`GET /api/v1/graph/{id}`)
returns `503` on this profile.

## Gaps and tracking

| Class | Gap operations | Tracking |
|---|---|---|
| `search` | `POST /api/v1/search/federated`, `GET /api/v1/memories/search` | Fortemi/fortemi#956 |
| `notes` | `PATCH /api/v1/notes/{id}`, restore, purge, bulk, versions, activity, timeline, full | Fortemi/fortemi#728 (handler migration) |
| `links_graph` | backlinks, `GET /api/v1/notes/{id}/links`, related, `GET /api/v1/graph/{id}` traversal, topology, cold spots, community | proposed: hosted graph traversal reads |
| `links_graph` | graph diagnostics, maintenance, SNN, PFNET | Fortemi/fortemi#955 |
| `archives_memories` | archive/memory administration, stats, overview, `memory/info` | Fortemi/fortemi#956 |
| `export` | Knowledge Shard and Knowledge Archive export/import, backup export/import, note export | Fortemi/fortemi#959 (note export); proposed: tenant-scoped Knowledge Shard export |
| `jobs_embeddings` | job control, embedding sets/configs, reprocess | Fortemi/fortemi#955; proposed: hosted job status and embedding execution |
| `realtime_mcp` | `GET /api/v1/ws`, `POST /api/v1/ingest/stream` (closed in hosted mode: `401`/`503`; no tenant binding yet) | Fortemi/fortemi#1163 |
| `collections` | `POST /api/v1/notes/{id}/move` | proposed: hosted note move. Set membership with `collection_id` on note creation. |

Related exclusions are tracked by Fortemi/fortemi#961 (attachments) and
Fortemi/fortemi#962 (document types). The `knowledge_health` diagnostics
(`/api/v1/health/knowledge`, `orphan-tags`, `stale-notes`, `unlinked-notes`,
`tag-cooccurrence`, `access-frequency`) read tenant knowledge without a tenant
binding, so hosted mode closes them (Fortemi/fortemi#1164): `401` without a
valid bearer, `503` with one. `/health`, `/livez`, `/readyz` and the aggregate
`/api/v1/health/streaming` probe stay public.

## Procedure

1. Provision the tenant and configure the identity provider claim per
   [hosted-bootstrap.md](hosted-bootstrap.md).
2. Start the API with the prerequisites above. To relax attachments:

   ```bash
   FORTEMI_ATTACHMENTS_ENABLED=false
   ```

3. Confirm the deployment reports the qualified profile (see Verification).
4. Restrict clients and MCP tool use to operations marked `qualified`.

## Verification

```bash
curl -s https://fortemi.example.com/api/v1/system/compatibility \
  | jq '.deployment | {hosted_multi_tenant_ready, hosted_profile}'
```

Expected output on a hosted deployment of this revision:

```json
{
  "hosted_multi_tenant_ready": false,
  "hosted_profile": {
    "name": "single_tenant_dedicated",
    "qualified_route_classes": ["search", "notes", "links_graph", "archives_memories", "export", "realtime_mcp", "collections", "inference", "account"]
  }
}
```

`qualified_route_classes` lists classes with at least one qualified operation;
consult the matrix for the exact operations. Community deployments omit
`hosted_profile`.

To check this page against the code:

```bash
cargo test -p matric-api --bin matric-api hosted_route_qualification
```

Expected output ends with `test result: ok.`

## Troubleshooting

- **Symptom**: a client receives `503` "has not completed tenant transaction
  migration" → the operation is not `qualified`; see Gaps and tracking.
- **Symptom**: the matrix test fails after a route or gate change → regenerate
  with `FORTEMI_UPDATE_ROUTE_QUALIFICATION=1 cargo test -p matric-api --bin matric-api hosted_route_qualification_matrix_doc_is_current`,
  review the diff, and update the evidence and gap tables.
- **Symptom**: `403` on a qualified collection route → the collection does not
  exist under the caller's tenant; deleted and foreign collections look alike.

## House rules for agents

- DO regenerate the matrix only through the test; never hand-edit the
  generated region.
- DO add hosted-mode evidence before adding an operation to the gate.
- DON'T describe a `partial` class as qualified without naming its operations.
- DON'T treat this profile as multi-tenant readiness.

## What NOT to fix

- `hosted_multi_tenant_ready` is intentionally `false`.
- Collection names are currently unique deployment-wide; this is harmless for
  one tenant and is tracked as a multi-tenant gap.

## Audit trail

| Date | Change | Applies to |
|---|---|---|
| 2026-10-08 | Initial matrix (Fortemi/fortemi#1154) | hosted `matric-api` builds from this revision |
| 2026-10-08 | Hosted mode closes `/api/v1/ws` and `/api/v1/ingest/stream` (Fortemi/fortemi#1163) | hosted `matric-api` builds from this revision |
| 2026-10-08 | Hosted mode closes knowledge diagnostics; `/api/v1/health/streaming` reclassified `public_protocol` (Fortemi/fortemi#1164) | hosted `matric-api` builds from this revision |

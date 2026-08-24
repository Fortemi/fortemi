---
title: Hosted tenant table inventory
date: 2026-08-24
status: construction evidence
authority: docs/architecture/adr/ADR-090-multi-tenancy-model.md
issues: [726, 727, 728, 729]
---

# Hosted tenant table inventory

This inventory covers live Fortemi PostgreSQL persistence only. It does not
describe the AIWG static index or Knowledge Shard transfer profiles.

The executable authority is `matric_db::tenancy::{TENANT_SCOPED_TABLES,
EXEMPT_PUBLIC_TABLES}`. Startup and CI compare that inventory with `pg_class`,
`pg_attribute`, and `pg_policy`; an unclassified public or `archive_*` table
fails the hosted catalog assertion.

## Tenant-scoped public tables

The following 84 tables require `tenant_id UUID NOT NULL`, a tenant index,
`ENABLE ROW LEVEL SECURITY`, `FORCE ROW LEVEL SECURITY`, and a policy whose
`USING` and `WITH CHECK` expressions use
`current_setting('app.current_tenant')::uuid`.

```text
activity_log api_key archive_inference_override archive_registry attachment
attachment_blob attachment_embedding call_sessions collection community
community_assignment community_set document_type embedding embedding_coarse
embedding_config embedding_set embedding_set_member entity_stats event_outbox
file_upload_audit fine_tuning_dataset fine_tuning_sample graph_diagnostics_history
graph_edge_artifact graph_source inbound_dlq inbound_source
incoming_webhook_receiver inference_config_audit job_attempt job_history job_queue
link model_3d_metadata named_location note note_access_log note_entity
note_graph_embedding note_original note_original_history note_revised_current
note_revision note_share_grant note_skos_concept note_tag note_template
note_token_embeddings oauth_authorization_code oauth_client oauth_token
pke_active_keyset pke_keysets pke_public_keys prov_agent_device prov_location
provenance provenance_activity provenance_edge realtime_media_stream_attempt
skos_audit_log skos_collection skos_collection_member skos_concept
skos_concept_in_scheme skos_concept_label skos_concept_merge skos_concept_note
skos_concept_scheme skos_mapping_relation_edge skos_semantic_relation_edge
structured_media_metadata tag transcript_segments tus_upload
usage_delivery_attempt usage_event_conflict usage_event_delivery usage_event_ledger
user_config user_metadata_label webhook webhook_delivery
```

`20260824010000_tenant_registry_and_forced_rls.sql` performs the legacy local
backfill and adds tenant-qualified foreign-key guards for existing foreign keys.
`20260824010100_tenant_qualified_note_collection.sql` closes the preexisting
unconstrained `note.collection_id` relationship.

## System-scoped and exempt tables

| Table | Classification | Isolation and authorization rationale |
|---|---|---|
| `_sqlx_migrations` | migration-only | Owned and accessed only by the migration role; excluded from application catalog checks. |
| `spatial_ref_sys` | extension/reference | PostGIS-owned public reference data; no customer records and no application writes. |
| `system_config` | system-scoped | Deployment-wide configuration. Mutation is operator-only through the #710 route policy and must be audited. |
| `tenant_registry` | system-scoped | Active-tenant authority queried before request tenant scope exists. Runtime receives read access only. |
| `usage_sink` | system-scoped | Deployment-wide delivery destination registry. Tenant usage events remain in RLS-protected ledgers. Configuration is operator-only. |

`user_secrets` is not present yet. Issue #730 owns its schema and must add it to
the executable tenant inventory in the same migration that creates it.

## Archive schemas

Archive schemas are a live persistence sub-plane, not separate tenants. Every
per-memory table is still tenant-scoped. System-scoped tables are never cloned.

PostgreSQL `CREATE TABLE ... LIKE ... INCLUDING ALL` does not copy RLS policy
state. `PgArchiveRepository` therefore applies forced tenant RLS after create
and synchronization, while migration `20260824010200_archive_schema_forced_rls.sql`
backfills existing archives. Hosted startup inspects all `archive_*` schemas and
fails when a table is missing, unclassified, nullable, not forced, or lacks the
tenant policy.

Archive DDL remains a migration/administrative operation in hosted mode. The
hardened runtime role is intentionally not granted schema-creation privileges.
Issue #728 remains open until archive administration and all ordinary handlers
execute through the request's tenant transaction.

## Evidence and remaining gates

- Clean-destination migrations and public/archive catalog tests pass against
  the versioned PostgreSQL test image.
- Same-pool tenant reuse, missing-GUC failure, privileged-role rejection, and a
  representative cross-tenant foreign-key attempt are executable tests.
- Row-count reconciliation and restore-based rollback must be captured against
  a representative deployment snapshot before #726 closes.
- Search/vector, jobs, backup/export, streaming, and every repository entry
  point still require the complete #728/#729 handler and negative-test matrix.

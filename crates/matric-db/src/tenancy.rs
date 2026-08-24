//! Transaction-scoped tenant binding and executable RLS inventory.
//!
//! The PostgreSQL custom setting is a request input, not ambient authorization
//! state. Hosted callers must obtain a [`TenantScopedConn`] only after canonical
//! authentication and active-tenant validation.

use sqlx::postgres::{PgConnection, PgPool};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{Error, Result};

use crate::archives::SHARED_TABLES;

/// Reserved tenant for community/personal-server data and migration backfills.
pub const LOCAL_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Public tables intentionally outside tenant RLS.
///
/// `_sqlx_migrations` is migration-owned and excluded from application catalog
/// checks separately. `tenant_registry` is the system-scoped authority queried
/// before a request tenant can be established.
pub const EXEMPT_PUBLIC_TABLES: &[&str] = &[
    "spatial_ref_sys",
    "system_config",
    "tenant_registry",
    "usage_sink",
];

/// Complete ADR-090 inventory for the schema at migration 20260824010000.
pub const TENANT_SCOPED_TABLES: &[&str] = &[
    "activity_log",
    "api_key",
    "archive_inference_override",
    "archive_registry",
    "audit_event",
    "attachment",
    "attachment_blob",
    "attachment_embedding",
    "call_sessions",
    "collection",
    "community",
    "community_assignment",
    "community_set",
    "document_type",
    "embedding",
    "embedding_coarse",
    "embedding_config",
    "embedding_set",
    "embedding_set_member",
    "entity_stats",
    "event_outbox",
    "file_upload_audit",
    "fine_tuning_dataset",
    "fine_tuning_sample",
    "graph_diagnostics_history",
    "graph_edge_artifact",
    "graph_source",
    "inbound_dlq",
    "inbound_source",
    "incoming_webhook_receiver",
    "inference_config_audit",
    "job_attempt",
    "job_history",
    "job_queue",
    "link",
    "model_3d_metadata",
    "named_location",
    "note",
    "note_access_log",
    "note_entity",
    "note_graph_embedding",
    "note_original",
    "note_original_history",
    "note_revised_current",
    "note_revision",
    "note_share_grant",
    "note_skos_concept",
    "note_tag",
    "note_template",
    "note_token_embeddings",
    "oauth_authorization_code",
    "oauth_client",
    "oauth_token",
    "pke_active_keyset",
    "pke_keysets",
    "pke_public_keys",
    "prov_agent_device",
    "prov_location",
    "provenance",
    "provenance_activity",
    "provenance_edge",
    "realtime_media_stream_attempt",
    "skos_audit_log",
    "skos_collection",
    "skos_collection_member",
    "skos_concept",
    "skos_concept_in_scheme",
    "skos_concept_label",
    "skos_concept_merge",
    "skos_concept_note",
    "skos_concept_scheme",
    "skos_mapping_relation_edge",
    "skos_semantic_relation_edge",
    "structured_media_metadata",
    "tag",
    "transcript_segments",
    "tus_upload",
    "usage_delivery_attempt",
    "usage_event_conflict",
    "usage_event_delivery",
    "usage_event_ledger",
    "user_config",
    "user_metadata_label",
    "webhook",
    "webhook_delivery",
];

/// A transaction whose first operation established `app.current_tenant`.
///
/// The wrapper deliberately exposes a connection executor, not the source pool.
/// Dropping without `commit` rolls the transaction back through SQLx.
pub struct TenantScopedConn<'pool> {
    transaction: Transaction<'pool, Postgres>,
    tenant_id: Uuid,
}

impl<'pool> TenantScopedConn<'pool> {
    /// Begin a transaction and bind its tenant before any tenant query.
    pub async fn begin(pool: &'pool PgPool, tenant_id: Uuid) -> Result<Self> {
        let mut transaction = pool.begin().await.map_err(Error::Database)?;
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tenant_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(Error::Database)?;

        let bound: String = sqlx::query_scalar("SELECT current_setting('app.current_tenant')")
            .fetch_one(&mut *transaction)
            .await
            .map_err(Error::Database)?;
        if bound != tenant_id.to_string() {
            return Err(Error::Config(
                "database tenant scope did not bind to the verified tenant".to_string(),
            ));
        }

        Ok(Self {
            transaction,
            tenant_id,
        })
    }

    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// Executor for repositories that have explicitly accepted a tenant scope.
    pub fn executor(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }

    pub async fn commit(self) -> Result<()> {
        self.transaction.commit().await.map_err(Error::Database)
    }

    pub async fn rollback(self) -> Result<()> {
        self.transaction.rollback().await.map_err(Error::Database)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantCatalogReport {
    pub missing_tables: Vec<String>,
    pub unclassified_tables: Vec<String>,
    pub missing_tenant_columns: Vec<String>,
    pub nullable_tenant_columns: Vec<String>,
    pub rls_disabled: Vec<String>,
    pub force_rls_disabled: Vec<String>,
    pub missing_policies: Vec<String>,
}

impl TenantCatalogReport {
    pub fn is_clean(&self) -> bool {
        self.missing_tables.is_empty()
            && self.unclassified_tables.is_empty()
            && self.missing_tenant_columns.is_empty()
            && self.nullable_tenant_columns.is_empty()
            && self.rls_disabled.is_empty()
            && self.force_rls_disabled.is_empty()
            && self.missing_policies.is_empty()
    }
}

/// Inspect the live catalog against the versioned inventory.
pub async fn inspect_tenant_catalog(pool: &PgPool) -> Result<TenantCatalogReport> {
    let rows = sqlx::query(
        r#"
        SELECT n.nspname,
               c.relname,
               c.relrowsecurity,
               c.relforcerowsecurity,
               a.attname IS NOT NULL AS has_tenant_id,
               COALESCE(a.attnotnull, false) AS tenant_not_null,
               EXISTS (
                 SELECT 1
                   FROM pg_policy p
                  WHERE p.polrelid = c.oid
                    AND pg_get_expr(p.polqual, p.polrelid) LIKE '%app.current_tenant%'
                    AND pg_get_expr(p.polwithcheck, p.polrelid) LIKE '%app.current_tenant%'
               ) AS has_tenant_policy
          FROM pg_class c
          JOIN pg_namespace n ON n.oid = c.relnamespace
          LEFT JOIN pg_attribute a
            ON a.attrelid = c.oid
           AND a.attname = 'tenant_id'
           AND a.attnum > 0
           AND NOT a.attisdropped
         WHERE (n.nspname = 'public' OR n.nspname LIKE 'archive\_%' ESCAPE '\')
           AND c.relkind = 'r'
           AND c.relname NOT LIKE '_sqlx_%'
         ORDER BY n.nspname, c.relname
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)?;

    let mut report = TenantCatalogReport {
        missing_tables: Vec::new(),
        unclassified_tables: Vec::new(),
        missing_tenant_columns: Vec::new(),
        nullable_tenant_columns: Vec::new(),
        rls_disabled: Vec::new(),
        force_rls_disabled: Vec::new(),
        missing_policies: Vec::new(),
    };

    let present: std::collections::HashSet<String> = rows
        .iter()
        .filter(|row| row.get::<String, _>("nspname") == "public")
        .map(|row| row.get::<String, _>("relname"))
        .collect();
    for table in TENANT_SCOPED_TABLES {
        if !present.contains(*table) {
            report.missing_tables.push((*table).to_string());
        }
    }

    let archive_tables: Vec<&str> = TENANT_SCOPED_TABLES
        .iter()
        .copied()
        .filter(|table| !SHARED_TABLES.contains(table))
        .collect();
    let archive_schemas: std::collections::HashSet<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("nspname"))
        .filter(|schema| schema != "public")
        .collect();
    for schema in &archive_schemas {
        let present: std::collections::HashSet<String> = rows
            .iter()
            .filter(|row| row.get::<String, _>("nspname") == *schema)
            .map(|row| row.get::<String, _>("relname"))
            .collect();
        for table in &archive_tables {
            if !present.contains(*table) {
                report.missing_tables.push(format!("{schema}.{table}"));
            }
        }
    }

    for row in rows {
        let schema: String = row.get("nspname");
        let table: String = row.get("relname");
        if schema == "public" && EXEMPT_PUBLIC_TABLES.contains(&table.as_str()) {
            continue;
        }
        let expected = if schema == "public" {
            TENANT_SCOPED_TABLES.contains(&table.as_str())
        } else {
            archive_tables.contains(&table.as_str())
        };
        let qualified_table = if schema == "public" {
            table.clone()
        } else {
            format!("{schema}.{table}")
        };
        if !expected {
            report.unclassified_tables.push(qualified_table);
            continue;
        }
        if !row.get::<bool, _>("has_tenant_id") {
            report.missing_tenant_columns.push(qualified_table.clone());
        } else if !row.get::<bool, _>("tenant_not_null") {
            report.nullable_tenant_columns.push(qualified_table.clone());
        }
        if !row.get::<bool, _>("relrowsecurity") {
            report.rls_disabled.push(qualified_table.clone());
        }
        if !row.get::<bool, _>("relforcerowsecurity") {
            report.force_rls_disabled.push(qualified_table.clone());
        }
        if !row.get::<bool, _>("has_tenant_policy") {
            report.missing_policies.push(qualified_table);
        }
    }

    Ok(report)
}

/// Fail closed when the hosted application role can bypass tenant policies or
/// owns tenant tables. Migration and runtime credentials must be distinct.
pub async fn assert_hosted_runtime_role(pool: &PgPool) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT r.rolname, r.rolsuper, r.rolbypassrls,
               EXISTS (
                 SELECT 1
                   FROM pg_class c
                   JOIN pg_namespace n ON n.oid = c.relnamespace
                  WHERE (
                            (n.nspname = 'public' AND c.relname = ANY($1))
                         OR n.nspname LIKE 'archive\_%' ESCAPE '\'
                        )
                    AND c.relowner = r.oid
               ) AS owns_tenant_table
          FROM pg_roles r
         WHERE r.rolname = current_user
        "#,
    )
    .bind(TENANT_SCOPED_TABLES)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)?;

    let is_superuser: bool = row.get("rolsuper");
    let bypasses_rls: bool = row.get("rolbypassrls");
    let owns_tenant_table: bool = row.get("owns_tenant_table");
    if is_superuser || bypasses_rls || owns_tenant_table {
        return Err(Error::Config(
            "hosted database role must be NOSUPERUSER, NOBYPASSRLS, and must not own tenant tables"
                .to_string(),
        ));
    }

    let report = inspect_tenant_catalog(pool).await?;
    if !report.is_clean() {
        return Err(Error::Config(
            "hosted database tenant catalog assertion failed".to_string(),
        ));
    }

    tracing::info!(
        target: "fortemi.security",
        tenant_table_count = TENANT_SCOPED_TABLES.len(),
        "Hosted database role and forced-RLS catalog assertions passed"
    );
    Ok(())
}

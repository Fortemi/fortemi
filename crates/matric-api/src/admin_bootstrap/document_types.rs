use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{set_tenant, BootstrapError, StepAction, BOOTSTRAP_LOCK, LOCAL_TENANT};

/// Copy the migration-seeded system registry, preserving tenant customizations.
pub(super) async fn seed(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> Result<StepAction, BootstrapError> {
    if tenant_id == LOCAL_TENANT {
        return Ok(StepAction::None);
    }
    set_tenant(conn, LOCAL_TENANT).await?;
    // The media and temporal/positional seed migrations omitted is_system for
    // nine built-ins. Include them without copying local custom types or
    // changing the original rows' flags.
    let templates: Vec<(serde_json::Value, Option<String>)> = sqlx::query_as(
        "SELECT to_jsonb(d), c.name FROM public.document_type d
         LEFT JOIN public.embedding_config c
           ON c.id = d.recommended_config_id AND c.tenant_id = d.tenant_id
         WHERE d.tenant_id = $1 AND (d.is_system OR d.name IN (
             'graphviz-diagram', 'mermaid-diagram', 'plantuml-diagram', 'svg-graphic',
             'midi-music', 'tracker-module', 'model-3d-cad', 'model-3d-generic',
             'model-3d-printable')) ORDER BY d.name",
    )
    .bind(LOCAL_TENANT)
    .fetch_all(&mut *conn)
    .await?;
    set_tenant(conn, tenant_id).await?;
    let mut inserted = 0;
    for (template, config_name) in templates {
        // References must resolve in the target tenant, never to the template's ID.
        inserted += sqlx::query(
            "INSERT INTO public.document_type
             SELECT * FROM jsonb_populate_record(NULL::public.document_type,
                 $1::jsonb || jsonb_build_object(
                     'id', $2::uuid, 'tenant_id', $3::uuid,
                     'recommended_config_id', (SELECT id FROM public.embedding_config
                         WHERE tenant_id = $3 AND name = $4),
                     'created_at', now(), 'updated_at', now()))
             ON CONFLICT (tenant_id, name) DO NOTHING",
        )
        .bind(template)
        .bind(Uuid::now_v7())
        .bind(tenant_id)
        .bind(config_name)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    }
    Ok(if inserted == 0 {
        StepAction::None
    } else {
        StepAction::Create
    })
}

/// Repair pre-existing external-OIDC tenants without changing their identity or defaults.
pub async fn backfill_empty(pool: &PgPool, tenant_id: Uuid) -> Result<(), BootstrapError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(BOOTSTRAP_LOCK)
        .execute(&mut *tx)
        .await?;
    set_tenant(&mut tx, tenant_id).await?;
    let empty: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM public.document_type WHERE tenant_id = $1)",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    if empty {
        seed(&mut tx, tenant_id).await?;
    }
    tx.commit().await?;
    Ok(())
}

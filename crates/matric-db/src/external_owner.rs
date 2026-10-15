//! Deployment-scoped ownership upgrade for external-OIDC single-tenant servers.
//!
//! Called only before the API and workers start, using migration credentials.
//! The operator must stop every old writer before starting the upgraded server.

use matric_core::{Error, Result};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Adopt the reserved local ownership graph without changing object identities.
/// An immutable receipt prevents a later configuration change from adopting a
/// different tenant. All DDL, transformations and validation commit together.
pub async fn adopt(pool: &PgPool, owner: Uuid) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let changed = adopt_tx(&mut tx, owner).await?;
    tx.commit().await?;
    Ok(changed)
}

/// Hosted upgrades share identities across memories without changing tenants.
pub async fn retire_identity_clones(pool: &PgPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    retire_identity_clones_on(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn retire_identity_clones_on(conn: &mut PgConnection) -> Result<()> {
    sqlx::raw_sql(include_str!("external_owner/retire_identity_clones.sql"))
        .execute(conn)
        .await?;
    Ok(())
}

async fn adopt_tx(conn: &mut PgConnection, owner: Uuid) -> Result<bool> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('fortemi.archive-ddl', 0))")
        .execute(&mut *conn)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *conn)
    .await?;
    if !privileged {
        return Err(Error::InvalidInput(
            "External ownership upgrade requires an owner/migration role with BYPASSRLS".into(),
        ));
    }
    let completed: Option<Uuid> = sqlx::query_scalar(
        "SELECT owner FROM public.external_oidc_ownership WHERE deployment=true",
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(completed) = completed {
        if completed != owner {
            return Err(Error::InvalidInput(
                "External canonical owner differs from the completed deployment upgrade".into(),
            ));
        }
        return Ok(false);
    }
    sqlx::query("SELECT set_config('app.external_owner', $1, true), set_config('app.current_tenant', $1, true)")
        .bind(owner.to_string()).execute(&mut *conn).await?;
    sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,'External OIDC default tenant','active') ON CONFLICT(id) DO NOTHING")
        .bind(owner).bind(format!("external-{}", owner.simple())).execute(&mut *conn).await?;
    let active: bool =
        sqlx::query_scalar("SELECT status='active' FROM public.tenant_registry WHERE id=$1")
            .bind(owner)
            .fetch_one(&mut *conn)
            .await?;
    if !active {
        return Err(Error::InvalidInput(
            "External canonical owner must be active; upgrade never reactivates tenants".into(),
        ));
    }
    sqlx::raw_sql(include_str!("external_owner/inventory.sql"))
        .execute(&mut *conn)
        .await?;
    sqlx::raw_sql(include_str!("external_owner/reconcile.sql"))
        .execute(&mut *conn)
        .await?;
    sqlx::raw_sql(include_str!("external_owner/transform.sql"))
        .execute(&mut *conn)
        .await?;
    retire_identity_clones_on(conn).await?;
    let archives: Vec<String> = sqlx::query_scalar("SELECT name FROM public.archive_registry WHERE tenant_id=$1 AND schema_name<>'public' ORDER BY name")
        .bind(owner).fetch_all(&mut *conn).await?;
    for archive in archives {
        crate::PgArchiveRepository::sync_archive_schema_on(conn, &archive).await?;
    }
    sqlx::raw_sql(include_str!("external_owner/baselines.sql"))
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT INTO public.external_oidc_ownership(deployment,owner) VALUES(true,$1)")
        .bind(owner)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

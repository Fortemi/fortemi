//! Database startup shared by the binary and acceptance tests.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Tenancy {
    Community,
    Hosted,
    #[cfg(feature = "hosted-auth")]
    External(Uuid),
}

/// Migrate and (only for external OIDC) adopt before returning the runtime pool.
/// Runtime repositories retain DML credentials. Only the narrow administrative
/// executor retains migration credentials for authorized archive/index DDL.
pub(super) async fn database(
    runtime_url: &str,
    migration_url: Option<&str>,
    tenancy: Tenancy,
) -> anyhow::Result<Database> {
    let migration_url = match tenancy {
        Tenancy::Community => runtime_url,
        _ => {
            let url = migration_url.ok_or_else(|| anyhow::anyhow!(
                "Hosted and external-OIDC modes require MIGRATION_DATABASE_URL with separate migration credentials"
            ))?;
            if url == runtime_url {
                anyhow::bail!("MIGRATION_DATABASE_URL and DATABASE_URL must use distinct roles");
            }
            url
        }
    };
    let migration_db = Database::connect(migration_url).await?;
    migration_db.migrate().await?;
    let mode = match tenancy {
        Tenancy::Community => return Ok(migration_db),
        Tenancy::Hosted => {
            matric_db::external_owner::retire_identity_clones(&migration_db.pool).await?;
            PoolConfig::new().hosted_unscoped()
        }
        #[cfg(feature = "hosted-auth")]
        Tenancy::External(owner) => {
            matric_db::external_owner::adopt(&migration_db.pool, owner).await?;
            PoolConfig::new().single_tenant(owner)
        }
    };
    let runtime = Database::connect_with_config(runtime_url, mode).await?;
    // Forced RLS is the isolation boundary for legacy repositories and workers.
    // A tenant GUC alone cannot constrain a superuser or a BYPASSRLS role.
    assert_hosted_runtime_role(&runtime.pool).await?;
    // Migrations revoke PUBLIC access. Admit only this verified runtime role
    // to the minimal pre-tenant PAT lookup, never the owner pool to auth DML.
    let runtime_role: String = sqlx::query_scalar("SELECT quote_ident(current_user::text)")
        .fetch_one(&runtime.pool)
        .await?;
    sqlx::query(&format!(
        "GRANT EXECUTE ON FUNCTION public.resolve_pat_tenant(text) TO {runtime_role}"
    ))
    .execute(&migration_db.pool)
    .await?;
    Ok(runtime.with_administrative_pool(migration_db.pool).await?)
}

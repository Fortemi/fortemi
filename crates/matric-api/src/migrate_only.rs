//! `matric-api --migrate-only`: apply SQLx migrations and exit (#1153).
//!
//! Orchestrators (for example the Helm chart's `pre-install,pre-upgrade` hook Job)
//! run this one-shot process before rolling API and worker Deployments, so schema
//! changes are applied once, with the migration role, before new pods start. API
//! startup keeps running migrations as well; once the hook has applied them that
//! step is a no-op guarded by the SQLx migration lock.

use matric_db::Database;
use tracing::info;

/// First CLI argument that selects the migrate-only process role.
pub const MIGRATE_ONLY_FLAG: &str = "--migrate-only";

/// Returns `Ok(true)` when the process was started as `matric-api --migrate-only`.
pub fn migrate_only_requested() -> anyhow::Result<bool> {
    let mut args = std::env::args_os().skip(1);
    match args.next() {
        Some(first) if first == MIGRATE_ONLY_FLAG => {
            if args.next().is_some() {
                anyhow::bail!("usage: matric-api {MIGRATE_ONLY_FLAG} (takes no further arguments)");
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Selects the connection URL used for migrations.
///
/// Hosted (multi-tenant) mode mirrors API startup: it requires a distinct
/// `MIGRATION_DATABASE_URL`. Community mode prefers `MIGRATION_DATABASE_URL`
/// when set and otherwise falls back to `DATABASE_URL`.
pub fn select_migration_url(
    multi_tenant: bool,
    database_url: Option<String>,
    migration_database_url: Option<String>,
) -> anyhow::Result<String> {
    if multi_tenant {
        let migration = migration_database_url.ok_or_else(|| {
            anyhow::anyhow!(
                "FORTEMI_MULTI_TENANT=true requires MIGRATION_DATABASE_URL for the owner/migration role"
            )
        })?;
        if database_url.as_deref() == Some(migration.as_str()) {
            anyhow::bail!("MIGRATION_DATABASE_URL and DATABASE_URL must use distinct hosted roles");
        }
        return Ok(migration);
    }
    migration_database_url.or(database_url).ok_or_else(|| {
        anyhow::anyhow!("--migrate-only requires DATABASE_URL or MIGRATION_DATABASE_URL")
    })
}

/// Applies all pending migrations and closes the pool.
pub async fn run_migrations_only(multi_tenant: bool) -> anyhow::Result<()> {
    let url = select_migration_url(
        multi_tenant,
        std::env::var("DATABASE_URL").ok(),
        std::env::var("MIGRATION_DATABASE_URL").ok(),
    )?;
    info!(
        multi_tenant,
        "Running database migrations (--migrate-only)..."
    );
    let db = Database::connect(&url).await?;
    db.migrate().await?;
    db.pool.close().await;
    info!("Database migrations complete (--migrate-only); exiting");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::select_migration_url;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn community_prefers_migration_url_then_database_url() {
        assert_eq!(
            select_migration_url(false, s("rt"), s("mig")).unwrap(),
            "mig"
        );
        assert_eq!(select_migration_url(false, s("rt"), None).unwrap(), "rt");
        assert!(select_migration_url(false, None, None).is_err());
    }

    #[test]
    fn hosted_requires_distinct_migration_url() {
        assert_eq!(
            select_migration_url(true, s("rt"), s("mig")).unwrap(),
            "mig"
        );
        assert!(select_migration_url(true, s("rt"), None).is_err());
        assert!(select_migration_url(true, s("same"), s("same")).is_err());
    }
}

use std::time::Duration;

use matric_core::{Error, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Service control-plane cursor. This is neither tenant data nor a registry snapshot.
#[derive(Default)]
pub(super) struct TenantCursor {
    pub after: Uuid,
    through: Option<Uuid>,
}

impl TenantCursor {
    pub async fn page(
        &mut self,
        pool: &PgPool,
        limit: u32,
        statement_timeout: Duration,
    ) -> Result<Vec<Uuid>> {
        let mut control = pool.begin().await.map_err(Error::Database)?;
        sqlx::query("SELECT set_config('statement_timeout',$1,true)")
            .bind(format!("{}ms", statement_timeout.as_millis()))
            .execute(&mut *control)
            .await
            .map_err(Error::Database)?;
        if self.through.is_none() {
            self.through = sqlx::query_scalar(
                "SELECT id FROM public.tenant_registry
                 WHERE status='active' AND id>'00000000-0000-0000-0000-000000000000'::uuid
                 ORDER BY id DESC LIMIT 1",
            )
            .fetch_optional(&mut *control)
            .await
            .map_err(Error::Database)?;
        }
        let tenants = sqlx::query_scalar(
            "SELECT id FROM public.tenant_registry
             WHERE status='active' AND id>$1 AND id<=$3 ORDER BY id LIMIT $2",
        )
        .bind(self.after)
        .bind(i64::from(limit))
        .bind(self.through.unwrap_or(Uuid::nil()))
        .fetch_all(&mut *control)
        .await
        .map_err(Error::Database)?;
        control.commit().await.map_err(Error::Database)?;
        Ok(tenants)
    }
}

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use matric_core::{new_v7, Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub iss: String,
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: Option<String>,
    pub groups: Vec<String>,
    pub current_scopes: Vec<String>,
    pub kind: String,
    pub azp: Option<String>,
    pub status: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_oidc_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct AppUserUpsert {
    pub tenant_id: Uuid,
    pub iss: String,
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: Option<String>,
    pub groups: Vec<String>,
    pub current_scopes: Vec<String>,
    pub kind: String,
    pub azp: Option<String>,
}

#[derive(Clone)]
pub struct PgAppUserRepository {
    pool: PgPool,
}

impl PgAppUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_subject(
        &self,
        tenant_id: Uuid,
        iss: &str,
        sub: &str,
    ) -> Result<Option<AppUser>> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let row = sqlx::query(
            "SELECT id, tenant_id, iss, sub, email, email_verified, display_name, groups,
                    current_scopes, kind, azp, status, first_seen_at, last_seen_at, last_oidc_at
             FROM public.app_user
             WHERE tenant_id = $1 AND iss = $2 AND sub = $3",
        )
        .bind(tenant_id)
        .bind(iss)
        .bind(sub)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        row.map(row_to_app_user).transpose()
    }

    pub async fn list(&self, tenant_id: Uuid) -> Result<Vec<AppUser>> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let rows = sqlx::query(
            "SELECT id, tenant_id, iss, sub, email, email_verified, display_name, groups,
                    current_scopes, kind, azp, status, first_seen_at, last_seen_at, last_oidc_at
             FROM public.app_user
             WHERE tenant_id = $1
             ORDER BY last_seen_at DESC, id DESC",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        rows.into_iter().map(row_to_app_user).collect()
    }

    pub async fn set_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<Option<AppUser>> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let row = sqlx::query(
            "UPDATE public.app_user
             SET status = $3
             WHERE tenant_id = $1 AND id = $2
             RETURNING id, tenant_id, iss, sub, email, email_verified, display_name, groups,
                       current_scopes, kind, azp, status, first_seen_at, last_seen_at, last_oidc_at",
        )
        .bind(tenant_id)
        .bind(id)
        .bind(status)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        row.map(row_to_app_user).transpose()
    }

    pub async fn upsert_oidc(&self, input: AppUserUpsert, force_write: bool) -> Result<AppUser> {
        validate_subject(&input.sub)?;
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, input.tenant_id).await?;
        let id = new_v7();
        let row = sqlx::query(
            "INSERT INTO public.app_user (
                 id, tenant_id, iss, sub, email, email_verified, display_name, groups,
                 current_scopes, kind, azp, status, first_seen_at, last_seen_at, last_oidc_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'active', now(), now(), now())
             ON CONFLICT (tenant_id, iss, sub) DO UPDATE SET
                 email = COALESCE(EXCLUDED.email, public.app_user.email),
                 email_verified = CASE
                     WHEN EXCLUDED.email IS NULL
                     THEN public.app_user.email_verified
                     ELSE EXCLUDED.email_verified
                 END,
                 display_name = COALESCE(EXCLUDED.display_name, public.app_user.display_name),
                 groups = CASE
                     WHEN cardinality(EXCLUDED.groups) = 0
                     THEN public.app_user.groups
                     ELSE EXCLUDED.groups
                 END,
                 current_scopes = EXCLUDED.current_scopes,
                 kind = EXCLUDED.kind,
                 azp = COALESCE(EXCLUDED.azp, public.app_user.azp),
                 last_seen_at = CASE
                     WHEN $12::boolean
                       OR public.app_user.current_scopes IS DISTINCT FROM EXCLUDED.current_scopes
                       OR public.app_user.last_seen_at < now() - interval '60 seconds'
                     THEN now()
                     ELSE public.app_user.last_seen_at
                 END,
                 last_oidc_at = CASE
                     WHEN $12::boolean
                       OR public.app_user.current_scopes IS DISTINCT FROM EXCLUDED.current_scopes
                       OR public.app_user.last_oidc_at < now() - interval '60 seconds'
                     THEN now()
                     ELSE public.app_user.last_oidc_at
                 END
             RETURNING id, tenant_id, iss, sub, email, email_verified, display_name, groups,
                       current_scopes, kind, azp, status, first_seen_at, last_seen_at, last_oidc_at",
        )
        .bind(id)
        .bind(input.tenant_id)
        .bind(&input.iss)
        .bind(&input.sub)
        .bind(input.email.as_deref())
        .bind(input.email_verified)
        .bind(input.display_name.as_deref())
        .bind(&input.groups)
        .bind(&input.current_scopes)
        .bind(&input.kind)
        .bind(input.azp.as_deref())
        .bind(force_write)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;
        let user = row_to_app_user(row)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(user)
    }
}

async fn bind_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
) -> Result<()> {
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

fn validate_subject(sub: &str) -> Result<()> {
    if sub.is_empty() || sub.len() > 255 || sub.chars().any(char::is_control) {
        return Err(Error::InvalidInput("invalid OIDC subject".to_string()));
    }
    Ok(())
}

fn row_to_app_user(row: sqlx::postgres::PgRow) -> Result<AppUser> {
    Ok(AppUser {
        id: row.try_get("id").map_err(Error::Database)?,
        tenant_id: row.try_get("tenant_id").map_err(Error::Database)?,
        iss: row.try_get("iss").map_err(Error::Database)?,
        sub: row.try_get("sub").map_err(Error::Database)?,
        email: row.try_get("email").map_err(Error::Database)?,
        email_verified: row.try_get("email_verified").map_err(Error::Database)?,
        display_name: row.try_get("display_name").map_err(Error::Database)?,
        groups: row.try_get("groups").map_err(Error::Database)?,
        current_scopes: row.try_get("current_scopes").map_err(Error::Database)?,
        kind: row.try_get("kind").map_err(Error::Database)?,
        azp: row.try_get("azp").map_err(Error::Database)?,
        status: row.try_get("status").map_err(Error::Database)?,
        first_seen_at: row.try_get("first_seen_at").map_err(Error::Database)?,
        last_seen_at: row.try_get("last_seen_at").map_err(Error::Database)?,
        last_oidc_at: row.try_get("last_oidc_at").map_err(Error::Database)?,
    })
}

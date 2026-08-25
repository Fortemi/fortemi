//! Tenant-scoped persistence for KMS-enveloped user provider credentials.

use std::fmt;

use chrono::{DateTime, Utc};
use matric_core::{Error, Result};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct UserSecretMetadata {
    pub id: Uuid,
    pub provider: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub rewrapped_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for UserSecretMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSecretMetadata")
            .field("id_present", &true)
            .field("provider_len", &self.provider.chars().count())
            .field("name_len", &self.name.chars().count())
            .field("created_at", &self.created_at)
            .field("last_used_at", &self.last_used_at)
            .field("revoked_at", &self.revoked_at)
            .field("rewrapped_at", &self.rewrapped_at)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct StoredUserSecret {
    pub metadata: UserSecretMetadata,
    pub encrypted_blob: Value,
}

#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct UserSecretRewrapCandidate {
    pub id: Uuid,
    pub user_id: String,
    pub encrypted_blob: Value,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for UserSecretRewrapCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSecretRewrapCandidate")
            .field("id_present", &!self.id.is_nil())
            .field("user_id_present", &!self.user_id.is_empty())
            .field("encrypted_blob", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct UserSecretRewrapJob {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub status: String,
    pub batch_size: i32,
    pub cursor_created_at: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
    pub scanned_count: i64,
    pub rewrapped_count: i64,
    pub skipped_count: i64,
    pub attempt_count: i32,
    pub lease_id: Option<Uuid>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub last_failure_class: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for UserSecretRewrapJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSecretRewrapJob")
            .field("id_present", &!self.id.is_nil())
            .field("tenant_id_present", &!self.tenant_id.is_nil())
            .field("status", &self.status)
            .field("batch_size", &self.batch_size)
            .field("cursor_present", &self.cursor_id.is_some())
            .field("scanned_count", &self.scanned_count)
            .field("rewrapped_count", &self.rewrapped_count)
            .field("skipped_count", &self.skipped_count)
            .field("attempt_count", &self.attempt_count)
            .field("lease_present", &self.lease_id.is_some())
            .field("retry_scheduled", &self.next_attempt_at.is_some())
            .field("last_failure_class", &self.last_failure_class)
            .field("completed", &self.completed_at.is_some())
            .finish()
    }
}

impl fmt::Debug for StoredUserSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredUserSecret")
            .field("metadata", &self.metadata)
            .field("encrypted_blob", &"[REDACTED]")
            .finish()
    }
}

pub struct PgUserSecretRepository;

impl PgUserSecretRepository {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_tx(
        connection: &mut PgConnection,
        id: Uuid,
        tenant_id: Uuid,
        user_id: &str,
        provider: &str,
        name: &str,
        encrypted_blob: Value,
    ) -> Result<UserSecretMetadata> {
        sqlx::query_as::<_, UserSecretMetadata>(
            r#"
            INSERT INTO public.user_secrets (
                id, tenant_id, user_id, provider, name, encrypted_blob
            ) VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, provider, name, created_at, last_used_at, revoked_at, rewrapped_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(provider)
        .bind(name)
        .bind(encrypted_blob)
        .fetch_one(connection)
        .await
        .map_err(Error::Database)
    }

    pub async fn list_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
    ) -> Result<Vec<UserSecretMetadata>> {
        sqlx::query_as::<_, UserSecretMetadata>(
            r#"
            SELECT id, provider, name, created_at, last_used_at, revoked_at, rewrapped_at
              FROM public.user_secrets
             WHERE tenant_id = $1 AND user_id = $2
             ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(connection)
        .await
        .map_err(Error::Database)
    }

    pub async fn get_active_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
        id: Uuid,
    ) -> Result<Option<StoredUserSecret>> {
        let row = sqlx::query(
            r#"
            SELECT id, provider, name, encrypted_blob,
                   created_at, last_used_at, revoked_at, rewrapped_at
              FROM public.user_secrets
             WHERE tenant_id = $1
               AND user_id = $2
               AND id = $3
               AND revoked_at IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(Error::Database)?;

        row.map(|row| {
            Ok(StoredUserSecret {
                metadata: UserSecretMetadata {
                    id: row.try_get("id").map_err(Error::Database)?,
                    provider: row.try_get("provider").map_err(Error::Database)?,
                    name: row.try_get("name").map_err(Error::Database)?,
                    created_at: row.try_get("created_at").map_err(Error::Database)?,
                    last_used_at: row.try_get("last_used_at").map_err(Error::Database)?,
                    revoked_at: row.try_get("revoked_at").map_err(Error::Database)?,
                    rewrapped_at: row.try_get("rewrapped_at").map_err(Error::Database)?,
                },
                encrypted_blob: row.try_get("encrypted_blob").map_err(Error::Database)?,
            })
        })
        .transpose()
    }

    pub async fn revoke_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
        id: Uuid,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE public.user_secrets
               SET revoked_at = COALESCE(revoked_at, clock_timestamp())
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(id)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_used_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
        id: Uuid,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE public.user_secrets
               SET last_used_at = GREATEST(
                    COALESCE(last_used_at, created_at),
                    clock_timestamp()
               )
             WHERE tenant_id = $1
               AND user_id = $2
               AND id = $3
               AND revoked_at IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(id)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn replace_wrapped_key_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
        id: Uuid,
        wrapped_key: Value,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE public.user_secrets
               SET encrypted_blob = jsonb_set(
                    encrypted_blob,
                    '{wrapped_key}',
                    $4,
                    false
               ), rewrapped_at = clock_timestamp()
             WHERE tenant_id = $1
               AND user_id = $2
               AND id = $3
               AND revoked_at IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(id)
        .bind(wrapped_key)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn compare_and_swap_wrapped_key_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
        id: Uuid,
        expected_wrapped_key: Value,
        next_wrapped_key: Value,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE public.user_secrets
               SET encrypted_blob = jsonb_set(
                    encrypted_blob, '{wrapped_key}', $5, false
               ), rewrapped_at = clock_timestamp()
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3
               AND encrypted_blob -> 'wrapped_key' = $4
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(id)
        .bind(expected_wrapped_key)
        .bind(next_wrapped_key)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn ensure_rewrap_job_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        job_id: Uuid,
        batch_size: i32,
    ) -> Result<UserSecretRewrapJob> {
        sqlx::query(
            r#"
            INSERT INTO public.user_secret_rewrap_job (id, tenant_id, batch_size)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(job_id)
        .bind(tenant_id)
        .bind(batch_size)
        .execute(&mut *connection)
        .await
        .map_err(Error::Database)?;
        Self::get_rewrap_job_tx(connection, tenant_id, job_id)
            .await?
            .ok_or_else(|| Error::NotFound("user secret rewrap job".to_string()))
    }

    pub async fn get_rewrap_job_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        job_id: Uuid,
    ) -> Result<Option<UserSecretRewrapJob>> {
        sqlx::query_as::<_, UserSecretRewrapJob>(
            r#"
            SELECT id, tenant_id, status, batch_size, cursor_created_at, cursor_id,
                   scanned_count, rewrapped_count, skipped_count, attempt_count,
                   lease_id, lease_expires_at, next_attempt_at, last_failure_class,
                   created_at, updated_at, completed_at
              FROM public.user_secret_rewrap_job
             WHERE tenant_id = $1 AND id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(job_id)
        .fetch_optional(connection)
        .await
        .map_err(Error::Database)
    }

    pub async fn claim_rewrap_job_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        job_id: Uuid,
        lease_id: Uuid,
        lease_seconds: i64,
    ) -> Result<Option<UserSecretRewrapJob>> {
        sqlx::query_as::<_, UserSecretRewrapJob>(
            r#"
            UPDATE public.user_secret_rewrap_job
               SET status = 'running', lease_id = $3,
                   lease_expires_at = clock_timestamp() + ($4 * INTERVAL '1 second'),
                   next_attempt_at = NULL, attempt_count = attempt_count + 1,
                   updated_at = clock_timestamp()
             WHERE tenant_id = $1 AND id = $2
               AND status <> 'completed' AND status <> 'failed'
               AND (next_attempt_at IS NULL OR next_attempt_at <= clock_timestamp())
               AND (lease_expires_at IS NULL OR lease_expires_at <= clock_timestamp())
            RETURNING id, tenant_id, status, batch_size, cursor_created_at, cursor_id,
                      scanned_count, rewrapped_count, skipped_count, attempt_count,
                      lease_id, lease_expires_at, next_attempt_at, last_failure_class,
                      created_at, updated_at, completed_at
            "#,
        )
        .bind(tenant_id)
        .bind(job_id)
        .bind(lease_id)
        .bind(lease_seconds)
        .fetch_optional(connection)
        .await
        .map_err(Error::Database)
    }

    pub async fn next_rewrap_batch_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<UserSecretRewrapCandidate>> {
        sqlx::query_as::<_, UserSecretRewrapCandidate>(
            r#"
            SELECT id, user_id, encrypted_blob, created_at
              FROM public.user_secrets
             WHERE tenant_id = $1
               AND ($2::timestamptz IS NULL OR (created_at, id) > ($2, $3))
             ORDER BY created_at, id
             LIMIT $4
            "#,
        )
        .bind(tenant_id)
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind(limit)
        .fetch_all(connection)
        .await
        .map_err(Error::Database)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn checkpoint_rewrap_job_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        job_id: Uuid,
        lease_id: Uuid,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
        scanned: i64,
        rewrapped: i64,
        skipped: i64,
        completed: bool,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE public.user_secret_rewrap_job
               SET status = CASE WHEN $9 THEN 'completed' ELSE 'pending' END,
                   cursor_created_at = COALESCE($4, cursor_created_at),
                   cursor_id = COALESCE($5, cursor_id),
                   scanned_count = scanned_count + $6,
                   rewrapped_count = rewrapped_count + $7,
                   skipped_count = skipped_count + $8,
                   lease_id = NULL, lease_expires_at = NULL,
                   last_failure_class = NULL, updated_at = clock_timestamp(),
                   completed_at = CASE WHEN $9 THEN clock_timestamp() ELSE NULL END
             WHERE tenant_id = $1 AND id = $2 AND lease_id = $3 AND status = 'running'
            "#,
        )
        .bind(tenant_id)
        .bind(job_id)
        .bind(lease_id)
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind(scanned)
        .bind(rewrapped)
        .bind(skipped)
        .bind(completed)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn fail_rewrap_job_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        job_id: Uuid,
        lease_id: Uuid,
        reason: &str,
        retryable: bool,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE public.user_secret_rewrap_job
               SET status = CASE WHEN $5 THEN 'retryable' ELSE 'failed' END,
                   next_attempt_at = CASE WHEN $5
                     THEN clock_timestamp() + INTERVAL '30 seconds' ELSE NULL END,
                   last_failure_class = $4, lease_id = NULL, lease_expires_at = NULL,
                   updated_at = clock_timestamp()
             WHERE tenant_id = $1 AND id = $2 AND lease_id = $3 AND status = 'running'
            "#,
        )
        .bind(tenant_id)
        .bind(job_id)
        .bind(lease_id)
        .bind(reason)
        .bind(retryable)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn hard_delete_subject_tx(
        connection: &mut PgConnection,
        tenant_id: Uuid,
        user_id: &str,
    ) -> Result<u64> {
        sqlx::query("DELETE FROM public.user_secrets WHERE tenant_id = $1 AND user_id = $2")
            .bind(tenant_id)
            .bind(user_id)
            .execute(connection)
            .await
            .map(|result| result.rows_affected())
            .map_err(Error::Database)
    }
}

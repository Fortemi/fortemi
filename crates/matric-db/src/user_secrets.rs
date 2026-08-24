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
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct StoredUserSecret {
    pub metadata: UserSecretMetadata,
    pub encrypted_blob: Value,
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
            RETURNING id, provider, name, created_at, last_used_at, revoked_at
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
            SELECT id, provider, name, created_at, last_used_at, revoked_at
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
                   created_at, last_used_at, revoked_at
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
        .bind(wrapped_key)
        .execute(connection)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }
}

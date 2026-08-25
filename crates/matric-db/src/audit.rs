//! Durable PostgreSQL implementation of the core audit sink contract.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use matric_core::audit::{AuditError, AuditEvent, AuditFailureDisposition, AuditSink};
use sqlx::postgres::PgConnection;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditSinkHealthStatus {
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditSinkHealth {
    pub status: AuditSinkHealthStatus,
    pub checked_at: DateTime<Utc>,
    pub consecutive_failures: u64,
    pub last_error_class: Option<&'static str>,
}

impl AuditSinkHealth {
    pub fn disposition_for(&self, event: &AuditEvent) -> AuditFailureDisposition {
        if self.status == AuditSinkHealthStatus::Ready {
            AuditFailureDisposition::Continue
        } else {
            event
                .failure_policy
                .disposition_when_unavailable(matric_core::audit::AuditAvailabilityPhase::Ready)
        }
    }
}

#[derive(Clone)]
pub struct PostgresAuditSink {
    pool: Pool<Postgres>,
    health: Arc<RwLock<AuditSinkHealth>>,
}

impl PostgresAuditSink {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self {
            pool,
            health: Arc::new(RwLock::new(AuditSinkHealth {
                status: AuditSinkHealthStatus::Ready,
                checked_at: Utc::now(),
                consecutive_failures: 0,
                last_error_class: None,
            })),
        }
    }

    pub fn health(&self) -> AuditSinkHealth {
        self.health
            .read()
            .expect("audit health lock poisoned")
            .clone()
    }

    pub async fn check_health(&self) -> Result<AuditSinkHealth, AuditError> {
        match sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
        {
            Ok(_) => {
                self.record_success();
                Ok(self.health())
            }
            Err(error) => {
                self.record_failure("database_unavailable");
                Err(database_error(error))
            }
        }
    }

    /// Persist an event on an existing tenant-scoped transaction.
    ///
    /// Callers use this when the audited mutation and its fail-closed completion
    /// record must commit or roll back together.
    pub async fn emit_tx(
        &self,
        connection: &mut PgConnection,
        event: AuditEvent,
    ) -> Result<(), AuditError> {
        let event = event.sanitized();
        let tenant_id = event_tenant_id(&event)?;

        match insert_event(connection, event, tenant_id).await {
            Ok(()) => {
                self.record_success();
                Ok(())
            }
            Err(error) => {
                self.record_failure("database_write_failed");
                Err(database_error(error))
            }
        }
    }

    fn record_success(&self) {
        let mut health = self.health.write().expect("audit health lock poisoned");
        health.status = AuditSinkHealthStatus::Ready;
        health.checked_at = Utc::now();
        health.consecutive_failures = 0;
        health.last_error_class = None;
    }

    fn record_failure(&self, error_class: &'static str) {
        let mut health = self.health.write().expect("audit health lock poisoned");
        health.status = AuditSinkHealthStatus::Unavailable;
        health.checked_at = Utc::now();
        health.consecutive_failures = health.consecutive_failures.saturating_add(1);
        health.last_error_class = Some(error_class);
    }
}

#[async_trait]
impl AuditSink for PostgresAuditSink {
    async fn emit(&self, event: AuditEvent) -> Result<(), AuditError> {
        let event = event.sanitized();
        let tenant_id = event_tenant_id(&event)?;

        let result = async {
            let mut tx = self.pool.begin().await?;
            sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
                .bind(tenant_id.to_string())
                .execute(&mut *tx)
                .await?;
            insert_event(&mut tx, event, tenant_id).await?;
            tx.commit().await
        }
        .await;

        match result {
            Ok(()) => {
                self.record_success();
                Ok(())
            }
            Err(error) => {
                self.record_failure("database_write_failed");
                Err(database_error(error))
            }
        }
    }

    async fn flush(&self) -> Result<(), AuditError> {
        self.check_health().await.map(|_| ())
    }
}

fn event_tenant_id(event: &AuditEvent) -> Result<Uuid, AuditError> {
    event
        .tenant_id
        .as_deref()
        .ok_or_else(|| AuditError::Sink("missing_tenant_context".to_string()))?
        .parse::<Uuid>()
        .map_err(|_| AuditError::Sink("invalid_tenant_context".to_string()))
}

async fn insert_event(
    connection: &mut PgConnection,
    event: AuditEvent,
    tenant_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO public.audit_event (
            id, tenant_id, schema_version, idempotency_key, event_ts, observed_ts,
            principal_id, resource_kind, resource_id, correlation_id, category,
            action, outcome, reason, severity, failure_policy, visibility,
            retention, source, attrs
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
            $12, $13, $14, $15, $16, $17, $18, $19, $20
        )
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(event.id)
    .bind(tenant_id)
    .bind(i16::try_from(event.schema_version).unwrap_or(i16::MAX))
    .bind(event.idempotency_key)
    .bind(event.event_ts)
    .bind(event.observed_ts)
    .bind(event.principal_id)
    .bind(event.resource_kind)
    .bind(event.resource_id)
    .bind(event.correlation_id)
    .bind(event.category)
    .bind(event.action)
    .bind(format!("{:?}", event.outcome))
    .bind(event.reason)
    .bind(format!("{:?}", event.severity))
    .bind(format!("{:?}", event.failure_policy))
    .bind(format!("{:?}", event.visibility))
    .bind(format!("{:?}", event.retention))
    .bind(format!("{:?}", event.source))
    .bind(serde_json::to_value(event.attrs).unwrap_or_default())
    .execute(connection)
    .await
    .map(|_| ())
}

fn database_error(error: sqlx::Error) -> AuditError {
    let class = match error {
        sqlx::Error::PoolClosed => "pool_closed",
        sqlx::Error::PoolTimedOut => "pool_timeout",
        sqlx::Error::Database(_) => "database_rejected",
        _ => "database_unavailable",
    };
    AuditError::Sink(class.to_string())
}

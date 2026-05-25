//! Shared event outbox helpers.
//!
//! Write paths that need durable publication should insert through this module,
//! preferably with `emit_event_tx` from the same transaction as the data change.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{new_v7, Error, Result};

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct EventOutboxRecord {
    pub id: Uuid,
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub payload: JsonValue,
    pub memory: Option<String>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateOutboxEvent {
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub payload: JsonValue,
    pub memory: Option<String>,
}

impl CreateOutboxEvent {
    pub fn new(
        event_type: impl Into<String>,
        entity_type: impl Into<String>,
        entity_id: Uuid,
        payload: JsonValue,
        memory: Option<String>,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            entity_type: entity_type.into(),
            entity_id,
            payload,
            memory,
        }
    }
}

#[derive(Clone)]
pub struct PgEventOutboxRepository {
    pool: Pool<Postgres>,
}

impl PgEventOutboxRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn emit_event(&self, event: CreateOutboxEvent) -> Result<EventOutboxRecord> {
        validate_event(&event)?;
        let id = new_v7();
        sqlx::query_as::<_, EventOutboxRecord>(
            r#"
            INSERT INTO event_outbox (id, event_type, entity_type, entity_id, payload, memory)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, event_type, entity_type, entity_id, payload, memory, created_at, published_at
            "#,
        )
        .bind(id)
        .bind(event.event_type.trim())
        .bind(event.entity_type.trim())
        .bind(event.entity_id)
        .bind(event.payload)
        .bind(event.memory.as_deref().map(str::trim))
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)
    }

    pub async fn emit_event_tx(
        tx: &mut Transaction<'_, Postgres>,
        event: CreateOutboxEvent,
    ) -> Result<EventOutboxRecord> {
        validate_event(&event)?;
        let id = new_v7();
        sqlx::query_as::<_, EventOutboxRecord>(
            r#"
            INSERT INTO event_outbox (id, event_type, entity_type, entity_id, payload, memory)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, event_type, entity_type, entity_id, payload, memory, created_at, published_at
            "#,
        )
        .bind(id)
        .bind(event.event_type.trim())
        .bind(event.entity_type.trim())
        .bind(event.entity_id)
        .bind(event.payload)
        .bind(event.memory.as_deref().map(str::trim))
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)
    }

    pub async fn list_unpublished(&self, limit: i64) -> Result<Vec<EventOutboxRecord>> {
        sqlx::query_as::<_, EventOutboxRecord>(
            r#"
            SELECT id, event_type, entity_type, entity_id, payload, memory, created_at, published_at
            FROM event_outbox
            WHERE published_at IS NULL
            ORDER BY created_at ASC, id ASC
            LIMIT $1
            "#,
        )
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)
    }

    pub async fn mark_published(&self, ids: &[Uuid]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let result = sqlx::query(
            "UPDATE event_outbox SET published_at = NOW() WHERE id = ANY($1) AND published_at IS NULL",
        )
        .bind(ids)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(result.rows_affected())
    }

    pub async fn count_by_event_type(&self, event_type: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type = $1")
            .bind(event_type.trim())
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(row.get("count"))
    }
}

fn validate_event(event: &CreateOutboxEvent) -> Result<()> {
    if event.event_type.trim().is_empty() {
        return Err(Error::InvalidInput(
            "outbox event_type is required".to_string(),
        ));
    }
    if event.entity_type.trim().is_empty() {
        return Err(Error::InvalidInput(
            "outbox entity_type is required".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_event_type() {
        let event = CreateOutboxEvent::new(
            " ",
            "note",
            Uuid::nil(),
            serde_json::json!({"ok": true}),
            None,
        );
        let err = validate_event(&event).expect_err("event_type must be required");
        assert!(matches!(err, Error::InvalidInput(_)));
    }

    #[test]
    fn accepts_valid_event_contract() {
        let event = CreateOutboxEvent::new(
            "transcript_final",
            "call_session",
            Uuid::nil(),
            serde_json::json!({"call_id": Uuid::nil()}),
            Some("default".to_string()),
        );
        validate_event(&event).expect("valid event");
    }
}

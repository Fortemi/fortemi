//! Shared event outbox helpers.
//!
//! Write paths that need durable publication should insert through this module,
//! preferably with `emit_event_tx` from the same transaction as the data change.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{new_v7, Error, Result};

#[derive(Clone, PartialEq, sqlx::FromRow)]
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

impl std::fmt::Debug for EventOutboxRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventOutboxRecord")
            .field("id_present", &true)
            .field("event_type_len", &text_len(&self.event_type))
            .field("entity_type_len", &text_len(&self.entity_type))
            .field("entity_id_present", &true)
            .field("payload_class", &json_class(&self.payload))
            .field(
                "payload_serialized_len",
                &json_serialized_len(&self.payload),
            )
            .field("memory_len", &self.memory.as_deref().map(text_len))
            .field("created_at", &self.created_at)
            .field("published_at", &self.published_at)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct CreateOutboxEvent {
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub payload: JsonValue,
    pub memory: Option<String>,
}

impl std::fmt::Debug for CreateOutboxEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateOutboxEvent")
            .field("event_type_len", &text_len(&self.event_type))
            .field("entity_type_len", &text_len(&self.entity_type))
            .field("entity_id_present", &true)
            .field("payload_class", &json_class(&self.payload))
            .field(
                "payload_serialized_len",
                &json_serialized_len(&self.payload),
            )
            .field("memory_len", &self.memory.as_deref().map(text_len))
            .finish()
    }
}

fn text_len(value: &str) -> usize {
    value.chars().count()
}

fn json_class(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn json_serialized_len(value: &JsonValue) -> usize {
    serde_json::to_string(value)
        .map(|serialized| serialized.chars().count())
        .unwrap_or_default()
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
    fn outbox_debug_redacts_payload_and_memory() {
        let record_id = Uuid::parse_str("018fd1a0-0000-7000-8000-00000000f501").unwrap();
        let entity_id = Uuid::parse_str("018fd1a0-0000-7000-8000-00000000f502").unwrap();
        let payload = serde_json::json!({
            "provider_call_id": "CA-secret-provider-call",
            "recording_url": "https://api.twilio.example/recordings?token=sk-secret",
            "nested": {
                "email": "operator@example.internal"
            }
        });
        let event = CreateOutboxEvent::new(
            "call_event.secret_type",
            "inbound_event",
            entity_id,
            payload.clone(),
            Some("archive-secret-memory".to_string()),
        );
        let record = EventOutboxRecord {
            id: record_id,
            event_type: "call_event.secret_type".to_string(),
            entity_type: "inbound_event".to_string(),
            entity_id,
            payload,
            memory: Some("archive-secret-memory".to_string()),
            created_at: Utc::now(),
            published_at: None,
        };

        let rendered = format!("{event:?}\n{record:?}");

        assert!(rendered.contains("CreateOutboxEvent"));
        assert!(rendered.contains("EventOutboxRecord"));
        assert!(rendered.contains("id_present"));
        assert!(rendered.contains("entity_id_present"));
        assert!(rendered.contains("payload_class"));
        assert!(rendered.contains("payload_serialized_len"));
        assert!(rendered.contains("memory_len"));
        assert!(!rendered.contains("018fd1a0-0000-7000-8000-00000000f501"));
        assert!(!rendered.contains("018fd1a0-0000-7000-8000-00000000f502"));
        assert!(!rendered.contains("CA-secret-provider-call"));
        assert!(!rendered.contains("recordings?token"));
        assert!(!rendered.contains("sk-secret"));
        assert!(!rendered.contains("operator@example.internal"));
        assert!(!rendered.contains("archive-secret-memory"));
        assert!(!rendered.contains("call_event.secret_type"));
    }

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

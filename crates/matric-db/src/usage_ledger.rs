//! Durable, immutable usage-event ledger.
//!
//! Validated Fortemi events are authoritative here. Sink delivery rows are
//! created transactionally and contain no credentials or provider payloads.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{new_v7, DuplicateIdentity, MeteringError, UsageEvent};

#[derive(Clone, PartialEq, FromRow)]
pub struct UsageLedgerRecord {
    pub event_id: Uuid,
    pub idempotency_key: String,
    pub schema_version: i16,
    pub event_time: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub event_fingerprint: String,
    pub event: JsonValue,
    pub inserted_at: DateTime<Utc>,
}

impl std::fmt::Debug for UsageLedgerRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageLedgerRecord")
            .field("event_id_present", &true)
            .field("idempotency_key_present", &true)
            .field("schema_version", &self.schema_version)
            .field("event_time", &self.event_time)
            .field("recorded_at", &self.recorded_at)
            .field("event_fingerprint_present", &true)
            .field("event_class", &json_class(&self.event))
            .field("inserted_at", &self.inserted_at)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UsageRecordOutcome {
    Accepted { delivery_count: u64 },
    ExactReplay,
}

#[derive(Clone)]
pub struct PgUsageLedgerRepository {
    pool: Pool<Postgres>,
}

impl PgUsageLedgerRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn register_sink(
        &self,
        sink_name: &str,
        required: bool,
    ) -> Result<(), MeteringError> {
        validate_sink_name(sink_name)?;
        sqlx::query(
            r#"
            INSERT INTO public.usage_sink (sink_name, required)
            VALUES ($1, $2)
            ON CONFLICT (sink_name) DO UPDATE
            SET required = EXCLUDED.required, updated_at = NOW()
            "#,
        )
        .bind(sink_name)
        .bind(required)
        .execute(&self.pool)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;
        Ok(())
    }

    pub async fn record_event(
        &self,
        event: &UsageEvent,
    ) -> Result<UsageRecordOutcome, MeteringError> {
        event.validate()?;
        let payload = serde_json::to_value(event).map_err(|_| MeteringError::BackendUnavailable)?;
        let fingerprint = event_fingerprint(event)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;

        let inserted = sqlx::query_as::<_, UsageLedgerRecord>(
            r#"
            INSERT INTO public.usage_event_ledger (
                event_id, idempotency_key, schema_version, event_time,
                recorded_at, event_fingerprint, event
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT DO NOTHING
            RETURNING event_id, idempotency_key, schema_version, event_time,
                      recorded_at, event_fingerprint, event, inserted_at
            "#,
        )
        .bind(event.event_id)
        .bind(&event.idempotency_key)
        .bind(
            i16::try_from(event.schema_version)
                .map_err(|_| MeteringError::UnsupportedSchemaVersion(event.schema_version))?,
        )
        .bind(event.event_time)
        .bind(event.recorded_at)
        .bind(&fingerprint)
        .bind(&payload)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;

        if inserted.is_some() {
            let delivery_count = sqlx::query(
                r#"
                INSERT INTO public.usage_event_delivery (event_id, sink_name)
                SELECT $1, sink_name
                FROM public.usage_sink
                WHERE enabled
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(event.event_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?
            .rows_affected();
            tx.commit()
                .await
                .map_err(|_| MeteringError::BackendUnavailable)?;
            return Ok(UsageRecordOutcome::Accepted { delivery_count });
        }

        let existing = sqlx::query_as::<_, UsageLedgerRecord>(
            r#"
            SELECT event_id, idempotency_key, schema_version, event_time,
                   recorded_at, event_fingerprint, event, inserted_at
            FROM public.usage_event_ledger
            WHERE event_id = $1 OR idempotency_key = $2
            ORDER BY CASE WHEN event_id = $1 THEN 0 ELSE 1 END
            LIMIT 1
            "#,
        )
        .bind(event.event_id)
        .bind(&event.idempotency_key)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;

        if existing.event_id == event.event_id
            && existing.idempotency_key == event.idempotency_key
            && existing.event_fingerprint == fingerprint
            && existing.event == payload
        {
            tx.commit()
                .await
                .map_err(|_| MeteringError::BackendUnavailable)?;
            return Ok(UsageRecordOutcome::ExactReplay);
        }

        let identity = if existing.event_id == event.event_id {
            DuplicateIdentity::EventId
        } else {
            DuplicateIdentity::IdempotencyKey
        };
        let identity_label = match identity {
            DuplicateIdentity::EventId => "event_id",
            DuplicateIdentity::IdempotencyKey => "idempotency_key",
        };
        sqlx::query(
            r#"
            INSERT INTO public.usage_event_conflict (
                conflict_id, incoming_event_id, existing_event_id,
                incoming_idempotency_key, conflict_identity,
                incoming_fingerprint, existing_fingerprint
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(new_v7())
        .bind(event.event_id)
        .bind(existing.event_id)
        .bind(&event.idempotency_key)
        .bind(identity_label)
        .bind(&fingerprint)
        .bind(&existing.event_fingerprint)
        .execute(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;

        Err(MeteringError::DuplicateConflict(identity))
    }

    pub async fn delivery_count(&self, event_id: Uuid) -> Result<i64, MeteringError> {
        sqlx::query("SELECT COUNT(*) AS count FROM public.usage_event_delivery WHERE event_id = $1")
            .bind(event_id)
            .fetch_one(&self.pool)
            .await
            .map(|row| row.get("count"))
            .map_err(|_| MeteringError::BackendUnavailable)
    }
}

fn event_fingerprint(event: &UsageEvent) -> Result<String, MeteringError> {
    let encoded = serde_json::to_vec(event).map_err(|_| MeteringError::BackendUnavailable)?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn validate_sink_name(value: &str) -> Result<(), MeteringError> {
    if value.is_empty()
        || value.len() > 64
        || !value.is_ascii()
        || !value.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_lowercase()
            } else {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'.' | b'-')
            }
        })
    {
        return Err(MeteringError::InvalidLabel("usage sink"));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::{
        UsageClass, UsageDimension, UsageMeasurement, UsageOutcome, UsageProducer, UsageQuantity,
        UsageSource, UsageSubject, UsageUnit,
    };

    fn test_event() -> UsageEvent {
        let event_time = Utc::now();
        let event_id = Uuid::now_v7();
        UsageEvent::new(
            format!("test:{event_id}:usage"),
            event_time,
            UsageSubject::anonymous("test-subject").unwrap(),
            UsageDimension::ApiRequest,
            UsageMeasurement::Measured(UsageQuantity::whole(1, UsageUnit::Count).unwrap()),
            UsageClass::BillableActual,
            UsageProducer::Api,
            UsageSource::LocalMeasured,
            UsageOutcome::Completed,
        )
        .unwrap()
        .with_identity(event_id, event_time)
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        let event = test_event();
        let replay = event.clone();
        let mut conflict = event.clone();
        conflict.outcome = UsageOutcome::FailedAfterPartialUsage;

        assert_eq!(
            event_fingerprint(&event).unwrap(),
            event_fingerprint(&replay).unwrap()
        );
        assert_ne!(
            event_fingerprint(&event).unwrap(),
            event_fingerprint(&conflict).unwrap()
        );
    }

    #[test]
    fn sink_names_are_strict_low_cardinality_labels() {
        for valid in ["openmeter", "stripe.v2", "warehouse_primary", "sink-2"] {
            assert!(validate_sink_name(valid).is_ok());
        }
        for invalid in [
            "",
            "Uppercase",
            "2starts_with_number",
            "contains/slash",
            "https://sink.example",
            "sink?token=secret",
        ] {
            assert_eq!(
                validate_sink_name(invalid),
                Err(MeteringError::InvalidLabel("usage sink"))
            );
        }
    }

    #[test]
    fn ledger_debug_redacts_event_identity_subject_and_payload() {
        let event = test_event();
        let payload = serde_json::to_value(&event).unwrap();
        let record = UsageLedgerRecord {
            event_id: event.event_id,
            idempotency_key: event.idempotency_key.clone(),
            schema_version: event.schema_version as i16,
            event_time: event.event_time,
            recorded_at: event.recorded_at,
            event_fingerprint: event_fingerprint(&event).unwrap(),
            event: payload,
            inserted_at: Utc::now(),
        };
        let rendered = format!("{record:?}");

        assert!(rendered.contains("event_id_present"));
        assert!(rendered.contains("idempotency_key_present"));
        assert!(rendered.contains("event_fingerprint_present"));
        assert!(!rendered.contains(&event.event_id.to_string()));
        assert!(!rendered.contains(&event.idempotency_key));
        assert!(!rendered.contains("test-subject"));
    }

    #[tokio::test]
    async fn database_replay_conflict_and_delivery_are_atomic_when_available() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping usage ledger DB test: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        let repo = PgUsageLedgerRepository::new(pool.clone());
        let sink_name = format!("test_sink_{}", Uuid::now_v7().simple());
        let event = test_event();
        repo.register_sink(&sink_name, false).await.unwrap();

        assert_eq!(
            repo.record_event(&event).await.unwrap(),
            UsageRecordOutcome::Accepted { delivery_count: 1 }
        );
        assert_eq!(
            repo.record_event(&event).await.unwrap(),
            UsageRecordOutcome::ExactReplay
        );
        assert_eq!(repo.delivery_count(event.event_id).await.unwrap(), 1);

        let mut conflict = event.clone();
        conflict.outcome = UsageOutcome::FailedAfterPartialUsage;
        assert_eq!(
            repo.record_event(&conflict).await,
            Err(MeteringError::DuplicateConflict(DuplicateIdentity::EventId))
        );
        let conflict_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM public.usage_event_conflict WHERE incoming_event_id = $1",
        )
        .bind(event.event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(conflict_count, 1);

        sqlx::query("DELETE FROM public.usage_event_ledger WHERE event_id = $1")
            .bind(event.event_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM public.usage_sink WHERE sink_name = $1")
            .bind(&sink_name)
            .execute(&pool)
            .await
            .unwrap();
    }
}

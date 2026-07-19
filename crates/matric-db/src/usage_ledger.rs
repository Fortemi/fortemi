//! Durable, immutable usage-event ledger.
//!
//! Validated Fortemi events are authoritative here. Sink delivery rows are
//! created transactionally and contain no credentials or provider payloads.

use chrono::{DateTime, Duration, Utc};
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

#[derive(Clone, PartialEq)]
pub struct UsageDeliveryClaim {
    pub event_id: Uuid,
    pub sink_name: String,
    pub external_idempotency_key: Uuid,
    pub attempt_id: Uuid,
    pub attempt_number: i32,
    pub lease_expires_at: DateTime<Utc>,
    pub event: UsageEvent,
}

impl std::fmt::Debug for UsageDeliveryClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageDeliveryClaim")
            .field("event_id_present", &true)
            .field("sink_name", &self.sink_name)
            .field("external_idempotency_key_present", &true)
            .field("attempt_id_present", &true)
            .field("attempt_number", &self.attempt_number)
            .field("lease_expires_at", &self.lease_expires_at)
            .field("event_class", &"validated_usage_event")
            .finish()
    }
}

#[derive(FromRow)]
struct DeliveryCandidate {
    event_id: Uuid,
    sink_name: String,
    external_idempotency_key: Uuid,
    attempt_count: i32,
    status: String,
    previous_lease_id: Option<Uuid>,
    event_fingerprint: String,
    event: JsonValue,
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
            SET enabled = TRUE, required = EXCLUDED.required, updated_at = NOW()
            "#,
        )
        .bind(sink_name)
        .bind(required)
        .execute(&self.pool)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;
        Ok(())
    }

    pub async fn set_sink_enabled(
        &self,
        sink_name: &str,
        enabled: bool,
    ) -> Result<bool, MeteringError> {
        validate_sink_name(sink_name)?;
        sqlx::query(
            r#"
            UPDATE public.usage_sink
            SET enabled = $1, updated_at = NOW()
            WHERE sink_name = $2
            "#,
        )
        .bind(enabled)
        .bind(sink_name)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(|_| MeteringError::BackendUnavailable)
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

    pub async fn claim_delivery(
        &self,
        sink_name: &str,
        lease_duration: Duration,
    ) -> Result<Option<UsageDeliveryClaim>, MeteringError> {
        validate_sink_name(sink_name)?;
        validate_lease_duration(lease_duration)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;

        let candidate = sqlx::query_as::<_, DeliveryCandidate>(
            r#"
            SELECT delivery.event_id, delivery.sink_name,
                   delivery.external_idempotency_key, delivery.attempt_count,
                   delivery.status, delivery.lease_id AS previous_lease_id,
                   ledger.event_fingerprint, ledger.event
            FROM public.usage_event_delivery AS delivery
            JOIN public.usage_event_ledger AS ledger
              ON ledger.event_id = delivery.event_id
            JOIN public.usage_sink AS sink
              ON sink.sink_name = delivery.sink_name
            WHERE delivery.sink_name = $1
              AND sink.enabled
              AND (
                (
                    delivery.status IN ('pending', 'retryable')
                    AND (
                        delivery.next_attempt_at IS NULL
                        OR delivery.next_attempt_at <= NOW()
                    )
                )
                OR (
                    delivery.status = 'in_flight'
                    AND delivery.lease_expires_at <= NOW()
                )
              )
            ORDER BY COALESCE(delivery.next_attempt_at, delivery.created_at),
                     delivery.created_at, delivery.event_id
            LIMIT 1
            FOR UPDATE OF delivery SKIP LOCKED
            "#,
        )
        .bind(sink_name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;

        let Some(candidate) = candidate else {
            tx.commit()
                .await
                .map_err(|_| MeteringError::BackendUnavailable)?;
            return Ok(None);
        };

        let event: UsageEvent = serde_json::from_value(candidate.event)
            .map_err(|_| MeteringError::BackendUnavailable)?;
        event.validate()?;
        if event.event_id != candidate.event_id
            || event_fingerprint(&event)? != candidate.event_fingerprint
        {
            return Err(MeteringError::BackendUnavailable);
        }

        let started_at: DateTime<Utc> = sqlx::query_scalar("SELECT NOW()")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;
        let lease_expires_at = started_at + lease_duration;
        let attempt_id = new_v7();
        if candidate.status == "in_flight" {
            let Some(previous_lease_id) = candidate.previous_lease_id else {
                return Err(MeteringError::BackendUnavailable);
            };
            let expired = sqlx::query(
                r#"
                UPDATE public.usage_delivery_attempt
                SET outcome = 'lease_expired', completed_at = $1,
                    reason_class = 'lease_expired'
                WHERE attempt_id = $2 AND outcome = 'in_flight'
                "#,
            )
            .bind(started_at)
            .bind(previous_lease_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?
            .rows_affected();
            if expired != 1 {
                return Err(MeteringError::BackendUnavailable);
            }
        }

        let attempt_number: i32 = sqlx::query_scalar(
            r#"
            UPDATE public.usage_event_delivery
            SET status = 'in_flight', attempt_count = attempt_count + 1,
                last_attempt_at = $1, next_attempt_at = NULL,
                lease_id = $2, lease_expires_at = $3, updated_at = $1
            WHERE event_id = $4 AND sink_name = $5
            RETURNING attempt_count
            "#,
        )
        .bind(started_at)
        .bind(attempt_id)
        .bind(lease_expires_at)
        .bind(candidate.event_id)
        .bind(&candidate.sink_name)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;
        if attempt_number != candidate.attempt_count + 1 {
            return Err(MeteringError::BackendUnavailable);
        }

        sqlx::query(
            r#"
            INSERT INTO public.usage_delivery_attempt (
                attempt_id, event_id, sink_name, attempt_number,
                started_at, lease_expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(attempt_id)
        .bind(candidate.event_id)
        .bind(&candidate.sink_name)
        .bind(attempt_number)
        .bind(started_at)
        .bind(lease_expires_at)
        .execute(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;
        Ok(Some(UsageDeliveryClaim {
            event_id: candidate.event_id,
            sink_name: candidate.sink_name,
            external_idempotency_key: candidate.external_idempotency_key,
            attempt_id,
            attempt_number,
            lease_expires_at,
            event,
        }))
    }

    pub async fn acknowledge_delivery(
        &self,
        claim: &UsageDeliveryClaim,
        exported_at: DateTime<Utc>,
    ) -> Result<bool, MeteringError> {
        self.complete_delivery(claim, "acknowledged", Some(exported_at), None, None)
            .await
    }

    pub async fn retry_delivery(
        &self,
        claim: &UsageDeliveryClaim,
        retry_at: DateTime<Utc>,
        reason_class: &str,
    ) -> Result<bool, MeteringError> {
        validate_reason_class(reason_class)?;
        if retry_at <= Utc::now() {
            return Err(MeteringError::InvalidIdentifier("usage retry time"));
        }
        self.complete_delivery(claim, "retryable", None, Some(retry_at), Some(reason_class))
            .await
    }

    pub async fn reject_delivery(
        &self,
        claim: &UsageDeliveryClaim,
        reason_class: &str,
    ) -> Result<bool, MeteringError> {
        validate_reason_class(reason_class)?;
        self.complete_delivery(claim, "terminal_rejected", None, None, Some(reason_class))
            .await
    }

    async fn complete_delivery(
        &self,
        claim: &UsageDeliveryClaim,
        outcome: &'static str,
        exported_at: Option<DateTime<Utc>>,
        retry_at: Option<DateTime<Utc>>,
        reason_class: Option<&str>,
    ) -> Result<bool, MeteringError> {
        validate_sink_name(&claim.sink_name)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;
        let completed_at: DateTime<Utc> = sqlx::query_scalar("SELECT NOW()")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;
        let updated = sqlx::query(
            r#"
            UPDATE public.usage_event_delivery
            SET status = $1, acknowledged_at = CASE
                    WHEN $1 = 'acknowledged' THEN $2 ELSE NULL
                END,
                exported_at = CASE
                    WHEN $1 = 'acknowledged' THEN $3 ELSE exported_at
                END,
                next_attempt_at = $4,
                terminal_reason = CASE
                    WHEN $1 = 'terminal_rejected' THEN $5 ELSE NULL
                END,
                lease_id = NULL, lease_expires_at = NULL, updated_at = $2
            WHERE event_id = $6 AND sink_name = $7
              AND status = 'in_flight' AND lease_id = $8
              AND lease_expires_at > $2
            "#,
        )
        .bind(outcome)
        .bind(completed_at)
        .bind(exported_at)
        .bind(retry_at)
        .bind(reason_class)
        .bind(claim.event_id)
        .bind(&claim.sink_name)
        .bind(claim.attempt_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?
        .rows_affected();
        if updated == 0 {
            tx.commit()
                .await
                .map_err(|_| MeteringError::BackendUnavailable)?;
            return Ok(false);
        }
        if updated != 1 {
            return Err(MeteringError::BackendUnavailable);
        }

        let attempt_updated = sqlx::query(
            r#"
            UPDATE public.usage_delivery_attempt
            SET outcome = $1, completed_at = $2, retry_at = $3,
                reason_class = $4
            WHERE attempt_id = $5 AND event_id = $6 AND sink_name = $7
              AND outcome = 'in_flight'
            "#,
        )
        .bind(outcome)
        .bind(completed_at)
        .bind(retry_at)
        .bind(reason_class)
        .bind(claim.attempt_id)
        .bind(claim.event_id)
        .bind(&claim.sink_name)
        .execute(&mut *tx)
        .await
        .map_err(|_| MeteringError::BackendUnavailable)?
        .rows_affected();
        if attempt_updated != 1 {
            return Err(MeteringError::BackendUnavailable);
        }

        tx.commit()
            .await
            .map_err(|_| MeteringError::BackendUnavailable)?;
        Ok(true)
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

fn validate_lease_duration(value: Duration) -> Result<(), MeteringError> {
    if value < Duration::seconds(1) || value > Duration::hours(1) {
        return Err(MeteringError::InvalidIdentifier("usage delivery lease"));
    }
    Ok(())
}

fn validate_reason_class(value: &str) -> Result<(), MeteringError> {
    validate_sink_name(value).map_err(|_| MeteringError::InvalidLabel("usage delivery reason"))
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

    static DATABASE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

    #[test]
    fn delivery_validation_is_bounded_and_debug_is_redacted() {
        assert!(validate_lease_duration(Duration::seconds(1)).is_ok());
        assert!(validate_lease_duration(Duration::hours(1)).is_ok());
        assert!(validate_lease_duration(Duration::zero()).is_err());
        assert!(validate_lease_duration(Duration::hours(2)).is_err());
        assert!(validate_reason_class("provider_timeout").is_ok());
        assert!(validate_reason_class("raw error: bearer secret").is_err());

        let event = test_event();
        let claim = UsageDeliveryClaim {
            event_id: event.event_id,
            sink_name: "test_sink".to_string(),
            external_idempotency_key: Uuid::now_v7(),
            attempt_id: Uuid::now_v7(),
            attempt_number: 1,
            lease_expires_at: Utc::now() + Duration::minutes(1),
            event: event.clone(),
        };
        let rendered = format!("{claim:?}");

        assert!(rendered.contains("event_id_present"));
        assert!(rendered.contains("external_idempotency_key_present"));
        assert!(rendered.contains("attempt_id_present"));
        assert!(!rendered.contains(&claim.event_id.to_string()));
        assert!(!rendered.contains(&claim.external_idempotency_key.to_string()));
        assert!(!rendered.contains(&claim.attempt_id.to_string()));
        assert!(!rendered.contains(&event.idempotency_key));
        assert!(!rendered.contains("test-subject"));
    }

    #[tokio::test]
    async fn database_replay_conflict_and_delivery_are_atomic_when_available() {
        let _guard = DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping usage ledger DB test: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        let repo = PgUsageLedgerRepository::new(pool.clone());
        let sink_name = format!("test_sink_{}", Uuid::now_v7().simple());
        let event = test_event();
        repo.register_sink(&sink_name, false).await.unwrap();

        let UsageRecordOutcome::Accepted { delivery_count } =
            repo.record_event(&event).await.unwrap()
        else {
            panic!("first usage event insert was not accepted");
        };
        assert!(delivery_count >= 1);
        assert_eq!(
            repo.record_event(&event).await.unwrap(),
            UsageRecordOutcome::ExactReplay
        );
        assert_eq!(
            repo.delivery_count(event.event_id).await.unwrap(),
            i64::try_from(delivery_count).unwrap()
        );

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

    #[tokio::test]
    async fn database_delivery_leases_recover_and_reject_stale_workers_when_available() {
        let _guard = DATABASE_TEST_LOCK.lock().await;
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping usage delivery lease DB test: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        let repo = PgUsageLedgerRepository::new(pool.clone());
        let sink_name = format!("lease_sink_{}", Uuid::now_v7().simple());
        let event = test_event();
        repo.register_sink(&sink_name, true).await.unwrap();
        repo.record_event(&event).await.unwrap();

        assert!(repo.set_sink_enabled(&sink_name, false).await.unwrap());
        assert!(repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .is_none());
        repo.register_sink(&sink_name, true).await.unwrap();

        let first = repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.attempt_number, 1);
        assert_eq!(first.event, event);
        assert!(repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .is_none());

        sqlx::query(
            r#"
            UPDATE public.usage_event_delivery
            SET lease_expires_at = NOW() - INTERVAL '1 second'
            WHERE event_id = $1 AND sink_name = $2
            "#,
        )
        .bind(event.event_id)
        .bind(&sink_name)
        .execute(&pool)
        .await
        .unwrap();

        let second = repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.attempt_number, 2);
        assert_ne!(second.attempt_id, first.attempt_id);
        assert_eq!(
            second.external_idempotency_key,
            first.external_idempotency_key
        );
        assert!(!repo.acknowledge_delivery(&first, Utc::now()).await.unwrap());

        let retry_at = Utc::now() + Duration::minutes(1);
        assert!(repo
            .retry_delivery(&second, retry_at, "provider_timeout")
            .await
            .unwrap());
        assert!(repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .is_none());

        sqlx::query(
            r#"
            UPDATE public.usage_event_delivery
            SET next_attempt_at = NOW() - INTERVAL '1 second'
            WHERE event_id = $1 AND sink_name = $2
            "#,
        )
        .bind(event.event_id)
        .bind(&sink_name)
        .execute(&pool)
        .await
        .unwrap();
        let third = repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(third.attempt_number, 3);
        assert!(repo
            .reject_delivery(&third, "provider_rejected")
            .await
            .unwrap());
        assert!(repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .is_none());

        let outcomes: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT outcome
            FROM public.usage_delivery_attempt
            WHERE event_id = $1 AND sink_name = $2
            ORDER BY attempt_number
            "#,
        )
        .bind(event.event_id)
        .bind(&sink_name)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            outcomes,
            vec!["lease_expired", "retryable", "terminal_rejected"]
        );
        assert!(sqlx::query(
            r#"
                UPDATE public.usage_delivery_attempt
                SET reason_class = 'rewritten'
                WHERE attempt_id = $1
                "#,
        )
        .bind(third.attempt_id)
        .execute(&pool)
        .await
        .is_err());

        let acknowledged = test_event();
        repo.record_event(&acknowledged).await.unwrap();
        let claim = repo
            .claim_delivery(&sink_name, Duration::minutes(5))
            .await
            .unwrap()
            .unwrap();
        assert!(repo.acknowledge_delivery(&claim, Utc::now()).await.unwrap());
        assert!(!repo.acknowledge_delivery(&claim, Utc::now()).await.unwrap());

        let delivery_states: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT status
            FROM public.usage_event_delivery
            WHERE sink_name = $1
            ORDER BY event_id
            "#,
        )
        .bind(&sink_name)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(delivery_states.contains(&"acknowledged".to_string()));
        assert!(delivery_states.contains(&"terminal_rejected".to_string()));

        sqlx::query("DELETE FROM public.usage_event_ledger WHERE event_id = ANY($1)")
            .bind([event.event_id, acknowledged.event_id])
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

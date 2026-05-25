//! Repository for provider-agnostic real-time call session persistence.

use chrono::Utc;
use matric_core::{
    CallSession, CreateCallSessionRequest, CreateTranscriptSegmentRequest, Error, Result,
    TranscriptSegment, UpdateCallSessionRequest,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::outbox::{CreateOutboxEvent, PgEventOutboxRepository};

/// PostgreSQL repository for call sessions and final transcript segments.
#[derive(Clone)]
pub struct PgCallSessionRepository {
    pool: PgPool,
}

impl PgCallSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a provider-agnostic call session row.
    pub async fn create_session(&self, req: CreateCallSessionRequest) -> Result<CallSession> {
        if req.provider.trim().is_empty() {
            return Err(Error::InvalidInput("provider is required".to_string()));
        }
        if req.provider_call_id.trim().is_empty() {
            return Err(Error::InvalidInput(
                "provider_call_id is required".to_string(),
            ));
        }

        let call_id = matric_core::new_v7();
        let started_at = req.started_at.unwrap_or_else(Utc::now);
        sqlx::query_as::<_, CallSession>(
            r#"
            INSERT INTO call_sessions (
                call_id, provider, provider_call_id, started_at, asr_backend,
                remote_party, archive_id, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING call_id, provider, provider_call_id, started_at, ended_at,
                      end_reason, asr_backend, remote_party, archive_id, metadata
            "#,
        )
        .bind(call_id)
        .bind(req.provider)
        .bind(req.provider_call_id)
        .bind(started_at)
        .bind(req.asr_backend)
        .bind(req.remote_party)
        .bind(req.archive_id)
        .bind(req.metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)
    }

    /// Read a call session by internal ID.
    pub async fn get_session(&self, call_id: Uuid) -> Result<Option<CallSession>> {
        sqlx::query_as::<_, CallSession>(
            r#"
            SELECT call_id, provider, provider_call_id, started_at, ended_at,
                   end_reason, asr_backend, remote_party, archive_id, metadata
            FROM call_sessions
            WHERE call_id = $1
            "#,
        )
        .bind(call_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)
    }

    /// Read a call session by provider and provider call ID.
    pub async fn get_session_by_provider_call_id(
        &self,
        provider: &str,
        provider_call_id: &str,
    ) -> Result<Option<CallSession>> {
        sqlx::query_as::<_, CallSession>(
            r#"
            SELECT call_id, provider, provider_call_id, started_at, ended_at,
                   end_reason, asr_backend, remote_party, archive_id, metadata
            FROM call_sessions
            WHERE provider = $1 AND provider_call_id = $2
            "#,
        )
        .bind(provider)
        .bind(provider_call_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)
    }

    /// Update nullable session metadata and lifecycle fields.
    pub async fn update_session(
        &self,
        call_id: Uuid,
        req: UpdateCallSessionRequest,
    ) -> Result<Option<CallSession>> {
        sqlx::query_as::<_, CallSession>(
            r#"
            UPDATE call_sessions
            SET ended_at = COALESCE($2, ended_at),
                end_reason = COALESCE($3, end_reason),
                asr_backend = COALESCE($4, asr_backend),
                remote_party = COALESCE($5, remote_party),
                archive_id = COALESCE($6, archive_id),
                metadata = COALESCE($7, metadata)
            WHERE call_id = $1
            RETURNING call_id, provider, provider_call_id, started_at, ended_at,
                      end_reason, asr_backend, remote_party, archive_id, metadata
            "#,
        )
        .bind(call_id)
        .bind(req.ended_at)
        .bind(req.end_reason)
        .bind(req.asr_backend)
        .bind(req.remote_party)
        .bind(req.archive_id)
        .bind(req.metadata)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)
    }

    /// Mark a call session ended with a standards-shaped end reason string.
    pub async fn end_session(
        &self,
        call_id: Uuid,
        end_reason: &str,
    ) -> Result<Option<CallSession>> {
        self.update_session(
            call_id,
            UpdateCallSessionRequest {
                ended_at: Some(Utc::now()),
                end_reason: Some(end_reason.to_string()),
                ..UpdateCallSessionRequest::default()
            },
        )
        .await
    }

    /// Persist a final transcript segment. Partial ASR hypotheses stay ephemeral.
    pub async fn create_transcript_segment(
        &self,
        req: CreateTranscriptSegmentRequest,
    ) -> Result<TranscriptSegment> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let segment = Self::create_transcript_segment_tx(&mut tx, req).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(segment)
    }

    /// Persist a final transcript segment and emit its durable outbox event atomically.
    pub async fn create_transcript_segment_with_outbox(
        &self,
        req: CreateTranscriptSegmentRequest,
        event_type: impl Into<String>,
        mut payload: serde_json::Value,
        memory: Option<String>,
    ) -> Result<(TranscriptSegment, crate::outbox::EventOutboxRecord)> {
        let call_id = req.call_id;
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let segment = Self::create_transcript_segment_tx(&mut tx, req).await?;

        if let Some(obj) = payload.as_object_mut() {
            obj.insert("call_id".to_string(), serde_json::json!(call_id));
            obj.insert("segment_id".to_string(), serde_json::json!(segment.id));
        }

        let outbox = PgEventOutboxRepository::emit_event_tx(
            &mut tx,
            CreateOutboxEvent::new(event_type, "call_session", call_id, payload, memory),
        )
        .await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok((segment, outbox))
    }

    async fn create_transcript_segment_tx(
        tx: &mut Transaction<'_, Postgres>,
        req: CreateTranscriptSegmentRequest,
    ) -> Result<TranscriptSegment> {
        if req.text.trim().is_empty() {
            return Err(Error::InvalidInput(
                "transcript segment text is required".to_string(),
            ));
        }

        let id = matric_core::new_v7();
        sqlx::query_as::<_, TranscriptSegment>(
            r#"
            INSERT INTO transcript_segments (
                id, call_id, speaker_label, text, start_ts, end_ts, confidence, sequence
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, call_id, speaker_label, text, start_ts, end_ts,
                      confidence, sequence, created_at
            "#,
        )
        .bind(id)
        .bind(req.call_id)
        .bind(req.speaker_label)
        .bind(req.text)
        .bind(req.start_ts)
        .bind(req.end_ts)
        .bind(req.confidence)
        .bind(req.sequence)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)
    }

    /// List persisted final transcript segments for a call in sequence order.
    pub async fn list_transcript_segments(&self, call_id: Uuid) -> Result<Vec<TranscriptSegment>> {
        sqlx::query_as::<_, TranscriptSegment>(
            r#"
            SELECT id, call_id, speaker_label, text, start_ts, end_ts,
                   confidence, sequence, created_at
            FROM transcript_segments
            WHERE call_id = $1
            ORDER BY sequence ASC, created_at ASC
            "#,
        )
        .bind(call_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)
    }

    /// List persisted final transcript segments for a call with API pagination.
    pub async fn list_transcript_segments_page(
        &self,
        call_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TranscriptSegment>> {
        sqlx::query_as::<_, TranscriptSegment>(
            r#"
            SELECT id, call_id, speaker_label, text, start_ts, end_ts,
                   confidence, sequence, created_at
            FROM transcript_segments
            WHERE call_id = $1
            ORDER BY sequence ASC, created_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(call_id)
        .bind(limit.max(0))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)
    }

    /// Return transcript segment count for a call.
    pub async fn transcript_segment_count(&self, call_id: Uuid) -> Result<i64> {
        let row =
            sqlx::query("SELECT COUNT(*) AS count FROM transcript_segments WHERE call_id = $1")
                .bind(call_id)
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;
        Ok(row.get("count"))
    }

    /// Aggregate realtime call metrics for `/api/v1/health/streaming`.
    pub async fn realtime_metrics(&self) -> Result<RealtimeCallMetrics> {
        let row = sqlx::query(
            r#"
            WITH durations AS (
                SELECT
                    COALESCE(asr_backend, 'unknown') AS asr_backend,
                    ended_at IS NULL AS active,
                    EXTRACT(EPOCH FROM (COALESCE(ended_at, NOW()) - started_at))::DOUBLE PRECISION AS duration_seconds,
                    CASE WHEN ended_at IS NOT NULL
                         THEN EXTRACT(EPOCH FROM (ended_at - started_at))::DOUBLE PRECISION
                         ELSE NULL
                    END AS completed_duration_seconds
                FROM call_sessions
            )
            SELECT
                COUNT(*)::BIGINT AS total_sessions,
                COUNT(*) FILTER (WHERE active)::BIGINT AS active_sessions,
                COUNT(*) FILTER (WHERE NOT active)::BIGINT AS completed_sessions,
                COALESCE(SUM(completed_duration_seconds), 0)::DOUBLE PRECISION AS completed_duration_sum_seconds,
                COUNT(completed_duration_seconds) FILTER (WHERE completed_duration_seconds <= 30)::BIGINT AS duration_le_30,
                COUNT(completed_duration_seconds) FILTER (WHERE completed_duration_seconds <= 60)::BIGINT AS duration_le_60,
                COUNT(completed_duration_seconds) FILTER (WHERE completed_duration_seconds <= 300)::BIGINT AS duration_le_300,
                COUNT(completed_duration_seconds) FILTER (WHERE completed_duration_seconds <= 900)::BIGINT AS duration_le_900,
                COUNT(completed_duration_seconds) FILTER (WHERE completed_duration_seconds <= 1800)::BIGINT AS duration_le_1800,
                COUNT(completed_duration_seconds) FILTER (WHERE completed_duration_seconds <= 3600)::BIGINT AS duration_le_3600,
                COUNT(completed_duration_seconds)::BIGINT AS duration_le_inf,
                (
                    SELECT COALESCE(jsonb_object_agg(asr_backend, total_seconds), '{}'::jsonb)
                    FROM (
                        SELECT asr_backend, COALESCE(SUM(duration_seconds), 0)::DOUBLE PRECISION AS total_seconds
                        FROM durations
                        GROUP BY asr_backend
                    ) backend_totals
                ) AS duration_seconds_by_backend
            FROM durations
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(RealtimeCallMetrics {
            total_sessions: row.get("total_sessions"),
            active_sessions: row.get("active_sessions"),
            completed_sessions: row.get("completed_sessions"),
            completed_duration_sum_seconds: row.get("completed_duration_sum_seconds"),
            duration_buckets: RealtimeDurationBuckets {
                le_30: row.get("duration_le_30"),
                le_60: row.get("duration_le_60"),
                le_300: row.get("duration_le_300"),
                le_900: row.get("duration_le_900"),
                le_1800: row.get("duration_le_1800"),
                le_3600: row.get("duration_le_3600"),
                le_inf: row.get("duration_le_inf"),
            },
            duration_seconds_by_backend: row.get("duration_seconds_by_backend"),
        })
    }
}

/// Aggregated realtime call metrics derived from persisted call sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RealtimeCallMetrics {
    pub total_sessions: i64,
    pub active_sessions: i64,
    pub completed_sessions: i64,
    pub completed_duration_sum_seconds: f64,
    pub duration_buckets: RealtimeDurationBuckets,
    pub duration_seconds_by_backend: serde_json::Value,
}

/// Prometheus-style cumulative duration buckets for ended sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RealtimeDurationBuckets {
    pub le_30: i64,
    pub le_60: i64,
    pub le_300: i64,
    pub le_900: i64,
    pub le_1800: i64,
    pub le_3600: i64,
    pub le_inf: i64,
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;

    static CALL_SESSION_DB_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

    async fn call_session_db_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
        CALL_SESSION_DB_TEST_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    #[test]
    fn repository_is_clone_send_sync() {
        fn assert_clone_send_sync<T: Clone + Send + Sync>() {}
        assert_clone_send_sync::<PgCallSessionRepository>();
    }

    fn backend_seconds(metrics: &RealtimeCallMetrics, backend: &str) -> f64 {
        metrics
            .duration_seconds_by_backend
            .get(backend)
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0)
    }

    #[tokio::test]
    async fn realtime_metrics_capture_mock_active_and_completed_sessions() {
        if std::env::var("INTEGRATION_TEST_DB").ok().as_deref() != Some("1") {
            return;
        }
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return,
        };
        let _guard = call_session_db_test_guard().await;
        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        let repo = PgCallSessionRepository::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let backend = format!("mock-metrics-{suffix}");
        let active_provider_call_id = format!("active-{suffix}");
        let completed_provider_call_id = format!("completed-{suffix}");
        let now = Utc::now();

        let before = repo.realtime_metrics().await.unwrap();
        let active = repo
            .create_session(CreateCallSessionRequest {
                provider: "mock".to_string(),
                provider_call_id: active_provider_call_id,
                started_at: Some(now - chrono::Duration::seconds(10)),
                asr_backend: Some(backend.clone()),
                remote_party: None,
                archive_id: None,
                metadata: serde_json::json!({"test": "active metrics"}),
            })
            .await
            .unwrap();
        let completed = repo
            .create_session(CreateCallSessionRequest {
                provider: "mock".to_string(),
                provider_call_id: completed_provider_call_id,
                started_at: Some(now - chrono::Duration::seconds(50)),
                asr_backend: Some(backend.clone()),
                remote_party: None,
                archive_id: None,
                metadata: serde_json::json!({"test": "completed metrics"}),
            })
            .await
            .unwrap();
        repo.update_session(
            completed.call_id,
            UpdateCallSessionRequest {
                ended_at: Some(now - chrono::Duration::seconds(5)),
                end_reason: Some("normal_hangup".to_string()),
                ..UpdateCallSessionRequest::default()
            },
        )
        .await
        .unwrap();

        let after = repo.realtime_metrics().await.unwrap();

        assert_eq!(after.total_sessions - before.total_sessions, 2);
        assert_eq!(after.active_sessions - before.active_sessions, 1);
        assert_eq!(after.completed_sessions - before.completed_sessions, 1);
        assert_eq!(
            after.duration_buckets.le_30 - before.duration_buckets.le_30,
            0
        );
        assert_eq!(
            after.duration_buckets.le_60 - before.duration_buckets.le_60,
            1
        );
        assert_eq!(
            after.duration_buckets.le_inf - before.duration_buckets.le_inf,
            1
        );
        assert!(
            (after.completed_duration_sum_seconds - before.completed_duration_sum_seconds) >= 45.0
        );
        assert!(backend_seconds(&after, &backend) >= 55.0);

        sqlx::query("DELETE FROM transcript_segments WHERE call_id = ANY($1)")
            .bind([active.call_id, completed.call_id])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM call_sessions WHERE call_id = ANY($1)")
            .bind([active.call_id, completed.call_id])
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn transcript_segment_with_outbox_is_atomic_when_db_is_available() {
        if std::env::var("INTEGRATION_TEST_DB").ok().as_deref() != Some("1") {
            return;
        }
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return,
        };
        let _guard = call_session_db_test_guard().await;
        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        let repo = PgCallSessionRepository::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let provider_call_id = format!("outbox-{suffix}");

        let session = repo
            .create_session(CreateCallSessionRequest {
                provider: "mock".to_string(),
                provider_call_id,
                started_at: Some(Utc::now()),
                asr_backend: Some("mock".to_string()),
                remote_party: None,
                archive_id: None,
                metadata: serde_json::json!({"test": "transcript outbox"}),
            })
            .await
            .unwrap();

        let (segment, outbox) = repo
            .create_transcript_segment_with_outbox(
                CreateTranscriptSegmentRequest {
                    call_id: session.call_id,
                    speaker_label: Some("speaker_0".to_string()),
                    text: "hello world".to_string(),
                    start_ts: Some(0.0),
                    end_ts: Some(1.0),
                    confidence: Some(0.99),
                    sequence: 1,
                },
                "transcript_final",
                serde_json::json!({
                    "text": "hello world",
                    "speaker": "speaker_0",
                    "start_ts": 0.0,
                    "end_ts": 1.0,
                    "confidence": 0.99,
                    "sequence": 1,
                }),
                None,
            )
            .await
            .unwrap();

        assert_eq!(segment.call_id, session.call_id);
        assert_eq!(outbox.event_type, "transcript_final");
        assert_eq!(outbox.entity_type, "call_session");
        assert_eq!(outbox.entity_id, session.call_id);
        assert_eq!(
            outbox.payload["call_id"],
            serde_json::json!(session.call_id)
        );
        assert_eq!(outbox.payload["segment_id"], serde_json::json!(segment.id));

        sqlx::query("DELETE FROM event_outbox WHERE id = $1")
            .bind(outbox.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM transcript_segments WHERE call_id = $1")
            .bind(session.call_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM call_sessions WHERE call_id = $1")
            .bind(session.call_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn realtime_metrics_query_executes_when_integration_db_is_available() {
        if std::env::var("INTEGRATION_TEST_DB").ok().as_deref() != Some("1") {
            return;
        }
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return,
        };
        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        let repo = PgCallSessionRepository::new(pool);
        let metrics = repo.realtime_metrics().await.unwrap();

        assert!(metrics.total_sessions >= metrics.active_sessions);
        assert!(metrics.total_sessions >= metrics.completed_sessions);
        assert!(metrics.duration_buckets.le_inf >= metrics.duration_buckets.le_3600);
    }

    #[test]
    fn call_session_types_are_serde_clone_send_sync() {
        fn assert_bounds<
            T: Clone + Send + Sync + serde::Serialize + for<'de> serde::Deserialize<'de>,
        >() {
        }
        assert_bounds::<CallSession>();
        assert_bounds::<TranscriptSegment>();
    }
}

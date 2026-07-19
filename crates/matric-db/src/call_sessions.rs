//! Repository for provider-agnostic real-time call session persistence.

use std::fmt;

use chrono::{DateTime, Utc};
use matric_core::{
    CallSession, CreateCallSessionRequest, CreateTranscriptSegmentRequest, Error, Result,
    TranscriptSegment, UpdateCallSessionRequest,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::outbox::{CreateOutboxEvent, PgEventOutboxRepository};

const REALTIME_BINDING_DIGEST_BYTES: usize = 32;

/// Persisted terminal state for one authoritative ASR media attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeMediaTerminalStatus {
    Completed,
    ClientInterrupted,
    ProviderInterrupted,
    StartFailed,
    CloseFailed,
    Failover,
}

impl RealtimeMediaTerminalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::ClientInterrupted => "client_interrupted",
            Self::ProviderInterrupted => "provider_interrupted",
            Self::StartFailed => "start_failed",
            Self::CloseFailed => "close_failed",
            Self::Failover => "failover",
        }
    }
}

/// Policy applied when a different provider binding arrives for an active call.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RealtimeMediaBindingPolicy {
    #[default]
    RejectActive,
    SupersedeActive,
}

/// Database-owned realtime media attempt.
#[derive(Clone, sqlx::FromRow)]
pub struct RealtimeMediaStreamAttempt {
    pub attempt_id: Uuid,
    pub call_id: Uuid,
    pub attempt_number: i32,
    pub claim_id: Uuid,
    pub provider_binding_sha256: Vec<u8>,
    pub sample_rate_hz: i32,
    pub accepted_samples: i64,
    pub status: String,
    pub claimed_at: DateTime<Utc>,
    pub last_sample_at: Option<DateTime<Utc>>,
    pub terminal_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for RealtimeMediaStreamAttempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RealtimeMediaStreamAttempt")
            .field("attempt_id_present", &true)
            .field("call_id_present", &true)
            .field("attempt_number", &self.attempt_number)
            .field("claim_id_present", &true)
            .field(
                "provider_binding_digest_present",
                &(!self.provider_binding_sha256.is_empty()),
            )
            .field("sample_rate_hz", &self.sample_rate_hz)
            .field("accepted_samples", &self.accepted_samples)
            .field("status", &self.status)
            .field("claimed_at", &self.claimed_at)
            .field("last_sample_at_set", &self.last_sample_at.is_some())
            .field("terminal_at_set", &self.terminal_at.is_some())
            .finish()
    }
}

/// Result of an atomic media-stream ownership claim.
#[derive(Clone, Debug)]
pub enum RealtimeMediaClaim {
    Claimed {
        attempt: RealtimeMediaStreamAttempt,
        superseded: Option<RealtimeMediaStreamAttempt>,
    },
    ActiveConflict,
    BindingReplay(RealtimeMediaStreamAttempt),
}

/// Result of replay-safe attempt finalization.
#[derive(Clone, Debug)]
pub enum RealtimeMediaFinalization {
    Finalized(RealtimeMediaStreamAttempt),
    Replay(RealtimeMediaStreamAttempt),
}

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
            INSERT INTO public.call_sessions (
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
            FROM public.call_sessions
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
            FROM public.call_sessions
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
            UPDATE public.call_sessions
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

    /// Atomically claim the sole active media binding for a call.
    ///
    /// The provider binding is a SHA-256 digest produced at the adapter
    /// boundary; raw provider identifiers never enter this repository.
    pub async fn claim_realtime_media_stream(
        &self,
        call_id: Uuid,
        provider_binding_sha256: &[u8],
        sample_rate_hz: u32,
        policy: RealtimeMediaBindingPolicy,
    ) -> Result<RealtimeMediaClaim> {
        if provider_binding_sha256.len() != REALTIME_BINDING_DIGEST_BYTES {
            return Err(Error::InvalidInput(
                "provider binding digest must be SHA-256".to_string(),
            ));
        }
        let sample_rate_hz = i32::try_from(sample_rate_hz)
            .ok()
            .filter(|value| (1..=384_000).contains(value))
            .ok_or_else(|| Error::InvalidInput("sample rate is out of range".to_string()))?;

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let eligible = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT call_id
            FROM public.call_sessions
            WHERE call_id = $1 AND ended_at IS NULL
            FOR UPDATE
            "#,
        )
        .bind(call_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        if eligible.is_none() {
            tx.rollback().await.map_err(Error::Database)?;
            return Err(Error::InvalidInput(
                "call session is not eligible for media binding".to_string(),
            ));
        }

        if let Some(existing) =
            Self::realtime_attempt_by_binding_tx(&mut tx, call_id, provider_binding_sha256).await?
        {
            tx.commit().await.map_err(Error::Database)?;
            return Ok(RealtimeMediaClaim::BindingReplay(existing));
        }

        let active = Self::active_realtime_attempt_tx(&mut tx, call_id).await?;
        let superseded = match (active, policy) {
            (Some(_), RealtimeMediaBindingPolicy::RejectActive) => {
                tx.commit().await.map_err(Error::Database)?;
                return Ok(RealtimeMediaClaim::ActiveConflict);
            }
            (Some(active), RealtimeMediaBindingPolicy::SupersedeActive) => Some(
                Self::finalize_realtime_attempt_tx(
                    &mut tx,
                    active.attempt_id,
                    active.claim_id,
                    RealtimeMediaTerminalStatus::Failover,
                )
                .await?,
            ),
            (None, _) => None,
        };

        let attempt_number = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT COALESCE(MAX(attempt_number), 0)::INTEGER + 1
            FROM public.realtime_media_stream_attempt
            WHERE call_id = $1
            "#,
        )
        .bind(call_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;
        let attempt_id = matric_core::new_v7();
        let claim_id = Uuid::new_v4();
        let attempt = sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            INSERT INTO public.realtime_media_stream_attempt (
                attempt_id, call_id, attempt_number, claim_id,
                provider_binding_sha256, sample_rate_hz
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING attempt_id, call_id, attempt_number, claim_id,
                      provider_binding_sha256, sample_rate_hz, accepted_samples,
                      status, claimed_at, last_sample_at, terminal_at
            "#,
        )
        .bind(attempt_id)
        .bind(call_id)
        .bind(attempt_number)
        .bind(claim_id)
        .bind(provider_binding_sha256)
        .bind(sample_rate_hz)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(RealtimeMediaClaim::Claimed {
            attempt,
            superseded,
        })
    }

    /// Persist samples only after the ASR session accepted them.
    pub async fn record_realtime_accepted_samples(
        &self,
        attempt_id: Uuid,
        claim_id: Uuid,
        accepted_samples: usize,
    ) -> Result<RealtimeMediaStreamAttempt> {
        let accepted_samples = i64::try_from(accepted_samples)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                Error::InvalidInput("accepted sample count must be positive".to_string())
            })?;
        sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            UPDATE public.realtime_media_stream_attempt
            SET accepted_samples = accepted_samples + $3,
                last_sample_at = NOW()
            WHERE attempt_id = $1
              AND claim_id = $2
              AND status = 'active'
            RETURNING attempt_id, call_id, attempt_number, claim_id,
                      provider_binding_sha256, sample_rate_hz, accepted_samples,
                      status, claimed_at, last_sample_at, terminal_at
            "#,
        )
        .bind(attempt_id)
        .bind(claim_id)
        .bind(accepted_samples)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| {
            Error::InvalidInput("realtime media attempt ownership is no longer active".to_string())
        })
    }

    /// Finalize an attempt once, returning the same terminal fact on replay.
    pub async fn finalize_realtime_media_stream(
        &self,
        attempt_id: Uuid,
        claim_id: Uuid,
        status: RealtimeMediaTerminalStatus,
        accepted_samples: u64,
    ) -> Result<RealtimeMediaFinalization> {
        let accepted_samples = i64::try_from(accepted_samples).map_err(|_| {
            Error::InvalidInput("accepted sample count is out of range".to_string())
        })?;
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        if let Some(attempt) = sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            UPDATE public.realtime_media_stream_attempt
            SET status = $3,
                accepted_samples = $4,
                last_sample_at = CASE
                    WHEN $4 > accepted_samples THEN NOW()
                    ELSE last_sample_at
                END,
                terminal_at = NOW()
            WHERE attempt_id = $1
              AND claim_id = $2
              AND status = 'active'
              AND accepted_samples <= $4
            RETURNING attempt_id, call_id, attempt_number, claim_id,
                      provider_binding_sha256, sample_rate_hz, accepted_samples,
                      status, claimed_at, last_sample_at, terminal_at
            "#,
        )
        .bind(attempt_id)
        .bind(claim_id)
        .bind(status.as_str())
        .bind(accepted_samples)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?
        {
            tx.commit().await.map_err(Error::Database)?;
            return Ok(RealtimeMediaFinalization::Finalized(attempt));
        }

        let existing = sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            SELECT attempt_id, call_id, attempt_number, claim_id,
                   provider_binding_sha256, sample_rate_hz, accepted_samples,
                   status, claimed_at, last_sample_at, terminal_at
            FROM public.realtime_media_stream_attempt
            WHERE attempt_id = $1 AND claim_id = $2
            "#,
        )
        .bind(attempt_id)
        .bind(claim_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;

        match existing {
            Some(attempt) if attempt.status != "active" => {
                Ok(RealtimeMediaFinalization::Replay(attempt))
            }
            _ => Err(Error::InvalidInput(
                "realtime media attempt ownership is invalid".to_string(),
            )),
        }
    }

    async fn realtime_attempt_by_binding_tx(
        tx: &mut Transaction<'_, Postgres>,
        call_id: Uuid,
        provider_binding_sha256: &[u8],
    ) -> Result<Option<RealtimeMediaStreamAttempt>> {
        sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            SELECT attempt_id, call_id, attempt_number, claim_id,
                   provider_binding_sha256, sample_rate_hz, accepted_samples,
                   status, claimed_at, last_sample_at, terminal_at
            FROM public.realtime_media_stream_attempt
            WHERE call_id = $1 AND provider_binding_sha256 = $2
            "#,
        )
        .bind(call_id)
        .bind(provider_binding_sha256)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)
    }

    async fn active_realtime_attempt_tx(
        tx: &mut Transaction<'_, Postgres>,
        call_id: Uuid,
    ) -> Result<Option<RealtimeMediaStreamAttempt>> {
        sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            SELECT attempt_id, call_id, attempt_number, claim_id,
                   provider_binding_sha256, sample_rate_hz, accepted_samples,
                   status, claimed_at, last_sample_at, terminal_at
            FROM public.realtime_media_stream_attempt
            WHERE call_id = $1 AND status = 'active'
            "#,
        )
        .bind(call_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)
    }

    async fn finalize_realtime_attempt_tx(
        tx: &mut Transaction<'_, Postgres>,
        attempt_id: Uuid,
        claim_id: Uuid,
        status: RealtimeMediaTerminalStatus,
    ) -> Result<RealtimeMediaStreamAttempt> {
        sqlx::query_as::<_, RealtimeMediaStreamAttempt>(
            r#"
            UPDATE public.realtime_media_stream_attempt
            SET status = $3, terminal_at = NOW()
            WHERE attempt_id = $1 AND claim_id = $2 AND status = 'active'
            RETURNING attempt_id, call_id, attempt_number, claim_id,
                      provider_binding_sha256, sample_rate_hz, accepted_samples,
                      status, claimed_at, last_sample_at, terminal_at
            "#,
        )
        .bind(attempt_id)
        .bind(claim_id)
        .bind(status.as_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)
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
            INSERT INTO public.transcript_segments (
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
            FROM public.transcript_segments
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
            FROM public.transcript_segments
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
        let row = sqlx::query(
            "SELECT COUNT(*) AS count FROM public.transcript_segments WHERE call_id = $1",
        )
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
                FROM public.call_sessions
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
#[derive(Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RealtimeCallMetrics {
    pub total_sessions: i64,
    pub active_sessions: i64,
    pub completed_sessions: i64,
    pub completed_duration_sum_seconds: f64,
    pub duration_buckets: RealtimeDurationBuckets,
    pub duration_seconds_by_backend: serde_json::Value,
}

impl std::fmt::Debug for RealtimeCallMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealtimeCallMetrics")
            .field("total_sessions", &self.total_sessions)
            .field("active_sessions", &self.active_sessions)
            .field("completed_sessions", &self.completed_sessions)
            .field(
                "completed_duration_sum_seconds",
                &self.completed_duration_sum_seconds,
            )
            .field("duration_buckets", &self.duration_buckets)
            .field(
                "duration_backend_count",
                &duration_backend_key_stats(&self.duration_seconds_by_backend).0,
            )
            .field(
                "duration_backend_key_total_len",
                &duration_backend_key_stats(&self.duration_seconds_by_backend).1,
            )
            .finish()
    }
}

fn duration_backend_key_stats(value: &serde_json::Value) -> (usize, usize) {
    value
        .as_object()
        .map(|object| {
            let total_len = object.keys().map(|key| key.chars().count()).sum();
            (object.len(), total_len)
        })
        .unwrap_or((0, 0))
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

    #[test]
    fn realtime_attempt_debug_redacts_binding_and_claim_material() {
        let attempt = RealtimeMediaStreamAttempt {
            attempt_id: Uuid::now_v7(),
            call_id: Uuid::now_v7(),
            attempt_number: 1,
            claim_id: Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
            provider_binding_sha256: vec![0x5a; REALTIME_BINDING_DIGEST_BYTES],
            sample_rate_hz: 16_000,
            accepted_samples: 320,
            status: "active".to_string(),
            claimed_at: Utc::now(),
            last_sample_at: None,
            terminal_at: None,
        };

        let rendered = format!("{attempt:?}");
        assert!(rendered.contains("provider_binding_digest_present: true"));
        assert!(rendered.contains("claim_id_present: true"));
        assert!(!rendered.contains("aaaaaaaa-aaaa"));
        assert!(!rendered.contains("[90, 90"));
    }

    fn backend_seconds(metrics: &RealtimeCallMetrics, backend: &str) -> f64 {
        metrics
            .duration_seconds_by_backend
            .get(backend)
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0)
    }

    #[test]
    fn realtime_call_metrics_debug_redacts_backend_labels() {
        let metrics = RealtimeCallMetrics {
            total_sessions: 3,
            active_sessions: 1,
            completed_sessions: 2,
            completed_duration_sum_seconds: 42.5,
            duration_buckets: RealtimeDurationBuckets {
                le_30: 1,
                le_60: 2,
                le_300: 2,
                le_900: 2,
                le_1800: 2,
                le_3600: 2,
                le_inf: 2,
            },
            duration_seconds_by_backend: serde_json::json!({
                "whisper://operator@example.internal?token=sk-secret": 42.5
            }),
        };

        let rendered = format!("{metrics:?}");

        assert!(rendered.contains("RealtimeCallMetrics"));
        assert!(rendered.contains("duration_backend_count"));
        assert!(rendered.contains("duration_backend_key_total_len"));
        assert!(!rendered.contains("whisper://"));
        assert!(!rendered.contains("operator@example.internal"));
        assert!(!rendered.contains("sk-secret"));
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

        sqlx::query("DELETE FROM public.transcript_segments WHERE call_id = ANY($1)")
            .bind([active.call_id, completed.call_id])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM public.call_sessions WHERE call_id = ANY($1)")
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

        sqlx::query("DELETE FROM public.event_outbox WHERE id = $1")
            .bind(outbox.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM public.transcript_segments WHERE call_id = $1")
            .bind(session.call_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM public.call_sessions WHERE call_id = $1")
            .bind(session.call_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn realtime_media_claims_samples_replays_and_failover_are_authoritative() {
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
        let suffix = Uuid::new_v4();
        let session = repo
            .create_session(CreateCallSessionRequest {
                provider: "twilio".to_string(),
                provider_call_id: format!("realtime-attempt-{suffix}"),
                started_at: Some(Utc::now()),
                asr_backend: Some("mock".to_string()),
                remote_party: None,
                archive_id: None,
                metadata: serde_json::json!({"test": "authoritative media attempt"}),
            })
            .await
            .unwrap();
        let binding_a = vec![0x11; REALTIME_BINDING_DIGEST_BYTES];
        let binding_b = vec![0x22; REALTIME_BINDING_DIGEST_BYTES];
        let binding_c = vec![0x33; REALTIME_BINDING_DIGEST_BYTES];

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let repo = repo.clone();
            let binding = binding_a.clone();
            tasks.push(tokio::spawn(async move {
                repo.claim_realtime_media_stream(
                    session.call_id,
                    &binding,
                    16_000,
                    RealtimeMediaBindingPolicy::RejectActive,
                )
                .await
                .unwrap()
            }));
        }
        let mut claims = Vec::with_capacity(tasks.len());
        for task in tasks {
            claims.push(task.await);
        }
        let claimed_count = claims
            .iter()
            .filter(|claim| matches!(claim.as_ref().unwrap(), RealtimeMediaClaim::Claimed { .. }))
            .count();
        let replay_count = claims
            .iter()
            .filter(|claim| {
                matches!(
                    claim.as_ref().unwrap(),
                    RealtimeMediaClaim::BindingReplay(_)
                )
            })
            .count();
        assert_eq!(claimed_count, 1);
        assert_eq!(replay_count, 7);

        let first = claims
            .into_iter()
            .map(|claim| claim.unwrap())
            .find_map(|claim| match claim {
                RealtimeMediaClaim::Claimed { attempt, .. }
                | RealtimeMediaClaim::BindingReplay(attempt) => Some(attempt),
                RealtimeMediaClaim::ActiveConflict => None,
            })
            .unwrap();
        assert_eq!(first.attempt_number, 1);
        assert_eq!(first.accepted_samples, 0);

        let conflict = repo
            .claim_realtime_media_stream(
                session.call_id,
                &binding_b,
                16_000,
                RealtimeMediaBindingPolicy::RejectActive,
            )
            .await
            .unwrap();
        assert!(matches!(conflict, RealtimeMediaClaim::ActiveConflict));

        let first = repo
            .record_realtime_accepted_samples(first.attempt_id, first.claim_id, 8_000)
            .await
            .unwrap();
        assert_eq!(first.accepted_samples, 8_000);
        assert!(first.last_sample_at.is_some());

        let terminal = match repo
            .finalize_realtime_media_stream(
                first.attempt_id,
                first.claim_id,
                RealtimeMediaTerminalStatus::Completed,
                16_000,
            )
            .await
            .unwrap()
        {
            RealtimeMediaFinalization::Finalized(attempt) => attempt,
            RealtimeMediaFinalization::Replay(_) => panic!("first finalization must win"),
        };
        assert_eq!(terminal.status, "completed");
        assert_eq!(terminal.accepted_samples, 16_000);
        assert!(terminal.terminal_at.is_some());

        let replay = repo
            .finalize_realtime_media_stream(
                first.attempt_id,
                first.claim_id,
                RealtimeMediaTerminalStatus::ProviderInterrupted,
                16_000,
            )
            .await
            .unwrap();
        assert!(matches!(
            replay,
            RealtimeMediaFinalization::Replay(ref attempt) if attempt.status == "completed"
        ));

        let repeated_binding = repo
            .claim_realtime_media_stream(
                session.call_id,
                &binding_a,
                16_000,
                RealtimeMediaBindingPolicy::RejectActive,
            )
            .await
            .unwrap();
        assert!(matches!(
            repeated_binding,
            RealtimeMediaClaim::BindingReplay(ref attempt)
                if attempt.attempt_id == first.attempt_id
        ));

        let second = match repo
            .claim_realtime_media_stream(
                session.call_id,
                &binding_b,
                16_000,
                RealtimeMediaBindingPolicy::RejectActive,
            )
            .await
            .unwrap()
        {
            RealtimeMediaClaim::Claimed {
                attempt,
                superseded: None,
            } => attempt,
            other => panic!("unexpected second claim: {other:?}"),
        };
        assert_eq!(second.attempt_number, 2);

        let third = match repo
            .claim_realtime_media_stream(
                session.call_id,
                &binding_c,
                16_000,
                RealtimeMediaBindingPolicy::SupersedeActive,
            )
            .await
            .unwrap()
        {
            RealtimeMediaClaim::Claimed {
                attempt,
                superseded: Some(superseded),
            } => {
                assert_eq!(superseded.attempt_id, second.attempt_id);
                assert_eq!(superseded.status, "failover");
                attempt
            }
            other => panic!("unexpected failover claim: {other:?}"),
        };
        assert_eq!(third.attempt_number, 3);

        assert!(repo
            .record_realtime_accepted_samples(second.attempt_id, second.claim_id, 1)
            .await
            .is_err());
        assert!(repo
            .finalize_realtime_media_stream(
                second.attempt_id,
                Uuid::new_v4(),
                RealtimeMediaTerminalStatus::ClientInterrupted,
                0,
            )
            .await
            .is_err());

        repo.finalize_realtime_media_stream(
            third.attempt_id,
            third.claim_id,
            RealtimeMediaTerminalStatus::CloseFailed,
            0,
        )
        .await
        .unwrap();
        let terminal_mutation = sqlx::query(
            r#"
            UPDATE public.realtime_media_stream_attempt
            SET accepted_samples = accepted_samples + 1
            WHERE attempt_id = $1
            "#,
        )
        .bind(third.attempt_id)
        .execute(&pool)
        .await;
        assert!(terminal_mutation.is_err());

        sqlx::query("DELETE FROM public.call_sessions WHERE call_id = $1")
            .bind(session.call_id)
            .execute(&pool)
            .await
            .unwrap();
        let remaining = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM public.realtime_media_stream_attempt WHERE call_id = $1",
        )
        .bind(session.call_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);
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

//! Repository for provider-agnostic real-time call session persistence.

use chrono::Utc;
use matric_core::{
    CallSession, CreateCallSessionRequest, CreateTranscriptSegmentRequest, Error, Result,
    TranscriptSegment, UpdateCallSessionRequest,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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
        .fetch_one(&self.pool)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_is_clone_send_sync() {
        fn assert_clone_send_sync<T: Clone + Send + Sync>() {}
        assert_clone_send_sync::<PgCallSessionRepository>();
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

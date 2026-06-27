//! Realtime transcript event emitter.
//!
//! Bridges provider-neutral ASR events into the shared durable event outbox.

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use matric_core::{CreateTranscriptSegmentRequest, Result};
use matric_db::{CreateOutboxEvent, Database};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::asr::TranscriptEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptEmitterSummary {
    pub partials: usize,
    pub finals: usize,
    pub errors: usize,
}

/// Consume ASR transcript events and emit durable outbox rows.
///
/// Current implementation uses the all-through-outbox strategy from #844:
/// partial hypotheses are durable `transcript_partial` rows and final segments
/// are persisted to `transcript_segments` in the same transaction as their
/// `transcript_final` outbox row.
pub async fn emit_transcript_events<S>(
    db: &Database,
    call_id: Uuid,
    mut events: S,
) -> Result<TranscriptEmitterSummary>
where
    S: Stream<Item = TranscriptEvent> + Unpin,
{
    let mut summary = TranscriptEmitterSummary::default();
    let mut sequence: i32 = 0;

    while let Some(event) = events.next().await {
        match event {
            TranscriptEvent::Partial { text, ts } => {
                sequence += 1;
                let payload = transcript_partial_payload(call_id, &text, ts, sequence);
                db.outbox
                    .emit_event(CreateOutboxEvent::new(
                        "transcript_partial",
                        "call_session",
                        call_id,
                        payload,
                        None,
                    ))
                    .await?;
                summary.partials += 1;
            }
            TranscriptEvent::Final {
                text,
                speaker_label,
                start_ts,
                end_ts,
                confidence,
            } => {
                sequence += 1;
                let payload = transcript_final_payload(
                    call_id,
                    &text,
                    speaker_label.as_deref(),
                    start_ts,
                    end_ts,
                    confidence,
                    sequence,
                );
                db.call_sessions
                    .create_transcript_segment_with_outbox(
                        CreateTranscriptSegmentRequest {
                            call_id,
                            speaker_label,
                            text,
                            start_ts,
                            end_ts,
                            confidence,
                            sequence,
                        },
                        "transcript_final",
                        payload,
                        None,
                    )
                    .await?;
                summary.finals += 1;
            }
            TranscriptEvent::Error { reason } => {
                sequence += 1;
                db.outbox
                    .emit_event(CreateOutboxEvent::new(
                        "call_event",
                        "call_session",
                        call_id,
                        transcript_error_payload(call_id, &reason, sequence),
                        None,
                    ))
                    .await?;
                summary.errors += 1;
            }
        }
    }

    Ok(summary)
}

pub(crate) fn transcript_partial_payload(
    call_id: Uuid,
    text: &str,
    ts: DateTime<Utc>,
    sequence: i32,
) -> JsonValue {
    serde_json::json!({
        "call_id": call_id,
        "text": text,
        "ts": ts,
        "sequence": sequence,
    })
}

pub(crate) fn transcript_final_payload(
    call_id: Uuid,
    text: &str,
    speaker_label: Option<&str>,
    start_ts: Option<f64>,
    end_ts: Option<f64>,
    confidence: Option<f32>,
    sequence: i32,
) -> JsonValue {
    serde_json::json!({
        "call_id": call_id,
        "text": text,
        "speaker": speaker_label,
        "start_ts": start_ts,
        "end_ts": end_ts,
        "confidence": confidence,
        "sequence": sequence,
    })
}

pub(crate) fn transcript_error_payload(call_id: Uuid, reason: &str, sequence: i32) -> JsonValue {
    serde_json::json!({
        "call_id": call_id,
        "event_type": "asr_error",
        "payload": {
            "reason_class": classify_asr_error_reason(reason),
            "reason_len": reason.chars().count(),
            "sequence": sequence,
        }
    })
}

fn classify_asr_error_reason(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.trim().is_empty() {
        "empty"
    } else if lower.contains("auth")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("api key")
        || lower.contains("token")
    {
        "auth"
    } else if lower.contains("parse") || lower.contains("json") || lower.contains("decode") {
        "parse"
    } else if lower.contains("timeout") || lower.contains("deadline") {
        "timeout"
    } else if lower.contains("connect") || lower.contains("socket") || lower.contains("network") {
        "transport"
    } else {
        "provider_error"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_payload_matches_outbox_contract() {
        let call_id = Uuid::nil();
        let ts = DateTime::parse_from_rfc3339("2026-05-24T17:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let payload = transcript_partial_payload(call_id, "hello wor", ts, 7);

        assert_eq!(payload["call_id"], serde_json::json!(call_id));
        assert_eq!(payload["text"], "hello wor");
        assert_eq!(payload["sequence"], 7);
        assert_eq!(payload["ts"], "2026-05-24T17:00:00Z");
    }

    #[test]
    fn final_payload_matches_outbox_contract() {
        let call_id = Uuid::nil();
        let payload = transcript_final_payload(
            call_id,
            "hello world",
            Some("speaker_0"),
            Some(1.25),
            Some(2.5),
            Some(0.98),
            8,
        );

        assert_eq!(payload["call_id"], serde_json::json!(call_id));
        assert_eq!(payload["text"], "hello world");
        assert_eq!(payload["speaker"], "speaker_0");
        assert_eq!(payload["start_ts"], 1.25);
        assert_eq!(payload["end_ts"], 2.5);
        assert_eq!(payload["confidence"], serde_json::json!(0.98_f32));
        assert_eq!(payload["sequence"], 8);
    }

    #[test]
    fn error_payload_redacts_provider_reason_text() {
        let call_id = Uuid::nil();
        let raw_reason = "Deepgram JSON parse failed for 東京 https://api.example.test/listen?token=sk-live-secret at /tmp/customer/audio.wav";
        let payload = transcript_error_payload(call_id, raw_reason, 9);
        let rendered = payload.to_string();

        assert_eq!(payload["call_id"], serde_json::json!(call_id));
        assert_eq!(payload["event_type"], "asr_error");
        assert_eq!(payload["payload"]["reason_class"], "auth");
        assert_eq!(payload["payload"]["reason_len"], raw_reason.chars().count());
        assert_eq!(payload["payload"]["sequence"], 9);

        for raw in [
            "Deepgram",
            "JSON parse failed",
            "東京",
            "api.example.test",
            "sk-live-secret",
            "/tmp/customer/audio.wav",
        ] {
            assert!(
                !rendered.contains(raw),
                "ASR error payload leaked raw reason value {raw:?}: {rendered}"
            );
        }
    }

    #[tokio::test]
    async fn emits_high_volume_transcripts_to_outbox_when_db_is_available() {
        if std::env::var("INTEGRATION_TEST_DB").ok().as_deref() != Some("1") {
            return;
        }
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return,
        };
        let db = Database::connect(&database_url)
            .await
            .expect("connect integration database");
        let suffix = Uuid::new_v4();
        let session = db
            .call_sessions
            .create_session(matric_core::CreateCallSessionRequest {
                provider: "mock".to_string(),
                provider_call_id: format!("emitter-{suffix}"),
                started_at: Some(Utc::now()),
                asr_backend: Some("mock".to_string()),
                remote_party: None,
                archive_id: None,
                metadata: serde_json::json!({"test": "high volume emitter"}),
            })
            .await
            .expect("create call session");

        let mut events = Vec::new();
        for index in 0..100 {
            events.push(TranscriptEvent::Partial {
                text: format!("partial {index}"),
                ts: Utc::now(),
            });
        }
        for index in 0..10 {
            events.push(TranscriptEvent::Final {
                text: format!("final {index}"),
                speaker_label: Some(format!("speaker_{}", index % 2)),
                start_ts: Some(index as f64),
                end_ts: Some(index as f64 + 0.5),
                confidence: Some(0.9),
            });
        }

        let summary = emit_transcript_events(&db, session.call_id, futures::stream::iter(events))
            .await
            .expect("emit transcript events");
        assert_eq!(summary.partials, 100);
        assert_eq!(summary.finals, 10);
        assert_eq!(summary.errors, 0);

        let partial_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_outbox WHERE entity_id = $1 AND event_type = 'transcript_partial'",
        )
        .bind(session.call_id)
        .fetch_one(db.pool())
        .await
        .expect("count partial outbox rows");
        assert_eq!(partial_count, 100);

        let final_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_outbox WHERE entity_id = $1 AND event_type = 'transcript_final'",
        )
        .bind(session.call_id)
        .fetch_one(db.pool())
        .await
        .expect("count final outbox rows");
        assert_eq!(final_count, 10);

        let segment_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transcript_segments WHERE call_id = $1")
                .bind(session.call_id)
                .fetch_one(db.pool())
                .await
                .expect("count transcript segments");
        assert_eq!(segment_count, 10);

        let final_payload: serde_json::Value = sqlx::query_scalar(
            r#"
            SELECT payload
            FROM event_outbox
            WHERE entity_id = $1 AND event_type = 'transcript_final'
            ORDER BY created_at ASC, id ASC
            LIMIT 1
            "#,
        )
        .bind(session.call_id)
        .fetch_one(db.pool())
        .await
        .expect("load final payload");
        assert_eq!(final_payload["call_id"], serde_json::json!(session.call_id));
        assert_eq!(final_payload["text"], "final 0");
        assert_eq!(final_payload["sequence"], 101);
        assert!(final_payload["segment_id"].as_str().is_some());

        sqlx::query("DELETE FROM event_outbox WHERE entity_id = $1")
            .bind(session.call_id)
            .execute(db.pool())
            .await
            .expect("cleanup outbox rows");
        sqlx::query("DELETE FROM transcript_segments WHERE call_id = $1")
            .bind(session.call_id)
            .execute(db.pool())
            .await
            .expect("cleanup transcript segments");
        sqlx::query("DELETE FROM call_sessions WHERE call_id = $1")
            .bind(session.call_id)
            .execute(db.pool())
            .await
            .expect("cleanup call session");
    }
}

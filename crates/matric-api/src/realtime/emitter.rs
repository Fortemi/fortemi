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
                        serde_json::json!({
                            "call_id": call_id,
                            "event_type": "asr_error",
                            "payload": {
                                "reason": reason,
                                "sequence": sequence,
                            }
                        }),
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
}

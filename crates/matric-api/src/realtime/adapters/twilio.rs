//! Twilio Programmable Voice adapter boundary helpers.
//!
//! Twilio wire types and webhook field names stay inside this module. Public
//! functions return standards-shaped [`MediaFrame`] and [`CallControlEvent`]
//! values for the rest of the realtime pipeline.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use matric_core::{Error, Result};
use serde::Deserialize;

use crate::realtime::{CallControlEvent, CallState, Codec, EndReason, MediaFrame};

const TWILIO_PROVIDER: &str = "twilio";

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
enum TwilioMediaEnvelope {
    Start {
        start: TwilioStart,
        #[serde(default, rename = "sequenceNumber")]
        sequence_number: Option<String>,
    },
    Media {
        media: TwilioMedia,
        #[serde(default, rename = "sequenceNumber")]
        sequence_number: Option<String>,
    },
    Stop {
        stop: TwilioStop,
        #[serde(default, rename = "sequenceNumber")]
        sequence_number: Option<String>,
    },
    Mark {
        mark: TwilioMark,
        #[serde(default, rename = "sequenceNumber")]
        sequence_number: Option<String>,
    },
    Dtmf {
        dtmf: TwilioDtmf,
        #[serde(default, rename = "sequenceNumber")]
        sequence_number: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwilioStart {
    call_sid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwilioMedia {
    payload: String,
    timestamp: String,
    chunk: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwilioStop {
    call_sid: String,
}

#[derive(Debug, Deserialize)]
struct TwilioMark {
    name: String,
}

#[derive(Debug, Deserialize)]
struct TwilioDtmf {
    digit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwilioTranslatedEvent {
    Media(MediaFrame),
    Control(CallControlEvent),
}

pub fn translate_media_stream_json(input: &str) -> Result<TwilioTranslatedEvent> {
    let envelope: TwilioMediaEnvelope = serde_json::from_str(input)?;
    match envelope {
        TwilioMediaEnvelope::Start {
            start,
            sequence_number,
        } => Ok(TwilioTranslatedEvent::Control(
            CallControlEvent::CallStarted {
                provider: TWILIO_PROVIDER.to_string(),
                provider_call_id: start.call_sid,
            },
        ))
        .map(|event| with_sequence(event, sequence_number)),
        TwilioMediaEnvelope::Media {
            media,
            sequence_number,
        } => {
            let payload = BASE64.decode(media.payload.as_bytes()).map_err(|err| {
                Error::InvalidInput(format!("invalid Twilio media payload: {err}"))
            })?;
            let timestamp_rtp = media.timestamp.parse::<u32>().map_err(|err| {
                Error::InvalidInput(format!("invalid Twilio media timestamp: {err}"))
            })?;
            let sequence = sequence_number
                .as_deref()
                .unwrap_or(&media.chunk)
                .parse::<u32>()
                .map_err(|err| Error::InvalidInput(format!("invalid Twilio sequence: {err}")))?;
            Ok(TwilioTranslatedEvent::Media(MediaFrame {
                codec: Codec::PcmuG711 { sample_rate: 8_000 },
                timestamp_rtp,
                sequence,
                marker: sequence <= 1,
                payload,
            }))
        }
        TwilioMediaEnvelope::Stop {
            stop,
            sequence_number,
        } => Ok(TwilioTranslatedEvent::Control(CallControlEvent::Custom {
            event_type: "call_media_stopped".to_string(),
            payload: serde_json::json!({"provider_call_id": stop.call_sid}),
        }))
        .map(|event| with_sequence(event, sequence_number)),
        TwilioMediaEnvelope::Mark {
            mark,
            sequence_number,
        } => Ok(TwilioTranslatedEvent::Control(CallControlEvent::Custom {
            event_type: "media_mark".to_string(),
            payload: serde_json::json!({"name": mark.name}),
        }))
        .map(|event| with_sequence(event, sequence_number)),
        TwilioMediaEnvelope::Dtmf {
            dtmf,
            sequence_number,
        } => {
            let digit = dtmf.digit.chars().next().ok_or_else(|| {
                Error::InvalidInput("Twilio DTMF event missing digit".to_string())
            })?;
            Ok(TwilioTranslatedEvent::Control(
                CallControlEvent::DtmfDigit { digit },
            ))
            .map(|event| with_sequence(event, sequence_number))
        }
    }
}

fn with_sequence(
    event: TwilioTranslatedEvent,
    _sequence_number: Option<String>,
) -> TwilioTranslatedEvent {
    event
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TwilioVoiceWebhookForm {
    call_sid: String,
    #[serde(default)]
    call_status: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default, rename = "RecordingSid")]
    _recording_sid: Option<String>,
    #[serde(default)]
    recording_url: Option<String>,
    #[serde(default)]
    recording_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwilioVoiceWebhookEvent {
    pub provider_call_id: String,
    pub control_event: CallControlEvent,
}

pub fn translate_voice_webhook_form(input: &[u8]) -> Result<TwilioVoiceWebhookEvent> {
    let form: TwilioVoiceWebhookForm = serde_urlencoded::from_bytes(input)
        .map_err(|err| Error::InvalidInput(format!("invalid Twilio webhook form: {err}")))?;
    form.into_event()
}

impl TwilioVoiceWebhookForm {
    fn into_event(self) -> Result<TwilioVoiceWebhookEvent> {
        let provider_call_id = self.call_sid.clone();
        if self
            .recording_status
            .as_deref()
            .is_some_and(|status| status.eq_ignore_ascii_case("completed"))
        {
            let url = self.recording_url.ok_or_else(|| {
                Error::InvalidInput("Twilio recording.completed missing RecordingUrl".to_string())
            })?;
            return Ok(TwilioVoiceWebhookEvent {
                provider_call_id,
                control_event: CallControlEvent::RecordingAvailable { url },
            });
        }

        let status = self
            .call_status
            .as_deref()
            .ok_or_else(|| Error::InvalidInput("Twilio webhook missing CallStatus".to_string()))?;
        let control_event = match status {
            "ringing" => CallControlEvent::Custom {
                event_type: "call_started".to_string(),
                payload: serde_json::json!({
                    "provider": TWILIO_PROVIDER,
                    "provider_call_id": self.call_sid,
                    "remote_party": remote_party(&self.from, &self.to, &self.direction),
                }),
            },
            "answered" | "in-progress" => CallControlEvent::StateChanged {
                state: CallState::Active,
            },
            "completed" => CallControlEvent::StateChanged {
                state: CallState::Ended {
                    reason: EndReason::NormalHangup,
                },
            },
            "failed" | "busy" | "no-answer" => CallControlEvent::StateChanged {
                state: CallState::Ended {
                    reason: EndReason::Failed,
                },
            },
            other => CallControlEvent::Custom {
                event_type: "state_change".to_string(),
                payload: serde_json::json!({"provider_status": other}),
            },
        };

        Ok(TwilioVoiceWebhookEvent {
            provider_call_id,
            control_event,
        })
    }
}

fn remote_party(
    from: &Option<String>,
    to: &Option<String>,
    direction: &Option<String>,
) -> Option<String> {
    match direction.as_deref() {
        Some("outbound-api") | Some("outbound-dial") => to.clone(),
        _ => from.clone().or_else(|| to.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_envelope_decodes_pcmu_payload() {
        let event = translate_media_stream_json(
            r#"{"event":"media","sequenceNumber":"7","media":{"payload":"AQIDBA==","timestamp":"160","chunk":"3"}}"#,
        )
        .unwrap();

        assert!(matches!(
            event,
            TwilioTranslatedEvent::Media(MediaFrame {
                codec: Codec::PcmuG711 { sample_rate: 8000 },
                timestamp_rtp: 160,
                sequence: 7,
                marker: false,
                payload,
            }) if payload == vec![1, 2, 3, 4]
        ));
    }

    #[test]
    fn dtmf_envelope_maps_to_control_event() {
        let event = translate_media_stream_json(
            r##"{"event":"dtmf","sequenceNumber":"8","dtmf":{"digit":"#"}}"##,
        )
        .unwrap();
        assert!(matches!(
            event,
            TwilioTranslatedEvent::Control(CallControlEvent::DtmfDigit { digit: '#' })
        ));
    }

    #[test]
    fn ringing_webhook_maps_to_call_started_payload() {
        let event = translate_voice_webhook_form(
            b"CallSid=CA123&CallStatus=ringing&From=%2B15551230000&To=%2B15559870000&Direction=inbound",
        )
        .unwrap();

        assert_eq!(event.provider_call_id, "CA123");
        assert!(matches!(
            event.control_event,
            CallControlEvent::Custom { ref event_type, ref payload }
                if event_type == "call_started"
                    && payload["provider_call_id"] == "CA123"
                    && payload["remote_party"] == "+15551230000"
        ));
    }

    #[test]
    fn recording_completed_maps_to_recording_available() {
        let event = translate_voice_webhook_form(
            b"CallSid=CA123&RecordingStatus=completed&RecordingSid=RE123&RecordingUrl=https%3A%2F%2Fapi.twilio.com%2Frecording.wav",
        )
        .unwrap();

        assert!(matches!(
            event.control_event,
            CallControlEvent::RecordingAvailable { ref url }
                if url == "https://api.twilio.com/recording.wav"
        ));
    }

    #[test]
    fn terminal_webhook_statuses_map_to_end_reasons() {
        let completed =
            translate_voice_webhook_form(b"CallSid=CA123&CallStatus=completed").unwrap();
        assert!(matches!(
            completed.control_event,
            CallControlEvent::StateChanged {
                state: CallState::Ended {
                    reason: EndReason::NormalHangup
                }
            }
        ));

        let failed = translate_voice_webhook_form(b"CallSid=CA123&CallStatus=busy").unwrap();
        assert!(matches!(
            failed.control_event,
            CallControlEvent::StateChanged {
                state: CallState::Ended {
                    reason: EndReason::Failed
                }
            }
        ));
    }
}

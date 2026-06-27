//! Twilio Programmable Voice adapter boundary helpers.
//!
//! Twilio wire types and webhook field names stay inside this module. Public
//! functions return standards-shaped [`MediaFrame`] and [`CallControlEvent`]
//! values for the rest of the realtime pipeline.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures::stream;
use matric_core::{Error, Result};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;

use crate::realtime::{
    CallControlEvent, CallControlEventStream, CallState, CallTransport, Codec, EndReason,
    MediaFrame, MediaFrameStream,
};

const TWILIO_PROVIDER: &str = "twilio";

pub fn provider_name() -> &'static str {
    TWILIO_PROVIDER
}

#[derive(Deserialize)]
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

impl fmt::Debug for TwilioMediaEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start {
                start,
                sequence_number,
            } => f
                .debug_struct("TwilioMediaEnvelope::Start")
                .field("start", start)
                .field(
                    "sequence_number_len",
                    &sequence_number.as_ref().map(String::len),
                )
                .finish(),
            Self::Media {
                media,
                sequence_number,
            } => f
                .debug_struct("TwilioMediaEnvelope::Media")
                .field("media", media)
                .field(
                    "sequence_number_len",
                    &sequence_number.as_ref().map(String::len),
                )
                .finish(),
            Self::Stop {
                stop,
                sequence_number,
            } => f
                .debug_struct("TwilioMediaEnvelope::Stop")
                .field("stop", stop)
                .field(
                    "sequence_number_len",
                    &sequence_number.as_ref().map(String::len),
                )
                .finish(),
            Self::Mark {
                mark,
                sequence_number,
            } => f
                .debug_struct("TwilioMediaEnvelope::Mark")
                .field("mark", mark)
                .field(
                    "sequence_number_len",
                    &sequence_number.as_ref().map(String::len),
                )
                .finish(),
            Self::Dtmf {
                dtmf,
                sequence_number,
            } => f
                .debug_struct("TwilioMediaEnvelope::Dtmf")
                .field("dtmf", dtmf)
                .field(
                    "sequence_number_len",
                    &sequence_number.as_ref().map(String::len),
                )
                .finish(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwilioStart {
    call_sid: String,
}

impl fmt::Debug for TwilioStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioStart")
            .field("call_sid_len", &self.call_sid.len())
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwilioMedia {
    payload: String,
    timestamp: String,
    chunk: String,
}

impl fmt::Debug for TwilioMedia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioMedia")
            .field("payload_len", &self.payload.len())
            .field("timestamp_len", &self.timestamp.len())
            .field("chunk_len", &self.chunk.len())
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwilioStop {
    call_sid: String,
}

impl fmt::Debug for TwilioStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioStop")
            .field("call_sid_len", &self.call_sid.len())
            .finish()
    }
}

#[derive(Deserialize)]
struct TwilioMark {
    name: String,
}

impl fmt::Debug for TwilioMark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioMark")
            .field("name_len", &self.name.len())
            .finish()
    }
}

#[derive(Deserialize)]
struct TwilioDtmf {
    digit: String,
}

impl fmt::Debug for TwilioDtmf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioDtmf")
            .field("digit_len", &self.digit.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum TwilioTranslatedEvent {
    Media(MediaFrame),
    Control(CallControlEvent),
}

impl fmt::Debug for TwilioTranslatedEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Media(frame) => f
                .debug_struct("TwilioTranslatedEvent::Media")
                .field("codec", &frame.codec)
                .field("timestamp_rtp", &frame.timestamp_rtp)
                .field("sequence", &frame.sequence)
                .field("marker", &frame.marker)
                .field("payload_len", &frame.payload.len())
                .finish(),
            Self::Control(event) => f
                .debug_struct("TwilioTranslatedEvent::Control")
                .field("event_class", &twilio_control_event_class(event))
                .finish(),
        }
    }
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
                remote_party: None,
                metadata: serde_json::json!({"source": "twilio_media_stream"}),
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
            payload: twilio_provider_call_reference_metadata(&stop.call_sid),
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

/// Deterministic Twilio Media Streams transport backed by fixture envelopes.
///
/// This keeps Twilio wire parsing behind the adapter boundary while allowing
/// the generic [`CallTransport`] contract to be exercised without a live
/// Twilio WebSocket. Production sockets should feed received envelopes through
/// the same translation functions before handing frames to downstream ASR.
#[derive(Clone)]
pub struct TwilioMediaStreamAdapter {
    provider_call_id: String,
    frames: Vec<MediaFrame>,
    control_events: Vec<CallControlEvent>,
    dropped_on_close: Option<String>,
    ended: Option<EndReason>,
}

impl fmt::Debug for TwilioMediaStreamAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioMediaStreamAdapter")
            .field("provider_call_id_len", &self.provider_call_id.len())
            .field("frames_count", &self.frames.len())
            .field(
                "frame_payload_lens",
                &self
                    .frames
                    .iter()
                    .map(|frame| frame.payload.len())
                    .collect::<Vec<_>>(),
            )
            .field("control_events_count", &self.control_events.len())
            .field(
                "control_event_classes",
                &self
                    .control_events
                    .iter()
                    .map(twilio_control_event_class)
                    .collect::<Vec<_>>(),
            )
            .field(
                "dropped_on_close_len",
                &self.dropped_on_close.as_ref().map(String::len),
            )
            .field("ended", &self.ended)
            .finish()
    }
}

impl TwilioMediaStreamAdapter {
    pub fn from_envelopes(
        provider_call_id: impl Into<String>,
        envelopes: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Self> {
        let provider_call_id = provider_call_id.into();
        let mut frames = Vec::new();
        let mut control_events = Vec::new();

        for envelope in envelopes {
            match translate_media_stream_json(envelope.as_ref())? {
                TwilioTranslatedEvent::Media(frame) => frames.push(frame),
                TwilioTranslatedEvent::Control(event) => control_events.push(event),
            }
        }

        Ok(Self {
            provider_call_id,
            frames,
            control_events,
            dropped_on_close: None,
            ended: None,
        })
    }

    pub fn dropped_on_close(mut self, reason: impl Into<String>) -> Self {
        self.dropped_on_close = Some(reason.into());
        self
    }
}

#[async_trait]
impl CallTransport for TwilioMediaStreamAdapter {
    fn adapter_name(&self) -> &str {
        TWILIO_PROVIDER
    }

    fn provider_call_id(&self) -> &str {
        &self.provider_call_id
    }

    fn frames(&mut self) -> MediaFrameStream {
        Box::pin(stream::iter(self.frames.clone()))
    }

    fn control_events(&mut self) -> CallControlEventStream {
        let mut events = self.control_events.clone();
        if let Some(reason) = &self.dropped_on_close {
            events.push(CallControlEvent::Dropped {
                reason: reason.clone(),
            });
        } else if let Some(reason) = &self.ended {
            events.push(CallControlEvent::StateChanged {
                state: CallState::Ended {
                    reason: reason.clone(),
                },
            });
        }
        Box::pin(stream::iter(events))
    }

    async fn end_call(&mut self, reason: EndReason) -> Result<()> {
        self.ended = Some(reason);
        Ok(())
    }
}

#[derive(Deserialize)]
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
    #[serde(default, rename = "ConsentConfirmed")]
    consent_confirmed: Option<String>,
    #[serde(default, rename = "RecordingConsent")]
    recording_consent: Option<String>,
    #[serde(default, rename = "DisclosurePlayed")]
    disclosure_played: Option<String>,
    #[serde(default, rename = "DisclosureVersion")]
    disclosure_version: Option<String>,
}

impl fmt::Debug for TwilioVoiceWebhookForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioVoiceWebhookForm")
            .field("call_sid_len", &self.call_sid.len())
            .field(
                "call_status_len",
                &self.call_status.as_ref().map(String::len),
            )
            .field("from_len", &self.from.as_ref().map(String::len))
            .field("to_len", &self.to.as_ref().map(String::len))
            .field("direction_len", &self.direction.as_ref().map(String::len))
            .field(
                "recording_sid_len",
                &self._recording_sid.as_ref().map(String::len),
            )
            .field(
                "recording_url_len",
                &self.recording_url.as_ref().map(String::len),
            )
            .field(
                "recording_status_len",
                &self.recording_status.as_ref().map(String::len),
            )
            .field(
                "consent_confirmed_len",
                &self.consent_confirmed.as_ref().map(String::len),
            )
            .field(
                "recording_consent_len",
                &self.recording_consent.as_ref().map(String::len),
            )
            .field(
                "disclosure_played_len",
                &self.disclosure_played.as_ref().map(String::len),
            )
            .field(
                "disclosure_version_len",
                &self.disclosure_version.as_ref().map(String::len),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TwilioVoiceWebhookEvent {
    pub provider_call_id: String,
    pub control_event: CallControlEvent,
}

impl fmt::Debug for TwilioVoiceWebhookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioVoiceWebhookEvent")
            .field("provider_call_id_len", &self.provider_call_id.len())
            .field(
                "control_event_class",
                &twilio_control_event_class(&self.control_event),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TwilioCallEventOutbox {
    pub event_type: &'static str,
    pub payload: Value,
}

impl fmt::Debug for TwilioCallEventOutbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwilioCallEventOutbox")
            .field("event_type", &self.event_type)
            .field("payload_class", &twilio_json_class(&self.payload))
            .field("payload_len", &self.payload.to_string().len())
            .finish()
    }
}

fn twilio_control_event_class(event: &CallControlEvent) -> &'static str {
    match event {
        CallControlEvent::CallStarted { .. } => "call_started",
        CallControlEvent::StateChanged { state } => match state {
            CallState::Ringing => "state_ringing",
            CallState::EarlyMedia => "state_early_media",
            CallState::Active => "state_active",
            CallState::OnHold => "state_on_hold",
            CallState::Ended { .. } => "state_ended",
        },
        CallControlEvent::DtmfDigit { .. } => "dtmf_digit",
        CallControlEvent::RecordingAvailable { .. } => "recording_available",
        CallControlEvent::Dropped { .. } => "dropped",
        CallControlEvent::Custom { .. } => "custom",
    }
}

fn twilio_json_class(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn twilio_provider_call_reference_metadata(provider_call_id: &str) -> Value {
    serde_json::json!({
        "provider_call_id_present": true,
        "provider_call_id_len": twilio_text_len(provider_call_id),
    })
}

fn twilio_text_len(value: &str) -> usize {
    value.chars().count()
}

fn twilio_url_class(raw: &str) -> &'static str {
    let lower = raw.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return "invalid_url";
    }
    let host = lower
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default()
        .split('/')
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    if host.contains("twilio.com") {
        "twilio_api"
    } else if host == "localhost"
        || host.ends_with(".local")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.20.")
        || host.starts_with("172.21.")
        || host.starts_with("172.22.")
        || host.starts_with("172.23.")
        || host.starts_with("172.24.")
        || host.starts_with("172.25.")
        || host.starts_with("172.26.")
        || host.starts_with("172.27.")
        || host.starts_with("172.28.")
        || host.starts_with("172.29.")
        || host.starts_with("172.30.")
        || host.starts_with("172.31.")
    {
        "local_or_private"
    } else {
        "external"
    }
}

pub fn translate_voice_webhook_form(input: &[u8]) -> Result<TwilioVoiceWebhookEvent> {
    let form: TwilioVoiceWebhookForm = serde_urlencoded::from_bytes(input)
        .map_err(|err| Error::InvalidInput(format!("invalid Twilio webhook form: {err}")))?;
    form.into_event()
}

pub fn call_event_outbox_for_control_event(
    event: &CallControlEvent,
    duplicate: bool,
) -> Option<TwilioCallEventOutbox> {
    match event {
        CallControlEvent::CallStarted {
            provider,
            remote_party,
            metadata,
            ..
        } if provider == TWILIO_PROVIDER => {
            let mut payload = serde_json::json!({
                "remote_party_present": remote_party.is_some(),
                "remote_party_len": remote_party.as_deref().map(twilio_text_len),
                "metadata_class": twilio_json_class(metadata),
                "metadata_len": metadata.to_string().chars().count(),
            });
            if duplicate {
                payload["duplicate"] = serde_json::json!(true);
            }
            Some(TwilioCallEventOutbox {
                event_type: "call_started",
                payload,
            })
        }
        CallControlEvent::StateChanged {
            state: CallState::Active,
        } => Some(TwilioCallEventOutbox {
            event_type: "state_change",
            payload: serde_json::json!({"to": "active"}),
        }),
        CallControlEvent::StateChanged {
            state: CallState::Ended { reason },
        } => Some(TwilioCallEventOutbox {
            event_type: "ended",
            payload: serde_json::json!({"reason": end_reason_label(reason)}),
        }),
        CallControlEvent::RecordingAvailable { url } => Some(TwilioCallEventOutbox {
            event_type: "recording_available",
            payload: serde_json::json!({
                "url_class": twilio_url_class(url),
                "url_len": twilio_text_len(url),
            }),
        }),
        CallControlEvent::Dropped { .. } => Some(TwilioCallEventOutbox {
            event_type: "ended",
            payload: serde_json::json!({"reason": end_reason_label(&EndReason::Dropped)}),
        }),
        _ => None,
    }
}

pub fn end_reason_label(reason: &EndReason) -> &'static str {
    match reason {
        EndReason::NormalHangup => "normal_hangup",
        EndReason::Dropped => "dropped",
        EndReason::Failed => "failed",
        EndReason::Cancelled => "cancelled",
    }
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
            "ringing" => CallControlEvent::CallStarted {
                provider: TWILIO_PROVIDER.to_string(),
                provider_call_id: self.call_sid,
                remote_party: remote_party(&self.from, &self.to, &self.direction),
                metadata: serde_json::json!({
                    "consent_confirmed": consent_truthy(&self.consent_confirmed)
                        || consent_truthy(&self.recording_consent),
                    "disclosure_played": consent_truthy(&self.disclosure_played),
                    "disclosure_version": self.disclosure_version,
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

fn consent_truthy(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).is_some_and(|value| {
        value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
            || value.eq_ignore_ascii_case("confirmed")
            || value == "1"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::adapters::MockAdapter;
    use crate::realtime::asr::{
        AsrSessionConfig, MockAsrBackend, StreamingASRBackend, TranscriptEvent,
    };
    use crate::realtime::codec::normalize_frame_to_pcm16k;
    use futures::StreamExt;

    async fn run_transport_through_mock_asr(
        transport: &mut dyn CallTransport,
    ) -> Vec<TranscriptEvent> {
        let frames: Vec<_> = transport.frames().collect().await;
        assert!(!frames.is_empty());

        let backend = MockAsrBackend::default();
        let mut session = backend
            .start_session(AsrSessionConfig {
                sample_rate_hz: 16_000,
                language: Some("en".to_string()),
                metadata: serde_json::json!({
                    "provider": transport.adapter_name(),
                    "provider_call_id": transport.provider_call_id(),
                }),
            })
            .await
            .unwrap();

        for frame in frames {
            let pcm = normalize_frame_to_pcm16k(&frame).unwrap();
            session.push_pcm16k(&pcm).await.unwrap();
        }
        session.close().await.unwrap();
        session.events().take(1).collect().await
    }

    #[test]
    fn twilio_wire_debug_redacts_provider_payloads_and_call_metadata() {
        let start: TwilioMediaEnvelope = serde_json::from_str(
            r#"{"event":"start","sequenceNumber":"seq-secret-1","start":{"callSid":"CAcustomer@example.com"}}"#,
        )
        .unwrap();
        let media: TwilioMediaEnvelope = serde_json::from_str(
            r#"{"event":"media","sequenceNumber":"seq-secret-2","media":{"payload":"c2stbGl2ZS10d2lsaW8tcGF5bG9hZA==","timestamp":"160-private","chunk":"chunk-mm_key"}}"#,
        )
        .unwrap();
        let stop: TwilioMediaEnvelope = serde_json::from_str(
            r#"{"event":"stop","sequenceNumber":"seq-secret-3","stop":{"callSid":"CApostgres://user:pass@db.internal/app"}}"#,
        )
        .unwrap();
        let mark: TwilioMediaEnvelope = serde_json::from_str(
            r#"{"event":"mark","sequenceNumber":"seq-secret-4","mark":{"name":"/srv/private/twilio-mark"}}"#,
        )
        .unwrap();
        let dtmf: TwilioMediaEnvelope = serde_json::from_str(
            r##"{"event":"dtmf","sequenceNumber":"seq-secret-5","dtmf":{"digit":"#"}}"##,
        )
        .unwrap();
        let webhook = TwilioVoiceWebhookForm {
            call_sid: "CAprivate-call".to_string(),
            call_status: Some("ringing".to_string()),
            from: Some("+15551230000".to_string()),
            to: Some("+15559870000".to_string()),
            direction: Some("inbound".to_string()),
            _recording_sid: Some("REprivate-recording".to_string()),
            recording_url: Some(
                "https://api.twilio.com/recording.wav?token=sk-live-recording".to_string(),
            ),
            recording_status: Some("completed".to_string()),
            consent_confirmed: Some("true".to_string()),
            recording_consent: Some("confirmed".to_string()),
            disclosure_played: Some("1".to_string()),
            disclosure_version: Some("v2026-private".to_string()),
        };

        let combined = format!("{start:?}\n{media:?}\n{stop:?}\n{mark:?}\n{dtmf:?}\n{webhook:?}");

        assert!(combined.contains("TwilioMediaEnvelope::Start"));
        assert!(combined.contains("call_sid_len"));
        assert!(combined.contains("payload_len"));
        assert!(combined.contains("recording_url_len"));
        assert!(combined.contains("sequence_number_len"));

        for raw in [
            "seq-secret",
            "CAcustomer",
            "customer@example.com",
            "c2stbGl2ZS10d2lsaW8tcGF5bG9hZA",
            "160-private",
            "chunk-mm_key",
            "postgres://user:pass",
            "db.internal",
            "/srv/private",
            "twilio-mark",
            "+15551230000",
            "+15559870000",
            "REprivate-recording",
            "api.twilio.com",
            "sk-live-recording",
            "v2026-private",
        ] {
            assert!(!combined.contains(raw), "raw value leaked: {raw}");
        }
    }

    #[test]
    fn twilio_translated_debug_redacts_provider_payloads_and_call_metadata() {
        let media = translate_media_stream_json(
            r#"{"event":"media","sequenceNumber":"7","media":{"payload":"c2stdG9rZW4tcGF5bG9hZA==","timestamp":"160","chunk":"3"}}"#,
        )
        .unwrap();
        let started = translate_voice_webhook_form(
            b"CallSid=CAcustomer@example.com&CallStatus=ringing&From=%2B15551230000&RecordingUrl=https%3A%2F%2Fapi.twilio.com%2Frecording.wav%3Ftoken%3Dsk-live-recording&ConsentConfirmed=true",
        )
        .unwrap();
        let outbox = call_event_outbox_for_control_event(&started.control_event, false).unwrap();
        let adapter = TwilioMediaStreamAdapter::from_envelopes(
            "CApostgres://user:pass@db.internal/app",
            [
                r#"{"event":"start","sequenceNumber":"1","start":{"callSid":"CAprivate-start"}}"#,
                r#"{"event":"media","sequenceNumber":"2","media":{"payload":"/////w==","timestamp":"160","chunk":"1"}}"#,
            ],
        )
        .unwrap()
        .dropped_on_close("socket closed for +15559870000 sk-live-drop");

        let combined = format!("{media:?}\n{started:?}\n{outbox:?}\n{adapter:?}");

        assert!(combined.contains("payload_len"));
        assert!(combined.contains("provider_call_id_len"));
        assert!(combined.contains("control_event_class"));
        assert!(combined.contains("payload_class"));
        assert!(combined.contains("dropped_on_close_len"));
        assert!(combined.contains("call_started"));

        for raw in [
            "c2stdG9rZW4tcGF5bG9hZA",
            "CAcustomer",
            "customer@example.com",
            "+15551230000",
            "api.twilio.com",
            "sk-live-recording",
            "postgres://user:pass",
            "db.internal",
            "CAprivate-start",
            "+15559870000",
            "sk-live-drop",
            "remote_party",
            "consent_confirmed",
        ] {
            assert!(!combined.contains(raw), "raw value leaked: {raw}");
        }
    }

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
    fn stop_envelope_redacts_provider_call_id_in_custom_payload() {
        let provider_call_id = "CA-secret-provider-call-id";
        let envelope = format!(
            r#"{{"event":"stop","sequenceNumber":"9","stop":{{"callSid":"{provider_call_id}"}}}}"#
        );
        let event = translate_media_stream_json(&envelope).unwrap();
        let payload = match event {
            TwilioTranslatedEvent::Control(CallControlEvent::Custom {
                event_type,
                payload,
            }) => {
                assert_eq!(event_type, "call_media_stopped");
                payload
            }
            other => panic!("unexpected translated event: {other:?}"),
        };

        assert_eq!(payload["provider_call_id_present"], true);
        assert_eq!(
            payload["provider_call_id_len"],
            provider_call_id.chars().count()
        );
        let rendered = serde_json::to_string(&payload).expect("serialize stop payload");
        assert!(!rendered.contains(provider_call_id));
        assert!(!rendered.contains("secret-provider"));
    }

    #[tokio::test]
    async fn fixture_adapter_runs_trait_backed_asr_pipeline() {
        let mut adapter = TwilioMediaStreamAdapter::from_envelopes(
            "CA123",
            [
                r#"{"event":"start","sequenceNumber":"1","start":{"callSid":"CA123"}}"#,
                r#"{"event":"media","sequenceNumber":"2","media":{"payload":"/////w==","timestamp":"160","chunk":"1"}}"#,
                r##"{"event":"dtmf","sequenceNumber":"3","dtmf":{"digit":"#"}}"##,
            ],
        )
        .unwrap();

        assert_eq!(adapter.adapter_name(), "twilio");
        assert_eq!(adapter.provider_call_id(), "CA123");

        let events: Vec<_> = adapter.control_events().collect().await;
        assert!(matches!(
            events.first(),
            Some(CallControlEvent::CallStarted {
                provider,
                provider_call_id,
                ..
            }) if provider == "twilio" && provider_call_id == "CA123"
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, CallControlEvent::DtmfDigit { digit: '#' })));

        let transcript_events = run_transport_through_mock_asr(&mut adapter).await;
        assert!(matches!(
            transcript_events.first(),
            Some(TranscriptEvent::Final { text, .. }) if text == "mock transcript"
        ));
    }

    #[tokio::test]
    async fn mock_and_twilio_transports_share_the_same_normalized_asr_contract() {
        let mut mock = MockAdapter::builder().sine_wave(440.0, 20).build();
        let mut twilio = TwilioMediaStreamAdapter::from_envelopes(
            "CA456",
            [r#"{"event":"media","sequenceNumber":"1","media":{"payload":"/////w==","timestamp":"0","chunk":"1"}}"#],
        )
        .unwrap();

        let mock_events = run_transport_through_mock_asr(&mut mock).await;
        let twilio_events = run_transport_through_mock_asr(&mut twilio).await;

        assert_eq!(
            mock_events
                .iter()
                .filter(|event| matches!(event, TranscriptEvent::Final { .. }))
                .count(),
            1
        );
        assert_eq!(
            twilio_events
                .iter()
                .filter(|event| matches!(event, TranscriptEvent::Final { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn fixture_adapter_can_emit_dropped_on_socket_close() {
        let mut adapter = TwilioMediaStreamAdapter::from_envelopes(
            "CA789",
            [r#"{"event":"media","sequenceNumber":"1","media":{"payload":"/////w==","timestamp":"0","chunk":"1"}}"#],
        )
        .unwrap()
        .dropped_on_close("fixture socket closed");

        let events: Vec<_> = adapter.control_events().collect().await;
        assert!(events.iter().any(|event| matches!(
            event,
            CallControlEvent::Dropped { reason } if reason == "fixture socket closed"
        )));
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
            CallControlEvent::CallStarted {
                ref provider,
                ref provider_call_id,
                ref remote_party,
                ..
            } if provider == "twilio"
                && provider_call_id == "CA123"
                && remote_party.as_deref() == Some("+15551230000")
        ));
    }

    #[test]
    fn ringing_webhook_preserves_consent_confirmation_metadata() {
        let event = translate_voice_webhook_form(
            b"CallSid=CA123&CallStatus=ringing&ConsentConfirmed=true&DisclosurePlayed=1&DisclosureVersion=v2026-05",
        )
        .unwrap();

        assert!(matches!(
            event.control_event,
            CallControlEvent::CallStarted { ref metadata, .. }
                if metadata["consent_confirmed"] == true
                    && metadata["disclosure_played"] == true
                    && metadata["disclosure_version"] == "v2026-05"
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

    #[test]
    fn webhook_control_events_map_to_call_event_outbox_contract() {
        let started = translate_voice_webhook_form(
            b"CallSid=CA123&CallStatus=ringing&From=%2B15551230000&ConsentConfirmed=true",
        )
        .unwrap();
        let started_outbox =
            call_event_outbox_for_control_event(&started.control_event, false).unwrap();
        assert_eq!(started_outbox.event_type, "call_started");
        assert_eq!(started_outbox.payload["remote_party_present"], true);
        assert_eq!(started_outbox.payload["remote_party_len"], 12);
        assert_eq!(started_outbox.payload["metadata_class"], "object");
        assert!(started_outbox.payload["metadata_len"].as_u64().unwrap() > 0);
        let started_payload =
            serde_json::to_string(&started_outbox.payload).expect("serialize started payload");
        assert!(!started_payload.contains("+15551230000"));
        assert!(!started_payload.contains("consent_confirmed"));

        let duplicate_started =
            call_event_outbox_for_control_event(&started.control_event, true).unwrap();
        assert_eq!(duplicate_started.payload["duplicate"], true);

        let active = translate_voice_webhook_form(b"CallSid=CA123&CallStatus=in-progress").unwrap();
        let active_outbox = call_event_outbox_for_control_event(&active.control_event, false)
            .expect("active outbox event");
        assert_eq!(active_outbox.event_type, "state_change");
        assert_eq!(active_outbox.payload["to"], "active");

        let completed =
            translate_voice_webhook_form(b"CallSid=CA123&CallStatus=completed").unwrap();
        let completed_outbox =
            call_event_outbox_for_control_event(&completed.control_event, false).unwrap();
        assert_eq!(completed_outbox.event_type, "ended");
        assert_eq!(completed_outbox.payload["reason"], "normal_hangup");

        let recording = translate_voice_webhook_form(
            b"CallSid=CA123&RecordingStatus=completed&RecordingSid=RE123&RecordingUrl=https%3A%2F%2Fapi.twilio.com%2Frecording.wav%3Ftoken%3Dsk-live-recording",
        )
        .unwrap();
        let recording_outbox =
            call_event_outbox_for_control_event(&recording.control_event, false).unwrap();
        assert_eq!(recording_outbox.event_type, "recording_available");
        assert_eq!(recording_outbox.payload["url_class"], "twilio_api");
        assert!(recording_outbox.payload["url_len"].as_u64().unwrap() > 0);
        let recording_payload =
            serde_json::to_string(&recording_outbox.payload).expect("serialize recording payload");
        assert!(!recording_payload.contains("api.twilio.com"));
        assert!(!recording_payload.contains("recording.wav"));
        assert!(!recording_payload.contains("sk-live-recording"));
    }
}

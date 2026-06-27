//! Pure in-process mock call transport for integration tests.

use std::fmt;

use async_trait::async_trait;
use futures::stream;
use matric_core::Result;

use crate::realtime::{
    CallControlEvent, CallControlEventStream, CallTransport, Codec, EndReason, MediaFrame,
    MediaFrameStream,
};

fn mock_text_len(value: &str) -> usize {
    value.chars().count()
}

#[derive(Clone)]
pub struct MockAdapter {
    provider_call_id: String,
    frames: Vec<MediaFrame>,
    dtmf: Vec<char>,
    dropped_after_frames: Option<usize>,
    ended: Option<EndReason>,
}

impl fmt::Debug for MockAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockAdapter")
            .field(
                "provider_call_id_len",
                &mock_text_len(&self.provider_call_id),
            )
            .field("frames_count", &self.frames.len())
            .field(
                "frames_payload_bytes",
                &self
                    .frames
                    .iter()
                    .map(|frame| frame.payload.len())
                    .sum::<usize>(),
            )
            .field("dtmf_count", &self.dtmf.len())
            .field("dropped_after_frames", &self.dropped_after_frames)
            .field("ended", &self.ended)
            .finish()
    }
}

#[derive(Clone)]
pub struct MockAdapterBuilder {
    provider_call_id: String,
    codec: Codec,
    frames: Vec<MediaFrame>,
    dtmf: Vec<char>,
    drop_frame: Option<u32>,
    dropped_after_frames: Option<usize>,
    codec_mismatch: Option<Codec>,
    seed: u64,
}

impl fmt::Debug for MockAdapterBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockAdapterBuilder")
            .field(
                "provider_call_id_len",
                &mock_text_len(&self.provider_call_id),
            )
            .field("codec", &self.codec)
            .field("frames_count", &self.frames.len())
            .field(
                "frames_payload_bytes",
                &self
                    .frames
                    .iter()
                    .map(|frame| frame.payload.len())
                    .sum::<usize>(),
            )
            .field("dtmf_count", &self.dtmf.len())
            .field("drop_frame_set", &self.drop_frame.is_some())
            .field("dropped_after_frames", &self.dropped_after_frames)
            .field("codec_mismatch", &self.codec_mismatch)
            .field("seed_set", &true)
            .finish()
    }
}

impl Default for MockAdapterBuilder {
    fn default() -> Self {
        Self {
            provider_call_id: "mock-call-1".to_string(),
            codec: Codec::L16 {
                sample_rate: 16_000,
                channels: 1,
            },
            frames: Vec::new(),
            dtmf: Vec::new(),
            drop_frame: None,
            dropped_after_frames: None,
            codec_mismatch: None,
            seed: 0x5eed,
        }
    }
}

impl MockAdapter {
    pub fn builder() -> MockAdapterBuilder {
        MockAdapterBuilder::default()
    }
}

impl MockAdapterBuilder {
    pub fn provider_call_id(mut self, provider_call_id: impl Into<String>) -> Self {
        self.provider_call_id = provider_call_id.into();
        self
    }

    pub fn codec(mut self, codec: Codec) -> Self {
        self.codec = codec;
        self
    }

    pub fn frames(mut self, frames: Vec<MediaFrame>) -> Self {
        self.frames = frames;
        self
    }

    pub fn fixture_payload(mut self, payload: &[u8], frame_bytes: usize) -> Self {
        let frame_bytes = frame_bytes.max(1);
        self.frames = payload
            .chunks(frame_bytes)
            .enumerate()
            .map(|(idx, chunk)| MediaFrame {
                codec: self.codec.clone(),
                timestamp_rtp: idx as u32 * 160,
                sequence: idx as u32,
                marker: idx == 0,
                payload: chunk.to_vec(),
            })
            .collect();
        self
    }

    pub fn sine_wave(mut self, frequency_hz: f32, duration_ms: u32) -> Self {
        let sample_rate = 16_000_u32;
        let total_samples = ((u64::from(sample_rate) * u64::from(duration_ms)) / 1_000) as usize;
        let mut payload = Vec::with_capacity(total_samples * 2);
        for idx in 0..total_samples {
            let phase = idx as f32 * frequency_hz * std::f32::consts::TAU / sample_rate as f32;
            let jitter = (next_lcg(&mut self.seed) & 0x7) as i16 - 3;
            let sample = (phase.sin() * 12_000.0) as i16 + jitter;
            payload.extend_from_slice(&sample.to_be_bytes());
        }
        self.codec = Codec::L16 {
            sample_rate,
            channels: 1,
        };
        self.fixture_payload(&payload, 320)
    }

    pub fn dtmf_sequence(mut self, digits: impl IntoIterator<Item = char>) -> Self {
        self.dtmf = digits.into_iter().collect();
        self
    }

    pub fn drop_frame(mut self, sequence: u32) -> Self {
        self.drop_frame = Some(sequence);
        self
    }

    pub fn dropped_after_frames(mut self, frame_count: usize) -> Self {
        self.dropped_after_frames = Some(frame_count);
        self
    }

    pub fn codec_mismatch(mut self, codec: Codec) -> Self {
        self.codec_mismatch = Some(codec);
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    pub fn build(self) -> MockAdapter {
        let mut frames = if self.frames.is_empty() {
            MockAdapterBuilder { ..self.clone() }
                .sine_wave(440.0, 20)
                .frames
        } else {
            self.frames.clone()
        };

        if let Some(codec) = self.codec_mismatch.clone() {
            if let Some(frame) = frames.first_mut() {
                frame.codec = codec;
            }
        }
        if let Some(sequence) = self.drop_frame {
            frames.retain(|frame| frame.sequence != sequence);
        }

        MockAdapter {
            provider_call_id: self.provider_call_id,
            frames,
            dtmf: self.dtmf,
            dropped_after_frames: self.dropped_after_frames,
            ended: None,
        }
    }
}

#[async_trait]
impl CallTransport for MockAdapter {
    fn adapter_name(&self) -> &str {
        "mock"
    }

    fn provider_call_id(&self) -> &str {
        &self.provider_call_id
    }

    fn frames(&mut self) -> MediaFrameStream {
        let frames = match self.dropped_after_frames {
            Some(limit) => self.frames.iter().take(limit).cloned().collect(),
            None => self.frames.clone(),
        };
        Box::pin(stream::iter(frames))
    }

    fn control_events(&mut self) -> CallControlEventStream {
        let mut events = vec![CallControlEvent::CallStarted {
            provider: self.adapter_name().to_string(),
            provider_call_id: self.provider_call_id.clone(),
            remote_party: None,
            metadata: serde_json::Value::Null,
        }];
        events.extend(
            self.dtmf
                .iter()
                .copied()
                .map(|digit| CallControlEvent::DtmfDigit { digit }),
        );
        if self.dropped_after_frames.is_some() {
            events.push(CallControlEvent::Dropped {
                reason: "mock dropped after configured duration".to_string(),
            });
        } else {
            events.push(CallControlEvent::StateChanged {
                state: crate::realtime::CallState::Ended {
                    reason: self.ended.clone().unwrap_or(EndReason::NormalHangup),
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

fn next_lcg(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::codec::normalize_frame_to_pcm16k;
    use futures::StreamExt;

    #[tokio::test]
    async fn mock_adapter_lifecycle_and_pcm_demo() {
        let mut adapter = MockAdapter::builder()
            .provider_call_id("call-demo")
            .sine_wave(440.0, 40)
            .dtmf_sequence(['1', '2', '#'])
            .seed(42)
            .build();

        let frames: Vec<_> = adapter.frames().collect().await;
        assert!(!frames.is_empty());
        let pcm = normalize_frame_to_pcm16k(&frames[0]).unwrap();
        assert!(!pcm.is_empty());

        let events: Vec<_> = adapter.control_events().collect().await;
        assert!(matches!(
            events.first(),
            Some(CallControlEvent::CallStarted { .. })
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, CallControlEvent::DtmfDigit { digit: '#' })));
    }

    #[tokio::test]
    async fn failure_injection_can_drop_frames_and_emit_dropped_event() {
        let mut adapter = MockAdapter::builder()
            .sine_wave(440.0, 60)
            .drop_frame(1)
            .dropped_after_frames(1)
            .codec_mismatch(Codec::Telephone { event_code: 4 })
            .build();

        let frames: Vec<_> = adapter.frames().collect().await;
        assert_eq!(frames.len(), 1);
        assert!(matches!(frames[0].codec, Codec::Telephone { .. }));

        let events: Vec<_> = adapter.control_events().collect().await;
        assert!(events
            .iter()
            .any(|event| matches!(event, CallControlEvent::Dropped { .. })));
    }

    #[test]
    fn mock_adapter_debug_redacts_fixture_identifiers_payloads_and_dtmf() {
        let provider_call_id = "CApostgres://user:pass@db.internal/app-東京";
        let builder = MockAdapter::builder()
            .provider_call_id(provider_call_id)
            .fixture_payload(b"audio payload with sk-live-secret-token", 8)
            .dtmf_sequence(['4', '2', '#'])
            .drop_frame(1)
            .dropped_after_frames(2)
            .codec_mismatch(Codec::Telephone { event_code: 7 })
            .seed(12_345);
        let adapter = builder.clone().build();

        let rendered = format!("{builder:?}\n{adapter:?}");

        for raw in [
            provider_call_id,
            "東京",
            "audio payload",
            "sk-live-secret-token",
            "'4'",
            "'2'",
            "'#'",
            "12345",
        ] {
            assert!(
                !rendered.contains(raw),
                "Mock adapter Debug output leaked raw value {raw:?}: {rendered}"
            );
        }

        for expected in [
            "provider_call_id_len",
            "frames_count",
            "frames_payload_bytes",
            "dtmf_count",
            "drop_frame_set",
            "seed_set",
        ] {
            assert!(
                rendered.contains(expected),
                "Mock adapter Debug output should retain safe metadata field {expected:?}: {rendered}"
            );
        }

        assert!(
            rendered.contains("provider_call_id_len: 41"),
            "Mock adapter Debug output should report provider call id character length, not bytes: {rendered}"
        );
    }
}

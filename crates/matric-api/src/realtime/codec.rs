//! Codec normalization for real-time ASR ingress.
//!
//! ADR-RTP-003 expects ASR backends to receive mono signed PCM at 16 kHz.
//! Provider adapters keep their native media payloads until this boundary so
//! downstream ASR code can stay provider-agnostic.
//!
//! Opus selection rationale: production Opus packet decoding should use the
//! `opus` crate because it is the maintained Rust binding to libopus and maps
//! directly to RFC 6716 frame semantics. This M1.4 normalizer keeps CI native
//! dependency-free by accepting deterministic PCM fixture payloads for Opus; the
//! provider adapter that first ingests real Opus packets should link `opus` at
//! that boundary and feed decoded PCM here.

use super::{Codec, MediaFrame};
use matric_core::{Error, Result};

pub const TARGET_SAMPLE_RATE_HZ: u32 = 16_000;

/// Normalize a provider media frame into mono signed 16 kHz PCM samples.
pub fn normalize_frame_to_pcm16k(frame: &MediaFrame) -> Result<Vec<i16>> {
    match &frame.codec {
        Codec::PcmuG711 { sample_rate } => {
            let pcm: Vec<i16> = frame.payload.iter().map(|b| decode_mulaw(*b)).collect();
            Ok(resample_linear(&pcm, *sample_rate, TARGET_SAMPLE_RATE_HZ))
        }
        Codec::PcmaG711 { sample_rate } => {
            let pcm: Vec<i16> = frame.payload.iter().map(|b| decode_alaw(*b)).collect();
            Ok(resample_linear(&pcm, *sample_rate, TARGET_SAMPLE_RATE_HZ))
        }
        Codec::L16 {
            sample_rate,
            channels,
        } => {
            let pcm = decode_l16_be(&frame.payload, *channels)?;
            Ok(resample_linear(&pcm, *sample_rate, TARGET_SAMPLE_RATE_HZ))
        }
        Codec::Opus {
            sample_rate,
            channels,
        } => {
            let pcm = decode_fixture_pcm_le(&frame.payload, *channels)?;
            Ok(resample_linear(&pcm, *sample_rate, TARGET_SAMPLE_RATE_HZ))
        }
        Codec::Telephone { .. } => Ok(Vec::new()),
    }
}

fn decode_fixture_pcm_le(payload: &[u8], channels: u8) -> Result<Vec<i16>> {
    if !payload.len().is_multiple_of(2) {
        return Err(Error::InvalidInput(
            "PCM fixture payload must contain whole i16 samples".to_string(),
        ));
    }
    let samples: Vec<i16> = payload
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    Ok(downmix_mono(&samples, channels))
}

fn decode_l16_be(payload: &[u8], channels: u8) -> Result<Vec<i16>> {
    if !payload.len().is_multiple_of(2) {
        return Err(Error::InvalidInput(
            "L16 payload must contain whole i16 samples".to_string(),
        ));
    }
    let samples: Vec<i16> = payload
        .chunks_exact(2)
        .map(|chunk| i16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();
    Ok(downmix_mono(&samples, channels))
}

fn downmix_mono(samples: &[i16], channels: u8) -> Vec<i16> {
    let channels = usize::from(channels.max(1));
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| {
            let sum: i32 = frame.iter().map(|sample| i32::from(*sample)).sum();
            (sum / i32::try_from(frame.len()).unwrap_or(1)) as i16
        })
        .collect()
}

fn resample_linear(samples: &[i16], source_rate: u32, target_rate: u32) -> Vec<i16> {
    if samples.is_empty() || source_rate == 0 {
        return Vec::new();
    }
    if source_rate == target_rate {
        return samples.to_vec();
    }

    let output_len =
        ((samples.len() as u64 * u64::from(target_rate)) / u64::from(source_rate)).max(1) as usize;
    let ratio = source_rate as f64 / target_rate as f64;
    (0..output_len)
        .map(|idx| {
            let source_pos = idx as f64 * ratio;
            let base = source_pos.floor() as usize;
            let frac = source_pos - base as f64;
            let a = samples.get(base).copied().unwrap_or(0) as f64;
            let b = samples.get(base + 1).copied().unwrap_or(samples[base]) as f64;
            (a + (b - a) * frac)
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64) as i16
        })
        .collect()
}

fn decode_mulaw(byte: u8) -> i16 {
    let u = !byte;
    let sign = u & 0x80;
    let exponent = (u >> 4) & 0x07;
    let mantissa = u & 0x0f;
    let sample = (((i32::from(mantissa) << 3) + 0x84) << u32::from(exponent)) - 0x84;
    if sign != 0 {
        -sample as i16
    } else {
        sample as i16
    }
}

fn decode_alaw(byte: u8) -> i16 {
    let a = byte ^ 0x55;
    let sign = a & 0x80;
    let exponent = (a >> 4) & 0x07;
    let mantissa = a & 0x0f;
    let sample = if exponent == 0 {
        (i32::from(mantissa) << 4) + 8
    } else {
        ((i32::from(mantissa) << 4) + 0x108) << u32::from(exponent - 1)
    };
    if sign != 0 {
        sample as i16
    } else {
        -sample as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcmu_fixture_normalizes_to_16khz() {
        let frame = MediaFrame {
            codec: Codec::PcmuG711 { sample_rate: 8_000 },
            timestamp_rtp: 0,
            sequence: 0,
            marker: true,
            payload: include_bytes!("../../tests/fixtures/realtime/mock-mulaw-8k.bin").to_vec(),
        };
        let pcm = normalize_frame_to_pcm16k(&frame).unwrap();
        assert_eq!(pcm.len(), frame.payload.len() * 2);
        assert!(pcm.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn pcma_path_normalizes_to_16khz() {
        let frame = MediaFrame {
            codec: Codec::PcmaG711 { sample_rate: 8_000 },
            timestamp_rtp: 0,
            sequence: 0,
            marker: true,
            payload: vec![0xd5, 0xc5, 0xf5, 0xe5],
        };
        assert_eq!(normalize_frame_to_pcm16k(&frame).unwrap().len(), 8);
    }

    #[test]
    fn l16_path_downmixes_and_resamples() {
        let frame = MediaFrame {
            codec: Codec::L16 {
                sample_rate: 8_000,
                channels: 2,
            },
            timestamp_rtp: 0,
            sequence: 0,
            marker: true,
            payload: vec![0, 10, 0, 30, 0, 20, 0, 40],
        };
        assert_eq!(
            normalize_frame_to_pcm16k(&frame).unwrap(),
            vec![20, 25, 30, 30]
        );
    }

    #[test]
    fn opus_fixture_path_uses_pcm_payload() {
        let frame = MediaFrame {
            codec: Codec::Opus {
                sample_rate: 48_000,
                channels: 1,
            },
            timestamp_rtp: 0,
            sequence: 0,
            marker: true,
            payload: include_bytes!("../../tests/fixtures/realtime/mock-opus-48k.bin").to_vec(),
        };
        assert!(!normalize_frame_to_pcm16k(&frame).unwrap().is_empty());
    }

    #[test]
    fn telephone_events_are_ignored() {
        let frame = MediaFrame {
            codec: Codec::Telephone { event_code: 1 },
            timestamp_rtp: 0,
            sequence: 0,
            marker: false,
            payload: vec![1, 2, 3, 4],
        };
        assert!(normalize_frame_to_pcm16k(&frame).unwrap().is_empty());
    }
}

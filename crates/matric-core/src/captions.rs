//! Caption/subtitle format rendering from transcription segments.
//!
//! Supports:
//! - **WebVTT** (W3C standard, `.vtt`) — used by HTML5 `<track>` elements
//! - **SRT** (SubRip, `.srt`) — widely supported by media players
//! - **RTTM** (Rich Transcription Time Marked, `.rttm`) — NIST standard for diarization
//!
//! All renderers accept a slice of `CaptionSegment` and produce a complete file string.

use std::fmt::{self, Write};

fn debug_len(value: &str) -> usize {
    value.chars().count()
}

fn optional_debug_len(value: Option<&String>) -> Option<usize> {
    value.map(|value| debug_len(value))
}

/// A timestamped text segment for caption rendering.
///
/// This is format-agnostic — convert from transcription segments, whisper output, etc.
#[derive(Clone)]
pub struct CaptionSegment {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
    /// Optional speaker label (for diarization / RTTM).
    pub speaker: Option<String>,
}

impl fmt::Debug for CaptionSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CaptionSegment")
            .field("start_secs_set", &self.start_secs.is_finite())
            .field("end_secs_set", &self.end_secs.is_finite())
            .field("duration_secs", &(self.end_secs - self.start_secs))
            .field("text_len", &debug_len(&self.text))
            .field("speaker_len", &optional_debug_len(self.speaker.as_ref()))
            .finish()
    }
}

/// Render segments as WebVTT (Web Video Text Tracks).
///
/// Format: <https://www.w3.org/TR/webvtt1/>
///
/// ```text
/// WEBVTT
///
/// 1
/// 00:00:00.000 --> 00:00:02.500
/// Hello, this is a test.
///
/// 2
/// 00:00:02.500 --> 00:00:05.000
/// A second line of text.
/// ```
pub fn render_webvtt(segments: &[CaptionSegment]) -> String {
    let mut out = String::with_capacity(segments.len() * 80);
    out.push_str("WEBVTT\n\n");

    for (i, seg) in segments.iter().enumerate() {
        let _ = writeln!(out, "{}", i + 1);
        let _ = writeln!(
            out,
            "{} --> {}",
            format_vtt_timestamp(seg.start_secs),
            format_vtt_timestamp(seg.end_secs)
        );
        // W3C WebVTT voice span: <v Speaker>text</v>
        if let Some(ref speaker) = seg.speaker {
            let _ = writeln!(out, "<v {}>{}  </v>", speaker, seg.text.trim());
        } else {
            let _ = writeln!(out, "{}", seg.text.trim());
        }
        out.push('\n');
    }

    out
}

/// Render segments as SRT (SubRip Subtitle).
///
/// Format: sequential index, `HH:MM:SS,mmm --> HH:MM:SS,mmm`, text, blank line.
/// When speaker labels are present, uses the common `Speaker: text` convention.
///
/// ```text
/// 1
/// 00:00:00,000 --> 00:00:02,500
/// Hello, this is a test.
///
/// 2
/// 00:00:02,500 --> 00:00:05,000
/// A second line of text.
/// ```
pub fn render_srt(segments: &[CaptionSegment]) -> String {
    let mut out = String::with_capacity(segments.len() * 80);

    for (i, seg) in segments.iter().enumerate() {
        let _ = writeln!(out, "{}", i + 1);
        let _ = writeln!(
            out,
            "{} --> {}",
            format_srt_timestamp(seg.start_secs),
            format_srt_timestamp(seg.end_secs)
        );
        // Common SRT convention: "Speaker: text"
        if let Some(ref speaker) = seg.speaker {
            let _ = writeln!(out, "{}: {}", speaker, seg.text.trim());
        } else {
            let _ = writeln!(out, "{}", seg.text.trim());
        }
        out.push('\n');
    }

    out
}

/// Render segments as RTTM (Rich Transcription Time Marked).
///
/// NIST format for speaker diarization evaluation.
/// Fields: `SPEAKER file channel start duration <NA> <NA> speaker <NA> <NA>`
///
/// ```text
/// SPEAKER file 1 0.000 2.500 <NA> <NA> speaker_0 <NA> <NA>
/// SPEAKER file 1 2.500 2.500 <NA> <NA> speaker_0 <NA> <NA>
/// ```
pub fn render_rttm(segments: &[CaptionSegment], file_id: &str) -> String {
    let mut out = String::with_capacity(segments.len() * 100);

    for seg in segments {
        let duration = seg.end_secs - seg.start_secs;
        let speaker = seg.speaker.as_deref().unwrap_or("speaker_0");
        let _ = writeln!(
            out,
            "SPEAKER {} 1 {:.3} {:.3} <NA> <NA> {} <NA> <NA>",
            file_id, seg.start_secs, duration, speaker
        );
    }

    out
}

/// Format seconds as WebVTT timestamp: `HH:MM:SS.mmm`
fn format_vtt_timestamp(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

/// Format seconds as SRT timestamp: `HH:MM:SS,mmm` (comma separator)
fn format_srt_timestamp(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_segments() -> Vec<CaptionSegment> {
        vec![
            CaptionSegment {
                start_secs: 0.0,
                end_secs: 2.5,
                text: "Hello, this is a test.".to_string(),
                speaker: None,
            },
            CaptionSegment {
                start_secs: 2.5,
                end_secs: 5.0,
                text: "A second line of text.".to_string(),
                speaker: None,
            },
            CaptionSegment {
                start_secs: 5.0,
                end_secs: 8.75,
                text: "And a third segment here.".to_string(),
                speaker: None,
            },
        ]
    }

    #[test]
    fn test_render_webvtt() {
        let segments = sample_segments();
        let vtt = render_webvtt(&segments);

        assert!(vtt.starts_with("WEBVTT\n\n"));
        assert!(vtt.contains("1\n00:00:00.000 --> 00:00:02.500\nHello, this is a test.\n"));
        assert!(vtt.contains("2\n00:00:02.500 --> 00:00:05.000\nA second line of text.\n"));
        assert!(vtt.contains("3\n00:00:05.000 --> 00:00:08.750\nAnd a third segment here.\n"));
    }

    #[test]
    fn test_render_srt() {
        let segments = sample_segments();
        let srt = render_srt(&segments);

        assert!(!srt.starts_with("WEBVTT"));
        assert!(srt.contains("1\n00:00:00,000 --> 00:00:02,500\nHello, this is a test.\n"));
        assert!(srt.contains("2\n00:00:02,500 --> 00:00:05,000\nA second line of text.\n"));
    }

    #[test]
    fn test_render_rttm() {
        let segments = vec![
            CaptionSegment {
                start_secs: 0.0,
                end_secs: 2.5,
                text: "Hello".to_string(),
                speaker: Some("alice".to_string()),
            },
            CaptionSegment {
                start_secs: 2.5,
                end_secs: 5.0,
                text: "World".to_string(),
                speaker: Some("bob".to_string()),
            },
        ];
        let rttm = render_rttm(&segments, "audio_file");

        assert!(rttm.contains("SPEAKER audio_file 1 0.000 2.500 <NA> <NA> alice <NA> <NA>"));
        assert!(rttm.contains("SPEAKER audio_file 1 2.500 2.500 <NA> <NA> bob <NA> <NA>"));
    }

    #[test]
    fn test_render_rttm_default_speaker() {
        let segments = vec![CaptionSegment {
            start_secs: 0.0,
            end_secs: 1.0,
            text: "Test".to_string(),
            speaker: None,
        }];
        let rttm = render_rttm(&segments, "file1");

        assert!(rttm.contains("speaker_0"));
    }

    #[test]
    fn test_render_empty_segments() {
        let empty: Vec<CaptionSegment> = vec![];
        assert_eq!(render_webvtt(&empty), "WEBVTT\n\n");
        assert_eq!(render_srt(&empty), "");
        assert_eq!(render_rttm(&empty, "file"), "");
    }

    #[test]
    fn caption_segment_debug_redacts_text_and_speaker() {
        let segment = CaptionSegment {
            start_secs: 1.25,
            end_secs: 3.75,
            text: "éé private@example.test sk-live-secret".to_string(),
            speaker: Some("ö丼".to_string()),
        };

        let rendered = format!("{segment:?}");

        for raw in ["éé", "private@example.test", "sk-live-secret", "ö丼"] {
            assert!(
                !rendered.contains(raw),
                "CaptionSegment Debug output leaked raw value {raw:?}: {rendered}"
            );
        }

        for expected in [
            "start_secs_set",
            "end_secs_set",
            "duration_secs",
            "text_len",
            "speaker_len",
            "speaker_len: Some(2)",
        ] {
            assert!(
                rendered.contains(expected),
                "CaptionSegment Debug output should retain safe metadata field {expected:?}: {rendered}"
            );
        }

        assert!(
            rendered.contains("text_len: 38"),
            "CaptionSegment Debug output should report Unicode character-count text length: {rendered}"
        );
    }

    #[test]
    fn test_vtt_timestamp_format() {
        assert_eq!(format_vtt_timestamp(0.0), "00:00:00.000");
        assert_eq!(format_vtt_timestamp(1.5), "00:00:01.500");
        assert_eq!(format_vtt_timestamp(61.123), "00:01:01.123");
        assert_eq!(format_vtt_timestamp(3661.0), "01:01:01.000");
    }

    #[test]
    fn test_render_webvtt_with_speakers() {
        let segments = vec![
            CaptionSegment {
                start_secs: 0.0,
                end_secs: 2.5,
                text: "Hello everyone.".to_string(),
                speaker: Some("Alice".to_string()),
            },
            CaptionSegment {
                start_secs: 2.5,
                end_secs: 5.0,
                text: "Hi Alice!".to_string(),
                speaker: Some("Bob".to_string()),
            },
            CaptionSegment {
                start_secs: 5.0,
                end_secs: 7.0,
                text: "No speaker here.".to_string(),
                speaker: None,
            },
        ];
        let vtt = render_webvtt(&segments);

        assert!(vtt.contains("<v Alice>Hello everyone.  </v>"));
        assert!(vtt.contains("<v Bob>Hi Alice!  </v>"));
        assert!(vtt.contains("No speaker here.\n"));
        // Ensure the no-speaker line doesn't have voice tags
        assert!(!vtt.contains("<v >No speaker here."));
    }

    #[test]
    fn test_render_srt_with_speakers() {
        let segments = vec![
            CaptionSegment {
                start_secs: 0.0,
                end_secs: 2.5,
                text: "Hello everyone.".to_string(),
                speaker: Some("Alice".to_string()),
            },
            CaptionSegment {
                start_secs: 2.5,
                end_secs: 5.0,
                text: "Hi Alice!".to_string(),
                speaker: Some("Bob".to_string()),
            },
            CaptionSegment {
                start_secs: 5.0,
                end_secs: 7.0,
                text: "No speaker here.".to_string(),
                speaker: None,
            },
        ];
        let srt = render_srt(&segments);

        assert!(srt.contains("Alice: Hello everyone.\n"));
        assert!(srt.contains("Bob: Hi Alice!\n"));
        assert!(srt.contains("No speaker here.\n"));
        // Ensure the no-speaker line doesn't have prefix
        assert!(!srt.contains(": No speaker here."));
    }

    #[test]
    fn test_srt_timestamp_format() {
        assert_eq!(format_srt_timestamp(0.0), "00:00:00,000");
        assert_eq!(format_srt_timestamp(1.5), "00:00:01,500");
        assert_eq!(format_srt_timestamp(3661.999), "01:01:01,999");
        assert_eq!(format_srt_timestamp(3662.0), "01:01:02,000");
    }
}

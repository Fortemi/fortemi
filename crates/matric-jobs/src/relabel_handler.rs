//! SpeakerRelabelHandler — re-renders transcripts with user-assigned speaker names.
//!
//! When a user edits the speaker config block in a note (mapping SPEAKER_00 → "Alice"),
//! a SpeakerRelabel job is queued. This handler reads the speaker map, updates the
//! attachment metadata segments with new names, and re-renders caption files.
//!
//! Does NOT re-run diarization — only relabels existing speaker assignments.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fmt;
use tracing::{error, info, warn};
use uuid::Uuid;

use matric_core::{captions, JobType};
use matric_db::{Database, SchemaContext};

use crate::handler::{JobContext, JobHandler, JobResult};

/// A speaker mapping entry from the user's config block.
#[derive(Clone, Serialize, Deserialize)]
pub struct SpeakerMapping {
    /// The diarization speaker ID (e.g., "SPEAKER_00").
    pub id: String,
    /// The user-assigned display name (e.g., "Alice").
    pub name: String,
    /// Optional role label (e.g., "Host", "Guest").
    #[serde(default)]
    pub role: Option<String>,
}

impl fmt::Debug for SpeakerMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpeakerMapping")
            .field("id_len", &self.id.len())
            .field("name_len", &self.name.len())
            .field("role_len", &self.role.as_ref().map(String::len))
            .finish()
    }
}

/// The speaker configuration block parsed from note content.
#[derive(Clone, Serialize, Deserialize)]
pub struct SpeakerConfig {
    pub speakers: Vec<SpeakerMapping>,
}

impl fmt::Debug for SpeakerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpeakerConfig")
            .field("speakers_count", &self.speakers.len())
            .finish()
    }
}

impl SpeakerConfig {
    /// Build a lookup map from speaker_id → display name.
    pub fn name_map(&self) -> HashMap<String, String> {
        self.speakers
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect()
    }

    /// Parse the speaker config from a fenced JSON block in note content.
    ///
    /// Looks for a block like:
    /// ```json:speakers
    /// { "speakers": [...] }
    /// ```
    pub fn parse_from_content(content: &str) -> Option<Self> {
        // Look for ```json:speakers ... ``` block
        let start_marker = "```json:speakers";
        let end_marker = "```";

        let start_idx = content.find(start_marker)?;
        let json_start = start_idx + start_marker.len();
        let remaining = &content[json_start..];
        let end_idx = remaining.find(end_marker)?;
        let json_str = remaining[..end_idx].trim();

        serde_json::from_str(json_str).ok()
    }

    /// Render a speaker config block for embedding in note content.
    pub fn render_block(&self) -> String {
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        format!(
            "## Speaker Configuration\n\n\
             Edit speaker names below to re-label the transcript. \
             Save the note to trigger reprocessing.\n\n\
             ```json:speakers\n{}\n```",
            json
        )
    }
}

fn extract_schema(ctx: &JobContext) -> &str {
    ctx.payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public")
}

fn schema_context(db: &Database, schema: &str) -> Result<SchemaContext, JobResult> {
    db.for_schema(schema)
        .map_err(|_| JobResult::Failed("Invalid schema".into()))
}

fn relabel_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("not found")
        || text.contains("no such")
        || text.contains("missing")
        || text.contains("unknown")
    {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else if text.contains("connection refused")
        || text.contains("cannot connect")
        || text.contains("connection")
    {
        "connection_failed"
    } else if text.contains("database") || text.contains("sql") || text.contains("postgres") {
        "database_error"
    } else if text.contains("storage") || text.contains("file") {
        "storage_error"
    } else {
        "operation_failed"
    }
}

#[cfg(test)]
fn relabel_text_len(text: &str) -> usize {
    text.chars().count()
}

pub struct SpeakerRelabelHandler {
    db: Database,
}

impl SpeakerRelabelHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for SpeakerRelabelHandler {
    fn job_type(&self) -> JobType {
        JobType::SpeakerRelabel
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing relabel job payload".into()),
        };

        let attachment_id: Uuid = match payload
            .get("attachment_id")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Missing or invalid attachment_id".into()),
        };

        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("Missing note_id for relabel".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(10, Some("Loading speaker config"));

        // 1. Read the note content to extract the speaker config block
        let speaker_map: HashMap<String, String> =
            if let Some(config_json) = payload.get("speaker_map") {
                // Direct map provided in payload (API-triggered relabel)
                match serde_json::from_value::<HashMap<String, String>>(config_json.clone()) {
                    Ok(map) => map,
                    Err(_) => return JobResult::Failed("Invalid speaker_map in payload".into()),
                }
            } else {
                // Parse from note content (user edit-triggered)
                let note = {
                    let mut tx = match schema_ctx.begin_tx().await {
                        Ok(t) => t,
                        Err(e) => {
                            let error_text = e.to_string();
                            return JobResult::Failed(format!(
                                "Schema tx failed ({})",
                                relabel_error_reason_code(&error_text)
                            ));
                        }
                    };
                    let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
                        Ok(n) => n,
                        Err(e) => {
                            let error_text = e.to_string();
                            return JobResult::Failed(format!(
                                "Failed to fetch note ({})",
                                relabel_error_reason_code(&error_text)
                            ));
                        }
                    };
                    let _ = tx.commit().await;
                    note
                };

                match SpeakerConfig::parse_from_content(&note.original.content) {
                    Some(config) => config.name_map(),
                    None => {
                        return JobResult::Failed(
                            "No speaker config block found in note content".into(),
                        )
                    }
                }
            };

        if speaker_map.is_empty() {
            return JobResult::Success(Some(json!({
                "status": "no_mappings",
                "message": "Speaker map is empty, nothing to relabel"
            })));
        }

        ctx.report_progress(20, Some("Loading attachment metadata"));

        // 2. Fetch the attachment and its transcript segments
        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        let attachment = {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    let error_text = e.to_string();
                    return JobResult::Failed(format!(
                        "Schema tx failed ({})",
                        relabel_error_reason_code(&error_text)
                    ));
                }
            };
            let att = match file_storage.get_tx(&mut tx, attachment_id).await {
                Ok(a) => a,
                Err(e) => {
                    let error_text = e.to_string();
                    return JobResult::Failed(format!(
                        "Failed to fetch attachment ({})",
                        relabel_error_reason_code(&error_text)
                    ));
                }
            };
            let _ = tx.commit().await;
            att
        };

        let segments_json = attachment
            .extracted_metadata
            .as_ref()
            .and_then(|m| m.get("transcript_segments"))
            .and_then(|v| v.as_array());

        let segments = match segments_json {
            Some(segs) if !segs.is_empty() => segs.clone(),
            _ => {
                return JobResult::Failed(
                    "No transcript segments found in attachment metadata".into(),
                )
            }
        };

        ctx.report_progress(40, Some("Applying speaker labels"));

        // 3. Apply the speaker map to segments
        let relabeled_segments: Vec<serde_json::Value> = segments
            .iter()
            .map(|seg| {
                let mut obj = seg.clone();
                if let Some(speaker_id) = seg.get("speaker_id").and_then(|s| s.as_str()) {
                    if let Some(new_name) = speaker_map.get(speaker_id) {
                        obj["speaker_id"] = json!(new_name);
                    }
                }
                obj
            })
            .collect();

        // 4. Update the attachment metadata with relabeled segments
        ctx.report_progress(50, Some("Updating attachment metadata"));

        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => {
                    let error_text = e.to_string();
                    return JobResult::Failed(format!(
                        "Schema tx failed for metadata update ({})",
                        relabel_error_reason_code(&error_text)
                    ));
                }
            };

            let mut metadata = attachment
                .extracted_metadata
                .clone()
                .unwrap_or_else(|| json!({}));

            metadata["transcript_segments"] = json!(relabeled_segments);

            // Record the relabel mapping
            metadata["speaker_relabel"] = json!({
                "mappings": speaker_map,
                "relabeled_at": chrono::Utc::now().to_rfc3339(),
            });

            if let Err(e) = file_storage
                .update_extracted_content_tx(
                    &mut tx,
                    attachment_id,
                    attachment.extracted_text.as_deref(),
                    Some(metadata),
                )
                .await
            {
                let error_text = e.to_string();
                return JobResult::Failed(format!(
                    "Failed to update metadata ({})",
                    relabel_error_reason_code(&error_text)
                ));
            }

            if let Err(e) = tx.commit().await {
                let error_text = e.to_string();
                return JobResult::Failed(format!(
                    "Failed to commit metadata ({})",
                    relabel_error_reason_code(&error_text)
                ));
            }
        }

        ctx.report_progress(70, Some("Re-rendering caption files"));

        // 5. Re-render caption files with relabeled speakers
        {
            let caption_segments: Vec<captions::CaptionSegment> = relabeled_segments
                .iter()
                .filter_map(|seg| {
                    let start = seg.get("start_secs")?.as_f64()?;
                    let end = seg.get("end_secs")?.as_f64()?;
                    let text = seg.get("text")?.as_str()?.to_string();
                    let speaker = seg
                        .get("speaker_id")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string());
                    Some(captions::CaptionSegment {
                        start_secs: start,
                        end_secs: end,
                        text,
                        speaker,
                    })
                })
                .collect();

            if !caption_segments.is_empty() {
                let filename = &attachment.filename;
                let base_name = filename
                    .rsplit_once('.')
                    .map(|(name, _)| name)
                    .unwrap_or(filename);

                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                    // Delete existing derived caption/transcript attachments to avoid duplicates
                    if let Err(e) = file_storage
                        .delete_derived_captions_tx(&mut tx, attachment_id)
                        .await
                    {
                        let error_text = e.to_string();
                        warn!(
                            error_len = error_text.len(),
                            error_reason = relabel_error_reason_code(&error_text),
                            "Failed to delete existing caption attachments"
                        );
                    }

                    // VTT file
                    let vtt = captions::render_webvtt(&caption_segments);
                    if let Err(e) = file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            attachment_id,
                            &format!("{}.vtt", base_name),
                            "text/vtt",
                            vtt.as_bytes(),
                            "caption",
                        )
                        .await
                    {
                        let error_text = e.to_string();
                        warn!(
                            error_len = error_text.len(),
                            error_reason = relabel_error_reason_code(&error_text),
                            "Failed to store relabeled VTT attachment"
                        );
                    }

                    // SRT file
                    let srt = captions::render_srt(&caption_segments);
                    if let Err(e) = file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            attachment_id,
                            &format!("{}.srt", base_name),
                            "application/x-subrip",
                            srt.as_bytes(),
                            "caption",
                        )
                        .await
                    {
                        let error_text = e.to_string();
                        warn!(
                            error_len = error_text.len(),
                            error_reason = relabel_error_reason_code(&error_text),
                            "Failed to store relabeled SRT attachment"
                        );
                    }

                    // Plain text transcript with speaker labels
                    let plain_text: String = caption_segments
                        .iter()
                        .map(|s| {
                            if let Some(ref speaker) = s.speaker {
                                format!("[{}] {}", speaker, s.text.trim())
                            } else {
                                s.text.trim().to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = file_storage
                        .store_derived_attachment_tx(
                            &mut tx,
                            note_id,
                            attachment_id,
                            &format!("{}.transcript.txt", base_name),
                            "text/plain",
                            plain_text.as_bytes(),
                            "transcript",
                        )
                        .await
                    {
                        let error_text = e.to_string();
                        warn!(
                            error_len = error_text.len(),
                            error_reason = relabel_error_reason_code(&error_text),
                            "Failed to store relabeled transcript attachment"
                        );
                    }

                    if let Err(e) = tx.commit().await {
                        let error_text = e.to_string();
                        error!(
                            error_len = error_text.len(),
                            error_reason = relabel_error_reason_code(&error_text),
                            "Failed to commit relabeled caption files"
                        );
                    } else {
                        info!(
                            attachment_present = true,
                            mappings = speaker_map.len(),
                            "Caption files re-rendered with relabeled speakers"
                        );
                    }
                }
            }
        }

        ctx.report_progress(95, Some("Relabel complete"));

        let result_json = json!({
            "mappings_applied": speaker_map.len(),
            "segments_processed": relabeled_segments.len(),
        });

        info!(
            attachment_present = true,
            mappings = speaker_map.len(),
            "Speaker relabel completed"
        );

        ctx.report_progress(100, Some("Done"));
        JobResult::Success(Some(result_json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speaker_config_parse_from_content() {
        let content = r#"## Speaker Configuration

Edit speaker names below to re-label the transcript.

```json:speakers
{
  "speakers": [
    { "id": "SPEAKER_00", "name": "Alice", "role": "Host" },
    { "id": "SPEAKER_01", "name": "Bob" }
  ]
}
```

## Transcript
..."#;

        let config = SpeakerConfig::parse_from_content(content).unwrap();
        assert_eq!(config.speakers.len(), 2);
        assert_eq!(config.speakers[0].id, "SPEAKER_00");
        assert_eq!(config.speakers[0].name, "Alice");
        assert_eq!(config.speakers[0].role.as_deref(), Some("Host"));
        assert_eq!(config.speakers[1].id, "SPEAKER_01");
        assert_eq!(config.speakers[1].name, "Bob");
        assert_eq!(config.speakers[1].role, None);
    }

    #[test]
    fn test_speaker_config_parse_no_block() {
        let content = "# Just a regular note\n\nNo speaker config here.";
        assert!(SpeakerConfig::parse_from_content(content).is_none());
    }

    #[test]
    fn test_speaker_config_name_map() {
        let config = SpeakerConfig {
            speakers: vec![
                SpeakerMapping {
                    id: "SPEAKER_00".to_string(),
                    name: "Alice".to_string(),
                    role: None,
                },
                SpeakerMapping {
                    id: "SPEAKER_01".to_string(),
                    name: "Bob".to_string(),
                    role: Some("Guest".to_string()),
                },
            ],
        };

        let map = config.name_map();
        assert_eq!(map.len(), 2);
        assert_eq!(map["SPEAKER_00"], "Alice");
        assert_eq!(map["SPEAKER_01"], "Bob");
    }

    #[test]
    fn test_speaker_config_render_block() {
        let config = SpeakerConfig {
            speakers: vec![SpeakerMapping {
                id: "SPEAKER_00".to_string(),
                name: "Alice".to_string(),
                role: Some("Host".to_string()),
            }],
        };

        let block = config.render_block();
        assert!(block.contains("## Speaker Configuration"));
        assert!(block.contains("```json:speakers"));
        assert!(block.contains("SPEAKER_00"));
        assert!(block.contains("Alice"));

        // Verify it can be re-parsed
        let parsed = SpeakerConfig::parse_from_content(&block).unwrap();
        assert_eq!(parsed.speakers.len(), 1);
        assert_eq!(parsed.speakers[0].name, "Alice");
    }

    #[test]
    fn test_speaker_config_empty_speakers() {
        let config = SpeakerConfig { speakers: vec![] };
        assert!(config.name_map().is_empty());
    }

    #[test]
    fn speaker_mapping_debug_redacts_user_assigned_labels() {
        let mapping = SpeakerMapping {
            id: "SPEAKER_00_internal-session".to_string(),
            name: "Alice Example bearer-token-fragment".to_string(),
            role: Some("Host sk-live-secret-role".to_string()),
        };

        let rendered = format!("{mapping:?}");

        assert!(rendered.contains("SpeakerMapping"));
        assert!(rendered.contains("id_len"));
        assert!(rendered.contains("name_len"));
        assert!(rendered.contains("role_len"));
        assert!(!rendered.contains("SPEAKER_00_internal-session"));
        assert!(!rendered.contains("Alice Example"));
        assert!(!rendered.contains("bearer-token-fragment"));
        assert!(!rendered.contains("sk-live-secret-role"));
    }

    #[test]
    fn speaker_config_debug_redacts_all_speaker_content() {
        let config = SpeakerConfig {
            speakers: vec![
                SpeakerMapping {
                    id: "SPEAKER_00".to_string(),
                    name: "Alice customer@example.com".to_string(),
                    role: Some("Host".to_string()),
                },
                SpeakerMapping {
                    id: "SPEAKER_01".to_string(),
                    name: "Bob mm_key_secret".to_string(),
                    role: None,
                },
            ],
        };

        let rendered = format!("{config:?}");

        assert!(rendered.contains("SpeakerConfig"));
        assert!(rendered.contains("speakers_count"));
        assert!(!rendered.contains("SPEAKER_00"));
        assert!(!rendered.contains("Alice"));
        assert!(!rendered.contains("customer@example.com"));
        assert!(!rendered.contains("Bob"));
        assert!(!rendered.contains("mm_key_secret"));
    }

    #[test]
    fn relabel_error_reason_code_uses_stable_classes() {
        assert_eq!(
            relabel_error_reason_code("database sql failed while writing captions"),
            "database_error"
        );
        assert_eq!(
            relabel_error_reason_code("file storage denied during transcript write"),
            "permission_denied"
        );
        assert_eq!(
            relabel_error_reason_code("Cannot connect to caption storage backend"),
            "connection_failed"
        );
        assert_eq!(
            relabel_error_reason_code("opaque backend diagnostic text"),
            "operation_failed"
        );
    }

    #[test]
    fn relabel_runtime_telemetry_helpers_redact_private_values() {
        let raw_error =
            "postgres://user:mm_key_secret@db.internal/app failed at /srv/private/relabel.vtt";
        let rendered = format!(
            "attachment_present=true; mappings=2; error_len={}; error_reason={}",
            relabel_text_len(raw_error),
            relabel_error_reason_code(raw_error)
        );

        assert!(rendered.contains("attachment_present=true"));
        assert!(rendered.contains("mappings=2"));
        assert!(rendered.contains("error_len="));
        assert!(rendered.contains("error_reason="));
        assert!(!rendered.contains("mm_key_secret"));
        assert!(!rendered.contains("postgres://"));
        assert!(!rendered.contains("db.internal"));
        assert!(!rendered.contains("/srv/private"));
    }
}

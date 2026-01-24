//! Service for reconstructing full documents from chunked notes.
//!
//! This service handles:
//! - Identifying if a note is part of a chunked document chain
//! - Fetching all chunks in a document chain
//! - Stitching chunks together with overlap removal
//! - Returning full document metadata

use chrono::{DateTime, Utc};
use matric_core::{Error, NoteFull, NoteRepository, Result};
use matric_db::Database;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Summary of a single chunk in a document chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkSummary {
    pub id: Uuid,
    pub sequence: u32,
    pub title: String,
    pub byte_range: (usize, usize),
}

/// Full document response with reconstructed content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullDocumentResponse {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub chunks: Option<Vec<ChunkSummary>>,
    pub total_chunks: Option<usize>,
    pub is_chunked: bool,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Service for document reconstruction operations.
pub struct ReconstructionService {
    db: Database,
}

impl ReconstructionService {
    /// Create a new reconstruction service.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Get the full document for a given note ID (works with both chunked and regular notes).
    ///
    /// # Arguments
    /// * `note_id` - Either a chain_id or any chunk's note_id
    ///
    /// # Returns
    /// A `FullDocumentResponse` with stitched content if chunked, or original content if regular.
    pub async fn get_full_document(&self, note_id: Uuid) -> Result<FullDocumentResponse> {
        // Fetch the note to check if it's chunked
        let note = self.db.notes.fetch(note_id).await?;

        // Check metadata for chunking information
        let chain_info = Self::extract_chain_info(&note);

        if let Some((chain_id, _sequence, total)) = chain_info {
            // This is a chunked note - reconstruct the full document
            self.reconstruct_chunked_document(chain_id, total).await
        } else {
            // Regular note - return as-is
            Self::return_regular_note(note)
        }
    }

    /// Extract chain information from note metadata.
    ///
    /// Returns (chain_id, sequence, total_chunks) if this is a chunked note.
    fn extract_chain_info(note: &NoteFull) -> Option<(Uuid, u32, u32)> {
        let metadata = &note.note.metadata;

        let chain_id = metadata
            .get("chain_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())?;

        let sequence = metadata
            .get("chunk_sequence")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)?;

        let total = metadata
            .get("total_chunks")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)?;

        Some((chain_id, sequence, total))
    }

    /// Reconstruct a full document from its chunks.
    async fn reconstruct_chunked_document(
        &self,
        chain_id: Uuid,
        total_chunks: u32,
    ) -> Result<FullDocumentResponse> {
        // Fetch all notes in the chain
        let notes = self.get_chain_notes(chain_id).await?;

        if notes.is_empty() {
            return Err(Error::NotFound(format!(
                "No chunks found for chain {}",
                chain_id
            )));
        }

        // Sort by sequence
        let mut sorted_notes = notes;
        sorted_notes.sort_by_key(|n| {
            n.note
                .metadata
                .get("chunk_sequence")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32
        });

        // Stitch content together
        let stitched_content = Self::stitch_chunks(&sorted_notes);

        // Extract original title (remove "Part X/Y" suffix)
        let title = sorted_notes
            .first()
            .and_then(|n| n.note.title.clone())
            .map(|t| extract_original_title(&t))
            .unwrap_or_else(|| "Untitled Document".to_string());

        // Deduplicate tags across all chunks
        let mut all_tags: Vec<String> = sorted_notes.iter().flat_map(|n| n.tags.clone()).collect();
        all_tags.sort();
        all_tags.dedup();

        // Build chunk summaries
        let chunk_summaries: Vec<ChunkSummary> = sorted_notes
            .iter()
            .enumerate()
            .map(|(idx, note)| {
                let sequence = note
                    .note
                    .metadata
                    .get("chunk_sequence")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(idx as u64) as u32;

                let byte_range = note
                    .note
                    .metadata
                    .get("byte_range")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| {
                        if arr.len() == 2 {
                            Some((
                                arr[0].as_u64().unwrap_or(0) as usize,
                                arr[1].as_u64().unwrap_or(0) as usize,
                            ))
                        } else {
                            None
                        }
                    })
                    .unwrap_or((0, 0));

                ChunkSummary {
                    id: note.note.id,
                    sequence,
                    title: note.note.title.clone().unwrap_or_default(),
                    byte_range,
                }
            })
            .collect();

        // Use the first note's timestamps
        let first_note = sorted_notes.first().unwrap();

        Ok(FullDocumentResponse {
            id: chain_id,
            title,
            content: stitched_content,
            chunks: Some(chunk_summaries),
            total_chunks: Some(total_chunks as usize),
            is_chunked: true,
            tags: all_tags,
            created_at: first_note.note.created_at_utc,
            updated_at: sorted_notes
                .iter()
                .map(|n| n.note.updated_at_utc)
                .max()
                .unwrap_or(first_note.note.updated_at_utc),
        })
    }

    /// Return a regular (non-chunked) note as a FullDocumentResponse.
    fn return_regular_note(note: NoteFull) -> Result<FullDocumentResponse> {
        // Use revised content if available, otherwise original
        let content = if !note.revised.content.is_empty() {
            note.revised.content.clone()
        } else {
            note.original.content.clone()
        };

        let title = note
            .note
            .title
            .clone()
            .unwrap_or_else(|| "Untitled Note".to_string());

        Ok(FullDocumentResponse {
            id: note.note.id,
            title,
            content,
            chunks: None,
            total_chunks: None,
            is_chunked: false,
            tags: note.tags.clone(),
            created_at: note.note.created_at_utc,
            updated_at: note.note.updated_at_utc,
        })
    }

    /// Fetch all notes in a document chain, ordered by sequence.
    async fn get_chain_notes(&self, chain_id: Uuid) -> Result<Vec<NoteFull>> {
        // Query notes where metadata->>'chain_id' = chain_id
        let note_ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM note
            WHERE metadata->>'chain_id' = $1
            AND archived = false
            ORDER BY (metadata->>'chunk_sequence')::int ASC
            "#,
        )
        .bind(chain_id.to_string())
        .fetch_all(self.db.pool())
        .await
        .map_err(Error::Database)?;

        let mut full_notes = Vec::new();
        for note_id in note_ids {
            let note = self.db.notes.fetch(note_id).await?;
            full_notes.push(note);
        }

        Ok(full_notes)
    }

    /// Stitch chunks together, removing overlaps.
    ///
    /// This function reconstructs the original document by:
    /// 1. Concatenating chunks in sequence order
    /// 2. Removing overlap regions if they exist
    /// 3. Preserving semantic boundaries
    fn stitch_chunks(notes: &[NoteFull]) -> String {
        if notes.is_empty() {
            return String::new();
        }

        if notes.len() == 1 {
            return if !notes[0].revised.content.is_empty() {
                notes[0].revised.content.clone()
            } else {
                notes[0].original.content.clone()
            };
        }

        let mut result = String::new();

        for (idx, note) in notes.iter().enumerate() {
            let content = if !note.revised.content.is_empty() {
                &note.revised.content
            } else {
                &note.original.content
            };

            if idx == 0 {
                // First chunk - add entirely
                result.push_str(content);
            } else {
                // Subsequent chunks - detect and remove overlap
                let overlap_size = detect_overlap(&result, content);
                let deduplicated = &content[overlap_size..];
                result.push_str(deduplicated);
            }
        }

        result
    }
}

/// Detect overlap between the end of accumulated content and the start of next chunk.
///
/// Returns the number of bytes to skip in the next chunk.
fn detect_overlap(accumulated: &str, next_chunk: &str) -> usize {
    const MAX_OVERLAP: usize = 500;

    let search_len = accumulated.len().min(MAX_OVERLAP);
    if search_len == 0 {
        return 0;
    }

    let suffix = &accumulated[accumulated.len() - search_len..];

    // Try to find the suffix in the beginning of next_chunk
    for i in (50..=search_len).rev() {
        if i > suffix.len() {
            continue;
        }
        let test_suffix = &suffix[suffix.len() - i..];
        if next_chunk.starts_with(test_suffix) {
            return test_suffix.len();
        }
    }

    0
}

/// Extract original title by removing chunk suffix patterns.
fn extract_original_title(title: &str) -> String {
    let patterns = [
        regex::Regex::new(r"\s*\(Part\s+\d+/\d+\)\s*$").unwrap(),
        regex::Regex::new(r"\s*-\s*Part\s+\d+\s+of\s+\d+\s*$").unwrap(),
        regex::Regex::new(r"\s*\[\d+/\d+\]\s*$").unwrap(),
    ];

    let mut cleaned = title.to_string();
    for pattern in &patterns {
        cleaned = pattern.replace(&cleaned, "").to_string();
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_original_title_with_part_suffix() {
        assert_eq!(
            extract_original_title("My Document (Part 1/3)"),
            "My Document"
        );
        assert_eq!(
            extract_original_title("Research Paper - Part 2 of 5"),
            "Research Paper"
        );
        assert_eq!(extract_original_title("Analysis [1/4]"), "Analysis");
    }

    #[test]
    fn test_extract_original_title_no_suffix() {
        assert_eq!(extract_original_title("Regular Title"), "Regular Title");
    }

    #[test]
    fn test_extract_original_title_edge_cases() {
        assert_eq!(extract_original_title(""), "");
        assert_eq!(
            extract_original_title("   Spaced Title (Part 1/2)   "),
            "Spaced Title"
        );
    }

    #[test]
    fn test_detect_overlap_with_exact_match() {
        let accumulated = "This is the first chunk with some overlap text that continues for more than fifty characters to meet the minimum overlap threshold";
        let next_chunk = "overlap text that continues for more than fifty characters to meet the minimum overlap threshold and this is new content";

        let overlap = detect_overlap(accumulated, next_chunk);
        assert_eq!(overlap, "overlap text that continues for more than fifty characters to meet the minimum overlap threshold".len());
    }

    #[test]
    fn test_detect_overlap_no_match() {
        let accumulated = "First chunk";
        let next_chunk = "Completely different content";

        let overlap = detect_overlap(accumulated, next_chunk);
        assert_eq!(overlap, 0);
    }

    #[test]
    fn test_detect_overlap_empty() {
        let accumulated = "";
        let next_chunk = "Some content";

        let overlap = detect_overlap(accumulated, next_chunk);
        assert_eq!(overlap, 0);
    }

    // Integration tests would require database connection
}

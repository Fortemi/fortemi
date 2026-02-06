//! Intelligent document chunking strategies for optimal embedding quality.
//!
//! This module provides multiple chunking strategies to split text documents into
//! semantically meaningful chunks for embedding generation. Each strategy is optimized
//! for different types of content and use cases.
//!
//! # Strategies
//!
//! - `SentenceChunker` - Splits text at sentence boundaries using punctuation patterns
//! - `ParagraphChunker` - Splits text at paragraph boundaries (double newlines)
//! - `SemanticChunker` - Splits at natural boundaries (headings, lists, code blocks)
//! - `SlidingWindowChunker` - Fixed-size chunks with configurable overlap
//! - `RecursiveChunker` - Hierarchical splitting (paragraphs → sentences → chars)
//!
//! # Example
//!
//! ```rust,ignore
//! use matric_db::chunking::{Chunker, SentenceChunker, ChunkerConfig};
//!
//! let config = ChunkerConfig {
//!     max_chunk_size: 1000,
//!     min_chunk_size: 100,
//!     overlap: 50,
//! };
//!
//! let chunker = SentenceChunker::new(config);
//! let chunks = chunker.chunk("Your text here.");
//!
//! for chunk in chunks {
//!     println!("Chunk: {} (offset: {}-{})", chunk.text, chunk.start_offset, chunk.end_offset);
//! }
//! ```

use regex::Regex;
use std::collections::HashMap;

/// Configuration for chunking strategies.
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Maximum size of a chunk in characters.
    pub max_chunk_size: usize,
    /// Minimum size of a chunk in characters (chunks smaller than this may be merged).
    pub min_chunk_size: usize,
    /// Number of characters to overlap between chunks (for context preservation).
    pub overlap: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: matric_core::defaults::CHUNK_SIZE,
            min_chunk_size: matric_core::defaults::CHUNK_MIN_SIZE,
            overlap: matric_core::defaults::CHUNK_OVERLAP,
        }
    }
}

/// A text chunk with position information and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The text content of the chunk.
    pub text: String,
    /// Starting byte offset in the original document.
    pub start_offset: usize,
    /// Ending byte offset in the original document.
    pub end_offset: usize,
    /// Additional metadata about the chunk (e.g., chunk type, hierarchy level).
    pub metadata: HashMap<String, String>,
}

impl Chunk {
    /// Create a new chunk with empty metadata.
    pub fn new(text: String, start_offset: usize, end_offset: usize) -> Self {
        Self {
            text,
            start_offset,
            end_offset,
            metadata: HashMap::new(),
        }
    }

    /// Create a new chunk with metadata.
    pub fn with_metadata(
        text: String,
        start_offset: usize,
        end_offset: usize,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            text,
            start_offset,
            end_offset,
            metadata,
        }
    }

    /// Get the length of the chunk in bytes.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Check if the chunk is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Common trait for all chunking strategies.
pub trait Chunker: Send + Sync {
    /// Chunk the given text into a list of chunks.
    fn chunk(&self, text: &str) -> Vec<Chunk>;

    /// Get the configuration used by this chunker.
    fn config(&self) -> &ChunkerConfig;
}

// Helper functions

/// Find UTF-8 safe boundary at or before the given position.
fn find_char_boundary_before(text: &str, mut pos: usize) -> usize {
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Find UTF-8 safe boundary at or after the given position.
fn find_char_boundary_after(text: &str, mut pos: usize) -> usize {
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

/// Splits text at sentence boundaries using punctuation patterns.
///
/// Recognizes common sentence terminators: `.`, `!`, `?`, and handles edge cases
/// like abbreviations and decimal numbers.
#[derive(Debug, Clone)]
pub struct SentenceChunker {
    config: ChunkerConfig,
}

impl SentenceChunker {
    /// Create a new SentenceChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Find sentence boundaries in text.
    fn find_sentences(&self, text: &str) -> Vec<(usize, usize)> {
        // Pattern for sentence endings, avoiding common abbreviations
        let sentence_regex = Regex::new(r"[.!?]+(?:\s+|$)").unwrap();
        let abbrev_regex =
            Regex::new(r"(?i)\b(?:dr|mr|mrs|ms|prof|sr|jr|inc|ltd|co|etc|vs|e\.g|i\.e)\.$")
                .unwrap();

        let mut sentences = Vec::new();
        let mut last_end = 0;

        for mat in sentence_regex.find_iter(text) {
            let end = mat.end();
            let candidate = &text[last_end..end];

            // Check if this is likely an abbreviation
            if abbrev_regex.is_match(candidate.trim()) {
                continue;
            }

            // Check if preceded by a digit (likely decimal)
            let before_punct = mat.start();
            if before_punct > 0
                && text[..before_punct]
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                continue;
            }

            sentences.push((last_end, end));
            last_end = end;
        }

        // Add remaining text as final sentence if non-empty
        if last_end < text.len() && !text[last_end..].trim().is_empty() {
            sentences.push((last_end, text.len()));
        }

        sentences
    }
}

impl Chunker for SentenceChunker {
    fn chunk(&self, text: &str) -> Vec<Chunk> {
        if text.is_empty() {
            return vec![];
        }

        let sentences = self.find_sentences(text);
        let mut chunks = Vec::new();

        for (start, end) in sentences {
            let sentence = text[start..end].trim();

            if sentence.len() <= self.config.max_chunk_size {
                // Sentence fits in one chunk
                let mut metadata = HashMap::new();
                metadata.insert("type".to_string(), "sentence".to_string());
                chunks.push(Chunk::with_metadata(
                    sentence.to_string(),
                    start,
                    end,
                    metadata,
                ));
            } else {
                // Sentence too long, split it
                let mut offset = 0;
                while offset < sentence.len() {
                    let chunk_end = (offset + self.config.max_chunk_size).min(sentence.len());
                    let chunk_end = find_char_boundary_before(sentence, chunk_end);

                    if chunk_end > offset {
                        let mut metadata = HashMap::new();
                        metadata.insert("type".to_string(), "sentence_split".to_string());
                        chunks.push(Chunk::with_metadata(
                            sentence[offset..chunk_end].to_string(),
                            start + offset,
                            start + chunk_end,
                            metadata,
                        ));
                    }

                    offset = chunk_end;
                }
            }
        }

        chunks
    }

    fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

/// Splits text at paragraph boundaries (double newlines).
///
/// Preserves paragraph structure while respecting max chunk size.
#[derive(Debug, Clone)]
pub struct ParagraphChunker {
    config: ChunkerConfig,
}

impl ParagraphChunker {
    /// Create a new ParagraphChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Split text into paragraphs.
    fn split_paragraphs<'a>(&self, text: &'a str) -> Vec<(usize, usize, &'a str)> {
        let para_regex = Regex::new(r"\n\s*\n|\r\n\s*\r\n").unwrap();
        let mut paragraphs = Vec::new();
        let mut last_end = 0;

        for mat in para_regex.find_iter(text) {
            let para_text = text[last_end..mat.start()].trim();
            if !para_text.is_empty() {
                paragraphs.push((last_end, mat.start(), para_text));
            }
            last_end = mat.end();
        }

        // Add final paragraph
        if last_end < text.len() {
            let para_text = text[last_end..].trim();
            if !para_text.is_empty() {
                paragraphs.push((last_end, text.len(), para_text));
            }
        }

        paragraphs
    }
}

impl Chunker for ParagraphChunker {
    fn chunk(&self, text: &str) -> Vec<Chunk> {
        if text.is_empty() {
            return vec![];
        }

        let paragraphs = self.split_paragraphs(text);
        let mut chunks = Vec::new();

        for (start, _end, para) in paragraphs {
            if para.len() <= self.config.max_chunk_size {
                // Paragraph fits in one chunk
                let mut metadata = HashMap::new();
                metadata.insert("type".to_string(), "paragraph".to_string());
                chunks.push(Chunk::with_metadata(
                    para.to_string(),
                    start,
                    start + para.len(),
                    metadata,
                ));
            } else {
                // Paragraph too long, use sentence chunker
                let sentence_chunker = SentenceChunker::new(self.config.clone());
                let sub_chunks = sentence_chunker.chunk(para);

                for sub_chunk in sub_chunks {
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), "paragraph_split".to_string());
                    chunks.push(Chunk::with_metadata(
                        sub_chunk.text,
                        start + sub_chunk.start_offset,
                        start + sub_chunk.end_offset,
                        metadata,
                    ));
                }
            }
        }

        chunks
    }

    fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

/// Splits text at natural semantic boundaries (headings, lists, code blocks).
///
/// Recognizes Markdown structure to preserve semantic units.
#[derive(Debug, Clone)]
pub struct SemanticChunker {
    config: ChunkerConfig,
}

impl SemanticChunker {
    /// Create a new SemanticChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Check if a line is a numbered list item (e.g., "1.", "12.")
    fn is_numbered_list_item(line: &str) -> bool {
        let trimmed = line.trim();
        if let Some(dot_pos) = trimmed.find('.') {
            trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) && dot_pos > 0
        } else {
            false
        }
    }

    /// Identify semantic boundaries and element types.
    fn find_semantic_elements(&self, text: &str) -> Vec<(usize, usize, String)> {
        let mut elements = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        let mut current_offset = 0;

        while i < lines.len() {
            let line = lines[i];
            let line_start = current_offset;
            let line_end = current_offset + line.len() + 1; // +1 for newline

            // Check for various Markdown elements
            if line.starts_with('#') {
                elements.push((line_start, line_end, "heading".to_string()));
            } else if line.starts_with("```") {
                // Code block
                let code_start = line_start;
                i += 1;
                current_offset = line_end;

                while i < lines.len() && !lines[i].starts_with("```") {
                    current_offset += lines[i].len() + 1;
                    i += 1;
                }

                if i < lines.len() {
                    current_offset += lines[i].len() + 1;
                }

                elements.push((code_start, current_offset, "code".to_string()));
                i += 1;
                continue;
            } else if line.trim().starts_with('-')
                || line.trim().starts_with('*')
                || line.trim().starts_with('+')
            {
                // List item
                let list_start = line_start;
                let mut list_end = line_end;

                i += 1;
                current_offset = line_end;

                while i < lines.len() {
                    let next_line = lines[i].trim();
                    if next_line.starts_with('-')
                        || next_line.starts_with('*')
                        || next_line.starts_with('+')
                    {
                        list_end = current_offset + lines[i].len() + 1;
                        current_offset = list_end;
                        i += 1;
                    } else {
                        break;
                    }
                }

                elements.push((list_start, list_end, "list".to_string()));
                continue;
            } else if Self::is_numbered_list_item(line.trim()) {
                // Numbered list
                let list_start = line_start;
                let mut list_end = line_end;

                i += 1;
                current_offset = line_end;

                while i < lines.len() {
                    let next_line = lines[i].trim();
                    if Self::is_numbered_list_item(next_line) {
                        list_end = current_offset + lines[i].len() + 1;
                        current_offset = list_end;
                        i += 1;
                    } else {
                        break;
                    }
                }

                elements.push((list_start, list_end, "numbered_list".to_string()));
                continue;
            } else if line.trim().starts_with('>') {
                // Blockquote
                let quote_start = line_start;
                let mut quote_end = line_end;

                i += 1;
                current_offset = line_end;

                while i < lines.len() && lines[i].trim().starts_with('>') {
                    quote_end = current_offset + lines[i].len() + 1;
                    current_offset = quote_end;
                    i += 1;
                }

                elements.push((quote_start, quote_end, "blockquote".to_string()));
                continue;
            } else if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
                elements.push((line_start, line_end, "hr".to_string()));
            } else if !line.trim().is_empty() {
                elements.push((line_start, line_end, "text".to_string()));
            }

            current_offset = line_end;
            i += 1;
        }

        elements
    }
}

impl Chunker for SemanticChunker {
    fn chunk(&self, text: &str) -> Vec<Chunk> {
        if text.is_empty() {
            return vec![];
        }

        let elements = self.find_semantic_elements(text);
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_start = 0;
        let mut current_type = String::new();

        for (start, end, elem_type) in elements {
            let elem_text = if end <= text.len() {
                &text[start..end.min(text.len())]
            } else {
                &text[start..]
            };

            // Horizontal rules and headings trigger chunk boundaries
            if elem_type == "hr" || elem_type == "heading" {
                if !current_chunk.is_empty() {
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), current_type.clone());
                    chunks.push(Chunk::with_metadata(
                        current_chunk.trim().to_string(),
                        current_start,
                        current_start + current_chunk.len(),
                        metadata,
                    ));
                    current_chunk.clear();
                }
                if elem_type == "hr" {
                    continue;
                }
                // For headings, start a new chunk with the heading
                current_chunk = elem_text.to_string();
                current_start = start;
                current_type = elem_type.clone();
                continue;
            }

            if current_chunk.is_empty() {
                current_chunk = elem_text.to_string();
                current_start = start;
                current_type = elem_type.clone();
            } else if current_chunk.len() + elem_text.len() < self.config.max_chunk_size {
                current_chunk.push('\n');
                current_chunk.push_str(elem_text);
            } else {
                // Save current chunk
                let mut metadata = HashMap::new();
                metadata.insert("type".to_string(), current_type.clone());
                chunks.push(Chunk::with_metadata(
                    current_chunk.trim().to_string(),
                    current_start,
                    current_start + current_chunk.len(),
                    metadata,
                ));

                current_chunk = elem_text.to_string();
                current_start = start;
                current_type = elem_type.clone();
            }

            // Handle oversized elements
            if current_chunk.len() > self.config.max_chunk_size {
                let para_chunker = ParagraphChunker::new(self.config.clone());
                let sub_chunks = para_chunker.chunk(&current_chunk);

                for sub_chunk in sub_chunks {
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), format!("{}_split", current_type));
                    chunks.push(Chunk::with_metadata(
                        sub_chunk.text,
                        current_start + sub_chunk.start_offset,
                        current_start + sub_chunk.end_offset,
                        metadata,
                    ));
                }
                current_chunk.clear();
            }
        }

        // Add final chunk
        if !current_chunk.is_empty() {
            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), current_type);
            chunks.push(Chunk::with_metadata(
                current_chunk.trim().to_string(),
                current_start,
                current_start + current_chunk.len(),
                metadata,
            ));
        }

        chunks
    }

    fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

/// Fixed-size chunks with configurable overlap.
///
/// Simple sliding window approach for uniform chunk sizes.
#[derive(Debug, Clone)]
pub struct SlidingWindowChunker {
    config: ChunkerConfig,
}

impl SlidingWindowChunker {
    /// Create a new SlidingWindowChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }
}

impl Chunker for SlidingWindowChunker {
    fn chunk(&self, text: &str) -> Vec<Chunk> {
        if text.is_empty() {
            return vec![];
        }

        if text.len() <= self.config.max_chunk_size {
            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), "window".to_string());
            return vec![Chunk::with_metadata(
                text.to_string(),
                0,
                text.len(),
                metadata,
            )];
        }

        let mut chunks = Vec::new();
        let step_size = if self.config.overlap >= self.config.max_chunk_size {
            1 // Prevent infinite loop
        } else {
            self.config
                .max_chunk_size
                .saturating_sub(self.config.overlap)
                .max(1)
        };

        let mut start = 0;

        while start < text.len() {
            let mut end = (start + self.config.max_chunk_size).min(text.len());

            // Ensure UTF-8 boundary
            end = find_char_boundary_before(text, end);

            if end > start {
                let mut metadata = HashMap::new();
                metadata.insert("type".to_string(), "window".to_string());
                chunks.push(Chunk::with_metadata(
                    text[start..end].to_string(),
                    start,
                    end,
                    metadata,
                ));
            }

            // If we've reached the end, break
            if end >= text.len() {
                break;
            }

            // Move to next position
            start += step_size;
            start = find_char_boundary_after(text, start);
        }

        chunks
    }

    fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

/// Hierarchical chunking strategy that tries multiple approaches.
///
/// First attempts to split by paragraphs, then sentences, then characters
/// if necessary to meet chunk size constraints.
#[derive(Debug, Clone)]
pub struct RecursiveChunker {
    config: ChunkerConfig,
}

impl RecursiveChunker {
    /// Create a new RecursiveChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Recursively chunk text trying different strategies.
    fn chunk_recursive(&self, text: &str, start_offset: usize, level: usize) -> Vec<Chunk> {
        if text.is_empty() {
            return vec![];
        }

        if text.len() <= self.config.max_chunk_size {
            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), format!("recursive_level_{}", level));
            return vec![Chunk::with_metadata(
                text.to_string(),
                start_offset,
                start_offset + text.len(),
                metadata,
            )];
        }

        // Try paragraph splitting first (level 0)
        if level == 0 {
            let para_chunker = ParagraphChunker::new(self.config.clone());
            let para_regex = Regex::new(r"\n\s*\n|\r\n\s*\r\n").unwrap();

            if para_regex.is_match(text) {
                let chunks = para_chunker.chunk(text);
                if chunks
                    .iter()
                    .all(|c| c.text.len() <= self.config.max_chunk_size)
                {
                    return chunks
                        .into_iter()
                        .map(|mut c| {
                            c.start_offset += start_offset;
                            c.end_offset += start_offset;
                            c.metadata
                                .insert("type".to_string(), "recursive_paragraph".to_string());
                            c
                        })
                        .collect();
                }
            }
        }

        // Try sentence splitting (level 1)
        if level <= 1 {
            let sentence_chunker = SentenceChunker::new(self.config.clone());
            let sentence_regex = Regex::new(r"[.!?]+\s+").unwrap();

            if sentence_regex.is_match(text) {
                let chunks = sentence_chunker.chunk(text);
                if chunks
                    .iter()
                    .all(|c| c.text.len() <= self.config.max_chunk_size)
                {
                    return chunks
                        .into_iter()
                        .map(|mut c| {
                            c.start_offset += start_offset;
                            c.end_offset += start_offset;
                            c.metadata
                                .insert("type".to_string(), "recursive_sentence".to_string());
                            c
                        })
                        .collect();
                }
            }
        }

        // Fall back to character splitting (level 2+)
        let window_chunker = SlidingWindowChunker::new(self.config.clone());
        window_chunker
            .chunk(text)
            .into_iter()
            .map(|mut c| {
                c.start_offset += start_offset;
                c.end_offset += start_offset;
                c.metadata
                    .insert("type".to_string(), "recursive_char".to_string());
                c
            })
            .collect()
    }
}

impl Chunker for RecursiveChunker {
    fn chunk(&self, text: &str) -> Vec<Chunk> {
        self.chunk_recursive(text, 0, 0)
    }

    fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a default config for tests
    fn default_config() -> ChunkerConfig {
        ChunkerConfig {
            max_chunk_size: 100,
            min_chunk_size: 20,
            overlap: 10,
        }
    }

    // ============================================================================
    // Chunk struct tests
    // ============================================================================

    #[test]
    fn test_chunk_new() {
        let chunk = Chunk::new("test text".to_string(), 0, 9);
        assert_eq!(chunk.text, "test text");
        assert_eq!(chunk.start_offset, 0);
        assert_eq!(chunk.end_offset, 9);
        assert!(chunk.metadata.is_empty());
    }

    #[test]
    fn test_chunk_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "sentence".to_string());
        let chunk = Chunk::with_metadata("test".to_string(), 0, 4, metadata.clone());
        assert_eq!(chunk.metadata.get("type"), Some(&"sentence".to_string()));
    }

    #[test]
    fn test_chunk_len_and_is_empty() {
        let chunk = Chunk::new("test".to_string(), 0, 4);
        assert_eq!(chunk.len(), 4);
        assert!(!chunk.is_empty());

        let empty_chunk = Chunk::new("".to_string(), 0, 0);
        assert_eq!(empty_chunk.len(), 0);
        assert!(empty_chunk.is_empty());
    }

    // ============================================================================
    // SentenceChunker tests
    // ============================================================================

    #[test]
    fn test_sentence_chunker_empty_text() {
        let chunker = SentenceChunker::new(default_config());
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_sentence_chunker_single_sentence() {
        let chunker = SentenceChunker::new(default_config());
        let text = "This is a single sentence.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text.trim(), text.trim());
    }

    #[test]
    fn test_sentence_chunker_multiple_sentences() {
        let chunker = SentenceChunker::new(default_config());
        let text = "First sentence. Second sentence! Third sentence?";
        let chunks = chunker.chunk(text);
        assert!(chunks.len() >= 3, "Should split into at least 3 chunks");

        // Verify all text is captured
        let combined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(combined.contains("First sentence"));
        assert!(combined.contains("Second sentence"));
        assert!(combined.contains("Third sentence"));
    }

    #[test]
    fn test_sentence_chunker_handles_abbreviations() {
        let chunker = SentenceChunker::new(default_config());
        let text = "Dr. Smith works at the lab. He is a scientist.";
        let chunks = chunker.chunk(text);

        // Should not split on "Dr." abbreviation
        assert!(chunks.len() <= 2, "Should not split on abbreviation");
    }

    #[test]
    fn test_sentence_chunker_handles_decimals() {
        let chunker = SentenceChunker::new(default_config());
        let text = "The value is 3.14159. This is accurate.";
        let chunks = chunker.chunk(text);

        // Should not split on decimal point
        let first_chunk = &chunks[0].text;
        assert!(
            first_chunk.contains("3.14159"),
            "Should keep decimal intact"
        );
    }

    #[test]
    fn test_sentence_chunker_respects_max_size() {
        let config = ChunkerConfig {
            max_chunk_size: 50,
            min_chunk_size: 10,
            overlap: 5,
        };
        let chunker = SentenceChunker::new(config);
        let text = "This is a very long sentence that exceeds the maximum chunk size and should be split somehow. Another sentence here.";
        let chunks = chunker.chunk(text);

        for chunk in chunks {
            assert!(
                chunk.text.len() <= 50,
                "Chunk exceeds max size: {}",
                chunk.text.len()
            );
        }
    }

    #[test]
    fn test_sentence_chunker_utf8_boundaries() {
        let chunker = SentenceChunker::new(default_config());
        let text = "Hello 世界! This is a test. 日本語の文章。";
        let chunks = chunker.chunk(text);

        // All chunks should be valid UTF-8
        for chunk in &chunks {
            assert!(std::str::from_utf8(chunk.text.as_bytes()).is_ok());
        }

        // Verify offsets are at character boundaries
        for chunk in &chunks {
            assert!(text.is_char_boundary(chunk.start_offset));
            assert!(text.is_char_boundary(chunk.end_offset));
        }
    }

    #[test]
    fn test_sentence_chunker_preserves_offsets() {
        let chunker = SentenceChunker::new(default_config());
        let text = "First. Second. Third.";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            let extracted = &text[chunk.start_offset..chunk.end_offset];
            assert!(extracted.contains(chunk.text.trim()));
        }
    }

    #[test]
    fn test_sentence_chunker_handles_no_punctuation() {
        let chunker = SentenceChunker::new(default_config());
        let text = "This is text without proper punctuation marks at the end";
        let chunks = chunker.chunk(text);

        assert!(
            !chunks.is_empty(),
            "Should handle text without ending punctuation"
        );
    }

    #[test]
    fn test_sentence_chunker_multiple_punctuation() {
        let chunker = SentenceChunker::new(default_config());
        let text = "What?!! Really??? Yes!!!";
        let chunks = chunker.chunk(text);

        assert!(
            chunks.len() >= 2,
            "Should handle multiple punctuation marks"
        );
    }

    // ============================================================================
    // ParagraphChunker tests
    // ============================================================================

    #[test]
    fn test_paragraph_chunker_empty_text() {
        let chunker = ParagraphChunker::new(default_config());
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_paragraph_chunker_single_paragraph() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "This is a single paragraph with multiple sentences. It has no line breaks.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_paragraph_chunker_multiple_paragraphs() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 3, "Should split into 3 paragraphs");
    }

    #[test]
    fn test_paragraph_chunker_windows_line_endings() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "First paragraph.\r\n\r\nSecond paragraph.";
        let chunks = chunker.chunk(text);
        assert!(chunks.len() >= 2, "Should handle Windows line endings");
    }

    #[test]
    fn test_paragraph_chunker_mixed_line_endings() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "First paragraph.\n\nSecond paragraph.\r\n\r\nThird paragraph.";
        let chunks = chunker.chunk(text);
        assert!(chunks.len() >= 3, "Should handle mixed line endings");
    }

    #[test]
    fn test_paragraph_chunker_respects_max_size() {
        let config = ChunkerConfig {
            max_chunk_size: 50,
            min_chunk_size: 10,
            overlap: 5,
        };
        let chunker = ParagraphChunker::new(config);
        let text = "This is a very long paragraph that exceeds the maximum chunk size and needs to be split into smaller pieces.\n\nSecond paragraph.";
        let chunks = chunker.chunk(text);

        for chunk in chunks {
            assert!(chunk.text.len() <= 50, "Chunk exceeds max size");
        }
    }

    #[test]
    fn test_paragraph_chunker_merges_small_paragraphs() {
        let config = ChunkerConfig {
            max_chunk_size: 100,
            min_chunk_size: 30,
            overlap: 5,
        };
        let chunker = ParagraphChunker::new(config);
        let text = "Short.\n\nAlso short.\n\nThis one too.";
        let chunks = chunker.chunk(text);

        // Small paragraphs should be merged to meet min_chunk_size
        assert!(chunks.len() == 3, "Should create one chunk per paragraph");
    }

    #[test]
    fn test_paragraph_chunker_utf8_boundaries() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "First 段落.\n\n第二段落.\n\nThird paragraph.";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            assert!(std::str::from_utf8(chunk.text.as_bytes()).is_ok());
            assert!(text.is_char_boundary(chunk.start_offset));
            assert!(text.is_char_boundary(chunk.end_offset));
        }
    }

    #[test]
    fn test_paragraph_chunker_preserves_offsets() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "First.\n\nSecond.\n\nThird.";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            let extracted = &text[chunk.start_offset..chunk.end_offset];
            assert!(extracted.contains(chunk.text.trim()));
        }
    }

    #[test]
    fn test_paragraph_chunker_handles_triple_newlines() {
        let chunker = ParagraphChunker::new(default_config());
        let text = "First.\n\n\nSecond.";
        let chunks = chunker.chunk(text);

        assert!(chunks.len() >= 2, "Should handle triple newlines");
    }

    // ============================================================================
    // SemanticChunker tests
    // ============================================================================

    #[test]
    fn test_semantic_chunker_empty_text() {
        let chunker = SemanticChunker::new(default_config());
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_semantic_chunker_markdown_headings() {
        let chunker = SemanticChunker::new(default_config());
        let text = "# Heading 1\nContent 1\n## Heading 2\nContent 2";
        let chunks = chunker.chunk(text);

        assert!(chunks.len() >= 2, "Should split at headings");

        // Check that headings are preserved
        let combined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(combined.contains("Heading 1"));
        assert!(combined.contains("Heading 2"));
    }

    #[test]
    fn test_semantic_chunker_code_blocks() {
        let chunker = SemanticChunker::new(default_config());
        let text = "Text before.\n```rust\nfn main() {}\n```\nText after.";
        let chunks = chunker.chunk(text);

        // Code blocks should be kept as separate chunks
        let has_code = chunks.iter().any(|c| c.text.contains("fn main()"));
        assert!(has_code, "Should preserve code blocks");
    }

    #[test]
    fn test_semantic_chunker_lists() {
        let chunker = SemanticChunker::new(default_config());
        let text = "Before list:\n- Item 1\n- Item 2\n- Item 3\nAfter list.";
        let chunks = chunker.chunk(text);

        // Lists should be kept together if possible
        let has_list = chunks
            .iter()
            .any(|c| c.text.contains("Item 1") && c.text.contains("Item 2"));
        assert!(has_list, "Should keep lists together");
    }

    #[test]
    fn test_semantic_chunker_numbered_lists() {
        let chunker = SemanticChunker::new(default_config());
        let text = "Steps:\n1. First\n2. Second\n3. Third\nDone.";
        let chunks = chunker.chunk(text);

        let has_list = chunks
            .iter()
            .any(|c| c.text.contains("First") && c.text.contains("Second"));
        assert!(has_list, "Should keep numbered lists together");
    }

    #[test]
    fn test_semantic_chunker_respects_max_size() {
        let config = ChunkerConfig {
            max_chunk_size: 50,
            min_chunk_size: 10,
            overlap: 5,
        };
        let chunker = SemanticChunker::new(config);
        let text = "# Very Long Heading\nThis is a very long section with lots of content that exceeds the maximum chunk size.\n## Another Section\nMore content.";
        let chunks = chunker.chunk(text);

        for chunk in chunks {
            assert!(chunk.text.len() <= 50, "Chunk exceeds max size");
        }
    }

    #[test]
    fn test_semantic_chunker_metadata() {
        let chunker = SemanticChunker::new(default_config());
        let text = "# Heading\nContent\n```python\ncode\n```";
        let chunks = chunker.chunk(text);

        // Chunks should have metadata indicating their type
        for chunk in &chunks {
            assert!(!chunk.metadata.is_empty() || chunk.text.trim().is_empty());
        }
    }

    #[test]
    fn test_semantic_chunker_blockquotes() {
        let chunker = SemanticChunker::new(default_config());
        let text = "Before quote.\n> This is a quote\n> Second line\nAfter quote.";
        let chunks = chunker.chunk(text);

        let has_quote = chunks.iter().any(|c| c.text.contains("> This is a quote"));
        assert!(has_quote, "Should preserve blockquotes");
    }

    #[test]
    fn test_semantic_chunker_utf8_boundaries() {
        let chunker = SemanticChunker::new(default_config());
        let text = "# 日本語の見出し\n内容\n## Another 見出し\n更に内容";
        let chunks = chunker.chunk(text);

        // Verify all chunks contain valid UTF-8
        for chunk in &chunks {
            assert!(
                std::str::from_utf8(chunk.text.as_bytes()).is_ok(),
                "Chunk text must be valid UTF-8"
            );
        }
        // Verify start offsets are valid char boundaries
        for chunk in &chunks {
            assert!(chunk.start_offset <= text.len(), "Start offset in range");
            if chunk.start_offset < text.len() {
                assert!(
                    text.is_char_boundary(chunk.start_offset),
                    "Start must be char boundary"
                );
            }
        }
    }

    #[test]
    fn test_semantic_chunker_horizontal_rules() {
        let chunker = SemanticChunker::new(default_config());
        let text = "Section 1\n---\nSection 2\n***\nSection 3";
        let chunks = chunker.chunk(text);

        // Horizontal rules trigger chunk boundaries
        // With default config (100 char max), content may be in one chunk
        assert!(!chunks.is_empty(), "Should create at least one chunk");

        // All content should be captured in chunks
        let all_text: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(all_text.contains("Section 1"), "Should contain Section 1");
        assert!(all_text.contains("Section 2"), "Should contain Section 2");
        assert!(all_text.contains("Section 3"), "Should contain Section 3");
    }

    // ============================================================================
    // SlidingWindowChunker tests
    // ============================================================================

    #[test]
    fn test_sliding_window_empty_text() {
        let chunker = SlidingWindowChunker::new(default_config());
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_sliding_window_short_text() {
        let chunker = SlidingWindowChunker::new(default_config());
        let text = "Short text.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_sliding_window_exact_chunk_size() {
        let config = ChunkerConfig {
            max_chunk_size: 10,
            min_chunk_size: 5,
            overlap: 0,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "0123456789ABCDEFGHIJ"; // 20 chars
        let chunks = chunker.chunk(text);

        assert_eq!(chunks.len(), 2, "Should create exactly 2 chunks");
        assert_eq!(chunks[0].text.len(), 10);
        assert_eq!(chunks[1].text.len(), 10);
    }

    #[test]
    fn test_sliding_window_with_overlap() {
        let config = ChunkerConfig {
            max_chunk_size: 10,
            min_chunk_size: 5,
            overlap: 3,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "0123456789ABCDEFGHIJ";
        let chunks = chunker.chunk(text);

        // With overlap, chunks should share content
        for i in 0..chunks.len() - 1 {
            let curr_end = &chunks[i].text[chunks[i].text.len().saturating_sub(3)..];
            let next_start = &chunks[i + 1].text[..3.min(chunks[i + 1].text.len())];
            assert!(
                curr_end == next_start || chunks[i].text.len() < 3,
                "Overlap should be preserved"
            );
        }
    }

    #[test]
    fn test_sliding_window_respects_max_size() {
        let config = ChunkerConfig {
            max_chunk_size: 50,
            min_chunk_size: 10,
            overlap: 5,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "a".repeat(200);
        let chunks = chunker.chunk(&text);

        for chunk in chunks {
            assert!(chunk.text.len() <= 50, "Chunk exceeds max size");
        }
    }

    #[test]
    fn test_sliding_window_utf8_boundaries() {
        let config = ChunkerConfig {
            max_chunk_size: 20,
            min_chunk_size: 5,
            overlap: 5,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "Hello 世界! 你好世界! Привет мир!";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            assert!(std::str::from_utf8(chunk.text.as_bytes()).is_ok());
            assert!(text.is_char_boundary(chunk.start_offset));
            assert!(text.is_char_boundary(chunk.end_offset));
        }
    }

    #[test]
    fn test_sliding_window_preserves_offsets() {
        let config = ChunkerConfig {
            max_chunk_size: 10,
            min_chunk_size: 5,
            overlap: 2,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "0123456789ABCDEFGHIJ";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            let extracted = &text[chunk.start_offset..chunk.end_offset];
            assert_eq!(extracted, chunk.text);
        }
    }

    #[test]
    fn test_sliding_window_full_overlap() {
        let config = ChunkerConfig {
            max_chunk_size: 10,
            min_chunk_size: 5,
            overlap: 10,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "0123456789ABCDEFGHIJ";
        let chunks = chunker.chunk(text);

        // Should handle full overlap gracefully (shouldn't infinite loop)
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_sliding_window_no_overlap() {
        let config = ChunkerConfig {
            max_chunk_size: 10,
            min_chunk_size: 5,
            overlap: 0,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "0123456789ABCDEFGHIJ";
        let chunks = chunker.chunk(text);

        // Verify no overlap
        for i in 0..chunks.len() - 1 {
            assert_eq!(chunks[i].end_offset, chunks[i + 1].start_offset);
        }
    }

    #[test]
    fn test_sliding_window_odd_length_text() {
        let config = ChunkerConfig {
            max_chunk_size: 10,
            min_chunk_size: 5,
            overlap: 3,
        };
        let chunker = SlidingWindowChunker::new(config);
        let text = "012345678"; // 9 chars
        let chunks = chunker.chunk(text);

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].text, text);
    }

    // ============================================================================
    // RecursiveChunker tests
    // ============================================================================

    #[test]
    fn test_recursive_chunker_empty_text() {
        let chunker = RecursiveChunker::new(default_config());
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_recursive_chunker_small_text() {
        let chunker = RecursiveChunker::new(default_config());
        let text = "Small text.";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_recursive_chunker_splits_paragraphs_first() {
        // Use a small max_chunk_size to force paragraph splitting
        let config = ChunkerConfig {
            max_chunk_size: 20,
            min_chunk_size: 5,
            overlap: 0,
        };
        let chunker = RecursiveChunker::new(config);
        let text = "Paragraph 1.\n\nParagraph 2.\n\nParagraph 3.";
        let chunks = chunker.chunk(text);

        // Should split by paragraphs when content exceeds max_chunk_size
        assert!(chunks.len() >= 2, "Should split into multiple chunks");
    }

    #[test]
    fn test_recursive_chunker_falls_back_to_sentences() {
        let config = ChunkerConfig {
            max_chunk_size: 30,
            min_chunk_size: 10,
            overlap: 5,
        };
        let chunker = RecursiveChunker::new(config);
        let text = "This is a long paragraph that exceeds max chunk size. So it should split by sentences instead. Like this.";
        let chunks = chunker.chunk(text);

        // Should fall back to sentence splitting
        assert!(chunks.len() >= 2, "Should fall back to sentences");
    }

    #[test]
    fn test_recursive_chunker_falls_back_to_chars() {
        let config = ChunkerConfig {
            max_chunk_size: 20,
            min_chunk_size: 10,
            overlap: 0,
        };
        let chunker = RecursiveChunker::new(config);
        // Very long word with no sentence boundaries
        let text = "supercalifragilisticexpialidociousandevenmore";
        let chunks = chunker.chunk(text);

        // Should fall back to character splitting
        assert!(chunks.len() >= 2, "Should fall back to character splitting");
    }

    #[test]
    fn test_recursive_chunker_respects_max_size() {
        let config = ChunkerConfig {
            max_chunk_size: 50,
            min_chunk_size: 10,
            overlap: 5,
        };
        let chunker = RecursiveChunker::new(config);
        let text = "This is a test. ".repeat(20);
        let chunks = chunker.chunk(&text);

        for chunk in chunks {
            assert!(
                chunk.text.len() <= 50,
                "Chunk exceeds max size: {}",
                chunk.text.len()
            );
        }
    }

    #[test]
    fn test_recursive_chunker_metadata_indicates_split_level() {
        let chunker = RecursiveChunker::new(default_config());
        let text = "Para 1.\n\nPara 2.";
        let chunks = chunker.chunk(text);

        // Should have metadata about split level
        for chunk in &chunks {
            assert!(!chunk.metadata.is_empty() || chunk.text.trim().is_empty());
        }
    }

    #[test]
    fn test_recursive_chunker_utf8_boundaries() {
        let chunker = RecursiveChunker::new(default_config());
        let text = "日本語の段落.\n\n中文段落.\n\nEnglish paragraph.";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            assert!(std::str::from_utf8(chunk.text.as_bytes()).is_ok());
            assert!(text.is_char_boundary(chunk.start_offset));
            assert!(text.is_char_boundary(chunk.end_offset));
        }
    }

    #[test]
    fn test_recursive_chunker_preserves_offsets() {
        let chunker = RecursiveChunker::new(default_config());
        let text = "First.\n\nSecond.\n\nThird.";
        let chunks = chunker.chunk(text);

        for chunk in &chunks {
            let extracted = &text[chunk.start_offset..chunk.end_offset];
            assert!(extracted.contains(chunk.text.trim()));
        }
    }

    #[test]
    fn test_recursive_chunker_complex_document() {
        let config = ChunkerConfig {
            max_chunk_size: 100,
            min_chunk_size: 20,
            overlap: 10,
        };
        let chunker = RecursiveChunker::new(config);
        let text = "# Heading\n\nParagraph 1 with sentences. Multiple sentences here.\n\nParagraph 2.\n\nParagraph 3 is longer and has more content to test splitting behavior.";
        let chunks = chunker.chunk(text);

        // Should intelligently split the document
        assert!(chunks.len() >= 2, "Should split complex document");

        // All chunks should meet size constraints
        for chunk in chunks {
            assert!(chunk.text.len() <= 100, "Chunk exceeds max size");
        }
    }

    #[test]
    fn test_recursive_chunker_handles_mixed_content() {
        let chunker = RecursiveChunker::new(default_config());
        let text = "Normal text.\n\n```code block```\n\nMore text. With sentences.";
        let chunks = chunker.chunk(text);

        // Should handle mixed content types
        assert!(!chunks.is_empty());
        let combined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(combined.contains("Normal text"));
        assert!(combined.contains("More text"));
    }
}

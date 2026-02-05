//! Service for managing document chunking in the note creation flow.
//!
//! This service handles automatic splitting of oversized content into multiple
//! linked notes, enabling proper processing of large documents that exceed
//! the model's context window.
//!
//! ## Chunk Metadata
//!
//! When a note is chunked, the `chunk_metadata` JSONB field in the `note` table
//! should be populated with information about the chunking operation. This allows
//! the system to track which notes are chunks and how they relate to each other.
//!
//! ### Example Metadata Structure
//!
//! For a parent note that has been split into chunks:
//! ```json
//! {
//!   "total_chunks": 3,
//!   "chunking_strategy": "semantic",
//!   "chunk_sequence": ["uuid-1", "uuid-2", "uuid-3"],
//!   "overlap_tokens": 50
//! }
//! ```
//!
//! For individual chunk notes:
//! ```json
//! {
//!   "parent_note_id": "parent-uuid",
//!   "chunk_index": 0,
//!   "total_chunks": 3,
//!   "chunking_strategy": "semantic"
//! }
//! ```
//!
//! This metadata enables:
//! - Reconstruction of the original document from chunks
//! - Navigation between related chunks
//! - Tracking of chunking strategies for analytics
//! - Efficient querying of chunked vs. non-chunked notes

use matric_core::Tokenizer;
use matric_db::chunking::{Chunk, Chunker, ChunkerConfig, SemanticChunker};

/// Service for document chunking operations.
pub struct ChunkingService {
    chunker: SemanticChunker,
    tokenizer: Box<dyn Tokenizer>,
}

impl ChunkingService {
    /// Create a new chunking service with the given configuration and tokenizer.
    pub fn new(config: ChunkerConfig, tokenizer: Box<dyn Tokenizer>) -> Self {
        Self {
            chunker: SemanticChunker::new(config),
            tokenizer,
        }
    }

    /// Check if content exceeds the given token limit and should be chunked.
    ///
    /// This uses the tokenizer to get an accurate token count of the content.
    ///
    /// # Arguments
    /// * `content` - The content to check
    /// * `limit` - The token limit to check against
    ///
    /// # Returns
    /// `true` if the content exceeds the limit and should be chunked
    pub fn should_chunk(&self, content: &str, limit: usize) -> bool {
        let token_count = self.tokenizer.count_tokens(content);
        token_count > limit
    }

    /// Chunk a document into smaller pieces using semantic chunking.
    ///
    /// This uses the SemanticChunker to split the content at natural
    /// boundaries (headings, paragraphs, code blocks, etc.).
    ///
    /// # Arguments
    /// * `content` - The content to chunk
    ///
    /// # Returns
    /// Vector of chunks with their text and metadata
    ///
    /// # Chunk Metadata Population
    ///
    /// When creating notes from these chunks, populate the `chunk_metadata`
    /// field in the note table to track the chunking relationship. See the
    /// module-level documentation for the expected metadata structure.
    pub fn chunk_document(&self, content: &str) -> Vec<Chunk> {
        self.chunker.chunk(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::tokenizer::TiktokenTokenizer;

    // Mock tokenizer for testing
    struct MockTokenizer {
        chars_per_token: usize,
    }

    impl Tokenizer for MockTokenizer {
        fn count_tokens(&self, text: &str) -> usize {
            text.len().div_ceil(self.chars_per_token)
        }

        fn encode(&self, text: &str) -> Vec<u32> {
            (0..self.count_tokens(text)).map(|i| i as u32).collect()
        }

        fn decode(&self, _tokens: &[u32]) -> String {
            String::new()
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    fn create_test_service() -> ChunkingService {
        let config = ChunkerConfig {
            max_chunk_size: 500,
            min_chunk_size: 100,
            overlap: 50,
        };
        let tokenizer = Box::new(MockTokenizer { chars_per_token: 4 });
        ChunkingService::new(config, tokenizer)
    }

    #[test]
    fn test_should_chunk_under_limit() {
        let service = create_test_service();
        let content = "Short content.";

        // With mock tokenizer (4 chars/token): 14 chars = 4 tokens
        assert!(!service.should_chunk(content, 100));
    }

    #[test]
    fn test_should_chunk_over_limit() {
        let service = create_test_service();
        let content = "x".repeat(1000); // 1000 chars = 250 tokens with mock

        assert!(service.should_chunk(&content, 100));
    }

    #[test]
    fn test_should_chunk_at_limit() {
        let service = create_test_service();
        let content = "x".repeat(400); // 400 chars = 100 tokens with mock

        // Exactly at limit should not chunk
        assert!(!service.should_chunk(&content, 100));
    }

    #[test]
    fn test_chunk_document_simple() {
        let service = create_test_service();
        let content = "# Heading\n\nParagraph 1\n\nParagraph 2";

        let chunks = service.chunk_document(content);

        assert!(!chunks.is_empty(), "Should produce chunks");

        // Verify all chunks have content
        for chunk in &chunks {
            assert!(!chunk.text.is_empty(), "Chunk should have text");
        }
    }

    #[test]
    fn test_chunk_document_respects_max_size() {
        let service = create_test_service();
        let long_paragraph = "word ".repeat(200); // 1000 chars

        let chunks = service.chunk_document(&long_paragraph);

        // All chunks should respect the max size
        for chunk in &chunks {
            assert!(
                chunk.text.len() <= 500,
                "Chunk exceeds max size: {} chars",
                chunk.text.len()
            );
        }
    }

    #[test]
    fn test_chunk_document_preserves_markdown_structure() {
        let service = create_test_service();
        let content = r#"# Heading 1
Content under heading 1

## Heading 2
Content under heading 2

```rust
fn main() {
    println!("Hello");
}
```

More content."#;

        let chunks = service.chunk_document(content);

        assert!(!chunks.is_empty(), "Should chunk markdown content");

        // Verify chunks have metadata
        for chunk in &chunks {
            assert!(
                !chunk.metadata.is_empty() || chunk.text.trim().is_empty(),
                "Chunks should have metadata"
            );
        }
    }

    #[test]
    fn test_chunk_document_empty_content() {
        let service = create_test_service();
        let chunks = service.chunk_document("");

        assert!(chunks.is_empty(), "Empty content should produce no chunks");
    }

    #[test]
    fn test_should_chunk_with_real_tokenizer() {
        let config = ChunkerConfig::default();
        let tokenizer =
            Box::new(TiktokenTokenizer::for_embeddings().expect("Failed to create tokenizer"));
        let service = ChunkingService::new(config, tokenizer);

        let short_text = "This is a short text.";
        assert!(!service.should_chunk(short_text, 10000));

        let long_text = "word ".repeat(5000); // Very long text
        assert!(service.should_chunk(&long_text, 1000));
    }

    #[test]
    fn test_chunk_offsets_are_valid() {
        let service = create_test_service();
        let content = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";

        let chunks = service.chunk_document(content);

        for chunk in &chunks {
            // Offsets should be within the content bounds
            assert!(chunk.start_offset <= content.len());
            assert!(chunk.end_offset <= content.len());
            assert!(chunk.start_offset < chunk.end_offset || chunk.text.is_empty());
        }
    }

    #[test]
    fn test_chunk_document_code_blocks() {
        let service = create_test_service();
        let content = r#"Before code

```python
def hello():
    print("Hello, world!")
    return True
```

After code"#;

        let chunks = service.chunk_document(content);

        // Code blocks should be preserved as single chunks when possible
        let has_code_chunk = chunks.iter().any(|c| c.text.contains("```"));
        assert!(has_code_chunk, "Should preserve code blocks");
    }

    #[test]
    fn test_service_creation_with_custom_config() {
        let config = ChunkerConfig {
            max_chunk_size: 1000,
            min_chunk_size: 200,
            overlap: 100,
        };
        let tokenizer = Box::new(MockTokenizer { chars_per_token: 4 });
        let service = ChunkingService::new(config, tokenizer);

        // Service should use the custom config
        let long_content = "x".repeat(2000);
        let chunks = service.chunk_document(&long_content);

        for chunk in &chunks {
            assert!(
                chunk.text.len() <= 1000,
                "Should respect custom max_chunk_size"
            );
        }
    }
}

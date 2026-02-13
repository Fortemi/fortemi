//! Centralized default constants for the matric-memory system.
//!
//! **This module is the single source of truth** for all shared default values.
//! All crates and the MCP server should reference these constants instead of
//! defining their own magic numbers.
//!
//! Organized by domain area. When adding new constants, place them in the
//! appropriate section and document the rationale for the chosen value.

// =============================================================================
// CHUNKING
// =============================================================================

/// Maximum characters per chunk for text splitting.
pub const CHUNK_SIZE: usize = 1000;

/// Minimum characters per chunk (smaller chunks may be merged).
pub const CHUNK_MIN_SIZE: usize = 100;

/// Overlap characters between adjacent chunks for context preservation.
pub const CHUNK_OVERLAP: usize = 100;

/// Chunk size as i32 (for serde default functions on DB-facing types).
pub const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;

/// Chunk overlap as i32 (for serde default functions on DB-facing types).
pub const CHUNK_OVERLAP_I32: i32 = CHUNK_OVERLAP as i32;

// =============================================================================
// EMBEDDING
// =============================================================================

/// Default embedding model name (Ollama).
pub const EMBED_MODEL: &str = "nomic-embed-text";

/// Default embedding vector dimension for nomic-embed-text.
pub const EMBED_DIMENSION: usize = 768;

// =============================================================================
// PAGINATION
// =============================================================================

/// Default page size for standard list endpoints (notes, tags, collections).
pub const PAGE_LIMIT: i64 = 50;

/// Default page size for large-result-set endpoints (health dashboard items).
pub const PAGE_LIMIT_LARGE: i64 = 100;

/// Default page size for search/memory endpoints.
pub const PAGE_LIMIT_SEARCH: i64 = 20;

/// Default page size for autocomplete/label search.
pub const PAGE_LIMIT_AUTOCOMPLETE: i64 = 10;

/// Internal "fetch everything" limit for aggregation queries.
pub const INTERNAL_FETCH_LIMIT: i64 = 10_000;

/// Default page offset.
pub const PAGE_OFFSET: i64 = 0;

// =============================================================================
// SNIPPET
// =============================================================================

/// Default snippet/preview length in characters for search results and lists.
pub const SNIPPET_LENGTH: usize = 200;

// =============================================================================
// SERVER
// =============================================================================

/// Default HTTP server port.
pub const SERVER_PORT: u16 = 3000;

/// Default rate limit: max requests per period.
pub const RATE_LIMIT_REQUESTS: u64 = 100;

/// Default rate limit: period in seconds.
pub const RATE_LIMIT_PERIOD_SECS: u64 = 60;

/// Default event bus broadcast channel capacity.
pub const EVENT_BUS_CAPACITY: usize = 256;

/// Default CORS max-age in seconds (1 hour).
pub const CORS_MAX_AGE_SECS: u64 = 3600;

/// Default webhook HTTP request timeout in seconds.
pub const WEBHOOK_TIMEOUT_SECS: u64 = 10;

/// Maximum request body size in bytes (2 GB, for database backups).
pub const MAX_BODY_SIZE_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Maximum notes per batch import.
pub const BATCH_IMPORT_MAX: usize = 100;

/// Default file storage inline threshold in bytes (1 MB).
pub const FILE_INLINE_THRESHOLD: usize = 1024 * 1024;

// =============================================================================
// INFERENCE
// =============================================================================

/// Default Ollama base URL.
pub const OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// Default generation model name (Ollama).
pub const GEN_MODEL: &str = "gpt-oss:20b";

/// Timeout for embedding requests in seconds.
pub const EMBED_TIMEOUT_SECS: u64 = 30;

/// Timeout for generation requests in seconds.
pub const GEN_TIMEOUT_SECS: u64 = 120;

// =============================================================================
// JOB PROCESSING
// =============================================================================

/// Default maximum retry count for failed jobs.
pub const JOB_MAX_RETRIES: i32 = 3;

/// Default auto-embed batch size.
pub const AUTO_EMBED_BATCH_SIZE: usize = 10;

/// Default auto-embed priority (1=highest, 10=lowest).
pub const AUTO_EMBED_PRIORITY: i32 = 5;

/// Default job worker poll interval in milliseconds.
pub const JOB_POLL_INTERVAL_MS: u64 = 500;

/// Default maximum concurrent jobs per worker.
pub const JOB_MAX_CONCURRENT: usize = 4;

/// Default job execution timeout in seconds (5 minutes).
pub const JOB_TIMEOUT_SECS: u64 = 300;

/// Page threshold for batch PDF extraction.
pub const LARGE_PDF_PAGE_THRESHOLD: usize = 100;

/// Pages per batch for large PDF extraction.
pub const PDF_BATCH_PAGES: usize = 50;

/// Per-command timeout for external extraction tools (seconds).
pub const EXTRACTION_CMD_TIMEOUT_SECS: u64 = 60;

// =============================================================================
// EXTRACTION SERVICE CONFIGURATION
// =============================================================================

/// Environment variable for the vision model name.
pub const ENV_OLLAMA_VISION_MODEL: &str = "OLLAMA_VISION_MODEL";

/// Environment variable for the Whisper transcription server URL.
pub const ENV_WHISPER_BASE_URL: &str = "WHISPER_BASE_URL";

/// Environment variable for the Whisper model name.
pub const ENV_WHISPER_MODEL: &str = "WHISPER_MODEL";

/// Default Whisper model.
pub const DEFAULT_WHISPER_MODEL: &str = "Systran/faster-distil-whisper-large-v3";

/// Environment variable to enable OCR processing.
pub const ENV_OCR_ENABLED: &str = "OCR_ENABLED";

/// Environment variable for LibreOffice path.
pub const ENV_LIBREOFFICE_PATH: &str = "LIBREOFFICE_PATH";

/// Default maximum text extraction size in bytes (10 MB).
pub const TEXT_EXTRACTION_MAX_BYTES: usize = 10 * 1024 * 1024;

// =============================================================================
// SEARCH
// =============================================================================

/// Default number of stale days for health dashboard queries.
pub const STALE_DAYS: i64 = 90;

/// Default number of periods for trend analysis.
pub const TREND_PERIODS: i64 = 30;

// =============================================================================
// TWO-STAGE RETRIEVAL
// =============================================================================

/// Default coarse embedding dimension for two-stage search.
pub const COARSE_DIM: i32 = 64;

/// Default coarse stage top-k results.
pub const COARSE_K: i32 = 100;

/// Default coarse stage HNSW ef_search.
pub const COARSE_EF_SEARCH: i32 = 64;

// =============================================================================
// TRI-MODAL FUSION WEIGHTS
// =============================================================================

/// Default semantic (dense vector) search weight.
pub const TRIMODAL_SEMANTIC_WEIGHT: f32 = 0.5;

/// Default lexical (FTS/BM25) search weight.
pub const TRIMODAL_LEXICAL_WEIGHT: f32 = 0.3;

/// Default graph (entity) search weight.
pub const TRIMODAL_GRAPH_WEIGHT: f32 = 0.2;

// =============================================================================
// FINE-TUNING
// =============================================================================

/// Default queries generated per document for fine-tuning.
pub const FINETUNE_QUERIES_PER_DOC: i32 = 4;

/// Default minimum quality score for fine-tuning samples.
pub const FINETUNE_MIN_QUALITY: f32 = 4.0;

/// Default validation split fraction (0.0-1.0).
pub const FINETUNE_VALIDATION_SPLIT: f32 = 0.1;

// =============================================================================
// MEMORY / ARCHIVE LIMITS
// =============================================================================

/// Maximum number of memories (archives) that can be created.
/// Configurable via `MAX_MEMORIES` env var.
pub const MAX_MEMORIES: i64 = 100;

// =============================================================================
// CROSS-ARCHIVE SEARCH
// =============================================================================

/// Default result limit for cross-archive search.
pub const CROSS_ARCHIVE_LIMIT: i64 = 20;

// =============================================================================
// TAGS
// =============================================================================

/// Maximum tag name length in characters.
pub const TAG_NAME_MAX_LENGTH: usize = 100;

// =============================================================================
// FILE SAFETY
// =============================================================================

/// Maximum file upload size in bytes (50 MB).
/// Configurable via `MATRIC_MAX_UPLOAD_SIZE_BYTES` env var.
/// This limit is enforced at three layers:
/// 1. Axum `DefaultBodyLimit` on the multipart upload route
/// 2. `validate_file()` size check in both upload handlers
/// 3. Advertised to agents via MCP `upload_attachment` response
pub const MAX_UPLOAD_SIZE_BYTES: usize = 50 * 1024 * 1024;

/// Maximum filename length (ext4/NTFS compatible).
pub const FILENAME_MAX_LENGTH: usize = 255;

// =============================================================================
// OAUTH
// =============================================================================

/// Default OAuth scope for new API keys.
pub const OAUTH_DEFAULT_SCOPE: &str = "read";

/// Default OAuth access token lifetime in seconds (1 hour).
pub const OAUTH_TOKEN_LIFETIME_SECS: u64 = 3600;

/// Default MCP OAuth access token lifetime in seconds (24 hours).
/// MCP sessions are long-lived interactive sessions. Combined with the sliding
/// window refresh (extends expiry on each authenticated request), this ensures
/// sessions stay alive as long as there's activity within 24 hours.
pub const OAUTH_MCP_TOKEN_LIFETIME_SECS: u64 = 86400;

// =============================================================================
// VERSIONING
// =============================================================================

/// Default maximum history versions kept per note.
pub const MAX_HISTORY_VERSIONS: i32 = 50;

// =============================================================================
// SIMILARITY THRESHOLDS (Tier 2 — Algorithm Parameters)
// =============================================================================

/// Minimum similarity score for creating semantic links between notes (prose/general).
pub const SEMANTIC_LINK_THRESHOLD: f32 = 0.7;

/// Minimum similarity score for creating semantic links between code-category notes.
/// Code embeddings (Rust, Python, JS etc.) cluster more tightly than prose, so a
/// higher threshold prevents false-positive links between unrelated code files.
pub const SEMANTIC_LINK_THRESHOLD_CODE: f32 = 0.85;

/// Minimum similarity score for context-update link filtering (stricter).
pub const CONTEXT_LINK_THRESHOLD: f32 = 0.75;

/// Minimum similarity score for including related notes in AI context.
pub const RELATED_NOTES_MIN_SIMILARITY: f32 = 0.5;

/// Confidence score for AI auto-tagging operations.
pub const AI_TAGGING_CONFIDENCE: f32 = 0.8;

/// Relevance decay factor per rank position in concept tagging.
pub const RELEVANCE_DECAY_FACTOR: f32 = 0.1;

/// Returns the semantic link threshold appropriate for the given document category.
///
/// Code-like categories (Code, Shell, Config, Iac, Database, Package) use a
/// stricter threshold because embedding models place programming-language
/// content closer together in vector space regardless of actual relatedness.
pub fn semantic_link_threshold_for(category: crate::models::DocumentCategory) -> f32 {
    use crate::models::DocumentCategory;
    match category {
        DocumentCategory::Code
        | DocumentCategory::Shell
        | DocumentCategory::Config
        | DocumentCategory::Iac
        | DocumentCategory::Database
        | DocumentCategory::Package => SEMANTIC_LINK_THRESHOLD_CODE,
        _ => SEMANTIC_LINK_THRESHOLD,
    }
}

// =============================================================================
// CONTENT PREVIEW SIZES (Tier 2)
// =============================================================================

/// Characters of content preview for embedding and title generation.
pub const PREVIEW_EMBEDDING: usize = 500;

/// Characters of snippet preview in AI context prompts.
pub const PREVIEW_CONTEXT_SNIPPET: usize = 150;

/// Characters of label preview in concept displays.
pub const PREVIEW_LABEL: usize = 100;

/// Characters of content preview for concept tagging analysis.
pub const PREVIEW_TAGGING: usize = 2000;

/// Characters of content preview for linked note context.
pub const PREVIEW_LINKED_NOTE: usize = 200;

// =============================================================================
// TITLE GENERATION (Tier 2)
// =============================================================================

/// Maximum length of AI-generated titles in characters.
pub const TITLE_MAX_LENGTH: usize = 80;

/// Minimum length of a valid AI-generated title in characters.
pub const TITLE_MIN_LENGTH: usize = 3;

/// Minimum concepts to suggest for auto-tagging.
pub const TAG_MIN_CONCEPTS: usize = 3;

/// Maximum concepts to suggest for auto-tagging.
pub const TAG_MAX_CONCEPTS: usize = 7;

// =============================================================================
// HEALTH SCORE WEIGHTS (Tier 2)
// =============================================================================

/// Weight of stale notes ratio in health score calculation (0-100 scale).
pub const HEALTH_WEIGHT_STALE: f64 = 30.0;

/// Weight of unlinked notes ratio in health score calculation.
pub const HEALTH_WEIGHT_UNLINKED: f64 = 40.0;

/// Weight of untagged notes ratio in health score calculation.
pub const HEALTH_WEIGHT_UNTAGGED: f64 = 30.0;

// =============================================================================
// HEALTH SCORE WEIGHTS (Tier 2)
// =============================================================================

/// Confidence when detected by filename pattern match (highest).
pub const DETECT_CONFIDENCE_FILENAME: f32 = 1.0;

/// Confidence when detected by MIME type.
pub const DETECT_CONFIDENCE_MIME: f32 = 0.95;

/// Confidence when detected by file extension.
pub const DETECT_CONFIDENCE_EXTENSION: f32 = 0.9;

/// Confidence when detected by content/magic pattern.
pub const DETECT_CONFIDENCE_CONTENT: f32 = 0.7;

/// Confidence for default fallback detection (lowest).
pub const DETECT_CONFIDENCE_DEFAULT: f32 = 0.1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_defaults_are_consistent() {
        // Use const block to satisfy clippy::assertions_on_constants
        const {
            assert!(CHUNK_SIZE == CHUNK_SIZE_I32 as usize);
            assert!(CHUNK_OVERLAP == CHUNK_OVERLAP_I32 as usize);
            assert!(CHUNK_MIN_SIZE < CHUNK_SIZE);
            assert!(CHUNK_OVERLAP < CHUNK_SIZE);
        }
    }

    #[test]
    fn trimodal_weights_sum_to_one() {
        // Runtime check needed for floating point arithmetic
        let sum = TRIMODAL_SEMANTIC_WEIGHT + TRIMODAL_LEXICAL_WEIGHT + TRIMODAL_GRAPH_WEIGHT;
        assert!((sum - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn health_weights_sum_to_100() {
        let sum = HEALTH_WEIGHT_STALE + HEALTH_WEIGHT_UNLINKED + HEALTH_WEIGHT_UNTAGGED;
        assert!((sum - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn detection_confidence_ordered() {
        // Runtime check needed for floating point comparisons
        let values = [
            DETECT_CONFIDENCE_DEFAULT,
            DETECT_CONFIDENCE_CONTENT,
            DETECT_CONFIDENCE_EXTENSION,
            DETECT_CONFIDENCE_MIME,
            DETECT_CONFIDENCE_FILENAME,
        ];
        for w in values.windows(2) {
            assert!(
                w[0] < w[1] || (w[0] - w[1]).abs() < f32::EPSILON,
                "Expected {} < {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn pagination_limits_ordered() {
        const {
            assert!(PAGE_LIMIT_AUTOCOMPLETE < PAGE_LIMIT_SEARCH);
            assert!(PAGE_LIMIT_SEARCH < PAGE_LIMIT);
            assert!(PAGE_LIMIT < PAGE_LIMIT_LARGE);
            assert!(PAGE_LIMIT_LARGE < INTERNAL_FETCH_LIMIT);
        }
    }

    #[test]
    fn code_threshold_stricter_than_default() {
        assert!(
            SEMANTIC_LINK_THRESHOLD_CODE > SEMANTIC_LINK_THRESHOLD,
            "Code threshold ({}) must be stricter than default ({})",
            SEMANTIC_LINK_THRESHOLD_CODE,
            SEMANTIC_LINK_THRESHOLD
        );
    }

    #[test]
    fn semantic_link_threshold_for_categories() {
        use crate::models::DocumentCategory;

        // Code-like categories get stricter threshold
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Code),
            SEMANTIC_LINK_THRESHOLD_CODE
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Shell),
            SEMANTIC_LINK_THRESHOLD_CODE
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Config),
            SEMANTIC_LINK_THRESHOLD_CODE
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Iac),
            SEMANTIC_LINK_THRESHOLD_CODE
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Database),
            SEMANTIC_LINK_THRESHOLD_CODE
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Package),
            SEMANTIC_LINK_THRESHOLD_CODE
        );

        // Non-code categories get default threshold
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Prose),
            SEMANTIC_LINK_THRESHOLD
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Docs),
            SEMANTIC_LINK_THRESHOLD
        );
        assert_eq!(
            semantic_link_threshold_for(DocumentCategory::Creative),
            SEMANTIC_LINK_THRESHOLD
        );
    }
}

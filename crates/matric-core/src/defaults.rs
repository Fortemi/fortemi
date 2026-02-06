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

/// Maximum filename length (ext4/NTFS compatible).
pub const FILENAME_MAX_LENGTH: usize = 255;

// =============================================================================
// OAUTH
// =============================================================================

/// Default OAuth scope for new API keys.
pub const OAUTH_DEFAULT_SCOPE: &str = "read";

// =============================================================================
// VERSIONING
// =============================================================================

/// Default maximum history versions kept per note.
pub const MAX_HISTORY_VERSIONS: i32 = 50;

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
    fn pagination_limits_ordered() {
        const {
            assert!(PAGE_LIMIT_AUTOCOMPLETE < PAGE_LIMIT_SEARCH);
            assert!(PAGE_LIMIT_SEARCH < PAGE_LIMIT);
            assert!(PAGE_LIMIT < PAGE_LIMIT_LARGE);
            assert!(PAGE_LIMIT_LARGE < INTERNAL_FETCH_LIMIT);
        }
    }
}

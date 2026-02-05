//! Structured logging schema and field name constants for matric-memory.
//!
//! All crates use these constants for consistent structured logging fields.
//! This ensures log aggregation tools (Loki, Elasticsearch) can query by
//! standardized field names across every subsystem.
//!
//! ## Log Level Contract
//!
//! | Level | Usage |
//! |-------|-------|
//! | ERROR | Degraded service, requires operator attention |
//! | WARN  | Recoverable issue, automatic fallback applied |
//! | INFO  | Lifecycle events (startup, shutdown), operation completions |
//! | DEBUG | Decision points, intermediate values, config choices |
//! | TRACE | Per-item iteration, high-volume data (search hits, chunks) |

// ─── Identity fields ───────────────────────────────────────────────────────

/// Correlation ID propagated across request → job → sub-calls.
/// Format: UUIDv7 (time-ordered).
pub const REQUEST_ID: &str = "request_id";

/// Subsystem originating the log event.
/// Values: "api", "search", "db", "inference", "jobs", "crypto"
pub const SUBSYSTEM: &str = "subsystem";

/// Component within a subsystem.
/// Examples: "hybrid_search", "rrf_fusion", "ollama", "pool", "worker"
pub const COMPONENT: &str = "component";

/// Logical operation name.
/// Examples: "search", "embed_texts", "generate", "claim_next"
pub const OPERATION: &str = "op";

// ─── Entity fields ─────────────────────────────────────────────────────────

/// Note UUID being operated on.
pub const NOTE_ID: &str = "note_id";

/// Job UUID being processed.
pub const JOB_ID: &str = "job_id";

/// Job type enum variant.
pub const JOB_TYPE: &str = "job_type";

/// Search query text.
pub const QUERY: &str = "query";

// ─── Measurement fields ────────────────────────────────────────────────────

/// Wall-clock duration in milliseconds.
pub const DURATION_MS: &str = "duration_ms";

/// Number of results returned by a search or query.
pub const RESULT_COUNT: &str = "result_count";

/// Number of chunks processed (embedding, chunking).
pub const CHUNK_COUNT: &str = "chunk_count";

/// Number of input texts sent to an embedding model.
pub const INPUT_COUNT: &str = "input_count";

/// Byte length of a prompt or response.
pub const PROMPT_LEN: &str = "prompt_len";

/// Byte length of a model response.
pub const RESPONSE_LEN: &str = "response_len";

// ─── Search-specific fields ────────────────────────────────────────────────

/// Number of FTS results before fusion.
pub const FTS_HITS: &str = "fts_hits";

/// Number of semantic results before fusion.
pub const SEMANTIC_HITS: &str = "semantic_hits";

/// FTS weight used in hybrid search.
pub const FTS_WEIGHT: &str = "fts_weight";

/// Semantic weight used in hybrid search.
pub const SEMANTIC_WEIGHT: &str = "semantic_weight";

/// Fusion method used ("rrf", "rsf").
pub const FUSION_METHOD: &str = "fusion_method";

/// RRF k parameter.
pub const RRF_K: &str = "rrf_k";

// ─── Database fields ───────────────────────────────────────────────────────

/// Number of active connections in the pool.
pub const POOL_SIZE: &str = "pool_size";

/// Number of idle connections in the pool.
pub const POOL_IDLE: &str = "pool_idle";

/// Database table or entity affected.
pub const DB_TABLE: &str = "db_table";

// ─── Inference fields ──────────────────────────────────────────────────────

/// Model name used for inference.
pub const MODEL: &str = "model";

/// Whether raw mode was used for generation.
pub const RAW_MODE: &str = "raw_mode";

// ─── Outcome fields ────────────────────────────────────────────────────────

/// Boolean success/failure indicator.
pub const SUCCESS: &str = "success";

/// Error message when an operation fails.
pub const ERROR_MSG: &str = "error";

/// Slow operation threshold exceeded.
pub const SLOW: &str = "slow";

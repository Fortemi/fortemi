//! # matric-search
//!
//! Hybrid search engine (FTS + semantic) for matric-memory.
//!
//! This crate provides:
//! - Full-text search using PostgreSQL tsvector/GIN
//! - Semantic search using pgvector similarity
//! - Hybrid search with Reciprocal Rank Fusion (RRF)
//! - Search result deduplication for chunked documents
//!
//! ## Example
//!
//! ```ignore
//! use matric_search::{HybridSearchEngine, HybridSearchConfig, SearchRequest};
//! use matric_db::Database;
//!
//! let db = Database::connect("postgres://...").await?;
//! let engine = HybridSearchEngine::new(db);
//!
//! // Simple hybrid search
//! let results = SearchRequest::new("machine learning")
//!     .with_embedding(query_vector)
//!     .with_limit(20)
//!     .execute(&engine)
//!     .await?;
//!
//! // FTS-only search
//! let results = SearchRequest::new("rust programming")
//!     .fts_only()
//!     .execute(&engine)
//!     .await?;
//! ```

pub mod adaptive_rrf;
pub mod deduplication;
pub mod hnsw_tuning;
pub mod hybrid;
pub mod rrf;

// Re-export core types
pub use matric_core::*;

// Re-export search types
pub use adaptive_rrf::{rrf_score, select_k, AdaptiveRrfConfig, QueryCharacteristics};
pub use deduplication::{ChainSearchInfo, DeduplicationConfig, EnhancedSearchHit};
pub use hnsw_tuning::{
    compute_ef, estimated_latency_ms, estimated_recall, HnswTuningConfig, RecallTarget,
};
pub use hybrid::{HybridSearch, HybridSearchConfig, HybridSearchEngine, SearchRequest};
pub use rrf::*;

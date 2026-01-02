//! # matric-search
//!
//! Hybrid search engine (FTS + semantic) for matric-memory.
//!
//! This crate provides:
//! - Full-text search using PostgreSQL tsvector/GIN
//! - Semantic search using pgvector similarity
//! - Hybrid search with Reciprocal Rank Fusion (RRF)

pub mod rrf;

// Re-export core types
pub use matric_core::*;
pub use rrf::*;

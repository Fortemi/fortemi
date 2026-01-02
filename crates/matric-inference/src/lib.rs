//! # matric-inference
//!
//! LLM inference backend abstraction for matric-memory.
//!
//! This crate provides:
//! - Pluggable inference backend trait
//! - Ollama implementation (default)
//! - OpenAI-compatible implementation (optional)

#[cfg(feature = "ollama")]
pub mod ollama;

// Re-export core types
pub use matric_core::*;

#[cfg(feature = "ollama")]
pub use ollama::OllamaBackend;

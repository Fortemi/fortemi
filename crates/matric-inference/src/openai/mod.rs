//! OpenAI-compatible inference backend.
//!
//! This module provides an inference backend that works with any
//! OpenAI-compatible API endpoint, including:
//!
//! - OpenAI cloud API
//! - Azure OpenAI
//! - Ollama (in OpenAI compatibility mode)
//! - vLLM
//! - LocalAI
//! - LM Studio
//!
//! # Example
//!
//! ```rust,no_run
//! use matric_inference::openai::{OpenAIBackend, OpenAIConfig};
//! use matric_core::EmbeddingBackend;
//!
//! #[tokio::main]
//! async fn main() {
//!     // From environment variables
//!     let backend = OpenAIBackend::from_env().unwrap();
//!
//!     // Or with custom config
//!     let config = OpenAIConfig {
//!         base_url: "http://localhost:11434/v1".to_string(), // Ollama
//!         api_key: None, // Not needed for local
//!         embed_model: "nomic-embed-text".to_string(),
//!         gen_model: "llama3".to_string(),
//!         embed_dimension: 768,
//!         timeout_seconds: 120,
//!         skip_tls_verify: false,
//!         http_referer: None,
//!         x_title: None,
//!     };
//!     let backend = OpenAIBackend::new(config).unwrap();
//!
//!     // Use like any other embedding backend
//!     let texts = vec!["Hello, world!".to_string()];
//!     let vectors = backend.embed_texts(&texts).await.unwrap();
//! }
//! ```

mod backend;
mod error;
mod streaming;
mod types;

pub use backend::{
    OpenAIBackend, OpenAIConfig, DEFAULT_DIMENSION, DEFAULT_EMBED_MODEL, DEFAULT_GEN_MODEL,
    DEFAULT_OPENAI_URL, DEFAULT_TIMEOUT_SECS,
};
pub use error::{to_matric_error, OpenAIErrorCode};
pub use streaming::{parse_sse_stream, StreamingGeneration, TokenStream};
pub use types::*;

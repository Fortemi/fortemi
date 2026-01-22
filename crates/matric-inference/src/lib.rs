//! # matric-inference
//!
//! LLM inference backend abstraction for matric-memory.
//!
//! This crate provides:
//! - Pluggable inference backend trait
//! - Ollama implementation (default)
//! - OpenAI-compatible implementation (optional, feature `openai`)
//! - Model-specific configuration for thinking models
//! - Model performance profiles and registry
//! - Thinking model detection and response parsing
//! - Model restriction and validation
//!
//! # Feature Flags
//!
//! - `ollama` (default): Enable Ollama backend
//! - `openai`: Enable OpenAI-compatible backend
//!
//! # Example
//!
//! ```rust,no_run
//! use matric_inference::OllamaBackend;
//! use matric_core::EmbeddingBackend;
//!
//! #[tokio::main]
//! async fn main() {
//!     let backend = OllamaBackend::from_env();
//!     let texts = vec!["Hello".to_string()];
//!     let embeddings = backend.embed_texts(&texts).await.unwrap();
//! }
//! ```

pub mod model_config;
pub mod profiles;
pub mod thinking;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "openai")]
pub mod openai;

// Re-export core types
pub use matric_core::*;

#[cfg(feature = "ollama")]
pub use ollama::OllamaBackend;

#[cfg(feature = "openai")]
pub use openai::{OpenAIBackend, OpenAIConfig};

pub use model_config::{
    is_model_restricted, validate_model, ModelRestriction, ModelValidationError, RestrictionType,
};
pub use profiles::{ModelProfile, ModelRegistry, TaskRequirements, ThinkingType};
pub use thinking::{detect_thinking_type, parse_thinking_response, ThinkingResponse};

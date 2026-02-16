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
//! - Model capability flags for knowledge management tasks
//! - Thinking model detection and response parsing
//! - Model restriction and validation
//! - Vision backend for image description
//! - Transcription backend for audio-to-text
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

pub mod capabilities;
pub mod discovery;
pub mod embedding_models;
pub mod eval;
pub mod few_shot;
pub mod hardware;
pub mod latency;
pub mod link_types;
pub mod model_config;
pub mod profiles;
pub mod provider;
pub mod refinement;
pub mod selector;
pub mod thinking;
pub mod transcription;
pub mod vision;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "openai")]
pub mod openai;

// Mock inference backend for testing
#[cfg(test)]
pub mod mock;

// Re-export core types
pub use matric_core::*;

#[cfg(feature = "ollama")]
pub use ollama::OllamaBackend;

#[cfg(feature = "openai")]
pub use openai::{OpenAIBackend, OpenAIConfig};

pub use capabilities::{
    known_model_capabilities, Capability, CapabilityRating, ModelCapabilities, QualityTier,
};
pub use discovery::{
    ConfigRecommendation, DiscoveredModel, DiscoveryError, DiscoveryResult, ModelDiscovery,
};
pub use embedding_models::{EmbeddingModelProfile, EmbeddingModelRegistry, EmbeddingSymmetry};
pub use eval::{
    content_revision_suite, cosine_similarity, evaluate_semantic, evaluate_title,
    semantic_similarity_suite, title_generation_suite, EvalReport, EvalResult, EvalSummary,
    RevisionTestCase, SemanticTestCase, TitleTestCase,
};
pub use few_shot::{
    default_revision_examples, default_title_examples, ExampleType, FewShotConfig, FewShotExample,
    FewShotPromptBuilder, SelectionStrategy,
};
pub use hardware::{
    cloud_comparisons, tier_model_recommendations, tier_quality_expectations, CloudComparison,
    HardwareTier, ModelRecommendation, OllamaSettings, SystemCapabilities, TierQualityExpectations,
};
pub use latency::{
    BatchEmbeddingConfig, ChunkingStrategy, ContextConfig, ContextOptimizer, LatencyOptimization,
    LatencyStats, LatencyTracker,
};
pub use link_types::{
    link_classification_prompt, parse_link_type, LinkClassification, SemanticLinkType,
};
pub use model_config::{
    is_model_restricted, validate_model, ModelRestriction, ModelValidationError, RestrictionType,
};
pub use profiles::{ModelProfile, ModelRegistry, TaskRequirements, ThinkingType};
pub use provider::{
    ParsedSlug, ProviderCapability, ProviderConfig, ProviderHealth, ProviderRegistry,
};
pub use refinement::{
    parse_quality_score, parse_react_response, react_revision_prompt, refine_with_critique_prompt,
    reflexion_prompt, self_critique_prompt, Episode, EpisodeOutcome, ReActStep, ReActTrace,
    RefineIteration, ReflexionMemory, SelfRefineConfig, SelfRefineResult,
};
pub use selector::{KmOperation, ModelSelection, ModelSelector, RecommendedConfig};
pub use thinking::{detect_thinking_type, parse_thinking_response, ThinkingResponse};
pub use transcription::{
    TranscriptionBackend, TranscriptionResult, TranscriptionSegment, WhisperBackend,
};
pub use vision::{OllamaVisionBackend, VisionBackend};

//! Handler modules for matric-api.
//!
//! This module contains HTTP handlers and background job handlers.

pub mod archives;
pub mod document_types;
pub mod jobs;
pub mod pke;

// Re-export job handlers for backwards compatibility
pub use jobs::{
    AiRevisionHandler, ConceptTaggingHandler, ContextUpdateHandler, EmbeddingHandler,
    LinkingHandler, PurgeNoteHandler, ReEmbedAllHandler, TitleGenerationHandler,
};

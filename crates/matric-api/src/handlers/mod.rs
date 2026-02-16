//! Handler modules for matric-api.
//!
//! This module contains HTTP handlers and background job handlers.

pub mod archives;
pub mod audio;
pub mod document_types;
pub mod jobs;
pub mod pke;
pub mod provenance;
pub mod vision;

// Re-export job handlers for backwards compatibility
pub use jobs::{
    AiRevisionHandler, ConceptTaggingHandler, ContextUpdateHandler,
    DocumentTypeInferenceHandler, EmbeddingHandler, ExifExtractionHandler, LinkingHandler,
    MetadataExtractionHandler, PurgeNoteHandler, ReEmbedAllHandler,
    RefreshEmbeddingSetHandler, TitleGenerationHandler,
};

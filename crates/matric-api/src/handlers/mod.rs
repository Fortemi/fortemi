//! Handler modules for matric-api.
//!
//! This module contains HTTP handlers and background job handlers.

pub mod archives;
pub mod audio;
pub mod chat;
pub mod document_types;
pub mod event_tokens;
pub mod inference_complete;
pub mod inference_config;
pub mod ingest_stream;
pub mod ingest_tokens;
pub mod jobs;
pub mod memory_export;
pub mod models;
pub mod pke;
pub mod provenance;
pub mod token_info;
pub mod user_principal;
#[cfg(feature = "hosted-auth")]
pub mod user_secrets;
pub mod vector_import;
pub mod vision;

// Re-export job handlers for backwards compatibility
pub use jobs::{
    AiRevisionContextualHandler, AiRevisionHandler, BuildVectorIndexHandler, ConceptTaggingHandler,
    ContextUpdateHandler, DocumentTypeInferenceHandler, EmbeddingHandler, ExifExtractionHandler,
    GraphMaintenanceHandler, LinkingHandler, MetadataExtractionHandler, PurgeNoteHandler,
    ReEmbedAllHandler, ReferenceExtractionHandler, RefreshEmbeddingSetHandler,
    RelatedConceptHandler, TitleGenerationHandler,
};

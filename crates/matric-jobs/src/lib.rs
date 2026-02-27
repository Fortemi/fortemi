//! # matric-jobs
//!
//! Background job queue system for matric-memory.
//!
//! This crate provides:
//! - Priority-based job queueing
//! - Async job processing with concurrent workers
//! - Progress tracking and notifications via broadcast channels
//! - Retry logic with configurable limits
//!
//! ## Example
//!
//! ```ignore
//! use matric_jobs::{JobWorker, WorkerConfig, WorkerBuilder, NoOpHandler};
//! use matric_db::Database;
//! use matric_core::JobType;
//!
//! let db = Database::connect("postgres://...").await?;
//!
//! // Create worker with handlers
//! let worker = WorkerBuilder::new(db)
//!     .with_config(WorkerConfig::default().with_poll_interval(1000))
//!     .with_handler(NoOpHandler::new(JobType::Embedding))
//!     .build()
//!     .await;
//!
//! // Start worker and get handle
//! let handle = worker.start();
//!
//! // Listen for events
//! let mut events = handle.events();
//! while let Ok(event) = events.recv().await {
//!     println!("Event: {:?}", event);
//! }
//!
//! // Graceful shutdown
//! handle.shutdown().await?;
//! ```

pub mod adapters;
pub mod audio_chunk_handler;
pub mod audio_transcription_handler;
pub mod diarization_handler;
pub mod extraction;
pub mod extraction_handler;
pub mod handler;
pub mod keyframe_assembly_handler;
pub mod keyframe_character_vision_handler;
pub mod keyframe_setting_vision_handler;
pub mod keyframe_vision_handler;
pub mod media_optimize_handler;
pub mod pause;
pub mod relabel_handler;
pub mod sprite_handler;
pub mod view_assembly_handler;
pub mod view_vision_handler;
pub mod worker;

// Re-export core types
pub use matric_core::*;

// Re-export job types
pub use audio_chunk_handler::AudioChunkTranscriptionHandler;
pub use audio_transcription_handler::AudioTranscriptionHandler;
pub use diarization_handler::SpeakerDiarizationHandler;
pub use extraction_handler::ExtractionHandler;
pub use handler::{JobContext, JobHandler, JobResult, NoOpHandler};
pub use keyframe_assembly_handler::KeyframeAssemblyHandler;
pub use keyframe_character_vision_handler::KeyframeCharacterVisionHandler;
pub use keyframe_setting_vision_handler::KeyframeSettingVisionHandler;
pub use keyframe_vision_handler::KeyframeVisionHandler;
pub use media_optimize_handler::MediaOptimizeHandler;
pub use pause::PauseState;
pub use relabel_handler::{SpeakerConfig, SpeakerRelabelHandler};
pub use sprite_handler::ThumbnailSpriteHandler;
pub use view_assembly_handler::ViewAssemblyHandler;
pub use view_vision_handler::ViewVisionHandler;
pub use worker::{JobWorker, WorkerBuilder, WorkerConfig, WorkerEvent, WorkerHandle};

// Re-export extraction types
pub use adapters::{
    ArchiveAdapter, AudioTranscribeAdapter, CodeAstAdapter, ContentSummarizer, EmailAdapter,
    Glb3DModelAdapter, OfficeConvertAdapter, PdfOcrAdapter, PdfTextAdapter, SpreadsheetAdapter,
    StructuredExtractAdapter, TextNativeAdapter, VideoMultimodalAdapter, VisionAdapter,
};
pub use extraction::ExtractionRegistry;

/// Default maximum retries for failed jobs.
pub const DEFAULT_MAX_RETRIES: i32 = matric_core::defaults::JOB_MAX_RETRIES;

/// Default polling interval for job processing (milliseconds).
pub const DEFAULT_POLL_INTERVAL_MS: u64 = matric_core::defaults::JOB_POLL_INTERVAL_MS;

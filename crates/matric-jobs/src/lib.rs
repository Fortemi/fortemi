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

pub mod handler;
pub mod worker;

// Re-export core types
pub use matric_core::*;

// Re-export job types
pub use handler::{JobContext, JobHandler, JobResult, NoOpHandler};
pub use worker::{JobWorker, WorkerBuilder, WorkerConfig, WorkerEvent, WorkerHandle};

/// Default maximum retries for failed jobs.
pub const DEFAULT_MAX_RETRIES: i32 = 3;

/// Default polling interval for job processing (milliseconds).
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 500;

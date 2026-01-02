//! # matric-jobs
//!
//! Background job queue system for matric-memory.
//!
//! This crate provides:
//! - Priority-based job queueing
//! - Async job processing
//! - Progress tracking and notifications
//! - Retry logic with configurable limits

// Re-export core types
pub use matric_core::*;

/// Default maximum retries for failed jobs.
pub const DEFAULT_MAX_RETRIES: i32 = 3;

/// Default polling interval for job processing (milliseconds).
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 500;

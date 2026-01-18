//! # matric-core
//!
//! Core types, traits, and abstractions for the matric-memory library.
//!
//! This crate provides the foundational data structures and trait definitions
//! that other matric-memory crates depend on.

pub mod error;
pub mod hardware;
pub mod models;
pub mod tags;
pub mod tokenizer;
pub mod traits;

// Re-export commonly used types at crate root
pub use error::{Error, Result};
pub use hardware::{ContextBudget, HardwareConfig};
pub use models::*;
pub use tags::*;
pub use tokenizer::*;
pub use traits::*;

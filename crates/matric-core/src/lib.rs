//! # matric-core
//!
//! Core types, traits, and abstractions for the matric-memory library.
//!
//! This crate provides the foundational data structures and trait definitions
//! that other matric-memory crates depend on.

pub mod collection_filter;
pub mod error;
pub mod fair;
pub mod hardware;
pub mod models;
pub mod search;
pub mod strict_filter;
pub mod tags;
pub mod temporal;
pub mod tokenizer;
pub mod traits;
pub mod uuid_utils;

// Re-export commonly used types at crate root
pub use collection_filter::{CollectionPathFilter, StrictCollectionFilter};
pub use error::{Error, Result};
pub use fair::{DublinCoreExport, FairScore, JsonLdContext, JsonLdExport};
pub use hardware::{ContextBudget, HardwareConfig};
pub use models::*;
pub use search::*;
pub use strict_filter::{
    MetadataFilter, SemanticScopeFilter, StrictFilter, StrictSecurityFilter, Visibility,
};
pub use tags::*;
pub use temporal::{NamedTemporalRange, StrictTemporalFilter};
pub use tokenizer::*;
pub use traits::*;
pub use uuid_utils::{extract_timestamp, is_v7, new_v7, v7_from_timestamp};

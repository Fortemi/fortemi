//! # matric-core
//!
//! Core types, traits, and abstractions for the matric-memory library.
//!
//! This crate provides the foundational data structures and trait definitions
//! that other matric-memory crates depend on.

pub mod collection_filter;
pub mod defaults;
pub mod embedding_provider;
pub mod error;
pub mod events;
pub mod exif;
pub mod fair;
pub mod file_safety;
pub mod hardware;
pub mod logging;
pub mod models;
pub mod search;
pub mod shard;
pub mod strict_filter;
pub mod tags;
pub mod temporal;
pub mod tokenizer;
pub mod traits;
pub mod uuid_utils;

// Re-export commonly used types at crate root
pub use collection_filter::{CollectionPathFilter, StrictCollectionFilter};
pub use embedding_provider::*;
pub use error::{Error, Result};
pub use events::{EventBus, ServerEvent};
pub use exif::{DeviceInfo, ExifMetadata, GpsCoordinates};
pub use fair::{DublinCoreExport, FairScore, JsonLdContext, JsonLdExport};
pub use file_safety::{
    detect_content_type, is_valid_mime_type, sanitize_filename, validate_file, ValidationResult,
};
pub use hardware::{ContextBudget, HardwareConfig};
pub use models::*;
pub use search::*;
pub use shard::*;
pub use strict_filter::{
    MetadataFilter, SemanticScopeFilter, StrictFilter, StrictSecurityFilter, Visibility,
};
pub use tags::*;
pub use temporal::{NamedTemporalRange, StrictTemporalFilter};
pub use tokenizer::*;
pub use traits::*;
pub use uuid_utils::{extract_timestamp, is_v7, new_v7, v7_from_timestamp};

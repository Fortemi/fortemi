//! Extraction adapter implementations.

pub mod structured_extract;
pub mod text_native;

pub use structured_extract::StructuredExtractAdapter;
pub use text_native::TextNativeAdapter;

//! Extraction adapter implementations.

pub mod pdf_text;
pub mod structured_extract;
pub mod text_native;

#[cfg(test)]
mod test_extraction_config;

pub use pdf_text::PdfTextAdapter;
pub use structured_extract::StructuredExtractAdapter;
pub use text_native::TextNativeAdapter;

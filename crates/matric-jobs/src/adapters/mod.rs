//! Extraction adapter implementations.

pub mod audio_transcribe;
pub mod code_ast;
pub mod content_summarizer;
pub mod exif;
pub mod glb_3d_model;
pub mod office_convert;
pub mod pdf_ocr;
pub mod pdf_text;
pub mod structured_extract;
pub mod text_native;
pub mod video_multimodal;
pub mod vision;

#[cfg(test)]
mod test_extraction_config;

pub use audio_transcribe::AudioTranscribeAdapter;
pub use code_ast::CodeAstAdapter;
pub use content_summarizer::ContentSummarizer;
pub use glb_3d_model::Glb3DModelAdapter;
pub use office_convert::OfficeConvertAdapter;
pub use pdf_ocr::PdfOcrAdapter;
pub use pdf_text::PdfTextAdapter;
pub use structured_extract::StructuredExtractAdapter;
pub use text_native::TextNativeAdapter;
pub use video_multimodal::VideoMultimodalAdapter;
pub use vision::VisionAdapter;

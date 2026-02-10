//! Vision extraction adapter — extracts image descriptions using vision models.

use std::sync::Arc;

use async_trait::async_trait;
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use matric_inference::vision::VisionBackend;
use serde_json::Value as JsonValue;

use super::exif::extract_exif_metadata;

/// Adapter for extracting descriptions from images using vision models.
///
/// Uses a pluggable `VisionBackend` (e.g., Ollama with LLaVA, qwen3-vl) to
/// generate AI descriptions of image content. Returns the description as
/// `ai_description` in the extraction result, along with basic metadata.
///
/// Requires a vision model to be configured via environment variables
/// (OLLAMA_VISION_MODEL) or injected at construction time.
pub struct VisionAdapter {
    backend: Arc<dyn VisionBackend>,
}

impl VisionAdapter {
    /// Create a new VisionAdapter with a specific backend.
    pub fn new(backend: Arc<dyn VisionBackend>) -> Self {
        Self { backend }
    }

    /// Create from environment variables using OllamaVisionBackend.
    ///
    /// Returns None if OLLAMA_VISION_MODEL is not set.
    pub fn from_env() -> Option<Self> {
        use matric_inference::vision::OllamaVisionBackend;

        let backend = OllamaVisionBackend::from_env()?;
        Some(Self::new(Arc::new(backend)))
    }
}

#[async_trait]
impl ExtractionAdapter for VisionAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Vision
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot extract vision description from empty image data".to_string(),
            ));
        }

        // Extract custom prompt from config if provided
        let custom_prompt = config.get("prompt").and_then(|v| v.as_str());

        // Call vision backend to describe the image
        let description = self
            .backend
            .describe_image(data, mime_type, custom_prompt)
            .await?;

        // Build metadata with image info
        let mut metadata = serde_json::json!({
            "model": self.backend.model_name(),
            "filename": filename,
            "mime_type": mime_type,
            "size_bytes": data.len(),
        });

        // Try to detect image dimensions using basic image format detection
        if let Some(dimensions) = detect_image_dimensions(data, mime_type) {
            metadata["width"] = serde_json::json!(dimensions.0);
            metadata["height"] = serde_json::json!(dimensions.1);
        }

        // Extract EXIF metadata (camera info, GPS, settings, etc.)
        if let Some(exif_data) = extract_exif_metadata(data) {
            if let Some(exif_obj) = exif_data.get("exif") {
                metadata["exif"] = exif_obj.clone();
            }
        }

        Ok(ExtractionResult {
            extracted_text: None,
            metadata,
            ai_description: Some(description),
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        self.backend.health_check().await
    }

    fn name(&self) -> &str {
        "vision"
    }
}

/// Detect image dimensions from common image format headers.
///
/// Supports PNG, JPEG, GIF, WebP basic detection.
/// Returns (width, height) if successful, None otherwise.
/// Detect image dimensions from common image format headers.
///
/// Supports PNG, JPEG, GIF, WebP basic detection.
/// Returns (width, height) if successful, None otherwise.
fn detect_image_dimensions(data: &[u8], mime_type: &str) -> Option<(u32, u32)> {
    let mime_lower = mime_type.to_lowercase();

    // PNG: 16 bytes header, width at offset 16-19, height at offset 20-23
    if mime_lower.contains("png") && data.len() >= 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((width, height));
    }

    // JPEG: Search for SOF0 marker (0xFFC0)
    if mime_lower.contains("jpeg") || mime_lower.contains("jpg") {
        for i in 0..data.len().saturating_sub(9) {
            if data[i] == 0xFF && data[i + 1] == 0xC0 {
                // SOF0 marker found, height at offset +5, width at offset +7
                let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((width, height));
            }
        }
    }

    // GIF: width at offset 6-7, height at offset 8-9
    if mime_lower.contains("gif") && data.len() >= 10 && &data[0..3] == b"GIF" {
        let width = u16::from_le_bytes([data[6], data[7]]) as u32;
        let height = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some((width, height));
    }

    // WebP: RIFF header, then VP8/VP8L/VP8X chunks
    if mime_lower.contains("webp")
        && data.len() >= 30
        && &data[0..4] == b"RIFF"
        && &data[8..12] == b"WEBP"
    {
        // Simple VP8 (lossy)
        if &data[12..16] == b"VP8 " && data.len() >= 30 {
            let width = u16::from_le_bytes([data[26], data[27]]) as u32 & 0x3FFF;
            let height = u16::from_le_bytes([data[28], data[29]]) as u32 & 0x3FFF;
            return Some((width, height));
        }
        // VP8L (lossless)
        if &data[12..16] == b"VP8L" && data.len() >= 25 {
            let bits = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
            let width = (bits & 0x3FFF) + 1;
            let height = ((bits >> 14) & 0x3FFF) + 1;
            return Some((width, height));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::Error;

    /// Mock vision backend for testing
    struct MockVisionBackend {
        description: String,
        health: bool,
        model: String,
    }

    impl MockVisionBackend {
        fn new(description: String) -> Self {
            Self {
                description,
                health: true,
                model: "test-vision-model".to_string(),
            }
        }

        fn with_health(mut self, health: bool) -> Self {
            self.health = health;
            self
        }
    }

    #[async_trait]
    impl VisionBackend for MockVisionBackend {
        async fn describe_image(
            &self,
            _image_data: &[u8],
            _mime_type: &str,
            _prompt: Option<&str>,
        ) -> Result<String> {
            Ok(self.description.clone())
        }

        async fn health_check(&self) -> Result<bool> {
            Ok(self.health)
        }

        fn model_name(&self) -> &str {
            &self.model
        }
    }

    #[test]
    fn test_vision_adapter_strategy() {
        let mock = MockVisionBackend::new("test description".to_string());
        let adapter = VisionAdapter::new(Arc::new(mock));
        assert_eq!(adapter.strategy(), ExtractionStrategy::Vision);
    }

    #[test]
    fn test_vision_adapter_name() {
        let mock = MockVisionBackend::new("test description".to_string());
        let adapter = VisionAdapter::new(Arc::new(mock));
        assert_eq!(adapter.name(), "vision");
    }

    #[tokio::test]
    async fn test_vision_adapter_health_check() {
        let mock = MockVisionBackend::new("test".to_string()).with_health(true);
        let adapter = VisionAdapter::new(Arc::new(mock));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_vision_adapter_health_check_failure() {
        let mock = MockVisionBackend::new("test".to_string()).with_health(false);
        let adapter = VisionAdapter::new(Arc::new(mock));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_vision_adapter_extract() {
        let mock = MockVisionBackend::new("A beautiful sunset over mountains".to_string());
        let adapter = VisionAdapter::new(Arc::new(mock));

        let image_data = b"fake image data";
        let result = adapter
            .extract(
                image_data,
                "sunset.jpg",
                "image/jpeg",
                &serde_json::json!({}),
            )
            .await;

        assert!(result.is_ok());
        let extraction = result.unwrap();
        assert_eq!(
            extraction.ai_description.as_deref(),
            Some("A beautiful sunset over mountains")
        );
        assert_eq!(extraction.metadata["model"], "test-vision-model");
        assert_eq!(extraction.metadata["filename"], "sunset.jpg");
        assert_eq!(extraction.metadata["mime_type"], "image/jpeg");
        assert_eq!(extraction.metadata["size_bytes"], image_data.len());
        assert!(extraction.extracted_text.is_none());
        assert!(extraction.preview_data.is_none());
    }

    #[tokio::test]
    async fn test_vision_adapter_extract_empty_data() {
        let mock = MockVisionBackend::new("should not be called".to_string());
        let adapter = VisionAdapter::new(Arc::new(mock));

        let result = adapter
            .extract(b"", "empty.jpg", "image/jpeg", &serde_json::json!({}))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("empty"),
            "Error should mention empty data: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_vision_adapter_extract_with_custom_prompt() {
        let mock = MockVisionBackend::new("Custom response".to_string());
        let adapter = VisionAdapter::new(Arc::new(mock));

        let config = serde_json::json!({
            "prompt": "Describe the main subject in this image"
        });

        let result = adapter
            .extract(b"image data", "test.png", "image/png", &config)
            .await;

        assert!(result.is_ok());
        let extraction = result.unwrap();
        assert_eq!(
            extraction.ai_description.as_deref(),
            Some("Custom response")
        );
    }

    #[test]
    fn test_detect_png_dimensions() {
        // Minimal PNG header with width=100, height=200
        let mut png_data = vec![0u8; 24];
        png_data[0..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png_data[16..20].copy_from_slice(&100u32.to_be_bytes());
        png_data[20..24].copy_from_slice(&200u32.to_be_bytes());

        let dims = detect_image_dimensions(&png_data, "image/png");
        assert_eq!(dims, Some((100, 200)));
    }

    #[test]
    fn test_detect_gif_dimensions() {
        // Minimal GIF header with width=320, height=240
        let mut gif_data = vec![0u8; 10];
        gif_data[0..3].copy_from_slice(b"GIF");
        gif_data[6..8].copy_from_slice(&320u16.to_le_bytes());
        gif_data[8..10].copy_from_slice(&240u16.to_le_bytes());

        let dims = detect_image_dimensions(&gif_data, "image/gif");
        assert_eq!(dims, Some((320, 240)));
    }

    #[test]
    fn test_detect_jpeg_dimensions() {
        // Minimal JPEG with SOF0 marker (0xFFC0) at offset 2
        let mut jpeg_data = vec![0u8; 20];
        jpeg_data[0] = 0xFF;
        jpeg_data[1] = 0xD8; // SOI marker
        jpeg_data[2] = 0xFF;
        jpeg_data[3] = 0xC0; // SOF0 marker
        jpeg_data[4] = 0x00;
        jpeg_data[5] = 0x11; // length
                             // height at offset 2+5=7, width at offset 2+7=9
        jpeg_data[7..9].copy_from_slice(&480u16.to_be_bytes());
        jpeg_data[9..11].copy_from_slice(&640u16.to_be_bytes());

        let dims = detect_image_dimensions(&jpeg_data, "image/jpeg");
        assert_eq!(dims, Some((640, 480)));
    }

    #[test]
    fn test_detect_dimensions_invalid_data() {
        let invalid_data = b"not an image";
        assert_eq!(detect_image_dimensions(invalid_data, "image/png"), None);
        assert_eq!(detect_image_dimensions(invalid_data, "image/jpeg"), None);
        assert_eq!(detect_image_dimensions(invalid_data, "image/gif"), None);
    }

    #[test]
    fn test_detect_dimensions_too_short() {
        let short_data = b"PNG";
        assert_eq!(detect_image_dimensions(short_data, "image/png"), None);
    }

    #[test]
    fn test_vision_adapter_constructor() {
        // Test that VisionAdapter can be constructed with a mock backend
        let mock = MockVisionBackend::new("test".to_string());
        let adapter = VisionAdapter::new(Arc::new(mock));
        assert_eq!(adapter.name(), "vision");
        assert_eq!(adapter.strategy(), ExtractionStrategy::Vision);
    }
}

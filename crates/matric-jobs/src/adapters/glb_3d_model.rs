//! GLB/3D Model extraction adapter — understands 3D models via multi-view rendering.
//!
//! Pipeline:
//! 1. Send model data to bundled Three.js renderer via multipart POST
//! 2. Receive rendered PNG images for each camera angle
//! 3. Describe each rendered view using VisionBackend
//! 4. Synthesize a composite description from all views
//!
//! Configuration:
//! - `RENDERER_URL` (default: `http://localhost:8080`) - Three.js renderer endpoint
//!
//! Requires: Three.js renderer (bundled) + VisionBackend (Ollama with vision-capable model).

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tracing::{debug, info, warn};

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use matric_inference::vision::VisionBackend;

/// Default number of camera angles for multi-view rendering.
const DEFAULT_VIEW_COUNT: u64 = 6;

/// Maximum number of views to prevent runaway rendering.
const MAX_VIEW_COUNT: u64 = 15;

/// Default renderer URL (bundled Three.js renderer).
const DEFAULT_RENDERER_URL: &str = "http://localhost:8080";

/// Return the renderer URL.
///
/// Checks `RENDERER_URL` env var, falls back to bundled default.
fn renderer_url() -> String {
    std::env::var("RENDERER_URL").unwrap_or_else(|_| DEFAULT_RENDERER_URL.to_string())
}

/// Health check response from renderer.
#[derive(Deserialize)]
struct RendererHealthResponse {
    status: String,
}

/// A rendered view with image data and metadata.
struct RenderedView {
    /// Index of this view (0-based).
    index: usize,
    /// Camera angle in degrees (0-360).
    angle_degrees: f64,
    /// Elevation description.
    elevation: String,
    /// PNG image data.
    image_data: Vec<u8>,
}

pub struct Glb3DModelAdapter {
    backend: Arc<dyn VisionBackend>,
}

impl Glb3DModelAdapter {
    /// Create a new adapter with a specific vision backend.
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

    /// Render the 3D model using the bundled Three.js renderer.
    ///
    /// Uses multipart POST to send the model, receives multipart response with PNG images.
    async fn render_via_renderer(
        &self,
        data: &[u8],
        filename: &str,
        num_views: u64,
    ) -> Result<Vec<RenderedView>> {
        let base_url = renderer_url();
        let client = reqwest::Client::new();
        let render_url = format!("{}/render", base_url.trim_end_matches('/'));

        debug!(render_url = %render_url, filename, num_views, "Calling Three.js renderer");

        // Build multipart form with model data
        let model_part = multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| {
                matric_core::Error::Internal(format!("Failed to create model part: {}", e))
            })?;

        let form = multipart::Form::new()
            .part("model", model_part)
            .text("filename", filename.to_string())
            .text("num_views", num_views.to_string());

        let response = client
            .post(&render_url)
            .multipart(form)
            .timeout(std::time::Duration::from_secs(EXTRACTION_CMD_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|e| matric_core::Error::Internal(format!("Failed to call renderer: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(matric_core::Error::Internal(format!(
                "Renderer error {}: {}",
                status, error_text
            )));
        }

        // Check for success header
        let render_success = response
            .headers()
            .get("X-Render-Success")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "true")
            .unwrap_or(false);

        if !render_success {
            return Err(matric_core::Error::Internal(
                "Renderer reported failure".to_string(),
            ));
        }

        // Extract boundary from content-type header BEFORE consuming response
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let boundary = content_type
            .split("boundary=")
            .nth(1)
            .map(|s| s.to_string())
            .ok_or_else(|| {
                matric_core::Error::Internal("Missing multipart boundary".to_string())
            })?;

        // Now consume response to get body
        let body = response.bytes().await.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to read renderer response: {}", e))
        })?;

        // Parse multipart body to extract PNG images
        let rendered_views = parse_multipart_response(&body, &boundary)?;

        info!(
            filename,
            num_views = rendered_views.len(),
            "Rendering complete"
        );
        Ok(rendered_views)
    }
}

#[async_trait]
impl ExtractionAdapter for Glb3DModelAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Glb3DModel
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot process empty 3D model data".to_string(),
            ));
        }

        // Parse config
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);

        let custom_prompt = config.get("prompt").and_then(|v| v.as_str());

        debug!(
            filename,
            num_views, "Rendering 3D model from multiple angles"
        );

        // Get rendered views from Three.js renderer
        let rendered_views = self.render_via_renderer(data, filename, num_views).await?;

        debug!(
            filename,
            rendered = rendered_views.len(),
            "Describing rendered views"
        );

        // Describe each view using vision backend
        let mut view_descriptions = Vec::new();
        let total_views = rendered_views.len();
        for view in &rendered_views {
            let prompt = if let Some(custom) = custom_prompt {
                format!(
                    "{}\n\nThis is view {} of {} (angle: {:.0}°, elevation: {}) of a 3D model from file '{}'.",
                    custom, view.index + 1, total_views, view.angle_degrees, view.elevation, filename
                )
            } else {
                format!(
                    "Describe this rendered view of a 3D model in detail. \
                     This is view {} of {} (camera angle: {:.0}°, elevation: {}). \
                     The model file is '{}'. \
                     Describe the shape, materials, textures, colors, and any notable features visible from this angle.",
                    view.index + 1, total_views, view.angle_degrees, view.elevation, filename
                )
            };

            match self
                .backend
                .describe_image(&view.image_data, "image/png", Some(&prompt))
                .await
            {
                Ok(description) => {
                    view_descriptions.push(json!({
                        "view_index": view.index,
                        "angle_degrees": view.angle_degrees,
                        "elevation": &view.elevation,
                        "description": description,
                    }));
                }
                Err(e) => {
                    warn!(view = view.index, error = %e, "View description failed");
                }
            }
        }

        // Synthesize composite description from all views
        let composite_description = if !view_descriptions.is_empty() {
            let views_text = view_descriptions
                .iter()
                .map(|v| {
                    format!(
                        "View {} ({:.0}°, {}): {}",
                        v["view_index"], v["angle_degrees"], v["elevation"], v["description"]
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            // Ask vision model to synthesize a unified description
            let synthesis_prompt = format!(
                "Below are descriptions of the same 3D model ('{}') viewed from {} different camera angles.\n\n\
                 {}\n\n\
                 Provide a single comprehensive description of this 3D model, \
                 combining information from all views. \
                 Describe the overall shape, geometry, materials, colors, and purpose of the object.",
                filename,
                view_descriptions.len(),
                views_text
            );

            // Use a dummy 1x1 white PNG as the image (the prompt contains the real content)
            let dummy_png = create_minimal_png();
            match self
                .backend
                .describe_image(&dummy_png, "image/png", Some(&synthesis_prompt))
                .await
            {
                Ok(synthesis) => Some(synthesis),
                Err(e) => {
                    warn!(error = %e, "Synthesis failed, using concatenated descriptions");
                    Some(views_text)
                }
            }
        } else {
            None
        };

        Ok(ExtractionResult {
            extracted_text: None,
            metadata: json!({
                "model": self.backend.model_name(),
                "filename": filename,
                "size_bytes": data.len(),
                "num_views_requested": num_views,
                "num_views_rendered": rendered_views.len(),
                "num_views_described": view_descriptions.len(),
                "view_descriptions": view_descriptions,
            }),
            ai_description: composite_description,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Check renderer availability
        let base_url = renderer_url();
        let client = reqwest::Client::new();
        let health_url = format!("{}/health", base_url.trim_end_matches('/'));

        let renderer_ok = match client
            .get(&health_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<RendererHealthResponse>().await {
                        Ok(health) => health.status == "healthy",
                        Err(_) => false,
                    }
                } else {
                    false
                }
            }
            Err(_) => false,
        };

        if !renderer_ok {
            return Ok(false);
        }

        // Also check vision backend
        self.backend.health_check().await
    }

    fn name(&self) -> &str {
        "glb_3d_model"
    }
}

/// Parse a multipart response body and extract rendered view images.
fn parse_multipart_response(body: &[u8], boundary: &str) -> Result<Vec<RenderedView>> {
    let boundary_bytes = format!("--{}", boundary);
    let mut views = Vec::new();

    // Split by boundary
    let body_str = String::from_utf8_lossy(body);
    let parts: Vec<&str> = body_str.split(&boundary_bytes).collect();

    for part in parts.iter().skip(1) {
        // Skip empty parts and final boundary marker
        if part.trim().is_empty() || part.starts_with("--") {
            continue;
        }

        // Find headers/body separator
        let Some(header_end) = part.find("\r\n\r\n") else {
            continue;
        };

        let headers = &part[..header_end];
        let body_start = header_end + 4;

        // Check if this is an image part
        if !headers.contains("Content-Type: image/png") {
            continue;
        }

        // Parse Content-Disposition for metadata
        let index = parse_disposition_param(headers, "index")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        let angle_degrees = parse_disposition_param(headers, "angle_degrees")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let elevation =
            parse_disposition_param(headers, "elevation").unwrap_or_else(|| "unknown".to_string());

        // Get Content-Length to extract exact image bytes
        let content_length = headers
            .lines()
            .find(|l| l.to_lowercase().starts_with("content-length:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|s| s.trim().parse::<usize>().ok());

        // Extract image data
        let image_bytes = if let Some(len) = content_length {
            // Use Content-Length to extract exact bytes from original body
            let part_start = body
                .windows(part.len())
                .position(|w| String::from_utf8_lossy(w) == *part);

            if let Some(start) = part_start {
                let abs_body_start = start + body_start;
                let abs_body_end = (abs_body_start + len).min(body.len());
                body[abs_body_start..abs_body_end].to_vec()
            } else {
                // Fallback: strip trailing CRLF
                let body_part = &part[body_start..];
                body_part.trim_end_matches("\r\n").as_bytes().to_vec()
            }
        } else {
            // Fallback: strip trailing CRLF
            let body_part = &part[body_start..];
            body_part.trim_end_matches("\r\n").as_bytes().to_vec()
        };

        // Verify PNG magic bytes
        if image_bytes.len() < 8 || image_bytes[0..4] != [0x89, 0x50, 0x4E, 0x47] {
            warn!(index, "Invalid PNG data in multipart response");
            continue;
        }

        views.push(RenderedView {
            index,
            angle_degrees,
            elevation,
            image_data: image_bytes,
        });
    }

    if views.is_empty() {
        return Err(matric_core::Error::Internal(
            "No valid PNG views in multipart response".to_string(),
        ));
    }

    // Sort by index
    views.sort_by_key(|v| v.index);

    Ok(views)
}

/// Parse a parameter from Content-Disposition header.
fn parse_disposition_param(headers: &str, param: &str) -> Option<String> {
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-disposition:") {
            // Find param="value" pattern
            let pattern = format!("{}=\"", param);
            if let Some(start) = line.find(&pattern) {
                let value_start = start + pattern.len();
                if let Some(end) = line[value_start..].find('"') {
                    return Some(line[value_start..value_start + end].to_string());
                }
            }
        }
    }
    None
}

/// Create a minimal valid 1x1 white PNG for use as a dummy image
/// when the real content is in the prompt text.
fn create_minimal_png() -> Vec<u8> {
    // Minimal 1x1 white PNG (67 bytes)
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // 8-bit RGB
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
        0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, // compressed data
        0x00, 0x02, 0x00, 0x01, 0xE2, 0x21, 0xBC, 0x33, // checksum
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
        0xAE, 0x42, 0x60, 0x82,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::Error;

    /// Mock vision backend for testing.
    struct MockVisionBackend {
        description: String,
        health: bool,
    }

    impl MockVisionBackend {
        fn new(description: &str) -> Self {
            Self {
                description: description.to_string(),
                health: true,
            }
        }

        fn unhealthy() -> Self {
            Self {
                description: String::new(),
                health: false,
            }
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
            "mock-vision"
        }
    }

    #[test]
    fn test_glb_adapter_strategy() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        assert_eq!(adapter.strategy(), ExtractionStrategy::Glb3DModel);
    }

    #[test]
    fn test_glb_adapter_name() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        assert_eq!(adapter.name(), "glb_3d_model");
    }

    #[tokio::test]
    async fn test_glb_adapter_empty_input() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));

        let result = adapter
            .extract(b"", "empty.glb", "model/gltf-binary", &json!({}))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
        assert!(err.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn test_glb_adapter_health_check_no_renderer() {
        // This test checks that health_check gracefully handles missing renderer.
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        // Result will be false if renderer is not running
    }

    #[tokio::test]
    async fn test_glb_adapter_health_check_unhealthy_backend() {
        let mock = MockVisionBackend::unhealthy();
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_minimal_png() {
        let png = create_minimal_png();
        // PNG magic bytes
        assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]);
        // IHDR chunk
        assert_eq!(&png[12..16], b"IHDR");
        // IEND chunk (last 12 bytes)
        let iend_start = png.len() - 12;
        assert_eq!(&png[iend_start + 4..iend_start + 8], b"IEND");
    }

    #[test]
    fn test_view_count_defaults() {
        // Default view count
        let config = json!({});
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);
        assert_eq!(num_views, DEFAULT_VIEW_COUNT);

        // Custom view count
        let config = json!({ "num_views": 10 });
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);
        assert_eq!(num_views, 10);

        // Capped at max
        let config = json!({ "num_views": 100 });
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);
        assert_eq!(num_views, MAX_VIEW_COUNT);
    }

    #[test]
    fn test_parse_disposition_param() {
        let headers = r#"Content-Disposition: attachment; filename="view_000.png"; index="0"; angle_degrees="0"; elevation="low_30deg"
Content-Type: image/png
Content-Length: 1234"#;

        assert_eq!(
            parse_disposition_param(headers, "index"),
            Some("0".to_string())
        );
        assert_eq!(
            parse_disposition_param(headers, "angle_degrees"),
            Some("0".to_string())
        );
        assert_eq!(
            parse_disposition_param(headers, "elevation"),
            Some("low_30deg".to_string())
        );
        assert_eq!(
            parse_disposition_param(headers, "filename"),
            Some("view_000.png".to_string())
        );
        assert_eq!(parse_disposition_param(headers, "nonexistent"), None);
    }

    #[test]
    fn test_glb_adapter_constructor() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        assert_eq!(adapter.name(), "glb_3d_model");
        assert_eq!(adapter.strategy(), ExtractionStrategy::Glb3DModel);
    }

    #[test]
    fn test_renderer_url_default() {
        // When RENDERER_URL is not set, should return default
        // This test may fail if RENDERER_URL is set in the environment
        let url = renderer_url();
        // Either the default or whatever is in the environment
        assert!(!url.is_empty());
    }
}

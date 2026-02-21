//! GLB/3D Model extraction adapter — understands 3D models via multi-view rendering.
//!
//! Pipeline:
//! 1. Send model data to bundled Open3D renderer via multipart POST
//! 2. Receive rendered PNG images for each camera angle
//! 3. Describe each rendered view using VisionBackend
//! 4. Synthesize a composite description from all views
//!
//! Configuration:
//! - `RENDERER_URL` (default: `http://localhost:8080`) - Open3D renderer endpoint
//!
//! Requires: Open3D renderer (bundled) + VisionBackend (Ollama with vision-capable model).

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

/// Default renderer URL (bundled Open3D renderer).
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
    /// Returns None only if OLLAMA_VISION_MODEL is explicitly set to empty string.
    pub fn from_env() -> Option<Self> {
        use matric_inference::vision::OllamaVisionBackend;

        let backend = OllamaVisionBackend::from_env()?;
        Some(Self::new(Arc::new(backend)))
    }

    /// Render the 3D model using the bundled Open3D renderer.
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

        debug!(render_url = %render_url, filename, num_views, "Calling Open3D renderer");

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

            // Use a 64×64 placeholder PNG (the prompt contains the real content).
            // Must be ≥ 32×32 to satisfy vision model image preprocessors.
            let dummy_png = create_placeholder_png();
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
///
/// Works directly on raw bytes to avoid O(n²) UTF-8 conversion overhead.
/// Each part boundary is located by scanning for `--<boundary>`, then
/// headers are parsed as ASCII/UTF-8 and Content-Length is used to slice
/// the exact binary image data from the original body.
fn parse_multipart_response(body: &[u8], boundary: &str) -> Result<Vec<RenderedView>> {
    let boundary_marker = format!("--{}", boundary).into_bytes();
    let header_sep = b"\r\n\r\n";
    let mut views = Vec::new();

    // Find each boundary marker in the raw byte stream
    let mut search_from = 0;
    let mut part_starts: Vec<usize> = Vec::new();
    while let Some(pos) = find_bytes(&body[search_from..], &boundary_marker) {
        part_starts.push(search_from + pos);
        search_from += pos + boundary_marker.len();
    }

    // Each part spans from (boundary_end) to (next_boundary_start)
    for (i, &start) in part_starts.iter().enumerate() {
        let part_begin = start + boundary_marker.len();

        // Skip closing `--` marker
        if body.get(part_begin..part_begin + 2) == Some(b"--") {
            continue;
        }

        // Determine part end (next boundary or end of body)
        let part_end = part_starts.get(i + 1).copied().unwrap_or(body.len());

        let part_data = &body[part_begin..part_end];

        // Find header/body separator (\r\n\r\n)
        let Some(hdr_end) = find_bytes(part_data, header_sep) else {
            continue;
        };

        // Headers are ASCII-safe, convert to string for easy parsing
        let headers = String::from_utf8_lossy(&part_data[..hdr_end]);
        let body_offset = hdr_end + header_sep.len(); // offset within part_data

        // Only process image/png parts
        if !headers.contains("Content-Type: image/png") {
            continue;
        }

        // Parse metadata from Content-Disposition
        let index = parse_disposition_param(&headers, "index")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        let angle_degrees = parse_disposition_param(&headers, "angle_degrees")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let elevation =
            parse_disposition_param(&headers, "elevation").unwrap_or_else(|| "unknown".to_string());

        // Determine image byte length from Content-Length or boundary-to-boundary
        let content_length: Option<usize> = headers
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|s| s.trim().parse().ok());

        let img_start = body_offset;
        let img_end = if let Some(len) = content_length {
            (img_start + len).min(part_data.len())
        } else {
            // Strip trailing \r\n before boundary
            let mut end = part_data.len();
            while end > img_start && (part_data[end - 1] == b'\r' || part_data[end - 1] == b'\n') {
                end -= 1;
            }
            end
        };

        let image_bytes = part_data[img_start..img_end].to_vec();

        // Verify PNG magic bytes
        if image_bytes.len() < 8 || image_bytes[0..4] != [0x89, 0x50, 0x4E, 0x47] {
            warn!(
                index,
                len = image_bytes.len(),
                "Invalid PNG data in multipart response"
            );
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

    views.sort_by_key(|v| v.index);
    Ok(views)
}

/// Find the first occurrence of `needle` in `haystack` (byte-level search).
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

/// Create a 64×64 gray placeholder PNG for use as a dummy image in the
/// synthesis step, where the real content lives in the prompt text.
///
/// Qwen3-VL (and similar vision models) require images ≥ 32×32 pixels;
/// a 1×1 image triggers a panic in their image preprocessor.
fn create_placeholder_png() -> Vec<u8> {
    // 64×64 RGB gray (#C8C8C8) PNG, 136 bytes
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x08, 0x02, 0x00, 0x00, 0x00, 0x25,
        0x0B, 0xE6, 0x89, 0x00, 0x00, 0x00, 0x4F, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0xED, 0xCF,
        0x41, 0x0D, 0x00, 0x00, 0x08, 0x04, 0x20, 0xB5, 0x7F, 0xB0, 0x8B, 0x65, 0x0A, 0x1F, 0x6E,
        0xD0, 0x80, 0x4E, 0x52, 0x9F, 0x4D, 0x3D, 0x27, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x70, 0x6F, 0x01, 0x58, 0xE3, 0x02, 0xD8,
        0x44, 0x13, 0xF4, 0x86, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60,
        0x82,
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
    fn test_create_placeholder_png() {
        let png = create_placeholder_png();
        // PNG magic bytes
        assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]);
        // IHDR chunk
        assert_eq!(&png[12..16], b"IHDR");
        // 64×64 dimensions (bytes 16-23 in IHDR)
        assert_eq!(&png[16..20], &[0x00, 0x00, 0x00, 0x40]); // width = 64
        assert_eq!(&png[20..24], &[0x00, 0x00, 0x00, 0x40]); // height = 64
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

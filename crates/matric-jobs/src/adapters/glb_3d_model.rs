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
use matric_core::{DerivedFile, ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
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

fn glb_text_len(text: &str) -> usize {
    text.len()
}

fn glb_filename_metadata(filename: &str) -> String {
    format!("filename_len={}", glb_text_len(filename))
}

fn glb_view_prompt(
    custom_prompt: Option<&str>,
    filename: &str,
    view_index: usize,
    total_views: usize,
    angle_degrees: f64,
    elevation: &str,
) -> String {
    let filename_metadata = glb_filename_metadata(filename);
    if let Some(custom) = custom_prompt {
        format!(
            "{}\n\nThis is view {} of {} (angle: {:.0}°, elevation: {}) of a 3D model ({}).",
            custom,
            view_index + 1,
            total_views,
            angle_degrees,
            elevation,
            filename_metadata
        )
    } else {
        format!(
            "Describe this rendered view of a 3D model in detail. \
             This is view {} of {} (camera angle: {:.0}°, elevation: {}). \
             The model file metadata is {}. \
             Describe the shape, materials, textures, colors, and any notable features visible from this angle.",
            view_index + 1,
            total_views,
            angle_degrees,
            elevation,
            filename_metadata
        )
    }
}

fn glb_synthesis_prompt(filename: &str, view_count: usize, views_text: &str) -> String {
    format!(
        "Below are descriptions of the same 3D model ({}) viewed from {} different camera angles.\n\n\
         {}\n\n\
         Provide a single comprehensive description of this 3D model, \
         combining information from all views. \
         Describe the overall shape, geometry, materials, colors, and purpose of the object.",
        glb_filename_metadata(filename),
        view_count,
        views_text
    )
}

fn renderer_destination_class(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains('@')
        || lower.contains("token=")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("password")
        || lower.contains("secret")
    {
        "credentialed_url"
    } else if lower.starts_with("http://localhost")
        || lower.starts_with("https://localhost")
        || lower.starts_with("http://127.")
        || lower.starts_with("https://127.")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("https://[::1]")
    {
        "local_http"
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        "http"
    } else {
        "other"
    }
}

fn glb_error_reason_code(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "timed_out"
    } else if lower.contains("connect") || lower.contains("connection") {
        "connection_failed"
    } else if lower.contains("decode") || lower.contains("json") || lower.contains("invalid") {
        "invalid_response"
    } else if lower.contains("permission") || lower.contains("denied") {
        "permission_denied"
    } else {
        "operation_failed"
    }
}

fn glb_renderer_unavailable_diagnostic(reason: &str) -> (&'static str, usize) {
    (glb_error_reason_code(reason), glb_text_len(reason))
}

fn renderer_test_status_class(status: &str) -> &'static str {
    match status {
        "passed" | "healthy" | "ok" | "success" => "success",
        "failed" | "degraded" | "unhealthy" | "error" => "failure",
        "" => "empty",
        _ => "custom",
    }
}

/// Health check response from renderer.
#[derive(Deserialize)]
struct RendererHealthResponse {
    status: String,
    #[serde(default)]
    render_test: Option<RenderTestResult>,
}

/// Result of the renderer's built-in test render (red cube).
#[derive(Deserialize)]
struct RenderTestResult {
    status: String,
    #[serde(default)]
    content_ratio: Option<f64>,
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

    /// Check if the Open3D renderer is available.
    ///
    /// Returns `Ok(())` if healthy, `Err(reason)` explaining why not.
    async fn check_renderer(&self) -> std::result::Result<(), String> {
        let base_url = renderer_url();
        let client = reqwest::Client::new();
        let health_url = format!("{}/health", base_url.trim_end_matches('/'));

        let response = client
            .get(&health_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                format!(
                    "3D model renderer unreachable; destination_class={}; url_len={}; error_len={}; error_reason={}. \
                     Ensure the Open3D renderer is running — in Docker bundle it starts \
                     automatically; for standalone, set RENDERER_URL.",
                    renderer_destination_class(&base_url),
                    glb_text_len(&base_url),
                    glb_text_len(&e.to_string()),
                    glb_error_reason_code(&e.to_string())
                )
            })?;

        if !response.status().is_success() {
            return Err(format!(
                "3D model renderer returned non-success; destination_class={}; url_len={}; status={} — check renderer logs",
                renderer_destination_class(&base_url),
                glb_text_len(&base_url),
                response.status().as_u16()
            ));
        }

        match response.json::<RendererHealthResponse>().await {
            Ok(health) if health.status == "healthy" => {
                // Renderer is up and test render passed
                if let Some(ref test) = health.render_test {
                    debug!(
                        test_status_class = renderer_test_status_class(&test.status),
                        test_status_len = glb_text_len(&test.status),
                        content_ratio = ?test.content_ratio,
                        "Renderer test render result"
                    );
                }
                Ok(())
            }
            Ok(health) if health.status == "degraded" => {
                // Renderer is up but test render failed — images will be grey
                let test_detail = health
                    .render_test
                    .as_ref()
                    .map(|t| format!(" (test render: {})", t.status))
                    .unwrap_or_default();
                Err(format!(
                    "3D model renderer is degraded; destination_class={}; url_len={}; test_status_len={} — renders may produce blank/grey images. \
                     Check GPU availability or set OPEN3D_CPU_RENDERING=true",
                    renderer_destination_class(&base_url),
                    glb_text_len(&base_url),
                    glb_text_len(&test_detail)
                ))
            }
            Ok(health) => Err(format!(
                "3D model renderer reports unexpected status; destination_class={}; url_len={}; status_len={} — \
                 GPU or CPU rendering may not be available (try setting OPEN3D_CPU_RENDERING=true)",
                renderer_destination_class(&base_url),
                glb_text_len(&base_url),
                glb_text_len(&health.status)
            )),
            Err(e) => Err(format!(
                "3D model renderer returned invalid health response; destination_class={}; url_len={}; error_len={}; error_reason={}",
                renderer_destination_class(&base_url),
                glb_text_len(&base_url),
                glb_text_len(&e.to_string()),
                glb_error_reason_code(&e.to_string())
            )),
        }
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

        debug!(
            render_destination_class = renderer_destination_class(&render_url),
            render_url_len = glb_text_len(&render_url),
            filename_len = glb_text_len(filename),
            num_views,
            "Calling Open3D renderer"
        );

        // Build multipart form with model data
        let model_part = multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| {
                matric_core::Error::Internal(format!(
                    "Failed to create model part; error_len={}; error_reason={}",
                    glb_text_len(&e.to_string()),
                    glb_error_reason_code(&e.to_string())
                ))
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
            .map_err(|e| {
                matric_core::Error::Internal(format!(
                    "Failed to call renderer; destination_class={}; url_len={}; error_len={}; error_reason={}",
                    renderer_destination_class(&render_url),
                    glb_text_len(&render_url),
                    glb_text_len(&e.to_string()),
                    glb_error_reason_code(&e.to_string())
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(matric_core::Error::Internal(format!(
                "Renderer error; status={}; body_len={}; body_reason={}",
                status.as_u16(),
                glb_text_len(&error_text),
                glb_error_reason_code(&error_text)
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

        // Check for blank renders — renderer detected views with no visible model
        let blank_views: u32 = response
            .headers()
            .get("X-Render-Blank-Views")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        if blank_views > 0 {
            warn!(
                filename_len = glb_text_len(filename),
                blank_views,
                "Renderer reports blank views — model may not be visible. \
                 Check GPU availability or software rendering configuration."
            );
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
            matric_core::Error::Internal(format!(
                "Failed to read renderer response; error_len={}; error_reason={}",
                glb_text_len(&e.to_string()),
                glb_error_reason_code(&e.to_string())
            ))
        })?;

        // Parse multipart body to extract PNG images
        let rendered_views = parse_multipart_response(&body, &boundary)?;

        // Validate individual view quality via PNG file size heuristic:
        // A 512×512 uniform-color PNG compresses to ~1-2KB.  A real render
        // with a visible model is typically 30-200KB.  Views under 10KB are
        // suspicious and likely blank (just background + grid lines).
        let mut blank_by_size = 0u32;
        for view in &rendered_views {
            if view.image_data.len() < 10_000 {
                blank_by_size += 1;
                warn!(
                    filename_len = glb_text_len(filename),
                    view = view.index,
                    png_bytes = view.image_data.len(),
                    "Rendered view PNG is suspiciously small — likely blank/grey"
                );
            }
        }
        if blank_by_size > 0 && blank_views == 0 {
            warn!(
                filename_len = glb_text_len(filename),
                blank_by_size, "PNG size heuristic detected blank views not caught by renderer"
            );
        }

        info!(
            filename_len = glb_text_len(filename),
            num_views = rendered_views.len(),
            blank_views,
            blank_by_size,
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

        // Check renderer availability before proceeding
        if let Err(reason) = self.check_renderer().await {
            return Err(matric_core::Error::Internal(reason));
        }

        // Parse config
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);

        let custom_prompt = config.get("prompt").and_then(|v| v.as_str());

        debug!(
            filename_len = glb_text_len(filename),
            num_views, "Rendering 3D model from multiple angles"
        );

        // Get rendered views from Three.js renderer
        let rendered_views = self.render_via_renderer(data, filename, num_views).await?;

        debug!(
            filename_len = glb_text_len(filename),
            rendered = rendered_views.len(),
            "Describing rendered views"
        );

        // When _skip_vision is set, defer vision LLM calls to atomic ViewVision
        // jobs (#533). The extraction handler will queue one ViewVision job per
        // rendered view after derived files are persisted.
        let skip_vision = config
            .get("_skip_vision")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut view_descriptions = Vec::new();
        let mut composite_description: Option<String> = None;
        let total_views = rendered_views.len();

        if !skip_vision {
            // Inline vision: describe each view immediately (original behavior)
            for view in &rendered_views {
                let prompt = glb_view_prompt(
                    custom_prompt,
                    filename,
                    view.index,
                    total_views,
                    view.angle_degrees,
                    &view.elevation,
                );

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
                        warn!(
                            view = view.index,
                            error_len = glb_text_len(&e.to_string()),
                            error_reason = glb_error_reason_code(&e.to_string()),
                            "View description failed"
                        );
                    }
                }
            }

            // Synthesize composite description from all views
            composite_description = if !view_descriptions.is_empty() {
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

                let synthesis_prompt =
                    glb_synthesis_prompt(filename, view_descriptions.len(), &views_text);

                let dummy_png = create_placeholder_png();
                match self
                    .backend
                    .describe_image(&dummy_png, "image/png", Some(&synthesis_prompt))
                    .await
                {
                    Ok(synthesis) => Some(synthesis),
                    Err(e) => {
                        warn!(
                            error_len = glb_text_len(&e.to_string()),
                            error_reason = glb_error_reason_code(&e.to_string()),
                            "Synthesis failed, using concatenated descriptions"
                        );
                        Some(views_text)
                    }
                }
            } else {
                None
            };
        } else {
            debug!(
                filename_len = glb_text_len(filename),
                total_views,
                "Vision deferred — ViewVision jobs will be queued by extraction handler"
            );
        }

        // Build derived files from rendered views so they persist as child attachments
        let base_name = filename
            .trim_end_matches(".glb")
            .trim_end_matches(".gltf")
            .trim_end_matches(".GLB")
            .trim_end_matches(".GLTF");
        let derived_files: Vec<DerivedFile> = rendered_views
            .iter()
            .map(|view| {
                // Find the matching AI description for this view (None when _skip_vision)
                let ai_description = view_descriptions
                    .iter()
                    .find(|vd| vd["view_index"].as_u64() == Some(view.index as u64))
                    .and_then(|vd| vd["description"].as_str())
                    .map(|s| s.to_string());
                DerivedFile {
                    filename: format!(
                        "{}_view_{:03}_{}.png",
                        base_name, view.index, view.elevation
                    ),
                    content_type: "image/png".to_string(),
                    data: view.image_data.clone(),
                    derivation_type: "3d_rendering".to_string(),
                    ai_description,
                    metadata: Some(json!({
                        "view_index": view.index,
                        "angle_degrees": view.angle_degrees,
                        "elevation": &view.elevation,
                        "total_views": total_views,
                    })),
                    source_path: None,
                }
            })
            .collect();

        // Use the first rendered view as preview thumbnail — but only if it
        // appears to have meaningful content (>10KB).  A blank/grey 512×512
        // PNG compresses to ~1-2KB; serving it as a thumbnail is worse than
        // having no thumbnail at all.
        let blank_count = rendered_views
            .iter()
            .filter(|v| v.image_data.len() < 10_000)
            .count();
        let preview_data = rendered_views
            .first()
            .filter(|v| v.image_data.len() >= 10_000)
            .map(|v| v.image_data.clone());

        if preview_data.is_none() && !rendered_views.is_empty() {
            warn!(
                filename,
                first_view_bytes = rendered_views[0].image_data.len(),
                "Skipping thumbnail — first rendered view appears blank"
            );
        }

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
                "render_quality": {
                    "blank_views": blank_count,
                    "total_views": rendered_views.len(),
                },
            }),
            ai_description: composite_description,
            preview_data,
            derived_files,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Register if vision backend is healthy — renderer is checked at extraction time.
        // This ensures GLB uploads get a clear error ("renderer unavailable") rather than
        // the generic "no adapter registered" error.
        let vision_ok = self.backend.health_check().await.unwrap_or(false);
        if !vision_ok {
            return Ok(false);
        }

        // Log renderer status at health-check time (informational, not blocking)
        match self.check_renderer().await {
            Ok(()) => {
                debug!("GLB adapter healthy: vision + renderer available");
            }
            Err(reason) => {
                let (renderer_reason, renderer_reason_len) =
                    glb_renderer_unavailable_diagnostic(&reason);
                warn!(
                    renderer_reason,
                    renderer_reason_len,
                    "GLB adapter registered with degraded capability; renderer not available"
                );
            }
        }

        Ok(true)
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

    #[test]
    fn renderer_destination_class_avoids_raw_url_parts() {
        let url = "https://token=mm_key_secret@renderer.internal/render?api_key=secret";
        assert_eq!(renderer_destination_class(url), "credentialed_url");
        assert_eq!(glb_text_len(url), url.len());
    }

    #[test]
    fn glb_error_reason_code_uses_stable_classes() {
        assert_eq!(
            glb_error_reason_code("connection refused for /srv/fortemi/private/model.glb"),
            "connection_failed"
        );
        assert_eq!(
            glb_error_reason_code("request timed out with token=mm_key_secret"),
            "timed_out"
        );
        assert_eq!(
            glb_error_reason_code("invalid json body"),
            "invalid_response"
        );
        assert_eq!(
            glb_error_reason_code("opaque backend text /srv/fortemi/model.glb"),
            "operation_failed"
        );
    }

    #[test]
    fn glb_renderer_unavailable_diagnostic_uses_metadata_only() {
        let reason =
            "connection refused for https://token=mm_key_secret@renderer.internal/private/model.glb";
        let (renderer_reason, renderer_reason_len) = glb_renderer_unavailable_diagnostic(reason);
        let rendered =
            format!("renderer_reason={renderer_reason} renderer_reason_len={renderer_reason_len}");

        assert_eq!(renderer_reason, "connection_failed");
        assert_eq!(renderer_reason_len, reason.len());
        assert!(rendered.contains("renderer_reason=connection_failed"));
        assert!(rendered.contains("renderer_reason_len="));
        assert!(!rendered.contains("mm_key_secret"));
        assert!(!rendered.contains("renderer.internal"));
        assert!(!rendered.contains("/private/model.glb"));
        assert!(!rendered.contains(reason));
    }

    #[test]
    fn renderer_test_status_telemetry_uses_class_and_length() {
        let status = "failed token=mm_key_secret /srv/fortemi/private/model.glb";
        let rendered = format!(
            "test_status_class={} test_status_len={}",
            renderer_test_status_class(status),
            glb_text_len(status)
        );

        assert_eq!(renderer_test_status_class("passed"), "success");
        assert_eq!(renderer_test_status_class("degraded"), "failure");
        assert_eq!(renderer_test_status_class(status), "custom");
        assert!(rendered.contains("test_status_class=custom"));
        assert!(rendered.contains("test_status_len="));
        assert!(!rendered.contains("mm_key_secret"));
        assert!(!rendered.contains("/srv/fortemi"));
        assert!(!rendered.contains("model.glb"));
    }

    #[test]
    fn glb_vision_prompts_use_filename_metadata_only() {
        let filename = "/srv/fortemi/private/customer@example.com/mm_key_secret_model.glb";
        let custom_prompt = "Describe visible geometry";
        let view_prompt = glb_view_prompt(Some(custom_prompt), filename, 0, 6, 45.0, "front");
        let default_prompt = glb_view_prompt(None, filename, 1, 6, 90.0, "side");
        let synthesis_prompt = glb_synthesis_prompt(filename, 2, "View 1: box\n\nView 2: sphere");
        let rendered = format!("{view_prompt}\n{default_prompt}\n{synthesis_prompt}");

        assert!(rendered.contains(custom_prompt));
        assert!(rendered.contains("filename_len="));
        assert!(!rendered.contains("/srv/fortemi"));
        assert!(!rendered.contains("customer@example.com"));
        assert!(!rendered.contains("mm_key_secret"));
        assert!(!rendered.contains("secret_model.glb"));
        assert!(!rendered.contains(filename));
    }

    #[tokio::test]
    async fn test_glb_adapter_empty_input() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));

        let result = adapter
            .extract(b"", "empty.glb", "model/gltf-binary", &json!({}))
            .await;

        assert!(result.is_err());
        // Error Display is redacted; assert on the typed variant's inner message.
        let err = match result.unwrap_err() {
            Error::InvalidInput(msg) => msg,
            other => panic!("expected InvalidInput, got: {other:?}"),
        };
        assert!(err.contains("empty"));
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

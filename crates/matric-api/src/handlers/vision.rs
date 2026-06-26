//! Vision HTTP handlers.
//!
//! Provides ad-hoc image description via vision LLM without requiring
//! attachment creation. Useful for preview, inline analysis, and MCP tooling.

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::fmt;
use tracing::warn;

use crate::{ApiError, AppState};
use matric_inference::OllamaVisionBackend;

const VISION_ANALYSIS_PROVIDER_DETAIL: &str =
    "Vision analysis backend failed. Check server logs for diagnostics.";

/// Response from image description.
#[derive(Serialize)]
pub struct DescribeImageResponse {
    /// AI-generated description of the image.
    pub description: String,
    /// Vision model used for description.
    pub model: String,
    /// Size of the uploaded image in bytes.
    pub image_size: usize,
}

impl fmt::Debug for DescribeImageResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DescribeImageResponse")
            .field("description_len", &self.description.len())
            .field("model_len", &self.model.len())
            .field("image_size", &self.image_size)
            .finish()
    }
}

/// Describe an image using the configured vision model.
///
/// Accepts multipart/form-data with an image file and returns an AI-generated description.
/// Requires `OLLAMA_VISION_MODEL` to be configured.
///
/// # Multipart Fields
/// - `file`: Image file (required)
/// - `prompt`: Custom description prompt (optional)
/// - `model`: Vision model slug override (optional, e.g. "llava:13b")
///
/// # Returns
/// - 200 OK with description, model name, and image size
/// - 400 Bad Request if file is missing or empty
/// - 503 Service Unavailable if vision model is not configured
#[utoipa::path(post, path = "/api/v1/vision/describe", tag = "Vision",
    responses((status = 200, description = "Image description result")))]
pub async fn describe_image(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<DescribeImageResponse>, ApiError> {
    let default_backend = state.vision_backend.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Vision model backend is not configured.".into())
    })?;

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;
    let mut prompt: Option<String> = None;
    let mut model_override: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("Invalid multipart vision request.".to_string()))?
    {
        let field_name = field.name().map(|n| n.to_string());
        match field_name.as_deref() {
            Some("file") => {
                content_type = field.content_type().map(|c| c.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| {
                            ApiError::BadRequest("Invalid uploaded image file.".to_string())
                        })?
                        .to_vec(),
                );
            }
            Some("prompt") => {
                prompt = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| ApiError::BadRequest("Invalid prompt field.".to_string()))?,
                );
            }
            Some("model") => {
                let val = field
                    .text()
                    .await
                    .map_err(|_| ApiError::BadRequest("Invalid model field.".to_string()))?;
                if !val.trim().is_empty() {
                    model_override = Some(val.trim().to_string());
                }
            }
            _ => {} // ignore unknown fields
        }
    }

    let image_bytes = file_data
        .ok_or_else(|| ApiError::BadRequest("Missing file in multipart form".to_string()))?;

    if image_bytes.is_empty() {
        return Err(ApiError::BadRequest("Image file is empty".into()));
    }

    let mime_type = content_type.as_deref().unwrap_or("image/png");

    // Use model override if specified, otherwise fall back to configured default
    let overridden_backend = model_override.map(|m| {
        let base_url = std::env::var("OLLAMA_BASE")
            .or_else(|_| std::env::var("OLLAMA_URL"))
            .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string());
        OllamaVisionBackend::new(base_url, m)
    });
    let backend: &dyn matric_inference::VisionBackend = match &overridden_backend {
        Some(b) => b,
        None => default_backend.as_ref(),
    };

    let description = backend
        .describe_image(&image_bytes, mime_type, prompt.as_deref())
        .await
        .map_err(|e| {
            let diagnostic = e.to_string();
            warn!(
                error_len = diagnostic.chars().count(),
                "Vision analysis backend failed"
            );
            ApiError::ProviderFailure {
                capability: "Vision analysis",
                detail: VISION_ANALYSIS_PROVIDER_DETAIL.to_string(),
            }
        })?;

    Ok(Json(DescribeImageResponse {
        description,
        model: backend.model_name().to_string(),
        image_size: image_bytes.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_provider_detail_is_fixed_and_redacted() {
        assert_eq!(
            VISION_ANALYSIS_PROVIDER_DETAIL,
            "Vision analysis backend failed. Check server logs for diagnostics."
        );
        assert!(!VISION_ANALYSIS_PROVIDER_DETAIL.contains("https://"));
        assert!(!VISION_ANALYSIS_PROVIDER_DETAIL.contains("token"));
        assert!(!VISION_ANALYSIS_PROVIDER_DETAIL.contains("/srv/fortemi"));
    }

    #[test]
    fn describe_image_response_debug_redacts_generated_description_and_model() {
        let response = DescribeImageResponse {
            description: "Generated description mentions customer@example.com, /srv/private/image.png, and sk-live-vision".to_string(),
            model: "llava-private-model-db.internal".to_string(),
            image_size: 8192,
        };

        let rendered = format!("{response:?}");

        assert!(rendered.contains("DescribeImageResponse"));
        assert!(rendered.contains("description_len"));
        assert!(rendered.contains("model_len"));
        assert!(rendered.contains("image_size"));

        for raw in [
            "Generated description",
            "customer@example.com",
            "/srv/private/image.png",
            "sk-live-vision",
            "llava-private-model",
            "db.internal",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }
}

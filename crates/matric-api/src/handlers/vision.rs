//! Vision HTTP handlers.
//!
//! Provides ad-hoc image description via vision LLM without requiring
//! attachment creation. Useful for preview, inline analysis, and MCP tooling.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::{ApiError, AppState};

/// Response from image description.
#[derive(Debug, Serialize)]
pub struct DescribeImageResponse {
    /// AI-generated description of the image.
    pub description: String,
    /// Vision model used for description.
    pub model: String,
    /// Size of the uploaded image in bytes.
    pub image_size: usize,
}

/// Describe an image using the configured vision model.
///
/// Accepts multipart/form-data with an image file and returns an AI-generated description.
/// Requires `OLLAMA_VISION_MODEL` to be configured.
///
/// # Multipart Fields
/// - `file`: Image file (required)
/// - `prompt`: Custom description prompt (optional)
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
    let backend = state.vision_backend.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Vision model not configured. Set OLLAMA_VISION_MODEL environment variable.".into(),
        )
    })?;

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;
    let mut prompt: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart error: {}", e)))?
    {
        let field_name = field.name().map(|n| n.to_string());
        match field_name.as_deref() {
            Some("file") => {
                content_type = field.content_type().map(|c| c.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?
                        .to_vec(),
                );
            }
            Some("prompt") => {
                prompt = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?,
                );
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

    let description = backend
        .describe_image(&image_bytes, mime_type, prompt.as_deref())
        .await
        .map_err(|e| ApiError::Internal(format!("Vision model error: {}", e)))?;

    Ok(Json(DescribeImageResponse {
        description,
        model: backend.model_name().to_string(),
        image_size: image_bytes.len(),
    }))
}

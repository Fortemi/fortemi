//! Vision HTTP handlers.
//!
//! Provides ad-hoc image description via vision LLM without requiring
//! attachment creation. Useful for preview, inline analysis, and MCP tooling.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState};

/// Request body for describing an image.
#[derive(Debug, Deserialize)]
pub struct DescribeImageRequest {
    /// Base64-encoded image data (required).
    pub image_data: String,
    /// MIME type of the image (e.g., "image/png", "image/jpeg").
    /// Defaults to "image/png" if not provided.
    pub mime_type: Option<String>,
    /// Custom prompt for the vision model.
    /// If omitted, uses the default description prompt.
    pub prompt: Option<String>,
}

/// Response from image description.
#[derive(Debug, Serialize)]
pub struct DescribeImageResponse {
    /// AI-generated description of the image.
    pub description: String,
    /// Vision model used for description.
    pub model: String,
    /// Size of the decoded image in bytes.
    pub image_size: usize,
}

/// Describe an image using the configured vision model.
///
/// Accepts base64-encoded image data and returns an AI-generated description.
/// Requires `OLLAMA_VISION_MODEL` to be configured.
///
/// # Request Body
/// - `image_data`: Base64-encoded image bytes (required)
/// - `mime_type`: Image MIME type (optional, defaults to "image/png")
/// - `prompt`: Custom description prompt (optional)
///
/// # Returns
/// - 200 OK with description, model name, and image size
/// - 400 Bad Request if image_data is missing or invalid base64
/// - 503 Service Unavailable if vision model is not configured
#[utoipa::path(post, path = "/api/v1/vision/describe", tag = "Vision",
    request_body = DescribeImageRequest,
    responses(
        (status = 200, description = "Image described successfully"),
        (status = 400, description = "Invalid request"),
        (status = 503, description = "Vision model not configured"),
    ))]
pub async fn describe_image(
    State(state): State<AppState>,
    Json(req): Json<DescribeImageRequest>,
) -> Result<Json<DescribeImageResponse>, ApiError> {
    let backend = state.vision_backend.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Vision model not configured. Set OLLAMA_VISION_MODEL environment variable.".into(),
        )
    })?;

    // Decode base64 image data
    use base64::Engine;
    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.image_data)
        .map_err(|e| ApiError::BadRequest(format!("Invalid base64 image data: {}", e)))?;

    if image_bytes.is_empty() {
        return Err(ApiError::BadRequest("Image data is empty".into()));
    }

    let mime_type = req.mime_type.as_deref().unwrap_or("image/png");

    let description = backend
        .describe_image(&image_bytes, mime_type, req.prompt.as_deref())
        .await
        .map_err(|e| ApiError::Internal(format!("Vision model error: {}", e)))?;

    Ok(Json(DescribeImageResponse {
        description,
        model: backend.model_name().to_string(),
        image_size: image_bytes.len(),
    }))
}

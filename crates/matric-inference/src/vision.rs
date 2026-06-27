//! Vision backend traits and implementations for image description.

use async_trait::async_trait;
use matric_core::Result;
use serde::{Deserialize, Serialize};

fn diagnostic_len(value: &str) -> usize {
    value.chars().count()
}

/// Backend for describing images using vision LLMs.
#[async_trait]
pub trait VisionBackend: Send + Sync {
    /// Describe an image, optionally with a custom prompt.
    async fn describe_image(
        &self,
        image_data: &[u8],
        mime_type: &str,
        prompt: Option<&str>,
    ) -> Result<String>;

    /// Check if the vision backend is available.
    async fn health_check(&self) -> Result<bool>;

    /// Get the model name being used.
    fn model_name(&self) -> &str;

    /// Unload the vision model from VRAM immediately.
    ///
    /// Called by the job worker after the vision tier drains to free GPU memory.
    /// Default implementation is a no-op for backends that don't support unloading.
    async fn unload(&self) -> Result<()> {
        Ok(())
    }
}

/// Ollama-based vision backend (e.g., qwen3-vl, llava).
pub struct OllamaVisionBackend {
    base_url: String,
    model: String,
    client: reqwest::Client,
    timeout_secs: u64,
}

impl OllamaVisionBackend {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
            timeout_secs: 120,
        }
    }

    /// Create from environment variables.
    /// Uses DEFAULT_OLLAMA_VISION_MODEL if OLLAMA_VISION_MODEL is not set.
    /// Returns None only if OLLAMA_VISION_MODEL is explicitly set to empty string.
    pub fn from_env() -> Option<Self> {
        let model = std::env::var(matric_core::defaults::ENV_OLLAMA_VISION_MODEL)
            .unwrap_or_else(|_| matric_core::defaults::DEFAULT_OLLAMA_VISION_MODEL.to_string());
        if model.is_empty() {
            return None;
        }
        let base_url = std::env::var("OLLAMA_BASE")
            .or_else(|_| std::env::var("OLLAMA_URL"))
            .unwrap_or_else(|_| matric_core::defaults::OLLAMA_URL.to_string());
        Some(Self::new(base_url, model))
    }
}

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    images: Vec<String>, // base64 encoded
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[async_trait]
impl VisionBackend for OllamaVisionBackend {
    async fn describe_image(
        &self,
        image_data: &[u8],
        _mime_type: &str,
        prompt: Option<&str>,
    ) -> Result<String> {
        use base64::Engine;
        let image_b64 = base64::engine::general_purpose::STANDARD.encode(image_data);

        let default_prompt =
            "Describe this image in detail. Include any text visible in the image.";
        let prompt = prompt.unwrap_or(default_prompt);

        let request = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            images: vec![image_b64],
            stream: false,
        };

        let url = format!("{}/api/generate", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .send()
            .await
            .map_err(|e| matric_core::Error::Internal(format!("Vision request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(matric_core::Error::Internal(
                crate::diagnostics::backend_status_error("Vision", status, &body),
            ));
        }

        let result: OllamaGenerateResponse = response.json().await.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to parse vision response: {}", e))
        })?;

        Ok(result.response)
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn unload(&self) -> Result<()> {
        use tracing::{debug, info};

        let url = format!("{}/api/generate", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": "",
            "keep_alive": "0"
        });
        info!(
            model_len = diagnostic_len(&self.model),
            "Unloading vision model from VRAM"
        );
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                matric_core::Error::Internal(format!("Vision model unload failed: {}", e))
            })?;
        let _ = resp.bytes().await;
        debug!(
            model_len = diagnostic_len(&self.model),
            "Vision model unloaded from VRAM"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_len_reports_length_without_model_content() {
        let model = "tenant/private-vision:user@example.com-token=secret";
        let len = diagnostic_len(model);

        assert_eq!(len, model.chars().count());
        let diagnostic = format!("model_len={len}");
        assert!(!diagnostic.contains("tenant/private-vision"));
        assert!(!diagnostic.contains("user@example.com"));
        assert!(!diagnostic.contains("token=secret"));
    }

    #[test]
    fn test_ollama_vision_backend_new() {
        let backend =
            OllamaVisionBackend::new("http://localhost:11434".to_string(), "llava".to_string());
        assert_eq!(backend.base_url, "http://localhost:11434");
        assert_eq!(backend.model, "llava");
        assert_eq!(backend.timeout_secs, 120);
        assert_eq!(backend.model_name(), "llava");
    }

    #[test]
    fn test_ollama_vision_backend_constructor_with_custom_params() {
        let backend =
            OllamaVisionBackend::new("http://test:11434".to_string(), "qwen3-vl".to_string());
        assert_eq!(backend.base_url, "http://test:11434");
        assert_eq!(backend.model, "qwen3-vl");
        assert_eq!(backend.timeout_secs, 120);
    }

    #[test]
    fn test_ollama_vision_backend_constructor_with_default_url() {
        let backend = OllamaVisionBackend::new(
            matric_core::defaults::OLLAMA_URL.to_string(),
            "llava".to_string(),
        );
        assert_eq!(backend.base_url, matric_core::defaults::OLLAMA_URL);
        assert_eq!(backend.model, "llava");
    }

    #[test]
    fn test_ollama_generate_request_serialization() {
        let request = OllamaGenerateRequest {
            model: "llava".to_string(),
            prompt: "Describe this image".to_string(),
            images: vec!["base64data".to_string()],
            stream: false,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "llava");
        assert_eq!(json["prompt"], "Describe this image");
        assert_eq!(json["images"][0], "base64data");
        assert_eq!(json["stream"], false);
    }

    #[test]
    fn test_ollama_generate_response_deserialization() {
        let json = r#"{"response": "A dog sitting on grass"}"#;
        let response: OllamaGenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.response, "A dog sitting on grass");
    }
}

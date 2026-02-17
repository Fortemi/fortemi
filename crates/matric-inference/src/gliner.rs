//! GLiNER NER backend for zero-shot named entity recognition.
//!
//! GLiNER (Zaratiana et al., NAACL 2024) is a 0.5B BERT-based model that
//! outperforms GPT-4 on zero-shot NER at 100-200x the speed. This module
//! provides a client for the GLiNER sidecar service.
//!
//! # Configuration
//!
//! - `GLINER_BASE_URL`: Base URL of the GLiNER sidecar (default: `http://localhost:8090`)
//! - Set to empty string to disable GLiNER.

use async_trait::async_trait;
use matric_core::Result;
use serde::{Deserialize, Serialize};

/// A named entity extracted by GLiNER.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NerEntity {
    /// The entity text as it appears in the source.
    pub text: String,
    /// The entity type label (e.g., "organization", "person", "tool").
    pub label: String,
    /// Confidence score from the NER model (0.0-1.0).
    pub score: f32,
    /// Character start offset in the source text.
    pub start: usize,
    /// Character end offset in the source text.
    pub end: usize,
}

/// Result of NER extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NerResult {
    /// Extracted entities.
    pub entities: Vec<NerEntity>,
    /// Model name used for extraction.
    pub model: String,
    /// Length of the text that was processed.
    pub text_length: usize,
}

/// Backend trait for named entity recognition.
#[async_trait]
pub trait NerBackend: Send + Sync {
    /// Extract named entities from text.
    async fn extract(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: Option<f32>,
    ) -> Result<NerResult>;

    /// Check if the NER backend is available.
    async fn health_check(&self) -> Result<bool>;

    /// Get the model name being used.
    fn model_name(&self) -> &str;
}

/// GLiNER sidecar client.
pub struct GlinerBackend {
    base_url: String,
    model: String,
    client: reqwest::Client,
    timeout_secs: u64,
}

impl GlinerBackend {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            model: String::new(), // Populated on first health check
            client: reqwest::Client::new(),
            timeout_secs: 30,
        }
    }

    /// Create from environment variables.
    /// Returns None if `GLINER_BASE_URL` is explicitly set to empty string.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var(matric_core::defaults::ENV_GLINER_BASE_URL)
            .unwrap_or_else(|_| String::new());
        if base_url.is_empty() {
            return None;
        }
        Some(Self::new(base_url))
    }
}

/// Request payload for the GLiNER `/extract` endpoint.
#[derive(Serialize)]
struct ExtractRequest<'a> {
    text: &'a str,
    entity_types: &'a [&'a str],
    #[serde(skip_serializing_if = "Option::is_none")]
    threshold: Option<f32>,
}

/// Health check response from GLiNER.
#[derive(Deserialize)]
struct HealthResponse {
    status: String,
    #[allow(dead_code)]
    model: String,
}

#[async_trait]
impl NerBackend for GlinerBackend {
    async fn extract(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: Option<f32>,
    ) -> Result<NerResult> {
        let url = format!("{}/extract", self.base_url);

        let request = ExtractRequest {
            text,
            entity_types,
            threshold,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .send()
            .await
            .map_err(|e| matric_core::Error::Internal(format!("GLiNER request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(matric_core::Error::Internal(format!(
                "GLiNER API returned {}: {}",
                status, body
            )));
        }

        let result: NerResult = response.json().await.map_err(|e| {
            matric_core::Error::Internal(format!("Failed to parse GLiNER response: {}", e))
        })?;

        Ok(result)
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(health) = resp.json::<HealthResponse>().await {
                        if health.status == "healthy" {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Err(_) => Ok(false),
        }
    }

    fn model_name(&self) -> &str {
        if self.model.is_empty() {
            "gliner"
        } else {
            &self.model
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ner_entity_serialization() {
        let entity = NerEntity {
            text: "Google DeepMind".to_string(),
            label: "organization".to_string(),
            score: 0.95,
            start: 0,
            end: 15,
        };

        let json = serde_json::to_value(&entity).unwrap();
        assert_eq!(json["text"], "Google DeepMind");
        assert_eq!(json["label"], "organization");
        assert!((json["score"].as_f64().unwrap() - 0.95).abs() < 0.001);

        let deserialized: NerEntity = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.text, entity.text);
        assert_eq!(deserialized.label, entity.label);
        assert!((deserialized.score - entity.score).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gliner_backend_new() {
        let backend = GlinerBackend::new("http://localhost:8090".to_string());
        assert_eq!(backend.base_url, "http://localhost:8090");
        assert_eq!(backend.timeout_secs, 30);
        assert_eq!(backend.model_name(), "gliner");
    }

    #[test]
    fn test_extract_request_serialization() {
        let types = ["organization", "person"];
        let req = ExtractRequest {
            text: "Google DeepMind published a paper",
            entity_types: &types,
            threshold: Some(0.3),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["text"], "Google DeepMind published a paper");
        assert_eq!(json["entity_types"].as_array().unwrap().len(), 2);
        assert!((json["threshold"].as_f64().unwrap() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_extract_request_no_threshold() {
        let types = ["tool"];
        let req = ExtractRequest {
            text: "Using PyTorch",
            entity_types: &types,
            threshold: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("threshold").is_none());
    }
}

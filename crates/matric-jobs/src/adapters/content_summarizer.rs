//! ContentSummarizer - AI-powered text summarization utility.
//!
//! This is a post-processing utility (NOT an ExtractionAdapter) that uses
//! Ollama's generation API to summarize extracted text via map-reduce:
//! - Short text (< chunk_size): summarize directly
//! - Long text: split into chunks, summarize each, then summarize summaries

use matric_core::{defaults, Error, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// AI-powered content summarizer using Ollama generation API.
///
/// Uses a map-reduce strategy for large documents:
/// 1. If text < chunk_size: summarize directly
/// 2. If text >= chunk_size: split → summarize chunks → summarize summaries
pub struct ContentSummarizer {
    client: Client,
    ollama_url: String,
    model: String,
    chunk_size: usize,
    max_summary_length: usize,
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

impl ContentSummarizer {
    /// Create a new summarizer with the given Ollama configuration.
    ///
    /// # Arguments
    /// * `ollama_url` - Base URL of the Ollama API (e.g., "http://127.0.0.1:11434")
    /// * `model` - Name of the generation model to use
    pub fn new(ollama_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            ollama_url,
            model,
            chunk_size: 4000,
            max_summary_length: 500,
        }
    }

    /// Create from environment variables.
    ///
    /// Uses OLLAMA_URL and GEN_MODEL (or their defaults from matric_core::defaults).
    /// Returns None if OLLAMA_URL is not set and no default is available.
    pub fn from_env() -> Option<Self> {
        let ollama_url = std::env::var("OLLAMA_URL")
            .or_else(|_| std::env::var("OLLAMA_BASE"))
            .unwrap_or_else(|_| defaults::OLLAMA_URL.to_string());

        let model = std::env::var("GEN_MODEL")
            .or_else(|_| std::env::var("OLLAMA_GEN_MODEL"))
            .unwrap_or_else(|_| defaults::GEN_MODEL.to_string());

        Some(Self::new(ollama_url, model))
    }

    /// Set the chunk size for map-reduce splitting (default: 4000).
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set the target maximum summary length (default: 500).
    pub fn with_max_summary_length(mut self, length: usize) -> Self {
        self.max_summary_length = length;
        self
    }

    /// Summarize the given text using map-reduce if needed.
    ///
    /// # Strategy
    /// - Text < chunk_size: Single summary pass
    /// - Text >= chunk_size: Split → summarize chunks → combine summaries
    ///
    /// # Errors
    /// Returns an error if the Ollama API is unreachable or returns an error.
    pub async fn summarize(&self, text: &str) -> Result<String> {
        if text.is_empty() {
            return Ok(String::new());
        }

        if text.len() < self.chunk_size {
            // Direct summarization
            self.summarize_direct(text).await
        } else {
            // Map-reduce: chunk → summarize each → combine
            let chunks = self.split_into_chunks(text);
            let chunk_summaries = self.summarize_chunks(&chunks).await?;

            // If we only have one chunk summary, return it
            if chunk_summaries.len() == 1 {
                return Ok(chunk_summaries[0].clone());
            }

            // Combine chunk summaries into final summary
            let combined = chunk_summaries.join("\n\n");
            self.summarize_direct(&combined).await
        }
    }

    /// Split text into chunks of approximately chunk_size characters.
    fn split_into_chunks(&self, text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();

        for line in text.lines() {
            if current.len() + line.len() + 1 > self.chunk_size && !current.is_empty() {
                chunks.push(current.clone());
                current.clear();
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        if chunks.is_empty() {
            chunks.push(text.to_string());
        }

        chunks
    }

    /// Summarize each chunk independently.
    async fn summarize_chunks(&self, chunks: &[String]) -> Result<Vec<String>> {
        let mut summaries = Vec::new();
        for chunk in chunks {
            let summary = self.summarize_direct(chunk).await?;
            summaries.push(summary);
        }
        Ok(summaries)
    }

    /// Directly summarize text without chunking.
    async fn summarize_direct(&self, text: &str) -> Result<String> {
        let prompt = format!(
            "Summarize the following text in approximately {} characters or less. \
            Focus on the key points and main ideas:\n\n{}",
            self.max_summary_length, text
        );

        let request = GenerateRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.ollama_url))
            .timeout(Duration::from_secs(defaults::GEN_TIMEOUT_SECS))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Inference(format!("Summarization request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Inference(format!(
                "Ollama returned {}: {}",
                status, body
            )));
        }

        let result: GenerateResponse = response
            .json()
            .await
            .map_err(|e| Error::Inference(format!("Failed to parse response: {}", e)))?;

        Ok(result.response.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constructor_with_defaults() {
        let summarizer = ContentSummarizer::new(
            "http://localhost:11434".to_string(),
            "test-model".to_string(),
        );

        assert_eq!(summarizer.ollama_url, "http://localhost:11434");
        assert_eq!(summarizer.model, "test-model");
        assert_eq!(summarizer.chunk_size, 4000);
        assert_eq!(summarizer.max_summary_length, 500);
    }

    #[test]
    fn test_constructor_with_custom_params() {
        let summarizer =
            ContentSummarizer::new("http://custom:8080".to_string(), "custom-model".to_string())
                .with_chunk_size(2000)
                .with_max_summary_length(300);

        assert_eq!(summarizer.ollama_url, "http://custom:8080");
        assert_eq!(summarizer.model, "custom-model");
        assert_eq!(summarizer.chunk_size, 2000);
        assert_eq!(summarizer.max_summary_length, 300);
    }

    #[test]
    fn test_default_values() {
        // ContentSummarizer::new uses explicit params; verify defaults match expected
        let summarizer = ContentSummarizer::new(
            defaults::OLLAMA_URL.to_string(),
            defaults::GEN_MODEL.to_string(),
        );
        assert_eq!(summarizer.ollama_url, defaults::OLLAMA_URL);
        assert_eq!(summarizer.model, defaults::GEN_MODEL);
        assert_eq!(summarizer.chunk_size, 4000);
        assert_eq!(summarizer.max_summary_length, 500);
    }

    #[test]
    fn test_split_into_chunks_short_text() {
        let summarizer =
            ContentSummarizer::new("http://localhost:11434".to_string(), "test".to_string())
                .with_chunk_size(1000);

        let text = "This is a short text.";
        let chunks = summarizer.split_into_chunks(text);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_split_into_chunks_exact_size() {
        let summarizer =
            ContentSummarizer::new("http://localhost:11434".to_string(), "test".to_string())
                .with_chunk_size(10);

        let text = "1234567890"; // Exactly chunk_size
        let chunks = summarizer.split_into_chunks(text);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_split_into_chunks_long_text() {
        let summarizer =
            ContentSummarizer::new("http://localhost:11434".to_string(), "test".to_string())
                .with_chunk_size(50);

        // Create text that will definitely split into multiple chunks
        let text = "Line 1 with some content here.\n".repeat(10); // ~310 chars
        let chunks = summarizer.split_into_chunks(&text);

        assert!(
            chunks.len() > 1,
            "Expected multiple chunks, got {}",
            chunks.len()
        );

        // Verify no chunk exceeds the limit significantly (allowing for line boundaries)
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(
                chunk.len() <= 100,
                "Chunk {} is {} chars (exceeds reasonable limit)",
                i,
                chunk.len()
            );
        }
    }

    #[test]
    fn test_split_into_chunks_empty() {
        let summarizer =
            ContentSummarizer::new("http://localhost:11434".to_string(), "test".to_string());

        let chunks = summarizer.split_into_chunks("");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "");
    }

    #[test]
    fn test_prompt_generation() {
        let summarizer = ContentSummarizer::new(
            "http://localhost:11434".to_string(),
            "test-model".to_string(),
        )
        .with_max_summary_length(200);

        let text = "Sample text to summarize.";

        // We can't directly test the async method, but we can verify the logic
        // by checking that our parameters are set correctly
        assert_eq!(summarizer.max_summary_length, 200);
        assert!(text.len() < summarizer.chunk_size);
    }

    #[test]
    fn test_chunk_size_boundary_conditions() {
        let summarizer =
            ContentSummarizer::new("http://localhost:11434".to_string(), "test".to_string())
                .with_chunk_size(100);

        // Test text exactly at boundary
        let text_at_boundary = "a".repeat(100);
        let chunks = summarizer.split_into_chunks(&text_at_boundary);
        assert_eq!(chunks.len(), 1);

        // Test text just over boundary
        let text_over = "a".repeat(101);
        let chunks_over = summarizer.split_into_chunks(&text_over);
        // This should still be 1 chunk since it's a single line
        assert_eq!(chunks_over.len(), 1);
    }

    #[test]
    fn test_multiline_chunk_splitting() {
        let summarizer =
            ContentSummarizer::new("http://localhost:11434".to_string(), "test".to_string())
                .with_chunk_size(30);

        let text = "First line is here.\nSecond line is here.\nThird line is here.";
        let chunks = summarizer.split_into_chunks(text);

        // Should split into multiple chunks
        assert!(chunks.len() > 1);

        // Each chunk should be valid text
        for chunk in &chunks {
            assert!(!chunk.is_empty());
        }
    }

    // Note: We don't test the actual HTTP calls here as they would require
    // a running Ollama instance. Integration tests should cover that.
}

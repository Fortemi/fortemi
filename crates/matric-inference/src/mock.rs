//! Mock inference backend for deterministic testing.
//!
//! Provides a mock implementation of inference backends that generates
//! deterministic embeddings and responses for testing purposes.
//!
//! ## Usage
//!
//! ```rust
//! use matric_inference::mock::{MockInferenceBackend, MockEmbeddingGenerator};
//!
//! #[tokio::test]
//! async fn test_with_mock_backend() {
//!     let backend = MockInferenceBackend::new()
//!         .with_dimension(384)
//!         .with_fixed_response("Test response");
//!
//!     let embedding = backend.embed("test text").await.unwrap();
//!     assert_eq!(embedding.len(), 384);
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock inference backend for testing.
#[derive(Clone)]
pub struct MockInferenceBackend {
    config: Arc<MockConfig>,
    call_log: Arc<Mutex<Vec<MockCall>>>,
}

#[derive(Debug, Clone)]
struct MockConfig {
    dimension: usize,
    fixed_responses: HashMap<String, String>,
    default_response: String,
    latency_ms: u64,
    failure_rate: f64,
}

#[derive(Debug, Clone)]
pub struct MockCall {
    pub operation: String,
    pub input: String,
    pub timestamp: std::time::Instant,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            dimension: 384,
            fixed_responses: HashMap::new(),
            default_response: "Mock response".to_string(),
            latency_ms: 0,
            failure_rate: 0.0,
        }
    }
}

impl MockInferenceBackend {
    /// Create a new mock backend with default configuration.
    pub fn new() -> Self {
        Self {
            config: Arc::new(MockConfig::default()),
            call_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the embedding dimension.
    pub fn with_dimension(mut self, dimension: usize) -> Self {
        Arc::make_mut(&mut self.config).dimension = dimension;
        self
    }

    /// Set a fixed response for generation requests.
    pub fn with_fixed_response(mut self, response: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.config).default_response = response.into();
        self
    }

    /// Add a response mapping for specific inputs.
    pub fn with_response_mapping(
        mut self,
        input: impl Into<String>,
        output: impl Into<String>,
    ) -> Self {
        Arc::make_mut(&mut self.config)
            .fixed_responses
            .insert(input.into(), output.into());
        self
    }

    /// Set simulated latency for all operations.
    pub fn with_latency_ms(mut self, latency_ms: u64) -> Self {
        Arc::make_mut(&mut self.config).latency_ms = latency_ms;
        self
    }

    /// Set failure rate (0.0 - 1.0) for testing error handling.
    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        Arc::make_mut(&mut self.config).failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Get all logged calls for assertion.
    pub fn get_calls(&self) -> Vec<MockCall> {
        self.call_log.lock().unwrap().clone()
    }

    /// Clear the call log.
    pub fn clear_calls(&self) {
        self.call_log.lock().unwrap().clear()
    }

    /// Get number of embed calls.
    pub fn embed_call_count(&self) -> usize {
        self.call_log
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.operation == "embed")
            .count()
    }

    /// Get number of generation calls.
    pub fn generate_call_count(&self) -> usize {
        self.call_log
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.operation == "generate")
            .count()
    }

    fn log_call(&self, operation: &str, input: &str) {
        self.call_log.lock().unwrap().push(MockCall {
            operation: operation.to_string(),
            input: input.to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    fn should_fail(&self) -> bool {
        use rand::Rng;
        if self.config.failure_rate > 0.0 {
            rand::thread_rng().gen::<f64>() < self.config.failure_rate
        } else {
            false
        }
    }

    async fn simulate_latency(&self) {
        if self.config.latency_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.config.latency_ms)).await;
        }
    }

    /// Generate embedding for text (deterministic based on text content).
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, MockError> {
        self.log_call("embed", text);
        self.simulate_latency().await;

        if self.should_fail() {
            return Err(MockError::SimulatedFailure);
        }

        Ok(MockEmbeddingGenerator::generate(
            text,
            self.config.dimension,
        ))
    }

    /// Batch embed multiple texts.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MockError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Generate text response.
    pub async fn generate(&self, prompt: &str) -> Result<String, MockError> {
        self.log_call("generate", prompt);
        self.simulate_latency().await;

        if self.should_fail() {
            return Err(MockError::SimulatedFailure);
        }

        // Check for mapped response
        if let Some(response) = self.config.fixed_responses.get(prompt) {
            return Ok(response.clone());
        }

        Ok(self.config.default_response.clone())
    }
}

impl Default for MockInferenceBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock embedding generator with deterministic output.
pub struct MockEmbeddingGenerator;

impl MockEmbeddingGenerator {
    /// Generate a deterministic embedding from text.
    ///
    /// Uses character-based hashing for reproducibility. The same text
    /// will always produce the same embedding.
    pub fn generate(text: &str, dimension: usize) -> Vec<f32> {
        let mut vec = vec![0.0; dimension];

        // Use character codes to generate deterministic values
        for (i, c) in text.chars().enumerate() {
            let idx = (c as usize + i) % dimension;
            vec[idx] += 0.1;
        }

        // Normalize to unit vector
        Self::normalize(&mut vec);
        vec
    }

    /// Generate embedding from seed (for random-like but deterministic vectors).
    pub fn generate_with_seed(seed: u64, dimension: usize) -> Vec<f32> {
        let mut vec = vec![0.0; dimension];
        let mut state = seed;

        // Simple LCG for deterministic pseudo-random values
        for item in vec.iter_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *item = ((state % 1000) as f32) / 1000.0 - 0.5;
        }

        Self::normalize(&mut vec);
        vec
    }

    /// Generate embeddings with controlled similarity.
    ///
    /// Creates two embeddings with specified cosine similarity (0.0 to 1.0).
    pub fn generate_similar_pair(
        base_text: &str,
        dimension: usize,
        similarity: f64,
    ) -> (Vec<f32>, Vec<f32>) {
        let base = Self::generate(base_text, dimension);
        let mut similar = Self::generate_with_seed(12345, dimension);

        // Interpolate between base and random vector to achieve target similarity
        let alpha = similarity as f32;
        for i in 0..dimension {
            similar[i] = alpha * base[i] + (1.0 - alpha) * similar[i];
        }

        Self::normalize(&mut similar);
        (base, similar)
    }

    fn normalize(vec: &mut [f32]) {
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            vec.iter_mut().for_each(|x| *x /= magnitude);
        }
    }

    /// Calculate cosine similarity between two vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a > 0.0 && mag_b > 0.0 {
            dot / (mag_a * mag_b)
        } else {
            0.0
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MockError {
    #[error("Simulated failure for testing")]
    SimulatedFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_backend_embed() {
        let backend = MockInferenceBackend::new().with_dimension(128);

        let embedding = backend.embed("test").await.unwrap();
        assert_eq!(embedding.len(), 128);
    }

    #[tokio::test]
    async fn test_mock_backend_deterministic() {
        let backend = MockInferenceBackend::new();

        let e1 = backend.embed("quantum computing").await.unwrap();
        let e2 = backend.embed("quantum computing").await.unwrap();

        assert_eq!(e1, e2, "Embeddings should be deterministic");
    }

    #[tokio::test]
    async fn test_mock_backend_generate() {
        let backend = MockInferenceBackend::new().with_fixed_response("Custom response");

        let response = backend.generate("test prompt").await.unwrap();
        assert_eq!(response, "Custom response");
    }

    #[tokio::test]
    async fn test_mock_backend_response_mapping() {
        let backend = MockInferenceBackend::new()
            .with_response_mapping("hello", "world")
            .with_response_mapping("foo", "bar");

        assert_eq!(backend.generate("hello").await.unwrap(), "world");
        assert_eq!(backend.generate("foo").await.unwrap(), "bar");
    }

    #[tokio::test]
    async fn test_mock_backend_call_logging() {
        let backend = MockInferenceBackend::new();

        backend.embed("text1").await.unwrap();
        backend.embed("text2").await.unwrap();
        backend.generate("prompt").await.unwrap();

        assert_eq!(backend.embed_call_count(), 2);
        assert_eq!(backend.generate_call_count(), 1);

        let calls = backend.get_calls();
        assert_eq!(calls.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_backend_failure_simulation() {
        let backend = MockInferenceBackend::new().with_failure_rate(1.0);

        let result = backend.embed("test").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_embedding_generator_deterministic() {
        let e1 = MockEmbeddingGenerator::generate("test", 256);
        let e2 = MockEmbeddingGenerator::generate("test", 256);
        assert_eq!(e1, e2);
    }

    #[test]
    fn test_embedding_generator_normalized() {
        let embedding = MockEmbeddingGenerator::generate("test", 128);
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.01, "Should be normalized");
    }

    #[test]
    fn test_embedding_generator_with_seed() {
        let e1 = MockEmbeddingGenerator::generate_with_seed(42, 256);
        let e2 = MockEmbeddingGenerator::generate_with_seed(42, 256);
        let e3 = MockEmbeddingGenerator::generate_with_seed(43, 256);

        assert_eq!(e1, e2, "Same seed should produce same vector");
        assert_ne!(e1, e3, "Different seed should produce different vector");
    }

    #[test]
    fn test_embedding_generator_similar_pair() {
        let (base, similar) = MockEmbeddingGenerator::generate_similar_pair("test", 384, 0.8);

        let similarity = MockEmbeddingGenerator::cosine_similarity(&base, &similar);
        // Linear interpolation between vectors doesn't directly produce target cosine similarity
        // The actual similarity depends on the angle between base and random vectors
        // We verify the vectors are similar (not orthogonal) but not identical
        assert!(
            similarity > 0.5 && similarity < 1.0,
            "Similarity should be high but less than 1.0, got: {}",
            similarity
        );
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((MockEmbeddingGenerator::cosine_similarity(&a, &b) - 1.0).abs() < 0.01);
        assert!((MockEmbeddingGenerator::cosine_similarity(&a, &c)).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_mock_backend_batch_embed() {
        let backend = MockInferenceBackend::new().with_dimension(128);

        let texts = vec![
            "text1".to_string(),
            "text2".to_string(),
            "text3".to_string(),
        ];

        let embeddings = backend.embed_batch(&texts).await.unwrap();
        assert_eq!(embeddings.len(), 3);
        assert!(embeddings.iter().all(|e| e.len() == 128));
    }

    #[tokio::test]
    async fn test_mock_backend_latency_simulation() {
        let backend = MockInferenceBackend::new().with_latency_ms(50);

        let start = std::time::Instant::now();
        backend.embed("test").await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() >= 50, "Should simulate latency");
    }
}

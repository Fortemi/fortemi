//! Integration tests for OpenAI-compatible backend.
//!
//! These tests work with any OpenAI-compatible API endpoint including:
//! - OpenAI cloud API
//! - Ollama (in OpenAI compatibility mode)
//! - vLLM, LocalAI, LM Studio, etc.
//!
//! # Quick Start (Ollama)
//!
//! ```bash
//! # Enable external integration tests and configure endpoint
//! RUN_EXTERNAL_TESTS=1 \
//! OPENAI_BASE_URL=http://localhost:11434/v1 \
//! OPENAI_EMBED_MODEL=nomic-embed-text \
//! OPENAI_GEN_MODEL=gpt-oss:20b \
//! OPENAI_EMBED_DIM=768 \
//! cargo test --package matric-inference --features openai,integration --test openai_integration_test -- --nocapture
//! ```
//!
//! # Against real OpenAI API
//!
//! ```bash
//! RUN_EXTERNAL_TESTS=1 \
//! OPENAI_API_KEY=sk-... \
//! cargo test --package matric-inference --features openai,integration --test openai_integration_test -- --nocapture
//! ```
//!
//! # Environment Variables
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | RUN_EXTERNAL_TESTS | (unset) | Set to "1" or "true" to enable tests |
//! | OPENAI_BASE_URL | https://api.openai.com/v1 | API endpoint |
//! | OPENAI_API_KEY | (none) | API key (optional for local) |
//! | OPENAI_EMBED_MODEL | text-embedding-3-small | Embedding model |
//! | OPENAI_GEN_MODEL | gpt-4o-mini | Generation model |
//! | OPENAI_EMBED_DIM | 1536 | Embedding dimension |
//! | OPENAI_TIMEOUT | 300 | Request timeout (seconds) |

#![cfg(all(feature = "openai", feature = "integration"))]

use matric_core::{EmbeddingBackend, GenerationBackend, InferenceBackend};
use matric_inference::openai::{OpenAIBackend, OpenAIConfig};

/// Check if external integration tests should run.
/// Set RUN_EXTERNAL_TESTS=1 or RUN_EXTERNAL_TESTS=true to enable.
fn should_run_external_tests() -> bool {
    std::env::var("RUN_EXTERNAL_TESTS")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Skip test with message if external tests are not enabled.
/// Returns true if the test should be skipped.
fn skip_if_external_tests_disabled(test_name: &str) -> bool {
    if !should_run_external_tests() {
        println!(
            "⏭️  Skipping {} - set RUN_EXTERNAL_TESTS=1 to enable external API tests",
            test_name
        );
        return true;
    }
    false
}

/// Helper to create backend from environment
fn create_backend() -> OpenAIBackend {
    OpenAIBackend::from_env().expect("Failed to create OpenAI backend from environment")
}

/// Helper to print test configuration
fn print_config(backend: &OpenAIBackend) {
    let config = backend.config();
    println!("\n=== OpenAI Backend Configuration ===");
    println!("  Base URL: {}", config.base_url);
    println!(
        "  API Key: {}",
        if config.api_key.is_some() {
            "SET"
        } else {
            "NOT SET"
        }
    );
    println!("  Embed Model: {}", config.embed_model);
    println!("  Gen Model: {}", config.gen_model);
    println!("  Embed Dimension: {}", config.embed_dimension);
    println!("  Timeout: {}s", config.timeout_seconds);
    println!("=====================================\n");
}

#[tokio::test]
async fn test_health_check() {
    if skip_if_external_tests_disabled("test_health_check") {
        return;
    }

    let backend = create_backend();
    print_config(&backend);

    println!("Testing health check...");
    let result = backend.health_check().await;

    match result {
        Ok(healthy) => {
            println!(
                "Health check result: {}",
                if healthy { "HEALTHY" } else { "UNHEALTHY" }
            );
            assert!(healthy, "Backend should be healthy");
        }
        Err(e) => {
            panic!("Health check failed with error: {}", e);
        }
    }
}

#[tokio::test]
async fn test_embedding_single() {
    if skip_if_external_tests_disabled("test_embedding_single") {
        return;
    }

    let backend = create_backend();
    print_config(&backend);

    println!("Testing single text embedding...");
    let texts = vec!["Hello, world!".to_string()];

    let result = backend.embed_texts(&texts).await;

    match result {
        Ok(embeddings) => {
            assert_eq!(embeddings.len(), 1, "Should return 1 embedding");
            let dim = embeddings[0].as_slice().len();
            println!("Embedding dimension: {}", dim);
            println!(
                "First 5 values: {:?}",
                &embeddings[0].as_slice()[..5.min(dim)]
            );
            assert_eq!(dim, backend.dimension(), "Dimension should match config");
        }
        Err(e) => {
            panic!("Embedding failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_embedding_batch() {
    if skip_if_external_tests_disabled("test_embedding_batch") {
        return;
    }

    let backend = create_backend();

    println!("Testing batch embedding...");
    let texts = vec![
        "The quick brown fox jumps over the lazy dog.".to_string(),
        "Machine learning is a subset of artificial intelligence.".to_string(),
        "Rust is a systems programming language focused on safety.".to_string(),
    ];

    let result = backend.embed_texts(&texts).await;

    match result {
        Ok(embeddings) => {
            assert_eq!(embeddings.len(), 3, "Should return 3 embeddings");
            println!("Generated {} embeddings", embeddings.len());

            for (i, emb) in embeddings.iter().enumerate() {
                println!("  [{}] dimension: {}", i, emb.as_slice().len());
                assert_eq!(emb.as_slice().len(), backend.dimension());
            }
        }
        Err(e) => {
            panic!("Batch embedding failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_embedding_empty() {
    if skip_if_external_tests_disabled("test_embedding_empty") {
        return;
    }

    let backend = create_backend();

    println!("Testing empty embedding request...");
    let texts: Vec<String> = vec![];

    let result = backend.embed_texts(&texts).await;

    match result {
        Ok(embeddings) => {
            assert!(
                embeddings.is_empty(),
                "Should return empty vec for empty input"
            );
            println!("Correctly returned empty result for empty input");
        }
        Err(e) => {
            panic!("Empty embedding request failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_generation_simple() {
    if skip_if_external_tests_disabled("test_generation_simple") {
        return;
    }

    let backend = create_backend();
    print_config(&backend);

    println!("Testing simple generation...");
    let prompt = "What is 2 + 2? Answer with just the number.";

    let result = backend.generate(prompt).await;

    match result {
        Ok(response) => {
            println!("Prompt: {}", prompt);
            println!("Response: {}", response);
            assert!(!response.is_empty(), "Response should not be empty");
            // Check if response contains "4" somewhere
            assert!(
                response.contains('4'),
                "Response should contain '4': got '{}'",
                response
            );
        }
        Err(e) => {
            panic!("Generation failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_generation_with_system() {
    if skip_if_external_tests_disabled("test_generation_with_system") {
        return;
    }

    let backend = create_backend();

    println!("Testing generation with system prompt...");
    let system = "You are a helpful assistant that responds only in JSON format.";
    let prompt = "List three colors.";

    let result = backend.generate_with_system(system, prompt).await;

    match result {
        Ok(response) => {
            println!("System: {}", system);
            println!("Prompt: {}", prompt);
            println!("Response: {}", response);
            assert!(!response.is_empty(), "Response should not be empty");
            // Response should look somewhat JSON-like (has brackets or braces)
            let looks_like_json = response.contains('{') || response.contains('[');
            println!("Looks like JSON: {}", looks_like_json);
        }
        Err(e) => {
            panic!("Generation with system prompt failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_generation_longer() {
    if skip_if_external_tests_disabled("test_generation_longer") {
        return;
    }

    let backend = create_backend();

    println!("Testing longer generation...");
    let prompt = "Explain what public key encryption is in 2-3 sentences.";

    let result = backend.generate(prompt).await;

    match result {
        Ok(response) => {
            println!("Prompt: {}", prompt);
            println!("Response ({} chars): {}", response.len(), response);
            assert!(!response.is_empty(), "Response should not be empty");
            assert!(response.len() > 50, "Response should be substantive");
        }
        Err(e) => {
            panic!("Longer generation failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_model_names() {
    if skip_if_external_tests_disabled("test_model_names") {
        return;
    }

    let backend = create_backend();

    let embed_model = EmbeddingBackend::model_name(&backend);
    let gen_model = GenerationBackend::model_name(&backend);

    println!("Embedding model: {}", embed_model);
    println!("Generation model: {}", gen_model);

    assert!(!embed_model.is_empty());
    assert!(!gen_model.is_empty());
}

/// Test semantic similarity - embeddings of similar texts should be closer
#[tokio::test]
async fn test_embedding_similarity() {
    if skip_if_external_tests_disabled("test_embedding_similarity") {
        return;
    }

    let backend = create_backend();

    println!("Testing embedding similarity...");
    let texts = vec![
        "The cat sat on the mat.".to_string(),     // [0] - about cats
        "A kitten rested on the rug.".to_string(), // [1] - similar to [0]
        "Python is a programming language.".to_string(), // [2] - different topic
    ];

    let result = backend.embed_texts(&texts).await;

    match result {
        Ok(embeddings) => {
            // Calculate cosine similarities
            let sim_01 = cosine_similarity(&embeddings[0], &embeddings[1]);
            let sim_02 = cosine_similarity(&embeddings[0], &embeddings[2]);
            let sim_12 = cosine_similarity(&embeddings[1], &embeddings[2]);

            println!("Similarity (cat/kitten): {:.4}", sim_01);
            println!("Similarity (cat/python): {:.4}", sim_02);
            println!("Similarity (kitten/python): {:.4}", sim_12);

            // Cat and kitten sentences should be more similar than cat and programming
            assert!(
                sim_01 > sim_02,
                "Cat/kitten ({:.4}) should be more similar than cat/python ({:.4})",
                sim_01,
                sim_02
            );
        }
        Err(e) => {
            panic!("Similarity test failed: {}", e);
        }
    }
}

/// Helper function to calculate cosine similarity
fn cosine_similarity(a: &matric_core::Vector, b: &matric_core::Vector) -> f32 {
    let a_slice = a.as_slice();
    let b_slice = b.as_slice();

    let dot: f32 = a_slice.iter().zip(b_slice.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a_slice.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b_slice.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Custom configuration test
#[tokio::test]
async fn test_custom_config() {
    if skip_if_external_tests_disabled("test_custom_config") {
        return;
    }

    let config = OpenAIConfig {
        base_url: std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
        api_key: std::env::var("OPENAI_API_KEY").ok(),
        embed_model: std::env::var("OPENAI_EMBED_MODEL")
            .unwrap_or_else(|_| "nomic-embed-text".to_string()),
        gen_model: std::env::var("OPENAI_GEN_MODEL").unwrap_or_else(|_| "gpt-oss:20b".to_string()),
        embed_dimension: std::env::var("OPENAI_EMBED_DIM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(768),
        timeout_seconds: 120,
        skip_tls_verify: false,
        http_referer: None,
        x_title: None,
    };

    let backend = OpenAIBackend::new(config).expect("Should create backend with custom config");

    println!("Created backend with custom config:");
    print_config(&backend);

    // Run a health check to verify connectivity
    let healthy = backend.health_check().await.unwrap_or(false);
    println!(
        "Health check: {}",
        if healthy { "PASSED" } else { "FAILED" }
    );
    assert!(healthy, "Backend should be reachable");
}

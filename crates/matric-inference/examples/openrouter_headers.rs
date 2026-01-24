//! Example demonstrating OpenRouter header support.
//!
//! This example shows how to configure the OpenAI-compatible backend
//! with OpenRouter-specific headers for app identification and ranking.
//!
//! # Usage
//!
//! ```bash
//! # Using environment variables
//! OPENAI_BASE_URL=https://openrouter.ai/api/v1 \
//! OPENAI_API_KEY=sk-or-... \
//! OPENAI_HTTP_REFERER=https://myapp.com \
//! OPENAI_X_TITLE="My App" \
//! cargo run --example openrouter_headers --features openai
//!
//! # Or without environment variables (uses direct config)
//! cargo run --example openrouter_headers --features openai
//! ```

use matric_core::{EmbeddingBackend, GenerationBackend};
use matric_inference::openai::{OpenAIBackend, OpenAIConfig};

#[tokio::main]
async fn main() {
    // Example 1: Using environment variables
    println!("=== Example 1: Using Environment Variables ===");
    if let Ok(backend) = OpenAIBackend::from_env() {
        let config = backend.config();
        println!("Base URL: {}", config.base_url);
        println!(
            "HTTP-Referer: {}",
            config
                .http_referer
                .as_ref()
                .unwrap_or(&"(not set)".to_string())
        );
        println!(
            "X-Title: {}",
            config.x_title.as_ref().unwrap_or(&"(not set)".to_string())
        );
        println!();
    } else {
        println!("Could not create backend from environment variables");
        println!();
    }

    // Example 2: Direct configuration
    println!("=== Example 2: Direct Configuration ===");
    let config = OpenAIConfig {
        base_url: "https://openrouter.ai/api/v1".to_string(),
        api_key: Some("sk-or-v1-...".to_string()), // Replace with your actual key
        embed_model: "text-embedding-3-small".to_string(),
        gen_model: "openai/gpt-4o-mini".to_string(),
        embed_dimension: 1536,
        timeout_seconds: 120,
        skip_tls_verify: false,
        http_referer: Some("https://myapp.com".to_string()),
        x_title: Some("My App".to_string()),
    };

    println!("Config created with headers:");
    println!("  HTTP-Referer: {:?}", config.http_referer);
    println!("  X-Title: {:?}", config.x_title);
    println!();

    if let Ok(backend) = OpenAIBackend::new(config) {
        println!("Backend created successfully!");
        println!("Embed model: {}", EmbeddingBackend::model_name(&backend));
        println!("Gen model: {}", GenerationBackend::model_name(&backend));
    } else {
        println!("Failed to create backend");
    }

    // Example 3: Optional headers (only HTTP-Referer)
    println!();
    println!("=== Example 3: Only HTTP-Referer ===");
    let config = OpenAIConfig {
        base_url: "https://openrouter.ai/api/v1".to_string(),
        api_key: Some("sk-or-v1-...".to_string()),
        embed_model: "text-embedding-3-small".to_string(),
        gen_model: "openai/gpt-4o-mini".to_string(),
        embed_dimension: 1536,
        timeout_seconds: 120,
        skip_tls_verify: false,
        http_referer: Some("https://myapp.com".to_string()),
        x_title: None, // No title
    };

    println!("Config with only HTTP-Referer:");
    println!("  HTTP-Referer: {:?}", config.http_referer);
    println!("  X-Title: {:?}", config.x_title);

    // Example 4: No optional headers
    println!();
    println!("=== Example 4: No Optional Headers ===");
    let config = OpenAIConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        api_key: Some("sk-...".to_string()),
        embed_model: "text-embedding-3-small".to_string(),
        gen_model: "gpt-4o-mini".to_string(),
        embed_dimension: 1536,
        timeout_seconds: 120,
        skip_tls_verify: false,
        http_referer: None,
        x_title: None,
    };

    println!("Standard OpenAI config (no extra headers):");
    println!("  HTTP-Referer: {:?}", config.http_referer);
    println!("  X-Title: {:?}", config.x_title);
}

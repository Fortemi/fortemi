//! Example demonstrating raw mode for thinking models.
//!
//! This example shows how the Ollama backend automatically enables raw mode
//! for thinking models like DeepSeek R1, which use `<think>` tags to expose
//! their chain-of-thought reasoning.
//!
//! Run with:
//! ```bash
//! cargo run --example raw_mode_demo --features ollama
//! ```
//!
//! Note: Requires a running Ollama instance with deepseek-r1:14b or similar model.

use matric_inference::model_config::requires_raw_mode;
use matric_inference::OllamaBackend;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Raw Mode Detection Demo ===\n");

    // Check various models
    let test_models = vec![
        "deepseek-r1:14b",
        "deepseek-r1:70b",
        "Mistral-Nemo-12B-Thinking",
        "llama3.1:8b",
        "gpt-oss:20b",
        "qwen2.5-coder:7b",
    ];

    println!("Model Raw Mode Requirements:");
    for model in &test_models {
        let needs_raw = requires_raw_mode(model);
        let status = if needs_raw { "YES" } else { "NO " };
        println!("  [{}] {}", status, model);
    }

    println!("\n=== Backend Configuration Demo ===\n");

    // Example 1: Create backend with thinking model
    let _thinking_backend = OllamaBackend::with_config(
        "http://localhost:11434".to_string(),
        "nomic-embed-text".to_string(),
        "deepseek-r1:14b".to_string(),
        768,
    );
    println!("Created backend with thinking model: deepseek-r1:14b");
    println!(
        "  Raw mode will be: {}",
        if requires_raw_mode("deepseek-r1:14b") {
            "ENABLED"
        } else {
            "disabled"
        }
    );

    // Example 2: Create backend with regular model
    let _regular_backend = OllamaBackend::with_config(
        "http://localhost:11434".to_string(),
        "nomic-embed-text".to_string(),
        "llama3.1:8b".to_string(),
        768,
    );
    println!("\nCreated backend with regular model: llama3.1:8b");
    println!(
        "  Raw mode will be: {}",
        if requires_raw_mode("llama3.1:8b") {
            "ENABLED"
        } else {
            "disabled"
        }
    );

    println!("\n=== Generation Example ===\n");
    println!("Note: Set OLLAMA_GEN_MODEL=deepseek-r1:14b to test generation with a thinking model");
    println!("Example prompt: 'Explain step-by-step how to solve 2+2'");
    println!("\nExpected output with raw mode:");
    println!("  <think>");
    println!("  Let me break this down:");
    println!("  - We have two numbers: 2 and 2");
    println!("  - Addition combines quantities");
    println!("  - 2 + 2 = 4");
    println!("  </think>");
    println!("  ");
    println!("  To solve 2+2, we add the two numbers together, which gives us 4.");

    // Uncomment to test with actual Ollama instance:
    /*
    if let Ok(model) = std::env::var("OLLAMA_GEN_MODEL") {
        if requires_raw_mode(&model) {
            println!("\n=== Live Test with {} ===\n", model);
            let backend = OllamaBackend::from_env();

            match backend.generate("Explain in 2-3 sentences what quantum computing is.").await {
                Ok(response) => {
                    println!("Response:\n{}\n", response);

                    if response.contains("<think>") {
                        println!("✓ Raw mode working! Found <think> tags in response.");
                    } else {
                        println!("Note: No <think> tags found. Model may not be a thinking model.");
                    }
                }
                Err(e) => {
                    println!("Error: {}", e);
                    println!("Make sure Ollama is running with the model available.");
                }
            }
        }
    }
    */

    Ok(())
}

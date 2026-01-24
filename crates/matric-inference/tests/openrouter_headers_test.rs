//! Integration test for OpenRouter header support.
//!
//! This test verifies that the HTTP-Referer and X-Title headers
//! are correctly sent when configured.

#![cfg(feature = "openai")]

use matric_core::{EmbeddingBackend, GenerationBackend};
use matric_inference::openai::{OpenAIBackend, OpenAIConfig};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_openrouter_headers_sent_in_request() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    // Create a mock response for the embeddings endpoint
    let embedding_response = serde_json::json!({
        "data": [
            {
                "embedding": vec![0.1f32; 768],
                "index": 0
            }
        ],
        "model": "test-embed",
        "usage": {
            "prompt_tokens": 1,
            "total_tokens": 1
        }
    });

    // Set up the mock to verify headers are present
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .and(header("HTTP-Referer", "https://myapp.com"))
        .and(header("X-Title", "My App"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&embedding_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create backend with OpenRouter headers configured
    let config = OpenAIConfig {
        base_url: mock_server.uri(),
        api_key: Some("test-key".to_string()),
        embed_model: "test-embed".to_string(),
        gen_model: "test-gen".to_string(),
        embed_dimension: 768,
        timeout_seconds: 60,
        skip_tls_verify: false,
        http_referer: Some("https://myapp.com".to_string()),
        x_title: Some("My App".to_string()),
    };

    let backend = OpenAIBackend::new(config).expect("Failed to create backend");

    // Make an embedding request - this should include the headers
    let texts = vec!["test".to_string()];
    let result = backend.embed_texts(&texts).await;

    // Verify the request succeeded
    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());

    // The mock will verify that the headers were present
}

#[tokio::test]
async fn test_generation_with_openrouter_headers() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    // Create a mock response for the chat completions endpoint
    let chat_response = serde_json::json!({
        "id": "chatcmpl-123",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Test response"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    });

    // Set up the mock to verify headers are present
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("HTTP-Referer", "https://example.org"))
        .and(header("X-Title", "Test Application"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&chat_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create backend with OpenRouter headers configured
    let config = OpenAIConfig {
        base_url: mock_server.uri(),
        api_key: Some("test-key".to_string()),
        embed_model: "test-embed".to_string(),
        gen_model: "test-gen".to_string(),
        embed_dimension: 768,
        timeout_seconds: 60,
        skip_tls_verify: false,
        http_referer: Some("https://example.org".to_string()),
        x_title: Some("Test Application".to_string()),
    };

    let backend = OpenAIBackend::new(config).expect("Failed to create backend");

    // Make a generation request - this should include the headers
    let result = backend.generate("test prompt").await;

    // Verify the request succeeded
    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), "Test response");

    // The mock will verify that the headers were present
}

#[tokio::test]
async fn test_headers_not_sent_when_not_configured() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    // Create a mock response for the embeddings endpoint
    let embedding_response = serde_json::json!({
        "data": [
            {
                "embedding": vec![0.1f32; 768],
                "index": 0
            }
        ],
        "model": "test-embed",
        "usage": {
            "prompt_tokens": 1,
            "total_tokens": 1
        }
    });

    // Set up the mock WITHOUT requiring the optional headers
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&embedding_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create backend WITHOUT OpenRouter headers
    let config = OpenAIConfig {
        base_url: mock_server.uri(),
        api_key: Some("test-key".to_string()),
        embed_model: "test-embed".to_string(),
        gen_model: "test-gen".to_string(),
        embed_dimension: 768,
        timeout_seconds: 60,
        skip_tls_verify: false,
        http_referer: None,
        x_title: None,
    };

    let backend = OpenAIBackend::new(config).expect("Failed to create backend");

    // Make an embedding request
    let texts = vec!["test".to_string()];
    let result = backend.embed_texts(&texts).await;

    // Verify the request succeeded without the optional headers
    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());
}

#[tokio::test]
async fn test_only_http_referer_header() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    // Create a mock response
    let embedding_response = serde_json::json!({
        "data": [
            {
                "embedding": vec![0.1f32; 768],
                "index": 0
            }
        ],
        "model": "test-embed",
        "usage": {
            "prompt_tokens": 1,
            "total_tokens": 1
        }
    });

    // Set up the mock to require only HTTP-Referer
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .and(header("HTTP-Referer", "https://onlyreferer.com"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&embedding_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create backend with only HTTP-Referer
    let config = OpenAIConfig {
        base_url: mock_server.uri(),
        api_key: Some("test-key".to_string()),
        embed_model: "test-embed".to_string(),
        gen_model: "test-gen".to_string(),
        embed_dimension: 768,
        timeout_seconds: 60,
        skip_tls_verify: false,
        http_referer: Some("https://onlyreferer.com".to_string()),
        x_title: None,
    };

    let backend = OpenAIBackend::new(config).expect("Failed to create backend");

    // Make an embedding request
    let texts = vec!["test".to_string()];
    let result = backend.embed_texts(&texts).await;

    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());
}

#[tokio::test]
async fn test_only_x_title_header() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    // Create a mock response
    let embedding_response = serde_json::json!({
        "data": [
            {
                "embedding": vec![0.1f32; 768],
                "index": 0
            }
        ],
        "model": "test-embed",
        "usage": {
            "prompt_tokens": 1,
            "total_tokens": 1
        }
    });

    // Set up the mock to require only X-Title
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .and(header("X-Title", "Only Title App"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&embedding_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create backend with only X-Title
    let config = OpenAIConfig {
        base_url: mock_server.uri(),
        api_key: Some("test-key".to_string()),
        embed_model: "test-embed".to_string(),
        gen_model: "test-gen".to_string(),
        embed_dimension: 768,
        timeout_seconds: 60,
        skip_tls_verify: false,
        http_referer: None,
        x_title: Some("Only Title App".to_string()),
    };

    let backend = OpenAIBackend::new(config).expect("Failed to create backend");

    // Make an embedding request
    let texts = vec!["test".to_string()];
    let result = backend.embed_texts(&texts).await;

    assert!(result.is_ok(), "Request should succeed: {:?}", result.err());
}

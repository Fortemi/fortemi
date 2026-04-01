//! Integration tests for the HotM consumer contract (Issue #549).
//!
//! Tests validate the POST /api/v1/chat endpoint behavior:
//! - Empty input returns 400
//! - Missing input field returns 400
//! - Invalid JSON returns 400/422
//! - Successful response matches HotM contract shape
//! - Response includes required fields (messages, actions, model_info)
//!
//! Requires a running API server with `API_BASE_URL` env var set.
//! Tests that require Ollama (generation) only run when chat is available.

/// Get the API base URL for testing.
fn api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

/// Check if the API server is reachable and API_BASE_URL is explicitly set.
async fn api_available() -> bool {
    if std::env::var("API_BASE_URL").is_err() {
        return false;
    }
    reqwest::Client::new()
        .get(format!("{}/health", api_base_url()))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Check if chat endpoint is configured (Ollama reachable).
async fn chat_available(client: &reqwest::Client) -> bool {
    let resp = client
        .get(format!("{}/health", api_base_url()))
        .send()
        .await
        .ok();
    if let Some(r) = resp {
        if let Ok(body) = r.json::<serde_json::Value>().await {
            return body
                .pointer("/capabilities/chat/available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
    }
    false
}

macro_rules! require_api {
    () => {
        if !api_available().await {
            eprintln!(
                "Skipping: API_BASE_URL not set or server not available at {}",
                api_base_url()
            );
            return;
        }
    };
}

// =============================================================================
// Error Cases (Issue #549 test cases #9, #16, #17)
// =============================================================================

/// Issue #549 test case #9: Empty input string returns 400.
#[tokio::test]
async fn test_chat_empty_input_returns_400() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"input": ""}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400, "Empty input should return 400");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.get("error").is_some(),
        "Error response should contain 'error' field"
    );
}

/// Issue #549 test case #9 variant: Whitespace-only input returns 400.
#[tokio::test]
async fn test_chat_whitespace_only_input_returns_400() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"input": "   \t\n  "}))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        400,
        "Whitespace-only input should return 400"
    );
}

/// Issue #549 test case #17: Missing `input` field returns 400/422.
#[tokio::test]
async fn test_chat_missing_input_field_returns_error() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"context": {"note_id": "abc"}}))
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 422,
        "Missing input should return 400 or 422, got {}",
        status
    );
}

/// Issue #549 test case #16: Invalid JSON body returns 400/422.
#[tokio::test]
async fn test_chat_invalid_json_returns_error() {
    require_api!();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .header("content-type", "application/json")
        .body("{not valid json")
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 422,
        "Invalid JSON should return 400 or 422, got {}",
        status
    );
}

// =============================================================================
// Happy Path (Issue #549 test cases #1-5) — require Ollama
// =============================================================================

/// Issue #549 test case #1: Simple conversation with no context.
#[tokio::test]
async fn test_chat_simple_conversation() {
    require_api!();
    let client = reqwest::Client::new();
    if !chat_available(&client).await {
        eprintln!("Skipping: chat not available (Ollama not configured)");
        return;
    }

    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"input": "What is 2+2?"}))
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();

    // Validate HotM contract fields exist
    assert!(body.get("messages").is_some(), "missing 'messages' field");
    assert!(body.get("actions").is_some(), "missing 'actions' field");
    assert!(
        body.get("model_info").is_some(),
        "missing 'model_info' field"
    );

    // At least one assistant message
    let messages = body["messages"].as_array().unwrap();
    assert!(!messages.is_empty(), "should have at least one message");
    assert_eq!(messages[0]["role"], "assistant");
    assert!(
        !messages[0]["content"].as_str().unwrap().is_empty(),
        "assistant content should not be empty"
    );
}

/// Issue #549 test case #2: Full context with all fields populated.
#[tokio::test]
async fn test_chat_with_full_context() {
    require_api!();
    let client = reqwest::Client::new();
    if !chat_available(&client).await {
        eprintln!("Skipping: chat not available (Ollama not configured)");
        return;
    }

    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({
            "input": "What is this note about?",
            "context": {
                "note_id": "00000000-0000-0000-0000-000000000000",
                "collection_id": "00000000-0000-0000-0000-000000000001",
                "search_query": "quantum computing",
                "conversation_history": [
                    {"role": "user", "content": "Hello"},
                    {"role": "assistant", "content": "Hi there!"}
                ]
            }
        }))
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(!body["messages"].as_array().unwrap().is_empty());
}

/// Issue #549 test case #5: Timestamp in response is ISO 8601.
#[tokio::test]
async fn test_chat_response_timestamp_format() {
    require_api!();
    let client = reqwest::Client::new();
    if !chat_available(&client).await {
        eprintln!("Skipping: chat not available (Ollama not configured)");
        return;
    }

    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"input": "Hello"}))
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let messages = body["messages"].as_array().unwrap();
    if let Some(ts) = messages[0].get("timestamp") {
        let ts_str = ts.as_str().unwrap();
        // Verify it parses as ISO 8601 / RFC 3339
        assert!(
            chrono::DateTime::parse_from_rfc3339(ts_str).is_ok(),
            "Timestamp '{}' should be valid RFC 3339/ISO 8601",
            ts_str
        );
    }
    // timestamp is optional per contract — HotM handles missing gracefully
}

/// Issue #549 test case #6: Empty context object behaves same as no context.
#[tokio::test]
async fn test_chat_empty_context_object() {
    require_api!();
    let client = reqwest::Client::new();
    if !chat_available(&client).await {
        eprintln!("Skipping: chat not available (Ollama not configured)");
        return;
    }

    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"input": "Hello", "context": {}}))
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

/// Issue #549 test case #7: Partial context with only note_id.
#[tokio::test]
async fn test_chat_partial_context_only_note_id() {
    require_api!();
    let client = reqwest::Client::new();
    if !chat_available(&client).await {
        eprintln!("Skipping: chat not available (Ollama not configured)");
        return;
    }

    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({
            "input": "What is this?",
            "context": {"note_id": "00000000-0000-0000-0000-000000000000"}
        }))
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

/// Issue #549: model_info is present and has expected fields.
#[tokio::test]
async fn test_chat_response_model_info_contract() {
    require_api!();
    let client = reqwest::Client::new();
    if !chat_available(&client).await {
        eprintln!("Skipping: chat not available (Ollama not configured)");
        return;
    }

    let resp = client
        .post(format!("{}/api/v1/chat", api_base_url()))
        .json(&serde_json::json!({"input": "Hi"}))
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let mi = body.get("model_info").expect("model_info must be present");

    // Required model_info fields
    assert!(mi.get("model").is_some(), "model_info.model required");
    assert!(
        mi.get("context_window").is_some(),
        "model_info.context_window required"
    );
    assert!(
        mi.get("estimated_available_context").is_some(),
        "model_info.estimated_available_context required"
    );
    assert!(
        mi.get("max_output_tokens").is_some(),
        "model_info.max_output_tokens required"
    );
    assert!(
        mi.get("supports_thinking").is_some(),
        "model_info.supports_thinking required"
    );
    assert!(
        mi.get("thinking_type").is_some(),
        "model_info.thinking_type required"
    );
    assert!(
        mi.get("speed_tok_s").is_some(),
        "model_info.speed_tok_s required"
    );
}

// =============================================================================
// Agent-Proxy Surface (Issue #549) — read endpoint validation
// =============================================================================

/// Issue #549 agent-proxy: GET /search returns expected shape.
#[tokio::test]
async fn test_search_endpoint_contract() {
    require_api!();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{}/api/v1/search?q=test&limit=1&mode=fts",
            api_base_url()
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    // Response should be an array or object with results
    assert!(
        body.is_array() || body.get("results").is_some(),
        "Search should return array or object with results"
    );
}

/// Issue #549 agent-proxy: GET /collections returns expected shape.
#[tokio::test]
async fn test_list_collections_endpoint_contract() {
    require_api!();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/v1/collections", api_base_url()))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    // Should be an array of collections
    assert!(body.is_array(), "Collections list should return an array");
}

/// Issue #549 agent-proxy: GET /concepts search returns expected shape.
#[tokio::test]
async fn test_search_concepts_endpoint_contract() {
    require_api!();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{}/api/v1/concepts?search=test&limit=5",
            api_base_url()
        ))
        .send()
        .await
        .unwrap();

    // 200 or 404-equivalent (no results) — should not be 5xx
    let status = resp.status().as_u16();
    assert!(
        status < 500,
        "Concepts search should not return 5xx, got {}",
        status
    );
}

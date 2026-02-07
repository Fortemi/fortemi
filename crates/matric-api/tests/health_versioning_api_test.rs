//! Integration tests for knowledge health and versioning HTTP endpoints.
//!
//! Tests verify endpoints via HTTP against a running API server:
//! - Knowledge health endpoints (/api/v1/health/*)
//! - Note versioning endpoints (/api/v1/notes/:id/versions/*)
//! - Note export endpoint (/api/v1/notes/:id/export)
//!
//! Test Pattern:
//! - Uses `#[tokio::test]` with HTTP-only operations for setup/teardown
//! - Tests HTTP endpoints via reqwest against API_BASE_URL (default: localhost:3000)
//! - Requires a running API server (tests skip gracefully if unavailable)
//! - Uses UUIDs for test data isolation

use uuid::Uuid;

/// Get the API base URL for testing.
/// Uses environment variable API_BASE_URL or defaults to localhost:3000.
fn api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

/// Check if the API server is reachable. Returns false if connection fails.
async fn api_available() -> bool {
    // Only run external integration tests when API_BASE_URL is explicitly set.
    // Without this guard, tests can accidentally hit stale API deployments on
    // the CI host (port 3000) that don't have the latest code.
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

/// Skip test if API server is not available. These are external integration
/// tests that require a running API server - they cannot run in CI without one.
/// Set API_BASE_URL=http://localhost:3000 to enable these tests.
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

/// Create a test note via HTTP and return its ID.
async fn create_test_note(client: &reqwest::Client, content: &str) -> Uuid {
    let base_url = api_base_url();
    let response = client
        .post(format!("{}/api/v1/notes", base_url))
        .json(&serde_json::json!({
            "content": content,
            "format": "markdown",
            "source": "test"
        }))
        .send()
        .await
        .expect("Failed to create test note");

    assert_eq!(response.status(), 201, "Create note should return 201");

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse create response");
    Uuid::parse_str(body["id"].as_str().unwrap()).expect("Invalid note ID in response")
}

/// Update a note's content via HTTP (creates a new version).
async fn update_test_note(client: &reqwest::Client, note_id: Uuid, new_content: &str) {
    let base_url = api_base_url();
    let response = client
        .patch(format!("{}/api/v1/notes/{}", base_url, note_id))
        .json(&serde_json::json!({
            "content": new_content,
            "revision_mode": "none"
        }))
        .send()
        .await
        .expect("Failed to update test note");

    assert!(
        response.status().is_success(),
        "Update note should succeed, got {}",
        response.status()
    );
}

/// Delete a test note permanently via HTTP.
async fn delete_test_note(client: &reqwest::Client, note_id: Uuid) {
    let base_url = api_base_url();
    // Soft delete first
    let _ = client
        .delete(format!("{}/api/v1/notes/{}", base_url, note_id))
        .send()
        .await;
    // Then purge
    let _ = client
        .post(format!("{}/api/v1/notes/{}/purge", base_url, note_id))
        .send()
        .await;
}

// =============================================================================
// KNOWLEDGE HEALTH ENDPOINT TESTS
// =============================================================================

#[tokio::test]
async fn test_get_knowledge_health_returns_health_score() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    // Create a test note to ensure we have some data
    let note_id = create_test_note(&client, "Test content for health check").await;

    // Get knowledge health
    let response = client
        .get(format!("{}/api/v1/health/knowledge", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200, "Health endpoint should return 200");

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse response JSON");

    // Verify required fields
    assert!(
        body.get("health_score").is_some(),
        "Response should include health_score"
    );
    assert!(
        body.get("total_notes").is_some(),
        "Response should include total_notes"
    );

    // Health score should be between 0 and 100
    let health_score = body["health_score"].as_f64().unwrap();
    assert!(
        (0.0..=100.0).contains(&health_score),
        "Health score should be 0-100, got {}",
        health_score
    );

    // Cleanup
    delete_test_note(&client, note_id).await;
}

#[tokio::test]
async fn test_get_knowledge_health_with_custom_stale_days() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    // Request with custom stale_days parameter
    let response = client
        .get(format!(
            "{}/api/v1/health/knowledge?stale_days=30",
            base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(
        body.get("health_score").is_some(),
        "Should still return health score"
    );
}

#[tokio::test]
async fn test_get_orphan_tags_returns_array() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let response = client
        .get(format!("{}/api/v1/health/orphan-tags", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Response should be an array or contain orphan tags data
    assert!(
        body.is_array() || body.get("orphan_tags").is_some(),
        "Should return orphan tag data"
    );
}

#[tokio::test]
async fn test_get_stale_notes_returns_data() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let response = client
        .get(format!("{}/api/v1/health/stale-notes?days=90", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(
        body.is_array() || body.get("stale_notes").is_some(),
        "Should return stale notes data"
    );
}

#[tokio::test]
async fn test_get_unlinked_notes_returns_data() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let response = client
        .get(format!("{}/api/v1/health/unlinked-notes", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(
        body.is_array() || body.get("unlinked_notes").is_some(),
        "Should return unlinked notes data"
    );
}

#[tokio::test]
async fn test_get_tag_cooccurrence_returns_data() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let response = client
        .get(format!("{}/api/v1/health/tag-cooccurrence", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(
        body.is_array() || body.get("cooccurrence_pairs").is_some(),
        "Should return tag cooccurrence data"
    );
}

// =============================================================================
// NOTE VERSIONING ENDPOINT TESTS
// =============================================================================

#[tokio::test]
async fn test_versioning_full_lifecycle() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    // Step 1: Create note (version 1)
    let note_id = create_test_note(&client, "Original content v1").await;

    // Step 2: Update note to create version 2
    update_test_note(&client, note_id, "Updated content v2").await;

    // Step 3: Update note again to create version 3
    update_test_note(&client, note_id, "Updated content v3").await;

    // Step 4: List versions
    let response = client
        .get(format!("{}/api/v1/notes/{}/versions", base_url, note_id))
        .send()
        .await
        .expect("Failed to list versions");

    assert_eq!(response.status(), 200, "List versions should return 200");

    let versions_body: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Should have version history data
    assert!(
        versions_body.get("note_id").is_some()
            || versions_body.get("versions").is_some()
            || versions_body.get("original_versions").is_some(),
        "Should return version data"
    );

    // Step 5: Get specific version
    let response = client
        .get(format!("{}/api/v1/notes/{}/versions/1", base_url, note_id))
        .send()
        .await
        .expect("Failed to get version");

    assert_eq!(response.status(), 200, "Get version should return 200");

    let version_data: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(version_data.get("content").is_some() || version_data.get("version_number").is_some());

    // Step 6: Diff versions
    let response = client
        .get(format!(
            "{}/api/v1/notes/{}/versions/diff?from=1&to=2",
            base_url, note_id
        ))
        .send()
        .await
        .expect("Failed to diff versions");

    assert_eq!(response.status(), 200, "Diff should return 200");

    // Step 7: Restore version 1
    let response = client
        .post(format!(
            "{}/api/v1/notes/{}/versions/1/restore",
            base_url, note_id
        ))
        .json(&serde_json::json!({
            "restore_tags": false
        }))
        .send()
        .await
        .expect("Failed to restore version");

    assert_eq!(response.status(), 200, "Restore should return 200");

    // Cleanup
    delete_test_note(&client, note_id).await;
}

#[tokio::test]
async fn test_get_nonexistent_version_returns_404() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let note_id = create_test_note(&client, "Test content").await;

    // Try to get version 999 which doesn't exist
    let response = client
        .get(format!(
            "{}/api/v1/notes/{}/versions/999",
            base_url, note_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        404,
        "Nonexistent version should return 404"
    );

    // Cleanup
    delete_test_note(&client, note_id).await;
}

#[tokio::test]
async fn test_versions_for_nonexistent_note_returns_404() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let fake_id = Uuid::new_v4();

    let response = client
        .get(format!("{}/api/v1/notes/{}/versions", base_url, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        404,
        "Versions for nonexistent note should return 404"
    );
}

// =============================================================================
// NOTE EXPORT ENDPOINT TESTS
// =============================================================================

#[tokio::test]
async fn test_export_note_returns_markdown() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let note_id = create_test_note(&client, "# Test Content\n\nThis is a test note.").await;

    // Export with default settings
    let response = client
        .get(format!("{}/api/v1/notes/{}/export", base_url, note_id))
        .send()
        .await
        .expect("Failed to export note");

    assert_eq!(response.status(), 200, "Export should return 200");

    // Verify markdown content
    let markdown = response.text().await.expect("Failed to read markdown");

    assert!(markdown.contains("---"), "Should include YAML frontmatter");
    assert!(
        markdown.contains("# Test Content"),
        "Should include note content"
    );

    // Cleanup
    delete_test_note(&client, note_id).await;
}

#[tokio::test]
async fn test_export_note_without_frontmatter() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let note_id = create_test_note(&client, "Plain content without frontmatter").await;

    // Export without frontmatter
    let response = client
        .get(format!(
            "{}/api/v1/notes/{}/export?include_frontmatter=false",
            base_url, note_id
        ))
        .send()
        .await
        .expect("Failed to export note");

    assert_eq!(response.status(), 200);

    let markdown = response.text().await.expect("Failed to read markdown");
    assert!(
        markdown.contains("Plain content"),
        "Should include note content"
    );

    // Cleanup
    delete_test_note(&client, note_id).await;
}

#[tokio::test]
async fn test_export_nonexistent_note_returns_404() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let fake_id = Uuid::new_v4();

    let response = client
        .get(format!("{}/api/v1/notes/{}/export", base_url, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        404,
        "Exporting nonexistent note should return 404"
    );
}

// =============================================================================
// EDGE CASES
// =============================================================================

#[tokio::test]
async fn test_diff_versions_with_invalid_range() {
    require_api!();
    let client = reqwest::Client::new();
    let base_url = api_base_url();

    let note_id = create_test_note(&client, "Test content").await;

    // Try to diff with invalid version numbers
    let response = client
        .get(format!(
            "{}/api/v1/notes/{}/versions/diff?from=999&to=1000",
            base_url, note_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should return 404 or 400 for invalid versions
    assert!(
        response.status() == 404 || response.status() == 400,
        "Invalid diff range should return 404 or 400, got {}",
        response.status()
    );

    // Cleanup
    delete_test_note(&client, note_id).await;
}

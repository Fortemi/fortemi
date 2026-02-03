//! Tests for OAuth token lifetime extension (Issue #219)
//!
//! This test module verifies:
//! 1. Tokens can be created with custom lifetimes (4 hours for MCP clients)
//! 2. Sliding window refresh extends token expiry on each use
//! 3. Token expiry information is available for warning headers

use chrono::{Duration, Utc};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::PgOAuthRepository;
use matric_core::ClientRegistrationRequest;

/// Helper to create a test database pool
async fn test_pool() -> Pool<Postgres> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());

    Pool::<Postgres>::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Helper to register a test client
async fn register_test_client(repo: &PgOAuthRepository) -> String {
    let req = ClientRegistrationRequest {
        client_name: format!("Test Client {}", Uuid::new_v4()),
        client_uri: Some("https://example.com".to_string()),
        redirect_uris: vec!["https://example.com/callback".to_string()],
        grant_types: vec!["client_credentials".to_string()],
        response_types: vec![],
        scope: Some("mcp read write".to_string()),
        logo_uri: None,
        contacts: None,
        tos_uri: None,
        policy_uri: None,
        software_id: None,
        software_version: None,
        software_statement: None,
        token_endpoint_auth_method: None,
    };

    let response = repo
        .register_client(req)
        .await
        .expect("Failed to register client");
    response.client_id
}

#[tokio::test]
async fn test_create_token_with_custom_lifetime() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Create token with 4-hour lifetime
    let four_hours = Duration::hours(4);
    let (access_token, _, token) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, four_hours)
        .await
        .expect("Failed to create token with custom lifetime");

    // Verify token was created
    assert!(access_token.starts_with("mm_at_"));
    assert_eq!(token.client_id, client_id);

    // Verify expiry is approximately 4 hours from now
    let now = Utc::now();
    let expected_expiry = now + four_hours;
    let diff = (token.access_token_expires_at - expected_expiry)
        .num_seconds()
        .abs();
    assert!(
        diff < 5,
        "Token expiry should be ~4 hours from now, but diff is {} seconds",
        diff
    );
}

#[tokio::test]
async fn test_sliding_window_refresh_extends_expiry() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Create token with 1-hour lifetime
    let one_hour = Duration::hours(1);
    let (access_token, _, original_token) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, one_hour)
        .await
        .expect("Failed to create token");

    let original_expiry = original_token.access_token_expires_at;

    // Wait a moment to ensure timestamp changes
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Validate token with sliding window refresh enabled
    let extended_token = repo
        .validate_and_extend_token(&access_token, one_hour)
        .await
        .expect("Failed to validate token")
        .expect("Token should be valid");

    // Verify expiry was extended
    assert!(
        extended_token.access_token_expires_at > original_expiry,
        "Token expiry should be extended. Original: {}, Extended: {}",
        original_expiry,
        extended_token.access_token_expires_at
    );

    // Verify new expiry is approximately 1 hour from now
    let now = Utc::now();
    let expected_new_expiry = now + one_hour;
    let diff = (extended_token.access_token_expires_at - expected_new_expiry)
        .num_seconds()
        .abs();
    assert!(
        diff < 5,
        "Extended expiry should be ~1 hour from now, but diff is {} seconds",
        diff
    );
}

#[tokio::test]
async fn test_expired_token_not_extended() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Create token with very short lifetime (1 second)
    let one_second = Duration::seconds(1);
    let (access_token, _, _) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, one_second)
        .await
        .expect("Failed to create token");

    // Wait for token to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Try to validate expired token
    let result = repo
        .validate_and_extend_token(&access_token, Duration::hours(1))
        .await
        .expect("DB operation should succeed");

    assert!(
        result.is_none(),
        "Expired token should not be validated or extended"
    );
}

#[tokio::test]
async fn test_get_token_expiry_info() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Create token with 10 minutes lifetime
    let ten_minutes = Duration::minutes(10);
    let (access_token, _, _) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, ten_minutes)
        .await
        .expect("Failed to create token");

    // Get expiry info
    let expiry_info = repo
        .get_token_expiry_info(&access_token)
        .await
        .expect("Failed to get expiry info")
        .expect("Token should exist");

    // Verify expiry info
    assert!(
        expiry_info.seconds_until_expiry > 0,
        "Token should not be expired yet"
    );
    assert!(
        expiry_info.seconds_until_expiry <= 600,
        "Token should expire in ~10 minutes (600 seconds), got {}",
        expiry_info.seconds_until_expiry
    );
    assert!(
        expiry_info.seconds_until_expiry >= 590,
        "Token expiry should be close to 10 minutes, got {}",
        expiry_info.seconds_until_expiry
    );
}

#[tokio::test]
async fn test_token_expiry_warning_threshold() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Create token with 3 minutes lifetime
    let three_minutes = Duration::minutes(3);
    let (access_token, _, _) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, three_minutes)
        .await
        .expect("Failed to create token");

    // Get expiry info
    let expiry_info = repo
        .get_token_expiry_info(&access_token)
        .await
        .expect("Failed to get expiry info")
        .expect("Token should exist");

    // Verify warning flag is set (< 5 minutes remaining)
    assert!(
        expiry_info.should_warn(),
        "Should warn when token has < 5 minutes remaining"
    );
}

#[tokio::test]
async fn test_mcp_client_token_lifetime() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // MCP clients should get 4-hour tokens
    let mcp_lifetime = Duration::hours(4);
    let (access_token, _, token) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, mcp_lifetime)
        .await
        .expect("Failed to create MCP token");

    // Verify token lifetime
    let now = Utc::now();
    let lifetime = token.access_token_expires_at - now;

    // Should be close to 4 hours (within 5 seconds tolerance)
    let expected_seconds = 4 * 3600; // 14400 seconds
    let actual_seconds = lifetime.num_seconds();
    let diff = (actual_seconds - expected_seconds).abs();

    assert!(
        diff < 5,
        "MCP token should have ~4 hour lifetime. Expected: {}s, Actual: {}s, Diff: {}s",
        expected_seconds,
        actual_seconds,
        diff
    );

    // Validate token
    let validated = repo
        .validate_access_token(&access_token)
        .await
        .expect("Failed to validate token")
        .expect("Token should be valid");

    assert_eq!(validated.client_id, client_id);
}

#[tokio::test]
async fn test_sliding_window_keeps_active_sessions_alive() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Create token with 2 seconds lifetime
    let short_lifetime = Duration::seconds(2);
    let (access_token, _, _) = repo
        .create_token_with_lifetime(&client_id, "mcp read", None, false, short_lifetime)
        .await
        .expect("Failed to create token");

    // Use token multiple times, extending it each time
    for i in 0..3 {
        // Wait 1 second (less than expiry)
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Validate and extend token
        let token = repo
            .validate_and_extend_token(&access_token, short_lifetime)
            .await
            .expect("Failed to validate token")
            .unwrap_or_else(|| panic!("Token should still be valid after {} uses", i + 1));

        // Token should still be valid because we keep extending it
        assert!(!token.revoked, "Token should not be revoked");
    }

    // Total elapsed time: ~3 seconds
    // Without sliding window, token would have expired after 2 seconds
    // With sliding window, it's still valid because we kept using it
}

#[tokio::test]
async fn test_backward_compatibility_default_lifetime() {
    let pool = test_pool().await;
    let repo = PgOAuthRepository::new(pool);
    let client_id = register_test_client(&repo).await;

    // Old code path: create_token should still work with default 1-hour lifetime
    let (_access_token, _, token) = repo
        .create_token(&client_id, "read", None, false)
        .await
        .expect("Failed to create token with default lifetime");

    // Verify default 1-hour lifetime
    let now = Utc::now();
    let lifetime = token.access_token_expires_at - now;
    let expected_seconds = 3600; // 1 hour
    let actual_seconds = lifetime.num_seconds();
    let diff = (actual_seconds - expected_seconds).abs();

    assert!(
        diff < 5,
        "Default token should have 1 hour lifetime. Expected: {}s, Actual: {}s",
        expected_seconds,
        actual_seconds
    );
}

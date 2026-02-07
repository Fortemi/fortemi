//! Integration tests for authentication middleware.
//!
//! Tests authentication protection for API endpoints (Issue #463).
//! These tests verify the OAuth infrastructure works correctly.

use matric_core::ClientRegistrationRequest;
use matric_db::Database;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string())
}

/// Helper to create a test OAuth client and get an access token.
async fn create_test_client_and_token(db: &Database) -> (String, String) {
    // Register OAuth client
    let registration = ClientRegistrationRequest {
        client_name: "Test Client".to_string(),
        redirect_uris: vec!["http://localhost/callback".to_string()],
        grant_types: vec!["client_credentials".to_string()],
        response_types: vec![],
        scope: Some("read write".to_string()),
        contacts: None,
        logo_uri: None,
        client_uri: None,
        policy_uri: None,
        tos_uri: None,
        software_id: None,
        software_version: None,
        software_statement: None,
        token_endpoint_auth_method: Some("client_secret_basic".to_string()),
    };

    let client = db
        .oauth
        .register_client(registration)
        .await
        .expect("Failed to register client");

    // Extract client_secret from Option
    let client_secret = client
        .client_secret
        .expect("Client secret should be present");

    // Validate client credentials
    let valid = db
        .oauth
        .validate_client(&client.client_id, &client_secret)
        .await
        .expect("Failed to validate client");
    assert!(valid, "Client credentials should be valid");

    // Create token using create_token_with_lifetime (what the oauth_token endpoint uses)
    let lifetime = chrono::Duration::hours(1);
    let (access_token, _refresh_token, _token) = db
        .oauth
        .create_token_with_lifetime(&client.client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token");

    (client.client_id, access_token)
}

#[tokio::test]
async fn test_oauth_client_registration_and_token_creation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    // Test that OAuth flow works end-to-end
    let (_client_id, token) = create_test_client_and_token(&db).await;
    assert!(!token.is_empty());
}

#[tokio::test]
async fn test_oauth_token_introspection_works() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (_client_id, token) = create_test_client_and_token(&db).await;

    // Introspect the token
    let introspection = db
        .oauth
        .introspect_token(&token)
        .await
        .expect("Failed to introspect token");

    assert!(introspection.active);
    assert_eq!(introspection.scope, Some("read write".to_string()));
    assert_eq!(introspection.token_type, Some("Bearer".to_string()));
}

#[tokio::test]
async fn test_invalid_token_is_not_active() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    // Try to introspect an invalid token
    let introspection = db
        .oauth
        .introspect_token("invalid_token_12345")
        .await
        .expect("Failed to introspect token");

    assert!(!introspection.active);
}

#[tokio::test]
async fn test_expired_token_is_not_active() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (_client_id, token) = create_test_client_and_token(&db).await;

    // Manually expire the token by updating the database
    sqlx::query("UPDATE oauth_token SET access_token_expires_at = NOW() - INTERVAL '1 hour' WHERE access_token_hash = encode(sha256($1::bytea), 'hex')")
        .bind(token.as_bytes())
        .execute(db.pool())
        .await
        .expect("Failed to expire token");

    // Introspect the expired token
    let introspection = db
        .oauth
        .introspect_token(&token)
        .await
        .expect("Failed to introspect token");

    assert!(!introspection.active);
}

#[tokio::test]
async fn test_revoked_token_is_not_active() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (_client_id, token) = create_test_client_and_token(&db).await;

    // Revoke the token
    let revoked = db
        .oauth
        .revoke_token(&token, None)
        .await
        .expect("Failed to revoke token");

    assert!(revoked, "Token should have been revoked");

    // Introspect the revoked token
    let introspection = db
        .oauth
        .introspect_token(&token)
        .await
        .expect("Failed to introspect token");

    assert!(!introspection.active);
}

#[tokio::test]
async fn test_token_has_correct_claims() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, token) = create_test_client_and_token(&db).await;

    // Introspect to get claims
    let introspection = db
        .oauth
        .introspect_token(&token)
        .await
        .expect("Failed to introspect token");

    assert!(introspection.active);
    assert_eq!(introspection.client_id, Some(client_id.clone()));
    assert_eq!(introspection.aud, Some(client_id));
    assert!(introspection.exp.is_some());
    assert!(introspection.iat.is_some());
}

#[tokio::test]
async fn test_multiple_tokens_for_same_client() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, token1) = create_test_client_and_token(&db).await;

    // Create another token for the same client
    let lifetime = chrono::Duration::hours(1);
    let (token2, _, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create second token");

    // Both tokens should be valid
    let intro1 = db
        .oauth
        .introspect_token(&token1)
        .await
        .expect("Failed to introspect token1");
    let intro2 = db
        .oauth
        .introspect_token(&token2)
        .await
        .expect("Failed to introspect token2");

    assert!(intro1.active);
    assert!(intro2.active);
    assert_ne!(token1, token2);
}

// Note: Full end-to-end middleware tests require the full Axum app setup
// The tests above verify the OAuth infrastructure works correctly
// The middleware implementation will use this infrastructure to protect routes

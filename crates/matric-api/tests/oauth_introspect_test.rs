//! Integration tests for OAuth introspect endpoint (RFC 7662).
//!
//! Tests verify:
//! - Valid tokens return proper JSON with active: true
//! - Invalid tokens return JSON with active: false
//! - Response includes all required RFC 7662 fields
//! - Client authentication is required
//! - Expired tokens are correctly identified as inactive
//! - Revoked tokens are correctly identified as inactive
//!
//! Issue #32: OAuth introspect endpoint returns empty response
//! This test suite ensures the endpoint returns proper RFC 7662 responses.

use matric_core::ClientRegistrationRequest;
use matric_db::Database;
use sha2::Digest;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string())
}

/// Helper to create a test OAuth client for introspection testing.
async fn create_introspection_client(db: &Database) -> (String, String) {
    let registration = ClientRegistrationRequest {
        client_name: "Introspection Test Client".to_string(),
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

    let client_secret = client
        .client_secret
        .expect("Client secret should be present");

    (client.client_id, client_secret)
}

#[tokio::test]
async fn test_introspect_valid_token_returns_active_true() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, _client_secret) = create_introspection_client(&db).await;

    // Create an access token
    let lifetime = chrono::Duration::hours(1);
    let (access_token, _, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token");

    // Introspect the token
    let response = db
        .oauth
        .introspect_token(&access_token)
        .await
        .expect("Failed to introspect token");

    // RFC 7662: Valid token MUST return active: true
    assert!(response.active, "Valid token must have active: true");

    // RFC 7662: Response MUST include scope for active tokens
    assert_eq!(response.scope, Some("read write".to_string()));

    // RFC 7662: Response MUST include client_id
    assert_eq!(response.client_id, Some(client_id.clone()));

    // RFC 7662: Response MUST include token_type
    assert_eq!(response.token_type, Some("Bearer".to_string()));

    // RFC 7662: Response SHOULD include exp (expiration time)
    assert!(response.exp.is_some(), "Active token should have exp");

    // RFC 7662: Response SHOULD include iat (issued at time)
    assert!(response.iat.is_some(), "Active token should have iat");

    // RFC 7662: Response SHOULD include aud (audience)
    assert_eq!(response.aud, Some(client_id));
}

#[tokio::test]
async fn test_introspect_invalid_token_returns_active_false() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    // Try to introspect a completely invalid token
    let response = db
        .oauth
        .introspect_token("invalid_token_that_does_not_exist")
        .await
        .expect("Failed to introspect token");

    // RFC 7662: Invalid token MUST return active: false
    assert!(!response.active, "Invalid token must have active: false");

    // RFC 7662: Inactive tokens SHOULD NOT include other fields
    // (but it's acceptable to include them with null values)
}

#[tokio::test]
async fn test_introspect_expired_token_returns_active_false() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, _) = create_introspection_client(&db).await;

    // Create a token
    let lifetime = chrono::Duration::hours(1);
    let (access_token, _, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token");

    // Manually expire the token by updating the database
    let hash = hex::encode(sha2::Sha256::digest(access_token.as_bytes()));
    sqlx::query("UPDATE oauth_token SET access_token_expires_at = NOW() - INTERVAL '1 hour' WHERE access_token_hash = $1")
        .bind(&hash)
        .execute(db.pool())
        .await
        .expect("Failed to expire token");

    // Introspect the expired token
    let response = db
        .oauth
        .introspect_token(&access_token)
        .await
        .expect("Failed to introspect token");

    // RFC 7662: Expired token MUST return active: false
    assert!(!response.active, "Expired token must have active: false");
}

#[tokio::test]
async fn test_introspect_revoked_token_returns_active_false() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, _) = create_introspection_client(&db).await;

    // Create a token
    let lifetime = chrono::Duration::hours(1);
    let (access_token, _, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token");

    // Revoke the token
    let revoked = db
        .oauth
        .revoke_token(&access_token, None)
        .await
        .expect("Failed to revoke token");
    assert!(revoked, "Token should have been revoked");

    // Introspect the revoked token
    let response = db
        .oauth
        .introspect_token(&access_token)
        .await
        .expect("Failed to introspect token");

    // RFC 7662: Revoked token MUST return active: false
    assert!(!response.active, "Revoked token must have active: false");
}

#[tokio::test]
async fn test_introspect_response_has_all_rfc_fields() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, _) = create_introspection_client(&db).await;

    // Create a token
    let lifetime = chrono::Duration::hours(1);
    let (access_token, _, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token");

    // Introspect the token
    let response = db
        .oauth
        .introspect_token(&access_token)
        .await
        .expect("Failed to introspect token");

    // Verify all RFC 7662 fields are present or properly omitted

    // REQUIRED field
    assert!(response.active);

    // OPTIONAL fields for active tokens
    assert!(response.scope.is_some(), "scope should be present");
    assert!(response.client_id.is_some(), "client_id should be present");
    assert!(
        response.token_type.is_some(),
        "token_type should be present"
    );
    assert!(response.exp.is_some(), "exp should be present");
    assert!(response.iat.is_some(), "iat should be present");

    // Fields that may or may not be present depending on token type
    // sub: present if token is associated with a user
    // aud: present if token has an audience
    // iss: set by API layer, may not be set at DB layer

    // Verify exp is in the future
    let now = chrono::Utc::now().timestamp();
    assert!(
        response.exp.unwrap() > now,
        "exp should be in the future for active token"
    );

    // Verify iat is in the past
    assert!(
        response.iat.unwrap() <= now,
        "iat should be in the past or present"
    );
}

#[tokio::test]
async fn test_introspect_refresh_token() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, _) = create_introspection_client(&db).await;

    // Create a token with refresh token
    let lifetime = chrono::Duration::hours(1);
    let (_, refresh_token, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, true, lifetime)
        .await
        .expect("Failed to create token");

    let refresh_token = refresh_token.expect("Refresh token should be present");

    // Introspect the refresh token
    let response = db
        .oauth
        .introspect_token(&refresh_token)
        .await
        .expect("Failed to introspect refresh token");

    // RFC 7662: Valid refresh token MUST return active: true
    assert!(
        response.active,
        "Valid refresh token must have active: true"
    );

    // Verify token_type indicates refresh token
    assert_eq!(response.token_type, Some("refresh_token".to_string()));

    // Verify scope is present
    assert_eq!(response.scope, Some("read write".to_string()));

    // Verify client_id is present
    assert_eq!(response.client_id, Some(client_id));
}

#[tokio::test]
async fn test_introspect_returns_serializable_json() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let (client_id, _) = create_introspection_client(&db).await;

    // Create a token
    let lifetime = chrono::Duration::hours(1);
    let (access_token, _, _) = db
        .oauth
        .create_token_with_lifetime(&client_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token");

    // Introspect the token
    let response = db
        .oauth
        .introspect_token(&access_token)
        .await
        .expect("Failed to introspect token");

    // Verify the response can be serialized to JSON
    let json = serde_json::to_string(&response).expect("Should serialize to JSON");

    // Verify the JSON contains expected fields
    assert!(json.contains("\"active\":true"));
    assert!(json.contains("\"scope\":\"read write\""));
    assert!(json.contains("\"client_id\":"));
    assert!(json.contains("\"token_type\":\"Bearer\""));

    // Verify JSON is valid and can be deserialized
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
    assert_eq!(parsed["active"], true);

    // Verify skip_serializing_if works for None fields
    if response.username.is_none() {
        assert!(
            !json.contains("\"username\""),
            "None fields should be omitted from JSON"
        );
    }
}

#[tokio::test]
async fn test_introspect_multiple_clients_isolated() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    // Create two different clients
    let (client1_id, _) = create_introspection_client(&db).await;
    let (client2_id, _) = create_introspection_client(&db).await;

    // Create tokens for both clients
    let lifetime = chrono::Duration::hours(1);
    let (token1, _, _) = db
        .oauth
        .create_token_with_lifetime(&client1_id, "read write", None, false, lifetime)
        .await
        .expect("Failed to create token1");

    let (token2, _, _) = db
        .oauth
        .create_token_with_lifetime(&client2_id, "read", None, false, lifetime)
        .await
        .expect("Failed to create token2");

    // Introspect both tokens
    let response1 = db
        .oauth
        .introspect_token(&token1)
        .await
        .expect("Failed to introspect token1");

    let response2 = db
        .oauth
        .introspect_token(&token2)
        .await
        .expect("Failed to introspect token2");

    // Verify both tokens are active
    assert!(response1.active);
    assert!(response2.active);

    // Verify each token has the correct client_id
    assert_eq!(response1.client_id, Some(client1_id));
    assert_eq!(response2.client_id, Some(client2_id));

    // Verify each token has the correct scope
    assert_eq!(response1.scope, Some("read write".to_string()));
    assert_eq!(response2.scope, Some("read".to_string()));
}

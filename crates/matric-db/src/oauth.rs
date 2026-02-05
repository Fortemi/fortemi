//! OAuth2 repository implementation.

use chrono::{Duration, Utc};
use hex;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    new_v7, ApiKey, ClientRegistrationRequest, ClientRegistrationResponse, CreateApiKeyRequest,
    CreateApiKeyResponse, Error, OAuthAuthorizationCode, OAuthClient, OAuthToken, Result,
    TokenIntrospectionResponse,
};

/// PostgreSQL implementation of OAuth2 repository.
pub struct PgOAuthRepository {
    pool: Pool<Postgres>,
}

impl PgOAuthRepository {
    /// Create a new PgOAuthRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Generate a cryptographically secure random string.
    fn generate_secret(length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::thread_rng();
        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Hash a secret using SHA256.
    fn hash_secret(secret: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify a secret against its hash.
    fn verify_secret(secret: &str, hash: &str) -> bool {
        Self::hash_secret(secret) == hash
    }

    // =========================================================================
    // CLIENT REGISTRATION (RFC 7591)
    // =========================================================================

    /// Register a new OAuth2 client (Dynamic Client Registration).
    pub async fn register_client(
        &self,
        req: ClientRegistrationRequest,
    ) -> Result<ClientRegistrationResponse> {
        let now = Utc::now();
        let id = new_v7();
        let client_id = format!("mm_{}", Self::generate_secret(24));
        let client_secret = Self::generate_secret(48);
        let client_secret_hash = Self::hash_secret(&client_secret);
        let registration_access_token = Self::generate_secret(64);
        let registration_access_token_hash = Self::hash_secret(&registration_access_token);

        // Default grant types if not specified
        let grant_types = if req.grant_types.is_empty() {
            vec![
                "authorization_code".to_string(),
                "refresh_token".to_string(),
            ]
        } else {
            req.grant_types
        };

        // Default response types if not specified
        let response_types = if req.response_types.is_empty() {
            vec!["code".to_string()]
        } else {
            req.response_types
        };

        let scope = req.scope.unwrap_or_else(|| "read".to_string());
        let token_endpoint_auth_method = req
            .token_endpoint_auth_method
            .unwrap_or_else(|| "client_secret_basic".to_string());

        let contacts: Vec<String> = req.contacts.unwrap_or_default();

        sqlx::query(
            r#"INSERT INTO oauth_client (
                id, client_id, client_secret_hash, client_name, client_uri, logo_uri,
                redirect_uris, grant_types, response_types, scope,
                token_endpoint_auth_method, software_id, software_version,
                software_statement, contacts, policy_uri, tos_uri,
                is_active, is_confidential, registration_access_token,
                client_id_issued_at, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $21, $21
            )"#,
        )
        .bind(id)
        .bind(&client_id)
        .bind(&client_secret_hash)
        .bind(&req.client_name)
        .bind(&req.client_uri)
        .bind(&req.logo_uri)
        .bind(&req.redirect_uris)
        .bind(&grant_types)
        .bind(&response_types)
        .bind(&scope)
        .bind(&token_endpoint_auth_method)
        .bind(&req.software_id)
        .bind(&req.software_version)
        .bind(&req.software_statement)
        .bind(&contacts)
        .bind(&req.policy_uri)
        .bind(&req.tos_uri)
        .bind(true) // is_active
        .bind(true) // is_confidential (clients with secrets)
        .bind(&registration_access_token_hash)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(ClientRegistrationResponse {
            client_id,
            client_secret: Some(client_secret),
            client_id_issued_at: now.timestamp(),
            client_secret_expires_at: 0, // Never expires
            client_name: req.client_name,
            redirect_uris: req.redirect_uris,
            grant_types,
            response_types,
            scope,
            token_endpoint_auth_method,
            registration_access_token: Some(registration_access_token),
            registration_client_uri: None, // Will be set by the API layer
        })
    }

    /// Get an OAuth2 client by client_id.
    pub async fn get_client(&self, client_id: &str) -> Result<Option<OAuthClient>> {
        let row = sqlx::query(
            r#"SELECT
                id, client_id, client_name, client_uri, logo_uri,
                redirect_uris, grant_types, response_types, scope,
                token_endpoint_auth_method, software_id, software_version,
                contacts, policy_uri, tos_uri,
                is_active, is_confidential,
                client_id_issued_at, client_secret_expires_at, created_at
            FROM oauth_client WHERE client_id = $1"#,
        )
        .bind(client_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| OAuthClient {
            id: r.get("id"),
            client_id: r.get("client_id"),
            client_name: r.get("client_name"),
            client_uri: r.get("client_uri"),
            logo_uri: r.get("logo_uri"),
            redirect_uris: r.get("redirect_uris"),
            grant_types: r.get("grant_types"),
            response_types: r.get("response_types"),
            scope: r.get("scope"),
            token_endpoint_auth_method: r.get("token_endpoint_auth_method"),
            software_id: r.get("software_id"),
            software_version: r.get("software_version"),
            contacts: r
                .get::<Option<Vec<String>>, _>("contacts")
                .unwrap_or_default(),
            policy_uri: r.get("policy_uri"),
            tos_uri: r.get("tos_uri"),
            is_active: r.get("is_active"),
            is_confidential: r.get("is_confidential"),
            client_id_issued_at: r.get("client_id_issued_at"),
            client_secret_expires_at: r.get("client_secret_expires_at"),
            created_at: r.get("created_at"),
        }))
    }

    /// Validate client credentials.
    pub async fn validate_client(&self, client_id: &str, client_secret: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT client_secret_hash, is_active FROM oauth_client WHERE client_id = $1",
        )
        .bind(client_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(r) => {
                let hash: String = r.get("client_secret_hash");
                let is_active: bool = r.get("is_active");
                Ok(is_active && Self::verify_secret(client_secret, &hash))
            }
            None => Ok(false),
        }
    }

    /// Check if client supports a grant type.
    pub async fn client_supports_grant(&self, client_id: &str, grant_type: &str) -> Result<bool> {
        let supports: Option<bool> = sqlx::query_scalar(
            "SELECT $2 = ANY(grant_types) FROM oauth_client WHERE client_id = $1 AND is_active = true",
        )
        .bind(client_id)
        .bind(grant_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(supports.unwrap_or(false))
    }

    /// Deactivate a client.
    pub async fn deactivate_client(&self, client_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE oauth_client SET is_active = false, updated_at = $1 WHERE client_id = $2",
        )
        .bind(Utc::now())
        .bind(client_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    // =========================================================================
    // AUTHORIZATION CODES
    // =========================================================================

    /// Create an authorization code.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_authorization_code(
        &self,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
        state: Option<&str>,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<String> {
        let code = Self::generate_secret(48);
        let now = Utc::now();
        let expires_at = now + Duration::minutes(10); // Codes expire in 10 minutes

        sqlx::query(
            r#"INSERT INTO oauth_authorization_code (
                code, client_id, redirect_uri, scope, state,
                code_challenge, code_challenge_method, user_id,
                expires_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(&code)
        .bind(client_id)
        .bind(redirect_uri)
        .bind(scope)
        .bind(state)
        .bind(code_challenge)
        .bind(code_challenge_method)
        .bind(user_id)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(code)
    }

    /// Consume an authorization code (single-use).
    pub async fn consume_authorization_code(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<OAuthAuthorizationCode> {
        let now = Utc::now();

        // Fetch and mark as used in a transaction
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        let row = sqlx::query(
            r#"SELECT
                code, client_id, redirect_uri, scope, state,
                code_challenge, code_challenge_method, user_id,
                used, expires_at, created_at
            FROM oauth_authorization_code
            WHERE code = $1 AND client_id = $2 AND redirect_uri = $3
            FOR UPDATE"#,
        )
        .bind(code)
        .bind(client_id)
        .bind(redirect_uri)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| Error::Unauthorized("Invalid authorization code".to_string()))?;

        // Check if already used
        let used: bool = row.get("used");
        if used {
            tx.rollback().await.map_err(Error::Database)?;
            return Err(Error::Unauthorized(
                "Authorization code has already been used".to_string(),
            ));
        }

        // Check if expired
        let expires_at: chrono::DateTime<Utc> = row.get("expires_at");
        if expires_at < now {
            tx.rollback().await.map_err(Error::Database)?;
            return Err(Error::Unauthorized(
                "Authorization code has expired".to_string(),
            ));
        }

        // Verify PKCE if present
        let code_challenge: Option<String> = row.get("code_challenge");
        let code_challenge_method: Option<String> = row.get("code_challenge_method");

        if let Some(challenge) = &code_challenge {
            let verifier = code_verifier.ok_or_else(|| {
                Error::Unauthorized("code_verifier required for PKCE".to_string())
            })?;

            let computed_challenge = match code_challenge_method.as_deref() {
                Some("S256") => {
                    let mut hasher = Sha256::new();
                    hasher.update(verifier.as_bytes());
                    base64_url_encode(&hasher.finalize())
                }
                Some("plain") | None => verifier.to_string(),
                _ => {
                    return Err(Error::Unauthorized(
                        "Unsupported code_challenge_method".to_string(),
                    ))
                }
            };

            if &computed_challenge != challenge {
                tx.rollback().await.map_err(Error::Database)?;
                return Err(Error::Unauthorized("Invalid code_verifier".to_string()));
            }
        }

        // Mark as used
        sqlx::query("UPDATE oauth_authorization_code SET used = true WHERE code = $1")
            .bind(code)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;

        Ok(OAuthAuthorizationCode {
            code: row.get("code"),
            client_id: row.get("client_id"),
            redirect_uri: row.get("redirect_uri"),
            scope: row.get("scope"),
            state: row.get("state"),
            code_challenge,
            code_challenge_method,
            user_id: row.get("user_id"),
            expires_at,
            created_at: row.get("created_at"),
        })
    }

    // =========================================================================
    // TOKENS
    // =========================================================================

    /// Create an access token (and optionally a refresh token).
    pub async fn create_token(
        &self,
        client_id: &str,
        scope: &str,
        user_id: Option<&str>,
        include_refresh: bool,
    ) -> Result<(String, Option<String>, OAuthToken)> {
        let now = Utc::now();
        let id = new_v7();

        let access_token = format!("mm_at_{}", Self::generate_secret(48));
        let access_token_hash = Self::hash_secret(&access_token);
        let access_token_expires_at = now + Duration::hours(1); // 1 hour

        let (refresh_token, refresh_token_hash, refresh_token_expires_at) = if include_refresh {
            let rt = format!("mm_rt_{}", Self::generate_secret(48));
            let rt_hash = Self::hash_secret(&rt);
            let rt_expires = now + Duration::days(30); // 30 days
            (Some(rt), Some(rt_hash), Some(rt_expires))
        } else {
            (None, None, None)
        };

        sqlx::query(
            r#"INSERT INTO oauth_token (
                id, access_token_hash, refresh_token_hash, token_type, scope,
                client_id, user_id, access_token_expires_at, refresh_token_expires_at,
                created_at
            ) VALUES ($1, $2, $3, 'Bearer', $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(id)
        .bind(&access_token_hash)
        .bind(&refresh_token_hash)
        .bind(scope)
        .bind(client_id)
        .bind(user_id)
        .bind(access_token_expires_at)
        .bind(refresh_token_expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        let token = OAuthToken {
            id,
            access_token_hash,
            refresh_token_hash: refresh_token_hash.clone(),
            token_type: "Bearer".to_string(),
            scope: scope.to_string(),
            client_id: client_id.to_string(),
            user_id: user_id.map(String::from),
            access_token_expires_at,
            refresh_token_expires_at,
            revoked: false,
            created_at: now,
        };

        Ok((access_token, refresh_token, token))
    }

    /// Create an access token with custom lifetime (and optionally a refresh token).
    ///
    /// This is used to support different token lifetimes for different client types.
    /// For example, MCP clients can have 4-hour tokens while web UI clients have 1-hour tokens.
    pub async fn create_token_with_lifetime(
        &self,
        client_id: &str,
        scope: &str,
        user_id: Option<&str>,
        include_refresh: bool,
        access_token_lifetime: Duration,
    ) -> Result<(String, Option<String>, OAuthToken)> {
        let now = Utc::now();
        let id = new_v7();

        let access_token = format!("mm_at_{}", Self::generate_secret(48));
        let access_token_hash = Self::hash_secret(&access_token);
        let access_token_expires_at = now + access_token_lifetime;

        let (refresh_token, refresh_token_hash, refresh_token_expires_at) = if include_refresh {
            let rt = format!("mm_rt_{}", Self::generate_secret(48));
            let rt_hash = Self::hash_secret(&rt);
            let rt_expires = now + Duration::days(30); // 30 days
            (Some(rt), Some(rt_hash), Some(rt_expires))
        } else {
            (None, None, None)
        };

        sqlx::query(
            r#"INSERT INTO oauth_token (
                id, access_token_hash, refresh_token_hash, token_type, scope,
                client_id, user_id, access_token_expires_at, refresh_token_expires_at,
                created_at
            ) VALUES ($1, $2, $3, 'Bearer', $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(id)
        .bind(&access_token_hash)
        .bind(&refresh_token_hash)
        .bind(scope)
        .bind(client_id)
        .bind(user_id)
        .bind(access_token_expires_at)
        .bind(refresh_token_expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        let token = OAuthToken {
            id,
            access_token_hash,
            refresh_token_hash: refresh_token_hash.clone(),
            token_type: "Bearer".to_string(),
            scope: scope.to_string(),
            client_id: client_id.to_string(),
            user_id: user_id.map(String::from),
            access_token_expires_at,
            refresh_token_expires_at,
            revoked: false,
            created_at: now,
        };

        Ok((access_token, refresh_token, token))
    }

    /// Validate an access token and extend its expiry (sliding window refresh).
    ///
    /// On each successful authentication, the token expiry is extended by the specified lifetime.
    /// This keeps active sessions alive indefinitely while idle sessions expire.
    pub async fn validate_and_extend_token(
        &self,
        access_token: &str,
        extend_by: Duration,
    ) -> Result<Option<OAuthToken>> {
        let hash = Self::hash_secret(access_token);
        let now = Utc::now();

        // Validate token first
        let row = sqlx::query(
            r#"SELECT
                id, access_token_hash, refresh_token_hash, token_type, scope,
                client_id, user_id, access_token_expires_at, refresh_token_expires_at,
                revoked, created_at
            FROM oauth_token
            WHERE access_token_hash = $1
              AND revoked = false
              AND access_token_expires_at > $2"#,
        )
        .bind(&hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(r) = row {
            // Extend expiry and update last_used_at
            let new_expiry = now + extend_by;
            sqlx::query(
                "UPDATE oauth_token SET access_token_expires_at = $1, last_used_at = $2 WHERE access_token_hash = $3",
            )
            .bind(new_expiry)
            .bind(now)
            .bind(&hash)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

            Ok(Some(OAuthToken {
                id: r.get("id"),
                access_token_hash: r.get("access_token_hash"),
                refresh_token_hash: r.get("refresh_token_hash"),
                token_type: r.get("token_type"),
                scope: r.get("scope"),
                client_id: r.get("client_id"),
                user_id: r.get("user_id"),
                access_token_expires_at: new_expiry, // Return updated expiry
                refresh_token_expires_at: r.get("refresh_token_expires_at"),
                revoked: r.get("revoked"),
                created_at: r.get("created_at"),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get token expiry information for warning headers.
    ///
    /// Returns the number of seconds until token expiry and the expiry timestamp.
    /// This can be used to add X-Token-Expires-In headers to API responses.
    pub async fn get_token_expiry_info(
        &self,
        access_token: &str,
    ) -> Result<Option<matric_core::TokenExpiryInfo>> {
        let hash = Self::hash_secret(access_token);
        let now = Utc::now();

        let row = sqlx::query(
            "SELECT access_token_expires_at FROM oauth_token WHERE access_token_hash = $1 AND revoked = false",
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| {
            let expires_at: chrono::DateTime<Utc> = r.get("access_token_expires_at");
            let seconds_until_expiry = (expires_at - now).num_seconds();
            matric_core::TokenExpiryInfo {
                seconds_until_expiry,
                expires_at,
            }
        }))
    }

    /// Validate an access token.
    pub async fn validate_access_token(&self, access_token: &str) -> Result<Option<OAuthToken>> {
        let hash = Self::hash_secret(access_token);
        let now = Utc::now();

        let row = sqlx::query(
            r#"SELECT
                id, access_token_hash, refresh_token_hash, token_type, scope,
                client_id, user_id, access_token_expires_at, refresh_token_expires_at,
                revoked, created_at
            FROM oauth_token
            WHERE access_token_hash = $1
              AND revoked = false
              AND access_token_expires_at > $2"#,
        )
        .bind(&hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Update last_used_at
        if row.is_some() {
            sqlx::query("UPDATE oauth_token SET last_used_at = $1 WHERE access_token_hash = $2")
                .bind(now)
                .bind(&hash)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
        }

        Ok(row.map(|r| OAuthToken {
            id: r.get("id"),
            access_token_hash: r.get("access_token_hash"),
            refresh_token_hash: r.get("refresh_token_hash"),
            token_type: r.get("token_type"),
            scope: r.get("scope"),
            client_id: r.get("client_id"),
            user_id: r.get("user_id"),
            access_token_expires_at: r.get("access_token_expires_at"),
            refresh_token_expires_at: r.get("refresh_token_expires_at"),
            revoked: r.get("revoked"),
            created_at: r.get("created_at"),
        }))
    }

    /// Refresh an access token using a refresh token.
    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<(String, Option<String>, OAuthToken)> {
        let hash = Self::hash_secret(refresh_token);
        let now = Utc::now();

        // Validate the refresh token
        let row = sqlx::query(
            r#"SELECT scope, user_id, refresh_token_expires_at
            FROM oauth_token
            WHERE refresh_token_hash = $1
              AND client_id = $2
              AND revoked = false
              AND (refresh_token_expires_at IS NULL OR refresh_token_expires_at > $3)"#,
        )
        .bind(&hash)
        .bind(client_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| Error::Unauthorized("Invalid refresh token".to_string()))?;

        let scope: String = row.get("scope");
        let user_id: Option<String> = row.get("user_id");

        // Revoke the old token
        sqlx::query(
            "UPDATE oauth_token SET revoked = true, revoked_at = $1, revoked_reason = 'refreshed' WHERE refresh_token_hash = $2",
        )
        .bind(now)
        .bind(&hash)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Create a new token pair with same lifetime as original
        // MCP clients (scope contains "mcp") get 4 hours, others get 1 hour
        let lifetime = if scope.contains("mcp") {
            Duration::hours(4)
        } else {
            Duration::hours(1)
        };
        self.create_token_with_lifetime(client_id, &scope, user_id.as_deref(), true, lifetime)
            .await
    }

    /// Revoke a token (access or refresh).
    pub async fn revoke_token(&self, token: &str, token_type_hint: Option<&str>) -> Result<bool> {
        let hash = Self::hash_secret(token);
        let now = Utc::now();

        // Try to revoke by access token first (or if hinted)
        let result = if token_type_hint != Some("refresh_token") {
            sqlx::query(
                "UPDATE oauth_token SET revoked = true, revoked_at = $1, revoked_reason = 'revoked' WHERE access_token_hash = $2 AND revoked = false",
            )
            .bind(now)
            .bind(&hash)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?
        } else {
            sqlx::postgres::PgQueryResult::default()
        };

        if result.rows_affected() > 0 {
            return Ok(true);
        }

        // Try to revoke by refresh token
        let result = sqlx::query(
            "UPDATE oauth_token SET revoked = true, revoked_at = $1, revoked_reason = 'revoked' WHERE refresh_token_hash = $2 AND revoked = false",
        )
        .bind(now)
        .bind(&hash)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    /// Introspect a token (RFC 7662).
    pub async fn introspect_token(&self, token: &str) -> Result<TokenIntrospectionResponse> {
        let hash = Self::hash_secret(token);
        let now = Utc::now();

        // Try access token first
        let row = sqlx::query(
            r#"SELECT
                scope, client_id, user_id, access_token_expires_at, created_at, revoked
            FROM oauth_token
            WHERE access_token_hash = $1"#,
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(r) = row {
            let revoked: bool = r.get("revoked");
            let expires_at: chrono::DateTime<Utc> = r.get("access_token_expires_at");
            let active = !revoked && expires_at > now;

            return Ok(TokenIntrospectionResponse {
                active,
                scope: Some(r.get("scope")),
                client_id: Some(r.get("client_id")),
                username: r.get("user_id"),
                token_type: Some("Bearer".to_string()),
                exp: Some(expires_at.timestamp()),
                iat: Some(r.get::<chrono::DateTime<Utc>, _>("created_at").timestamp()),
                sub: r.get("user_id"),
                aud: Some(r.get("client_id")),
                iss: None, // Will be set by the API layer
            });
        }

        // Try refresh token
        let row = sqlx::query(
            r#"SELECT
                scope, client_id, user_id, refresh_token_expires_at, created_at, revoked
            FROM oauth_token
            WHERE refresh_token_hash = $1"#,
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(r) = row {
            let revoked: bool = r.get("revoked");
            let expires_at: Option<chrono::DateTime<Utc>> = r.get("refresh_token_expires_at");
            let active = !revoked && expires_at.map(|e| e > now).unwrap_or(true);

            return Ok(TokenIntrospectionResponse {
                active,
                scope: Some(r.get("scope")),
                client_id: Some(r.get("client_id")),
                username: r.get("user_id"),
                token_type: Some("refresh_token".to_string()),
                exp: expires_at.map(|e| e.timestamp()),
                iat: Some(r.get::<chrono::DateTime<Utc>, _>("created_at").timestamp()),
                sub: r.get("user_id"),
                aud: Some(r.get("client_id")),
                iss: None,
            });
        }

        // Token not found - return inactive
        Ok(TokenIntrospectionResponse {
            active: false,
            scope: None,
            client_id: None,
            username: None,
            token_type: None,
            exp: None,
            iat: None,
            sub: None,
            aud: None,
            iss: None,
        })
    }

    // =========================================================================
    // API KEYS
    // =========================================================================

    /// Create an API key.
    pub async fn create_api_key(&self, req: CreateApiKeyRequest) -> Result<CreateApiKeyResponse> {
        let now = Utc::now();
        let id = new_v7();

        // Generate API key: mm_key_<random>
        let key = format!("mm_key_{}", Self::generate_secret(32));
        let key_prefix = key.chars().take(12).collect::<String>();
        let key_hash = Self::hash_secret(&key);

        let expires_at = req
            .expires_in_days
            .map(|days| now + Duration::days(days as i64));

        sqlx::query(
            r#"INSERT INTO api_key (
                id, key_hash, key_prefix, name, description, scope,
                is_active, expires_at, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, true, $7, $8, $8)"#,
        )
        .bind(id)
        .bind(&key_hash)
        .bind(&key_prefix)
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.scope)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(CreateApiKeyResponse {
            id,
            api_key: key, // Only shown once
            key_prefix,
            name: req.name,
            scope: req.scope,
            expires_at,
            created_at: now,
        })
    }

    /// Validate an API key and return its details.
    pub async fn validate_api_key(&self, api_key: &str) -> Result<Option<ApiKey>> {
        let hash = Self::hash_secret(api_key);
        let now = Utc::now();

        let row = sqlx::query(
            r#"SELECT
                id, key_prefix, name, description, scope,
                rate_limit_per_minute, rate_limit_per_hour,
                last_used_at, use_count, is_active, expires_at, created_at
            FROM api_key
            WHERE key_hash = $1
              AND is_active = true
              AND (expires_at IS NULL OR expires_at > $2)"#,
        )
        .bind(&hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(r) = &row {
            // Update last_used_at and use_count
            let id: Uuid = r.get("id");
            sqlx::query(
                "UPDATE api_key SET last_used_at = $1, use_count = use_count + 1 WHERE id = $2",
            )
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        Ok(row.map(|r| ApiKey {
            id: r.get("id"),
            key_prefix: r.get("key_prefix"),
            name: r.get("name"),
            description: r.get("description"),
            scope: r.get("scope"),
            rate_limit_per_minute: r.get("rate_limit_per_minute"),
            rate_limit_per_hour: r.get("rate_limit_per_hour"),
            last_used_at: r.get("last_used_at"),
            use_count: r.get("use_count"),
            is_active: r.get("is_active"),
            expires_at: r.get("expires_at"),
            created_at: r.get("created_at"),
        }))
    }

    /// List all API keys (without the actual key).
    pub async fn list_api_keys(&self) -> Result<Vec<ApiKey>> {
        let rows = sqlx::query(
            r#"SELECT
                id, key_prefix, name, description, scope,
                rate_limit_per_minute, rate_limit_per_hour,
                last_used_at, use_count, is_active, expires_at, created_at
            FROM api_key
            ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| ApiKey {
                id: r.get("id"),
                key_prefix: r.get("key_prefix"),
                name: r.get("name"),
                description: r.get("description"),
                scope: r.get("scope"),
                rate_limit_per_minute: r.get("rate_limit_per_minute"),
                rate_limit_per_hour: r.get("rate_limit_per_hour"),
                last_used_at: r.get("last_used_at"),
                use_count: r.get("use_count"),
                is_active: r.get("is_active"),
                expires_at: r.get("expires_at"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    /// Revoke an API key.
    pub async fn revoke_api_key(&self, id: Uuid) -> Result<bool> {
        let result =
            sqlx::query("UPDATE api_key SET is_active = false, updated_at = $1 WHERE id = $2")
                .bind(Utc::now())
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    // =========================================================================
    // CLEANUP
    // =========================================================================

    /// Clean up expired tokens and codes.
    pub async fn cleanup_expired(&self) -> Result<(u64, u64)> {
        let now = Utc::now();

        // Delete expired authorization codes
        let codes_deleted =
            sqlx::query("DELETE FROM oauth_authorization_code WHERE expires_at < $1")
                .bind(now)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?
                .rows_affected();

        // Delete very old revoked tokens (keep for 30 days for audit)
        let audit_cutoff = now - Duration::days(30);
        let tokens_deleted = sqlx::query(
            r#"DELETE FROM oauth_token
            WHERE access_token_expires_at < $1
              AND (refresh_token_expires_at IS NULL OR refresh_token_expires_at < $1)"#,
        )
        .bind(audit_cutoff)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?
        .rows_affected();

        Ok((codes_deleted, tokens_deleted))
    }
}

/// Base64 URL-safe encoding without padding (for PKCE).
fn base64_url_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret() {
        let secret = PgOAuthRepository::generate_secret(32);
        assert_eq!(secret.len(), 32);
        assert!(secret.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_hash_and_verify() {
        let secret = "test_secret_123";
        let hash = PgOAuthRepository::hash_secret(secret);
        assert!(PgOAuthRepository::verify_secret(secret, &hash));
        assert!(!PgOAuthRepository::verify_secret("wrong_secret", &hash));
    }

    #[test]
    fn test_base64_url_encode() {
        let data = b"test data";
        let encoded = base64_url_encode(data);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
    }
}

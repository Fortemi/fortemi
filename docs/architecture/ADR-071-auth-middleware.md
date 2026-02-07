# ADR-071: Authentication Middleware for API Security

**Status:** Proposed
**Date:** 2026-02-06
**Author:** Security Engineer
**Related Issues:** Gitea Issue #71 - OAuth/Auth critical security deficiencies

---

## Context and Problem Statement

The matric-memory API currently has **ZERO authentication enforcement**. While the OAuth2 infrastructure is fully implemented at the database layer (token creation, introspection, revocation, client registration), and extractors (`Auth`, `RequireAuth`) exist for handlers to opt-in to authentication, **no middleware enforces authentication at the router level**.

This means:
- All API endpoints (`/api/v1/*`) are publicly accessible without any credentials
- OAuth tokens can be created but are never validated
- API keys can be created but are never checked
- Sensitive operations (note deletion, SKOS modification, backup export) are completely unprotected
- The MCP server OAuth flow works but provides no actual security

This is a **critical security vulnerability** that makes the entire authentication system non-functional.

### Current State Analysis

#### What Works (Database Layer)
Located in `crates/matric-db/src/oauth.rs`:

1. **OAuth2 Client Management**
   - `register_client()` - RFC 7591 Dynamic Client Registration
   - `validate_client()` - Client credential validation
   - `get_client()` - Retrieve client metadata

2. **OAuth2 Token Lifecycle**
   - `create_token()` - Generate access/refresh tokens
   - `create_token_with_lifetime()` - Custom lifetime support (MCP: 4hr, Web: 1hr)
   - `validate_access_token()` - Token validation with expiry check
   - `validate_and_extend_token()` - Sliding window refresh
   - `introspect_token()` - RFC 7662 Token Introspection
   - `revoke_token()` - RFC 7009 Token Revocation

3. **API Key Management**
   - `create_api_key()` - Generate API keys (prefix: `mm_key_`)
   - `validate_api_key()` - API key validation with expiry check
   - `list_api_keys()` - List all keys
   - `revoke_api_key()` - Deactivate key

4. **OAuth2 Endpoints** (in `crates/matric-api/src/main.rs`)
   - `POST /oauth/register` - Client registration (line 912)
   - `POST /oauth/token` - Token issuance (line 913)
   - `POST /oauth/introspect` - Token introspection (line 914)
   - `POST /oauth/revoke` - Token revocation (line 915)
   - `GET /oauth/authorize` - Authorization consent page (line 910)
   - `POST /oauth/authorize` - Authorization grant (line 910)
   - `GET /.well-known/oauth-authorization-server` - Discovery metadata (line 901)

5. **Authentication Extractors** (in `crates/matric-api/src/main.rs`)
   - `Auth` struct (line 5533) - Optional authentication via `FromRequestParts`
   - `RequireAuth` struct (line 5604) - Mandatory authentication via `FromRequestParts`
   - `AuthPrincipal` enum (in `crates/matric-core/src/models.rs` line 2330)
     - `OAuthClient { client_id, scope, user_id }`
     - `ApiKey { key_id, scope }`
     - `Anonymous`

6. **Token Format Conventions**
   - OAuth access tokens: `mm_at_<48-chars>` (SHA256 hashed)
   - OAuth refresh tokens: `mm_rt_<48-chars>` (SHA256 hashed)
   - API keys: `mm_key_<48-chars>` (SHA256 hashed)
   - Client IDs: `mm_<24-chars>`

#### What Does NOT Work (Middleware Layer)

Located in `crates/matric-api/src/main.rs` lines 988-1018:

```rust
// Middleware stack
.layer(axum::middleware::from_fn_with_state(
    state.clone(),
    rate_limit_middleware,  // <-- ONLY rate limiting applied
))
.layer(TraceLayer::new_for_http())
.layer(PropagateRequestIdLayer::x_request_id())
.layer(SetRequestIdLayer::x_request_id(MakeRequestUuidV7))
.layer(CorsLayer::new()...)
.layer(RequestBodyLimitLayer::new(max_body_size))
```

**CRITICAL GAP:** No authentication middleware is applied to the router.

#### Existing Authentication Logic (Unused)

The `Auth::from_request_parts()` implementation (lines 5538-5597) contains the complete token validation logic:

```rust
async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
    let auth_header = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());

    let principal = match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = header.trim_start_matches("Bearer ").trim();

            // OAuth token (mm_at_*)
            if token.starts_with("mm_at_") {
                match state.db.oauth.validate_access_token(token).await {
                    Ok(Some(oauth_token)) => {
                        // Sliding window refresh
                        let lifetime = token_lifetime_for_scope(&oauth_token.scope);
                        state.db.oauth.validate_and_extend_token(token, lifetime).await;
                        AuthPrincipal::OAuthClient { client_id, scope, user_id }
                    }
                    _ => AuthPrincipal::Anonymous
                }
            }
            // API key (mm_key_*)
            else if token.starts_with("mm_key_") {
                match state.db.oauth.validate_api_key(token).await {
                    Ok(Some(api_key)) => AuthPrincipal::ApiKey { key_id, scope }
                    _ => AuthPrincipal::Anonymous
                }
            }
            else {
                AuthPrincipal::Anonymous
            }
        }
        _ => AuthPrincipal::Anonymous,
    };

    Ok(Auth { principal })
}
```

This logic is **never invoked** because:
1. Handlers do not use `Auth` or `RequireAuth` extractors
2. No middleware layer rejects unauthenticated requests

### Test Coverage

Comprehensive OAuth tests exist in `crates/matric-api/tests/auth_middleware_test.rs`:
- Client registration and token creation
- Token introspection (active tokens)
- Invalid token rejection
- Expired token rejection
- Revoked token rejection
- Token claims validation
- Multiple tokens for same client

**HOWEVER:** These tests only verify the database layer. No tests verify that the API routes actually require authentication.

---

## Decision

Implement an **authentication middleware** that enforces authentication for all API routes except:
1. Health check endpoints (`/health`, `/api/v1/health/*`)
2. OAuth endpoints (`/oauth/*`, `/.well-known/*`)
3. Public documentation (`/swagger-ui`, `/api-docs`)

The middleware will be **opt-in via environment variable** to allow backward compatibility during transition.

### Architecture Design

#### 1. Middleware Function

Add `auth_middleware` adjacent to `rate_limit_middleware` (after line 1599 in `crates/matric-api/src/main.rs`):

```rust
/// Authentication middleware - enforces authentication on protected routes.
///
/// Enabled via REQUIRE_AUTH environment variable (default: false for backward compat).
///
/// Protected routes: All /api/v1/* except health endpoints
/// Public routes: /health, /oauth/*, /.well-known/*, /swagger-ui, /api-docs
async fn auth_middleware(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Check if auth is required
    let require_auth = std::env::var("REQUIRE_AUTH")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    if !require_auth {
        // Auth disabled - pass through
        return Ok(next.run(request).await);
    }

    let path = request.uri().path();

    // Allow public routes
    if is_public_route(path) {
        return Ok(next.run(request).await);
    }

    // Extract and validate token
    let auth_header = request.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let principal = match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = header.trim_start_matches("Bearer ").trim();

            // Validate OAuth token
            if token.starts_with("mm_at_") {
                match state.db.oauth.validate_access_token(token).await {
                    Ok(Some(oauth_token)) => {
                        // Extend token (sliding window)
                        let lifetime = token_lifetime_for_scope(&oauth_token.scope);
                        let _ = state.db.oauth.validate_and_extend_token(token, lifetime).await;

                        AuthPrincipal::OAuthClient {
                            client_id: oauth_token.client_id,
                            scope: oauth_token.scope,
                            user_id: oauth_token.user_id,
                        }
                    }
                    _ => AuthPrincipal::Anonymous,
                }
            }
            // Validate API key
            else if token.starts_with("mm_key_") {
                match state.db.oauth.validate_api_key(token).await {
                    Ok(Some(api_key)) => {
                        AuthPrincipal::ApiKey {
                            key_id: api_key.id,
                            scope: api_key.scope,
                        }
                    }
                    _ => AuthPrincipal::Anonymous,
                }
            }
            else {
                AuthPrincipal::Anonymous
            }
        }
        _ => AuthPrincipal::Anonymous,
    };

    // Reject unauthenticated requests
    if !principal.is_authenticated() {
        tracing::warn!("Unauthenticated request to protected route: {}", path);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthorized",
                "error_description": "Authentication required. Provide a valid Bearer token in the Authorization header."
            })),
        ));
    }

    // Attach principal to request extensions for handlers
    let mut request = request;
    request.extensions_mut().insert(Auth { principal });

    Ok(next.run(request).await)
}

/// Check if a route is public (does not require authentication).
fn is_public_route(path: &str) -> bool {
    // Health checks
    if path == "/health" || path.starts_with("/api/v1/health/") {
        return true;
    }

    // OAuth endpoints (authentication happens via client credentials)
    if path.starts_with("/oauth/") || path.starts_with("/.well-known/") {
        return true;
    }

    // Documentation
    if path.starts_with("/swagger-ui") || path.starts_with("/api-docs") {
        return true;
    }

    false
}
```

#### 2. Apply Middleware to Router

Modify middleware stack in `main.rs` lines 988-992:

```rust
// Middleware
.layer(axum::middleware::from_fn_with_state(
    state.clone(),
    auth_middleware,  // <-- ADD: Authentication layer (opt-in via REQUIRE_AUTH)
))
.layer(axum::middleware::from_fn_with_state(
    state.clone(),
    rate_limit_middleware,  // <-- KEEP: Rate limiting
))
```

**IMPORTANT:** Auth middleware must be applied **before** rate limiting to reject unauthenticated requests early (reduces attack surface).

#### 3. Environment Configuration

Add to `.env` and `docker-compose.bundle.yml`:

```bash
# Enable authentication enforcement (default: false for backward compatibility)
REQUIRE_AUTH=true
```

#### 4. Request Extension Storage

The middleware stores the validated `Auth` principal in `request.extensions_mut()` so handlers can access it via:

```rust
async fn some_handler(
    Extension(auth): Extension<Auth>,  // <-- Middleware injects this
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Access auth.principal
}
```

This avoids re-validating the token in every handler.

---

## Implementation Steps

### Phase 1: Infrastructure (Low Risk)

1. **Add `auth_middleware` function** (after line 1599 in `main.rs`)
   - Copy token validation logic from `Auth::from_request_parts()`
   - Add `is_public_route()` helper
   - Add `REQUIRE_AUTH` env var check
   - Add request extension storage

2. **Add `is_public_route()` helper** (after `auth_middleware`)
   - Whitelist `/health`, `/api/v1/health/*`
   - Whitelist `/oauth/*`, `/.well-known/*`
   - Whitelist `/swagger-ui`, `/api-docs`

3. **Write middleware unit tests** (new file: `tests/auth_middleware_enforcement_test.rs`)
   - Test: Unauthenticated request to `/api/v1/notes` returns 401
   - Test: Valid OAuth token to `/api/v1/notes` returns 200
   - Test: Valid API key to `/api/v1/notes` returns 200
   - Test: Expired token to `/api/v1/notes` returns 401
   - Test: Public routes (`/health`, `/oauth/token`) always return 200
   - Test: `REQUIRE_AUTH=false` allows all requests

### Phase 2: Apply Middleware (High Risk)

4. **Apply middleware to router** (lines 988-992 in `main.rs`)
   - Add `auth_middleware` layer **before** `rate_limit_middleware`
   - Verify middleware order: auth → rate_limit → tracing → CORS

5. **Update environment configuration**
   - Add `REQUIRE_AUTH=false` to `.env.example`
   - Add `REQUIRE_AUTH=${REQUIRE_AUTH:-false}` to `docker-compose.bundle.yml`
   - Document in `CLAUDE.md` under "Environment Configuration"

### Phase 3: Documentation and Rollout

6. **Update OpenAPI spec** (`crates/matric-api/src/openapi.yaml`)
   - Add `securitySchemes` for OAuth2 and Bearer tokens
   - Add `security` requirement to all `/api/v1/*` endpoints
   - Document public endpoints explicitly

7. **Update deployment docs**
   - Add migration guide to `docs/content/releasing.md`
   - Add authentication section to `CLAUDE.md`
   - Create runbook for enabling auth in production

8. **Staged rollout plan**
   - **Week 1:** Deploy with `REQUIRE_AUTH=false` (auth available but not enforced)
   - **Week 2:** Enable `REQUIRE_AUTH=true` in staging environment
   - **Week 3:** Monitor logs for authentication failures
   - **Week 4:** Enable `REQUIRE_AUTH=true` in production

---

## Consequences

### Positive

1. **Security restored** - API endpoints require valid authentication
2. **OAuth functional** - Existing OAuth infrastructure becomes enforceable
3. **API key support** - API keys work as intended
4. **Backward compatible** - Opt-in via `REQUIRE_AUTH` env var
5. **Sliding window tokens** - Token expiry extends on use (better UX)
6. **Performance optimized** - Middleware validates once, handlers reuse via extensions
7. **Audit trail** - Failed auth attempts logged via tracing

### Negative

1. **Breaking change** - Enabling `REQUIRE_AUTH=true` breaks existing unauthenticated clients
2. **Migration required** - All clients must obtain OAuth tokens or API keys
3. **Increased complexity** - Token lifecycle management required
4. **Performance impact** - Database query on every authenticated request (token validation)
5. **MCP dependency** - MCP clients must use OAuth client credentials flow

### Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Production outage** if enabled without client migration | **CRITICAL** | Default `REQUIRE_AUTH=false`, staged rollout |
| **Token expiry** breaks long-running MCP sessions | **HIGH** | Sliding window refresh (tokens extend on use) |
| **Database load** from token validation on every request | **MEDIUM** | Add Redis cache for validated tokens (future ADR) |
| **API key leakage** in logs/error messages | **MEDIUM** | Never log token values, only prefixes |
| **OAuth endpoints** become unreachable if auth middleware breaks | **HIGH** | Whitelist `/oauth/*` in `is_public_route()` |
| **Health checks fail** if auth required | **HIGH** | Whitelist `/health` in `is_public_route()` |
| **CI/CD tests fail** without tokens | **MEDIUM** | Keep `REQUIRE_AUTH=false` in test env |

---

## Rollback Plan

If authentication causes production issues:

### Immediate Rollback (< 5 minutes)

1. **Disable enforcement** via environment variable:
   ```bash
   docker compose -f docker-compose.bundle.yml down
   # Edit .env: REQUIRE_AUTH=false
   docker compose -f docker-compose.bundle.yml up -d
   ```

2. **Verify health check** returns 200:
   ```bash
   curl http://localhost:3000/health
   ```

3. **Verify API access** restored:
   ```bash
   curl http://localhost:3000/api/v1/notes
   ```

### Code Rollback (if middleware has bugs)

1. **Comment out middleware layer** in `main.rs` lines 989-992:
   ```rust
   // .layer(axum::middleware::from_fn_with_state(
   //     state.clone(),
   //     auth_middleware,
   // ))
   ```

2. **Rebuild and redeploy**:
   ```bash
   docker compose -f docker-compose.bundle.yml build
   docker compose -f docker-compose.bundle.yml up -d
   ```

3. **Git revert** if necessary:
   ```bash
   git revert <commit-hash>
   git push origin main
   ```

---

## Testing Strategy

### Unit Tests (crates/matric-api/tests/auth_middleware_enforcement_test.rs)

```rust
#[tokio::test]
async fn test_unauthenticated_request_to_protected_route_returns_401() {
    std::env::set_var("REQUIRE_AUTH", "true");
    let app = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_valid_oauth_token_to_protected_route_returns_200() {
    std::env::set_var("REQUIRE_AUTH", "true");
    let app = build_test_app().await;
    let token = create_test_oauth_token().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/notes")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_public_route_always_accessible() {
    std::env::set_var("REQUIRE_AUTH", "true");
    let app = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_oauth_endpoint_accessible_without_bearer_token() {
    std::env::set_var("REQUIRE_AUTH", "true");
    let app = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/oauth/token")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 400 (missing credentials), not 401 (auth required)
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
}
```

### Integration Tests (crates/matric-api/tests/auth_integration_test.rs)

- Full OAuth flow: Register client → Get token → Access API
- Full API key flow: Create key → Access API → Revoke key
- Token expiry and sliding window refresh
- Multiple concurrent authenticated requests
- Scope validation (admin vs read vs write)

### Manual Testing Checklist

- [ ] Health check accessible without auth
- [ ] OAuth registration works without auth
- [ ] OAuth token endpoint works without auth
- [ ] API endpoints require auth when `REQUIRE_AUTH=true`
- [ ] Valid token grants access
- [ ] Expired token denied
- [ ] Revoked token denied
- [ ] Invalid token denied
- [ ] Missing token denied
- [ ] Swagger UI accessible without auth
- [ ] MCP server can authenticate and access API

---

## Alternative Approaches Considered

### Alternative 1: JWT Middleware with Stateless Validation

**Approach:** Replace database token validation with JWT signing/verification.

**Pros:**
- No database query per request
- Scalable (stateless)
- Industry standard

**Cons:**
- Cannot revoke tokens before expiry
- Requires migration from current SHA256-hashed token storage
- Breaking change to existing OAuth clients
- Loses sliding window refresh capability

**Decision:** Rejected. Current database-backed tokens support revocation and sliding window, which are critical for MCP long-running sessions.

### Alternative 2: Per-Handler Authentication (No Middleware)

**Approach:** Require handlers to use `RequireAuth` extractor explicitly.

**Pros:**
- Fine-grained control per endpoint
- No global middleware complexity

**Cons:**
- Easy to forget authentication on new endpoints (security gap)
- No centralized enforcement
- Inconsistent authentication behavior
- Code duplication across handlers

**Decision:** Rejected. Middleware provides defense-in-depth; handlers can still use `RequireAuth` for scope checks.

### Alternative 3: Always-On Authentication (No REQUIRE_AUTH Flag)

**Approach:** Enforce authentication immediately without opt-in flag.

**Pros:**
- Simpler implementation
- Forces immediate migration

**Cons:**
- **CRITICAL:** Breaks all existing deployments
- No staged rollout possible
- High risk of production outage

**Decision:** Rejected. Opt-in via `REQUIRE_AUTH` provides safer migration path.

---

## Open Questions

1. **Token caching:** Should validated tokens be cached in Redis to reduce database load?
   - **Answer:** Defer to future ADR. Start with database validation, optimize if metrics show bottleneck.

2. **Scope enforcement:** Should middleware reject requests based on scope (e.g., `read` vs `write`)?
   - **Answer:** No. Middleware only checks authentication. Handlers use `RequireAuth::require_scope()` for authorization.

3. **Rate limiting per principal:** Should rate limits differ by OAuth client vs API key vs anonymous?
   - **Answer:** Defer to future ADR. Current rate limiting is global; per-principal limits require separate implementation.

4. **Token rotation:** Should access tokens auto-rotate before expiry?
   - **Answer:** Not needed. Sliding window refresh extends expiry on use. Refresh tokens handle long-lived sessions.

5. **Audit logging:** Should all authentication failures be logged?
   - **Answer:** Yes. Middleware already logs via `tracing::warn!()`. Consider future structured logging ADR.

---

## References

- **Gitea Issue #71:** OAuth/Auth critical security deficiencies
- **RFC 6749:** OAuth 2.0 Authorization Framework
- **RFC 7662:** OAuth 2.0 Token Introspection
- **RFC 7009:** OAuth 2.0 Token Revocation
- **RFC 7591:** OAuth 2.0 Dynamic Client Registration
- **Axum Middleware Guide:** https://docs.rs/axum/latest/axum/middleware/
- **matric-db OAuth implementation:** `crates/matric-db/src/oauth.rs`
- **matric-api Auth extractors:** `crates/matric-api/src/main.rs` lines 5533-5643

---

## Implementation Tracking

| Task | File | Lines | Status | Notes |
|------|------|-------|--------|-------|
| Add `auth_middleware()` | `main.rs` | After 1599 | TODO | Copy logic from `Auth::from_request_parts()` |
| Add `is_public_route()` | `main.rs` | After `auth_middleware` | TODO | Whitelist health, OAuth, docs |
| Apply middleware | `main.rs` | 988-992 | TODO | Add before `rate_limit_middleware` |
| Add `REQUIRE_AUTH` env var | `.env.example` | N/A | TODO | Default: `false` |
| Update docker-compose | `docker-compose.bundle.yml` | N/A | TODO | `REQUIRE_AUTH=${REQUIRE_AUTH:-false}` |
| Write unit tests | `tests/auth_middleware_enforcement_test.rs` | New file | TODO | 6 test cases |
| Write integration tests | `tests/auth_integration_test.rs` | New file | TODO | Full OAuth + API key flows |
| Update OpenAPI spec | `openapi.yaml` | Security section | TODO | Add securitySchemes and security |
| Update CLAUDE.md | `CLAUDE.md` | Auth section | TODO | Document `REQUIRE_AUTH` |
| Update release docs | `docs/content/releasing.md` | Migration guide | TODO | Staged rollout plan |

---

## Approval

This ADR is **proposed** and requires review by:
- [ ] Security Engineer
- [ ] Backend Lead
- [ ] DevOps Engineer (rollout plan)
- [ ] Product Owner (UX impact of token requirement)

**Estimated Implementation Time:** 2-3 days (excluding tests and documentation)
**Estimated Rollout Time:** 4 weeks (staged with monitoring)

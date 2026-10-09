//! Real `auth_middleware` behaviour for routes hosted mode closes (#1163, #1164).
//! Handlers are stubs: reaching one proves the middleware admitted the request.
use super::*;
use tower::ServiceExt;

const REACHED: StatusCode = StatusCode::IM_A_TEAPOT;

async fn reached() -> StatusCode {
    REACHED
}

async fn lazy_state(require_auth: bool, multi_tenant: bool) -> AppState {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:9/unused")
        .expect("lazy pool");
    let mut state = tests::build_call_api_test_state(Database::new(pool), "unused").await;
    state.require_auth = require_auth;
    state.multi_tenant = multi_tenant;
    state
}

fn app(state: AppState) -> Router {
    let mut router = Router::new();
    for path in hosted_exempt_routes::KNOWLEDGE_DIAGNOSTIC_PATHS {
        router = router.route(path, get(reached));
    }
    router
        .route(hosted_exempt_routes::LEGACY_WEBSOCKET_PATH, get(reached))
        .route(hosted_exempt_routes::INGEST_STREAM_PATH, post(reached))
        .route("/health", get(reached))
        .route("/livez", get(reached))
        .route("/readyz", get(reached))
        .route("/api/v1/health/streaming", get(reached))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

async fn status(app: &Router, method: Method, path: &str, bearer: Option<&str>) -> StatusCode {
    let mut request = axum::http::Request::builder().method(method).uri(path);
    if let Some(token) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

fn closed() -> Vec<(Method, &'static str)> {
    let mut routes = vec![
        (Method::GET, hosted_exempt_routes::LEGACY_WEBSOCKET_PATH),
        (Method::POST, hosted_exempt_routes::INGEST_STREAM_PATH),
    ];
    routes.extend(
        hosted_exempt_routes::KNOWLEDGE_DIAGNOSTIC_PATHS
            .into_iter()
            .map(|path| (Method::GET, path)),
    );
    routes
}

const PROBES: [&str; 4] = ["/health", "/livez", "/readyz", "/api/v1/health/streaming"];

#[tokio::test]
async fn hosted_unauthenticated_closed_routes_return_401_even_without_require_auth() {
    for require_auth in [true, false] {
        let app = app(lazy_state(require_auth, true).await);
        for (method, path) in closed() {
            assert_eq!(
                status(&app, method.clone(), path, None).await,
                StatusCode::UNAUTHORIZED,
                "{method} {path} require_auth={require_auth}"
            );
            assert_eq!(
                status(&app, method, path, Some("not-a-hosted-token")).await,
                StatusCode::UNAUTHORIZED
            );
        }
        for path in PROBES {
            assert_eq!(
                status(&app, Method::GET, path, None).await,
                REACHED,
                "{path}"
            );
        }
    }
}

#[tokio::test]
async fn hosted_cors_preflight_on_closed_routes_stays_exempt() {
    let app = app(lazy_state(true, true).await);
    for (_, path) in closed() {
        assert!(!hosted_exempt_routes::hosted_requires_bearer(
            &Method::OPTIONS,
            path
        ));
        // No OPTIONS route is registered: the router answers, not auth.
        assert_ne!(
            status(&app, Method::OPTIONS, path, None).await,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn community_closed_routes_and_probes_keep_their_exemption() {
    for require_auth in [true, false] {
        let app = app(lazy_state(require_auth, false).await);
        for (method, path) in closed() {
            assert_eq!(
                status(&app, method.clone(), path, None).await,
                REACHED,
                "{method} {path} require_auth={require_auth}"
            );
        }
        for path in PROBES {
            assert_eq!(
                status(&app, Method::GET, path, None).await,
                REACHED,
                "{path}"
            );
        }
    }
}

#[cfg(feature = "hosted-auth")]
mod hosted_bearer {
    use super::*;

    struct FixtureTenants(Uuid, Uuid);

    #[async_trait::async_trait]
    impl HostedAuthenticator for FixtureTenants {
        async fn authenticate(
            &self,
            token: &str,
        ) -> Result<fortemi_auth_core::AuthContext, fortemi_auth_core::AuthError> {
            let tenant_id = match token {
                "tenant-a" => self.0,
                "tenant-b" => self.1,
                _ => return Err(fortemi_auth_core::AuthError::MalformedToken),
            };
            Ok(fortemi_auth_core::AuthContext {
                tenant_id,
                scopes: vec!["read".into(), "write".into(), "mcp".into()],
                principal_id: "hosted-exempt-fixture".into(),
                credential: fortemi_auth_core::Credential::Bearer(fortemi_auth_core::JwtToken {
                    jti: None,
                    algorithm: "RS256".into(),
                    key_id: "fixture-only".into(),
                }),
                issued_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::minutes(5),
                session_id: None,
                principal_kind: fortemi_auth_core::PrincipalKind::Human,
                scope_grants: Vec::new(),
                dropped_scope_count: 0,
            })
        }
    }

    pub(super) async fn hosted_app(require_auth: bool) -> Router {
        let mut state = lazy_state(require_auth, true).await;
        state.hosted_auth = Some(Arc::new(FixtureTenants(Uuid::new_v4(), Uuid::new_v4())));
        app(state)
    }

    #[tokio::test]
    async fn hosted_verified_bearer_is_refused_503_by_the_tenant_gate() {
        for require_auth in [true, false] {
            let app = hosted_app(require_auth).await;
            for token in ["tenant-a", "tenant-b"] {
                for (method, path) in closed() {
                    assert_eq!(
                        status(&app, method.clone(), path, Some(token)).await,
                        StatusCode::SERVICE_UNAVAILABLE,
                        "{method} {path} {token} require_auth={require_auth}"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn hosted_knowledge_diagnostics_disclose_no_tenant_counts() {
        let app = hosted_app(true).await;
        for path in hosted_exempt_routes::KNOWLEDGE_DIAGNOSTIC_PATHS {
            let mut bodies = Vec::new();
            for token in ["tenant-a", "tenant-b"] {
                let request = axum::http::Request::get(path)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap();
                let response = app.clone().oneshot(request).await.unwrap();
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                let body: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                for field in ["total_notes", "orphan_tags", "stale_notes", "notes"] {
                    assert!(body.get(field).is_none(), "{path} leaked {field}");
                }
                bodies.push(body["detail"].clone());
            }
            assert_eq!(bodies[0], bodies[1], "{path} must not vary by tenant");
        }
    }
}

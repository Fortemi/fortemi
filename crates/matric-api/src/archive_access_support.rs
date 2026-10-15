//! Shared production-router fixtures for archive authorization tests.
use super::*;
pub(super) async fn setup(admin: &sqlx::PgPool) -> AppState {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(admin)
        .await
        .unwrap();
    let tenant = Uuid::new_v4();
    let url = runtime_url(admin, tenant).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap();
    production_state_with_event_bus(db, tenant, Arc::new(EventBus::new(256))).await
}

pub(super) fn request(
    token: &str,
    method: Method,
    path: &str,
    body: serde_json::Value,
    memory: Option<&str>,
) -> axum::http::Request<Body> {
    let mut req = oidc_request(method, path, body, memory);
    req.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    // Use the same caller-supplied request ID to compare complete error bodies.
    req.headers_mut().insert(
        "x-request-id",
        "00000000-0000-4000-8000-000000000001".parse().unwrap(),
    );
    req
}

pub(super) async fn call_as(
    app: &Router,
    token: &str,
    method: Method,
    path: &str,
    body: serde_json::Value,
    memory: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        app.clone()
            .oneshot(request(token, method, path, body, memory)),
    )
    .await
    .expect("bounded production request")
    .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}

pub(super) async fn create(app: &Router, token: &str, name: &str) -> String {
    let (status, body) = call_as(
        app,
        token,
        Method::POST,
        "/api/v1/archives",
        serde_json::json!({"name":name}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["schema_name"].as_str().unwrap().into()
}

pub(super) struct OnlyArchive {
    pub(super) default: Arc<dyn AuthorizationPolicy>,
    pub(super) name: String,
    pub(super) schema: String,
    pub(super) user: Uuid,
    pub(super) checked: Arc<AtomicUsize>,
    pub(super) errors: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl AuthorizationPolicy for OnlyArchive {
    async fn authorize(
        &self,
        principal: &AuthPrincipal,
        action: &matric_core::Action,
        resource: &Resource,
        ctx: &AuthzContext,
    ) -> Result<Decision, AuthzError> {
        assert_eq!(ctx.environment["app_user_id"], self.user.to_string());
        self.default
            .authorize(principal, action, resource, ctx)
            .await
    }
    async fn may_select_archive(
        &self,
        principal: &AuthPrincipal,
        archive: &Resource,
        ctx: &AuthzContext,
    ) -> Result<bool, AuthzError> {
        assert!(principal.is_authenticated());
        assert_eq!(ctx.environment["app_user_id"], self.user.to_string());
        assert_eq!(
            archive.id.as_deref(),
            ctx.environment["archive_name"].as_str()
        );
        assert_eq!(
            archive.attrs["archive_schema"],
            ctx.environment["archive_schema"]
        );
        self.checked.fetch_add(1, Ordering::Relaxed);
        if self.errors.load(Ordering::Relaxed) {
            return Err(AuthzError::Policy(
                "synthetic grant store unavailable".into(),
            ));
        }
        Ok(archive.id.as_deref() == Some(&self.name)
            && archive.attrs["archive_schema"] == self.schema)
    }
    fn policy_id(&self) -> &'static str {
        "test_archive_grants"
    }
    fn policy_version(&self) -> &'static str {
        "1201"
    }
}

pub(super) async fn restricted(
    admin: &sqlx::PgPool,
) -> (AppState, Router, Router, String, String, String) {
    let mut state = setup(admin).await;
    state.ingest_token_store =
        matric_api::services::IngestTokenStore::with_ttl(&std::env::var("REDIS_URL").unwrap(), 60)
            .await
            .unwrap();
    let token = external_oidc_test_issuer::issuer().token(serde_json::json!({"groups":["admin"]}));
    let original = crate::production_router(state.clone(), None);
    let a = create(&original, &token, "archive_a").await;
    let b = create(&original, &token, "archive_b").await;
    // Startup/archive provisioning queues maintenance jobs. These regressions
    // seed their own queue and assert only changes caused by the tested calls.
    sqlx::query("DELETE FROM job_queue")
        .execute(&state.db.pool)
        .await
        .unwrap();
    let identity = validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    state.authorization_policy = Arc::new(OnlyArchive {
        default: state.authorization_policy.clone(),
        name: "archive_a".into(),
        schema: a.clone(),
        user: sse::stable_user_id(&identity).unwrap(),
        checked: Arc::new(AtomicUsize::new(0)),
        errors: Arc::new(AtomicBool::new(false)),
    });
    let app = crate::production_router(state.clone(), None);
    (state, app, original, token, a, b)
}

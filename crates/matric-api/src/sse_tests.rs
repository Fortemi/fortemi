use super::*;
use futures::StreamExt;
use tower::ServiceExt;

fn identity(scopes: &str, tenant: Option<Uuid>, human: bool) -> ValidatedBearerIdentity {
    ValidatedBearerIdentity {
        principal: AuthPrincipal::OAuthClient {
            client_id: if tenant.is_some() {
                "external-oidc"
            } else {
                "sse-test"
            }
            .into(),
            scope: scopes.into(),
            user_id: human.then(|| "synthetic-reader".into()),
        },
        tenant_id: tenant,
        expires_at: Some(Utc::now() + chrono::Duration::minutes(5)),
        stream_identity: None,
        parent_credential: None,
        #[cfg(feature = "hosted-auth")]
        request_principal: None,
        #[cfg(feature = "hosted-auth")]
        canonical_context: tenant.map(|tenant_id| fortemi_auth_core::AuthContext {
            tenant_id,
            principal_id: "synthetic-reader".into(),
            credential: fortemi_auth_core::Credential::Bearer(fortemi_auth_core::JwtToken {
                jti: None,
                algorithm: "RS256".into(),
                key_id: "synthetic-key".into(),
            }),
            issued_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            scopes: scopes.split_whitespace().map(str::to_string).collect(),
            session_id: None,
            principal_kind: if human {
                fortemi_auth_core::PrincipalKind::Human
            } else {
                fortemi_auth_core::PrincipalKind::Service
            },
            scope_grants: Vec::new(),
            dropped_scope_count: 0,
        }),
    }
}

async fn state() -> AppState {
    let url = std::env::var("DATABASE_URL").expect("explicit disposable test database");
    let db = Database::connect(&url).await.unwrap();
    let mut state = tests::build_call_api_test_state(db, &url).await;
    state.require_auth = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state
}

async fn archive(state: &AppState) -> (String, String) {
    let name = format!("sse_{}", Uuid::new_v4().simple());
    let archive = state
        .db
        .archives
        .create_archive_schema(&name, None)
        .await
        .unwrap();
    (archive.name, archive.schema_name)
}

async fn remove_archive_fixture(state: &AppState, selected: &(String, String)) {
    state
        .db
        .archives
        .drop_archive_schema(&selected.0)
        .await
        .unwrap();
}

fn router(
    state: AppState,
    identity: ValidatedBearerIdentity,
    archive: &(String, String),
) -> Router {
    Router::new()
        .route("/api/v1/events", get(sse_events))
        .layer(Extension(ArchiveContext {
            name: Some(archive.0.clone()),
            schema: archive.1.clone(),
            is_default: false,
        }))
        .layer(Extension(identity))
        .with_state(state)
}

fn request(name: &str, cursor: Option<Uuid>) -> axum::http::Request<Body> {
    let mut request = axum::http::Request::builder()
        .uri(format!("/api/v1/events?memory={name}"))
        .header(header::AUTHORIZATION, "Bearer synthetic-verified-token");
    if let Some(cursor) = cursor {
        request = request.header("Last-Event-ID", cursor.to_string());
    }
    request.body(Body::empty()).unwrap()
}

fn emit(bus: &EventBus, memory: Option<&str>, tenant: Option<Uuid>, note_id: Uuid) {
    bus.emit_with_context(
        ServerEvent::NoteDeleted { note_id },
        EventContext {
            memory: memory.map(str::to_string),
            tenant_id: tenant.map(|id| id.to_string()),
            ..Default::default()
        },
    );
}

pub(super) async fn next_frame(
    stream: &mut (impl futures::Stream<Item = Result<Bytes, axum::Error>> + Unpin),
) -> serde_json::Value {
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("SSE frame timeout")
        .unwrap()
        .unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("SSE data frame");
    serde_json::from_str(data).unwrap()
}

#[tokio::test]
async fn read_only_live_and_reconnect_replay_share_archive_filter() {
    let state = state().await;
    let selected = archive(&state).await;
    let other = archive(&state).await;
    let identity = identity("read", None, true);
    let app = router(state.clone(), identity, &selected);
    let response = app
        .clone()
        .oneshot(request(&selected.0, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut live = response.into_body().into_data_stream();
    let first = Uuid::new_v4();
    emit(&state.event_bus, Some(&selected.0), None, first);
    let first_frame = next_frame(&mut live).await;
    assert_eq!(first_frame["payload"]["note_id"], first.to_string());
    assert_eq!(first_frame["memory"], selected.0);
    assert_eq!(first_frame["archive_name"], selected.0);
    assert_eq!(first_frame["archive_schema"], selected.1);
    assert!(first_frame.get("tenant_id").is_none());
    let cursor = Uuid::parse_str(first_frame["event_id"].as_str().unwrap()).unwrap();
    drop(live);

    emit(&state.event_bus, Some(&other.0), None, Uuid::new_v4());
    emit(&state.event_bus, None, None, Uuid::new_v4());
    emit(
        &state.event_bus,
        Some(&selected.0),
        Some(Uuid::new_v4()),
        Uuid::new_v4(),
    );
    let second = Uuid::new_v4();
    state.event_bus.emit_with_context(
        ServerEvent::NoteDeleted { note_id: second },
        EventContext {
            memory: Some(selected.1.clone()),
            archive_schema: Some(selected.1.clone()),
            ..Default::default()
        },
    );
    let replay_response = app
        .oneshot(request(&selected.0, Some(cursor)))
        .await
        .unwrap();
    assert_eq!(replay_response.status(), StatusCode::OK);
    let mut replay = replay_response.into_body().into_data_stream();
    let replay_frame = next_frame(&mut replay).await;
    assert_eq!(replay_frame["payload"]["note_id"], second.to_string());
    assert_eq!(replay_frame["archive_name"], selected.0);
    assert_eq!(replay_frame["archive_schema"], selected.1);
    assert!(replay_frame.get("tenant_id").is_none());

    emit(&state.event_bus, Some(&other.0), None, Uuid::new_v4());
    emit(&state.event_bus, None, None, Uuid::new_v4());
    let third = Uuid::new_v4();
    emit(&state.event_bus, Some(&selected.0), None, third);
    assert_eq!(
        next_frame(&mut replay).await["payload"]["note_id"],
        third.to_string()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), replay.next())
            .await
            .is_err()
    );
    remove_archive_fixture(&state, &selected).await;
    remove_archive_fixture(&state, &other).await;
}

#[tokio::test]
async fn expired_replay_cursor_still_sends_resync_control() {
    let state = state().await;
    let selected = archive(&state).await;
    let response = router(state.clone(), identity("read", None, true), &selected)
        .oneshot(request(&selected.0, Some(Uuid::new_v4())))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let data = next_frame(&mut response.into_body().into_data_stream()).await;
    assert_eq!(data["buffer_capacity"], state.event_bus.replay_capacity());
    remove_archive_fixture(&state, &selected).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn single_tenant_external_oidc_mcp_and_read_are_admitted_and_audited() {
    let mut state = state().await;
    state.auth_mode = AuthMode::ExternalOidc;
    state.oidc_issuer = Some("https://issuer.example".into());
    state.audit_sink = Arc::new(matric_db::PostgresAuditSink::new(state.db.pool.clone()));
    let selected = archive(&state).await;
    for (scopes, human) in [("mcp", false), ("read", true)] {
        state.event_bus = Arc::new(EventBus::new(32));
        let tenant = Uuid::new_v4();
        sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES ($1,$2,'SSE Test','active')")
            .bind(tenant).bind(format!("sse-{tenant}")).execute(&state.db.pool).await.unwrap();
        let app = router(
            state.clone(),
            identity(scopes, Some(tenant), human),
            &selected,
        );
        let response = app
            .clone()
            .oneshot(request(&selected.0, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_event WHERE tenant_id=$1 AND outcome='Success' AND attrs->>'action_name'='event_stream:get'")
            .bind(tenant).fetch_one(&state.db.pool).await.unwrap();
        assert_eq!(
            count, 2,
            "stream and archive decisions must be durably audited"
        );
        emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
        let frame = next_frame(&mut response.into_body().into_data_stream()).await;
        assert_eq!(frame["tenant_id"], tenant.to_string());
        assert_eq!(frame["memory"], selected.0);
        assert_eq!(frame["archive_name"], selected.0);
        assert_eq!(frame["archive_schema"], selected.1);
        let cursor = Uuid::parse_str(frame["event_id"].as_str().unwrap()).unwrap();
        emit(
            &state.event_bus,
            Some(&selected.0),
            Some(Uuid::new_v4()),
            Uuid::new_v4(),
        );
        emit(
            &state.event_bus,
            Some("other-archive"),
            None,
            Uuid::new_v4(),
        );
        emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
        let replay = app
            .oneshot(request(&selected.0, Some(cursor)))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let mut replay = replay.into_body().into_data_stream();
        let replay_frame = next_frame(&mut replay).await;
        for key in ["memory", "archive_name", "archive_schema", "tenant_id"] {
            assert_eq!(frame[key], replay_frame[key]);
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), replay.next())
                .await
                .is_err()
        );
    }
    remove_archive_fixture(&state, &selected).await;
}

struct UnavailableAuditSink;

#[async_trait::async_trait]
impl AuditSink for UnavailableAuditSink {
    async fn emit(&self, _event: AuditEvent) -> Result<(), matric_core::audit::AuditError> {
        Err(matric_core::audit::AuditError::Sink(
            "synthetic-unavailable".into(),
        ))
    }
    async fn flush(&self) -> Result<(), matric_core::audit::AuditError> {
        Ok(())
    }
}

#[tokio::test]
async fn audit_failure_remains_fail_closed_with_service_unavailable() {
    let mut state = state().await;
    state.audit_sink = Arc::new(UnavailableAuditSink);
    let selected = archive(&state).await;
    let response = router(state.clone(), identity("mcp", None, false), &selected)
        .oneshot(request(&selected.0, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    remove_archive_fixture(&state, &selected).await;
}

#[tokio::test]
async fn single_and_multi_tenant_live_and_replay_envelopes_are_consistent() {
    let state = state().await;
    let tenant = Uuid::new_v4();
    let selected = (
        "synthetic-archive".to_string(),
        "archive_synthetic".to_string(),
    );
    for multi_tenant in [false, true] {
        let subscription = sse::Subscription {
            principal: identity("mcp", Some(tenant), false).principal,
            tenant_id: Some(tenant),
            multi_tenant,
            archive: Some(selected.clone()),
        };
        let envelope = EventEnvelope::with_context(
            ServerEvent::NoteDeleted {
                note_id: Uuid::new_v4(),
            },
            EventContext {
                memory: Some(if multi_tenant {
                    selected.1.clone()
                } else {
                    selected.0.clone()
                }),
                tenant_id: multi_tenant.then(|| tenant.to_string()),
                ..Default::default()
            },
        );
        let live = subscription
            .prepare(&state.db, envelope.clone())
            .await
            .unwrap();
        let replay = subscription
            .prepare(&state.db, envelope.clone())
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&live).unwrap(),
            serde_json::to_value(&replay).unwrap()
        );
        assert_eq!(
            live.memory.as_ref(),
            Some(if multi_tenant {
                &selected.1
            } else {
                &selected.0
            })
        );
        assert_eq!(live.archive_name.as_ref(), Some(&selected.0));
        assert_eq!(live.archive_schema.as_ref(), Some(&selected.1));
        assert_eq!(live.tenant_id, Some(tenant.to_string()));
        assert_eq!(live.payload_version, 1);
        let mut foreign = envelope.clone();
        foreign.tenant_id = Some(Uuid::new_v4().to_string());
        assert!(subscription.prepare(&state.db, foreign).await.is_none());
        if multi_tenant {
            let mut unscoped = envelope;
            unscoped.tenant_id = None;
            assert!(subscription.prepare(&state.db, unscoped).await.is_none());
        }
    }
}

#[test]
fn machine_credentials_do_not_gain_read_admission() {
    for human in [false, true] {
        let identity = identity("read", None, human);
        let input =
            sse::authorization_input(&identity.principal, sse::identity_is_human(&identity), None);
        assert_eq!(
            input.action.required_scopes,
            vec![if human { "read" } else { "mcp" }]
        );
    }
    let principal = AuthPrincipal::ApiKey {
        key_id: Uuid::new_v4(),
        scope: "read".into(),
    };
    let input = sse::authorization_input(&principal, false, None);
    assert_eq!(input.action.required_scopes, vec!["mcp"]);
    #[cfg(feature = "hosted-auth")]
    for human in [false, true] {
        let identity = identity("read", Some(Uuid::new_v4()), human);
        assert_eq!(sse::identity_is_human(&identity), human);
    }
}

#[test]
fn archive_selection_requires_the_same_tenant() {
    let principal = identity("read", None, true).principal;
    let tenant = Uuid::new_v4();
    assert!(sse::may_select_archive(
        &principal,
        Some(tenant),
        Some(tenant),
        Some(("synthetic", "archive_synthetic")),
        false
    ));
    assert!(!sse::may_select_archive(
        &principal,
        Some(tenant),
        Some(Uuid::new_v4()),
        Some(("synthetic", "archive_synthetic")),
        false
    ));
    assert!(!sse::may_select_archive(
        &principal,
        Some(tenant),
        None,
        Some(("synthetic", "archive_synthetic")),
        false
    ));
}

fn middleware_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/events", get(sse_events))
        .route(
            "/api/v1/events/tokens",
            post(handlers::event_tokens::mint_event_stream_token),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

async fn local_token(state: &AppState, scopes: &str, human: bool) -> String {
    let registration: ClientRegistrationRequest = serde_json::from_value(serde_json::json!({
        "client_name": "SSE admission fixture", "scope": scopes,
        "redirect_uris": ["http://localhost/callback"]
    }))
    .unwrap();
    let client = state.db.oauth.register_client(registration).await.unwrap();
    state
        .db
        .oauth
        .create_token_with_lifetime(
            &client.client_id,
            scopes,
            human.then_some("synthetic-reader"),
            false,
            chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
        .0
}

fn bearer_request(name: &str, token: &str, cursor: Option<Uuid>) -> axum::http::Request<Body> {
    let mut req = request(name, cursor);
    req.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    req
}

#[tokio::test]
async fn middleware_admits_human_read_and_mcp_but_refuses_machine_read() {
    let state = state().await;
    let selected = archive(&state).await;
    for (scopes, human, status) in [
        ("read", true, StatusCode::OK),
        ("read", false, StatusCode::FORBIDDEN),
        ("mcp", false, StatusCode::OK),
    ] {
        let token = local_token(&state, scopes, human).await;
        let response = middleware_router(state.clone())
            .oneshot(bearer_request(&selected.0, &token, None))
            .await
            .unwrap();
        assert_eq!(response.status(), status, "scopes={scopes}, human={human}");
    }
    remove_archive_fixture(&state, &selected).await;
}

async fn assert_stream_expires(
    state: &AppState,
    response: Response,
    selected: &(String, String),
    expiry: DateTime<Utc>,
) {
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
    assert_eq!(next_frame(&mut stream).await["archive_schema"], selected.1);
    tokio::time::sleep(
        (expiry - Utc::now()).to_std().unwrap_or_default() + std::time::Duration::from_millis(20),
    )
    .await;
    emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .is_none(),
        "delivery must stop at the validated credential expiry"
    );
}

#[tokio::test]
async fn local_oauth_and_expiring_api_key_delivery_stops_at_validated_expiry() {
    let mut state = state().await;
    state.oauth_token_lifetime = chrono::Duration::seconds(1);
    let selected = archive(&state).await;
    let token = local_token(&state, "read", true).await;
    let response = middleware_router(state.clone())
        .oneshot(bearer_request(&selected.0, &token, None))
        .await
        .unwrap();
    let expiry = state
        .db
        .oauth
        .get_token_expiry_info(&token)
        .await
        .unwrap()
        .unwrap()
        .expires_at;
    assert_stream_expires(&state, response, &selected, expiry).await;

    let key = state
        .db
        .oauth
        .create_api_key(
            serde_json::from_value(serde_json::json!({
                "name": "SSE expiry", "scope": "mcp", "expires_in_days": 1
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let expiry = Utc::now() + chrono::Duration::seconds(1);
    sqlx::query("UPDATE api_key SET expires_at=$1 WHERE id=$2")
        .bind(expiry)
        .bind(key.id)
        .execute(&state.db.pool)
        .await
        .unwrap();
    let response = middleware_router(state.clone())
        .oneshot(bearer_request(&selected.0, &key.api_key, None))
        .await
        .unwrap();
    assert_stream_expires(&state, response, &selected, expiry).await;
    remove_archive_fixture(&state, &selected).await;
}

#[cfg(feature = "hosted-auth")]
struct OidcFixture(fortemi_auth_core::AuthContext);

#[cfg(feature = "hosted-auth")]
#[async_trait::async_trait]
impl HostedAuthenticator for OidcFixture {
    async fn authenticate(
        &self,
        token: &str,
    ) -> Result<fortemi_auth_core::AuthContext, fortemi_auth_core::AuthError> {
        if token != "synthetic-oidc" {
            return Err(fortemi_auth_core::AuthError::MalformedToken);
        }
        Ok(self.0.clone())
    }
}

#[cfg(feature = "hosted-auth")]
async fn oidc_state(scopes: &str) -> (AppState, Uuid) {
    let mut state = state().await;
    let tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES ($1,$2,'SSE Test','active')")
        .bind(tenant).bind(format!("sse-{tenant}")).execute(&state.db.pool).await.unwrap();
    state.auth_mode = AuthMode::ExternalOidc;
    state.oidc_issuer = Some("https://issuer.example".into());
    state.audit_sink = Arc::new(matric_db::PostgresAuditSink::new(state.db.pool.clone()));
    state.hosted_auth = Some(Arc::new(OidcFixture(
        identity(scopes, Some(tenant), true)
            .canonical_context
            .unwrap(),
    )));
    (state, tenant)
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn oidc_and_pat_delivery_stops_at_validated_expiry() {
    let (mut state, tenant) = oidc_state("read").await;
    let selected = archive(&state).await;
    let mut context = identity("read", Some(tenant), true)
        .canonical_context
        .unwrap();
    context.expires_at = Utc::now() + chrono::Duration::seconds(1);
    let expiry = context.expires_at;
    state.hosted_auth = Some(Arc::new(OidcFixture(context)));
    let response = middleware_router(state.clone())
        .oneshot(bearer_request(&selected.0, "synthetic-oidc", None))
        .await
        .unwrap();
    assert_stream_expires(&state, response, &selected, expiry).await;

    let user = state
        .app_users
        .upsert_oidc(
            matric_db::AppUserUpsert {
                tenant_id: tenant,
                iss: "https://issuer.example".into(),
                sub: "synthetic-pat-reader".into(),
                email: None,
                email_verified: false,
                display_name: None,
                groups: vec![],
                current_scopes: vec!["read".into()],
                kind: "user".into(),
                azp: None,
            },
            true,
        )
        .await
        .unwrap();
    let repo = matric_db::PgPersonalAccessTokenRepository::new(
        state.db.pool.clone(),
        matric_db::PersonalAccessTokenCrypto::for_tests(),
    );
    let expiry = Utc::now() + chrono::Duration::seconds(1);
    let pat = repo
        .create(tenant, user.id, "SSE expiry", &["read".into()], expiry)
        .await
        .unwrap();
    state.personal_access_tokens = Some(repo);
    let response = middleware_router(state.clone())
        .oneshot(bearer_request(&selected.0, &pat.token, None))
        .await
        .unwrap();
    assert_stream_expires(&state, response, &selected, expiry).await;
    remove_archive_fixture(&state, &selected).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn query_token_restores_oidc_identity_for_postgres_audit_and_expires() {
    let (mut state, tenant) = oidc_state("admin").await;
    let selected = archive(&state).await;
    let redis_url =
        std::env::var("REDIS_URL").expect("CI must provision disposable Redis and REDIS_URL");
    state.ingest_token_store = matric_api::services::IngestTokenStore::with_ttl(&redis_url, 2)
        .await
        .unwrap();
    let app = middleware_router(state.clone());
    let minted = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/v1/events/tokens")
                .header(header::AUTHORIZATION, "Bearer synthetic-oidc")
                .header("X-Fortemi-Memory", &selected.0)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(minted.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(minted.into_body(), 4096)
        .await
        .unwrap();
    let minted: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = minted["token"].as_str().unwrap();
    let stored = state
        .ingest_token_store
        .validate_event_stream(token)
        .await
        .unwrap();
    let verified = stored.event_identity.as_ref().unwrap();
    assert_eq!(verified.tenant_id, Some(tenant));
    assert!(verified.user_id.is_some());
    assert!(sse::revalidate_query_identity(&state, verified.clone())
        .await
        .is_some());
    assert!(verified.human);
    assert!(
        matches!(&verified.principal, AuthPrincipal::OAuthClient { user_id: Some(user), .. } if user == "synthetic-reader")
    );
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/api/v1/events?token={token}&memory={}",
                    selected.0
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_event WHERE tenant_id=$1 AND outcome='Success' AND attrs->>'action_name'='event_stream:get' AND principal_id='oauth_user:synthetic-reader'")
        .bind(tenant).fetch_one(&state.db.pool).await.unwrap();
    assert_eq!(
        count, 2,
        "query stream and archive decisions must be durably audited under the minting principal"
    );
    assert_stream_expires(&state, response, &selected, stored.expires_at.unwrap()).await;
    let expired = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/api/v1/events?token={token}&memory={}",
                    selected.0
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
    remove_archive_fixture(&state, &selected).await;
}

#[tokio::test]
async fn worker_archive_events_reach_scoped_live_replay_and_admin_monitoring() {
    let state = state().await;
    let selected = state
        .db
        .archives
        .create_archive_schema(&format!("sseworker_{}", Uuid::new_v4().simple()), None)
        .await
        .unwrap();
    let selected = (selected.name, selected.schema_name);
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let note = state
        .db
        .for_schema(&selected.1)
        .unwrap()
        .query(move |tx| {
            Box::pin(async move {
                notes
                    .insert_tx(
                        tx,
                        CreateNoteRequest {
                            content: "Synthetic worker note".into(),
                            format: "markdown".into(),
                            source: "test".into(),
                            collection_id: None,
                            tags: None,
                            metadata: None,
                            document_type_id: None,
                            title: None,
                        },
                    )
                    .await
            })
        })
        .await
        .unwrap();
    let job_id = state
        .db
        .jobs
        .queue(
            Some(note),
            JobType::Embedding,
            0,
            Some(serde_json::json!({"schema": selected.1})),
            None,
        )
        .await
        .unwrap();
    let reader = local_token(&state, "read", true).await;
    let admin = local_token(&state, "admin", false).await;
    let app = middleware_router(state.clone());
    let scoped = app
        .clone()
        .oneshot(bearer_request(&selected.0, &reader, None))
        .await
        .unwrap();
    assert_eq!(scoped.status(), StatusCode::OK);
    let mut scoped = scoped.into_body().into_data_stream();
    let mut broad_request = bearer_request(&selected.0, &admin, None);
    *broad_request.uri_mut() = "/api/v1/events".parse().unwrap();
    let broad = app.clone().oneshot(broad_request).await.unwrap();
    assert_eq!(broad.status(), StatusCode::OK);
    let mut broad = broad.into_body().into_data_stream();
    worker_events::emit_job_event(
        &state.event_bus,
        &state.db,
        job_id,
        ServerEvent::JobQueued {
            job_id,
            job_type: "Embedding".into(),
            note_id: Some(note),
        },
    )
    .await;
    let anchor = next_frame(&mut scoped).await;
    assert_eq!(next_frame(&mut broad).await["event_id"], anchor["event_id"]);
    let (sender, receiver) = tokio::sync::broadcast::channel(32);
    let bridge = tokio::spawn(worker_events::bridge_worker_events(
        receiver,
        state.event_bus.clone(),
        state.db.clone(),
    ));
    let send_cascade = || {
        sender
            .send(WorkerEvent::JobStarted {
                job_id,
                job_type: JobType::Embedding,
            })
            .unwrap();
        sender
            .send(WorkerEvent::JobProgress {
                job_id,
                percent: 40,
                message: Some("Synthetic progress".into()),
            })
            .unwrap();
        sender
            .send(WorkerEvent::JobCompleted {
                job_id,
                job_type: JobType::Embedding,
            })
            .unwrap();
    };
    send_cascade();
    for expected in [
        "job.started",
        "job.progress",
        "job.completed",
        "index.embedding.updated",
        "note.updated",
    ] {
        let live = next_frame(&mut scoped).await;
        assert_eq!(live["event_type"], expected);
        assert_eq!(live["memory"], selected.0);
        assert_eq!(live["archive_schema"], selected.1);
        assert_eq!(next_frame(&mut broad).await["event_id"], live["event_id"]);
    }
    drop(scoped);
    let cursor = Uuid::parse_str(anchor["event_id"].as_str().unwrap()).unwrap();
    let replay = app
        .clone()
        .oneshot(bearer_request(&selected.0, &reader, Some(cursor)))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    let mut replay = replay.into_body().into_data_stream();
    for expected in [
        "job.started",
        "job.progress",
        "job.completed",
        "index.embedding.updated",
        "note.updated",
    ] {
        let frame = next_frame(&mut replay).await;
        assert_eq!(frame["event_type"], expected);
        assert_eq!(frame["archive_name"], selected.0);
        assert_eq!(frame["archive_schema"], selected.1);
    }
    state.event_bus.emit(ServerEvent::QueueStatus {
        total_jobs: 3,
        running: 1,
        pending: 2,
    });
    assert_eq!(next_frame(&mut broad).await["event_type"], "queue.status");
    // Unattributed archive events must not be mistaken for system-wide events.
    state.event_bus.emit(ServerEvent::NoteDeleted {
        note_id: Uuid::new_v4(),
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), broad.next())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), replay.next())
            .await
            .is_err()
    );
    let mut broad_request = bearer_request(&selected.0, &admin, Some(cursor));
    *broad_request.uri_mut() = "/api/v1/events".parse().unwrap();
    let broad_replay = app.oneshot(broad_request).await.unwrap();
    assert_eq!(broad_replay.status(), StatusCode::OK);
    let mut broad_replay = broad_replay.into_body().into_data_stream();
    for expected in [
        "job.started",
        "job.progress",
        "job.completed",
        "index.embedding.updated",
        "note.updated",
        "queue.status",
    ] {
        assert_eq!(next_frame(&mut broad_replay).await["event_type"], expected);
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), broad_replay.next())
            .await
            .is_err()
    );
    drop(broad_replay);
    drop(sender);
    bridge.await.unwrap();
    drop(broad);
    drop(replay);
    sqlx::query("DELETE FROM job_queue WHERE id=$1")
        .bind(job_id)
        .execute(&state.db.pool)
        .await
        .unwrap();
    state
        .db
        .archives
        .drop_archive_schema(&selected.0)
        .await
        .unwrap();
}

#[test]
fn openapi_sse_authentication_is_distinct_from_middleware_exemption() {
    let yaml = openapi_yaml_with_problem_contract();
    validate_generated_openapi(&yaml).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
    let operation = &value["paths"]["/api/v1/events"]["get"];
    assert_eq!(
        operation["security"],
        serde_yaml::to_value(serde_json::json!([
            {"bearerAuth": []}, {"eventStreamToken": []}
        ]))
        .unwrap()
    );
    assert_eq!(
        operation["x-fortemi-anonymous-access"]["mode"].as_str(),
        Some("anonymous-local")
    );
    assert_eq!(
        value["components"]["securitySchemes"]["eventStreamToken"]["in"].as_str(),
        Some("query")
    );
    assert_eq!(
        value["paths"]["/health"]["get"]["security"]
            .as_sequence()
            .unwrap()
            .len(),
        0
    );
    assert!(!value["paths"]["/api/v1/notes"]["get"]["security"]
        .as_sequence()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn query_token_cannot_outlive_local_oauth_minting_credential() {
    let mut state = state().await;
    state.oauth_token_lifetime = chrono::Duration::seconds(1);
    state.oauth_mcp_token_lifetime = chrono::Duration::seconds(1);
    let selected = archive(&state).await;
    let redis_url =
        std::env::var("REDIS_URL").expect("CI must provision disposable Redis and REDIS_URL");
    state.ingest_token_store = matric_api::services::IngestTokenStore::with_ttl(&redis_url, 60)
        .await
        .unwrap();
    let bearer = local_token(&state, "admin", true).await;
    let app = middleware_router(state.clone());
    let minted = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/v1/events/tokens")
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                .header("X-Fortemi-Memory", &selected.0)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(minted.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(minted.into_body(), 4096)
        .await
        .unwrap();
    let minted: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = minted["token"].as_str().unwrap();
    let stored = state
        .ingest_token_store
        .validate_event_stream(token)
        .await
        .unwrap();
    let parent_expiry = state
        .db
        .oauth
        .get_token_expiry_info(&bearer)
        .await
        .unwrap()
        .unwrap()
        .expires_at;
    assert_eq!(
        stored.expires_at,
        stored.event_identity.as_ref().unwrap().expires_at
    );
    // PostgreSQL timestamps round the validator's nanoseconds to microseconds.
    assert!(
        (stored.expires_at.unwrap() - parent_expiry)
            .num_microseconds()
            .unwrap()
            .abs()
            <= 1
    );
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/api/v1/events?token={token}&memory={}",
                    selected.0
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_stream_expires(&state, response, &selected, parent_expiry).await;
    remove_archive_fixture(&state, &selected).await;
}

async fn mint_query_token(app: &Router, name: &str, bearer: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/v1/events/tokens")
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                .header("X-Fortemi-Memory", name)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["token"]
        .as_str()
        .unwrap()
        .into()
}

fn query_request(name: &str, token: &str, cursor: Option<Uuid>) -> axum::http::Request<Body> {
    let mut req = request(name, cursor);
    *req.uri_mut() = format!("/api/v1/events?token={token}&memory={name}")
        .parse()
        .unwrap();
    req.headers_mut().remove(header::AUTHORIZATION);
    req
}

async fn query_replay_cursor(
    state: &AppState,
    app: &Router,
    selected: &(String, String),
    token: &str,
) -> Uuid {
    let response = app
        .clone()
        .oneshot(query_request(&selected.0, token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
    let frame = next_frame(&mut stream).await;
    let cursor = Uuid::parse_str(frame["event_id"].as_str().unwrap()).unwrap();
    drop(stream);
    emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
    cursor
}

async fn query_store(state: &mut AppState) {
    let redis_url =
        std::env::var("REDIS_URL").expect("CI must provision disposable Redis and REDIS_URL");
    state.ingest_token_store = matric_api::services::IngestTokenStore::with_ttl(&redis_url, 60)
        .await
        .unwrap();
}

#[tokio::test]
async fn query_token_reconnect_rejects_revoked_local_oauth_parent() {
    let mut state = state().await;
    query_store(&mut state).await;
    let selected = archive(&state).await;
    let bearer = local_token(&state, "admin", true).await;
    let app = middleware_router(state.clone());
    let token = mint_query_token(&app, &selected.0, &bearer).await;
    let cursor = query_replay_cursor(&state, &app, &selected, &token).await;
    assert!(state.db.oauth.revoke_token(&bearer, None).await.unwrap());
    let response = app
        .oneshot(query_request(&selected.0, &token, Some(cursor)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    remove_archive_fixture(&state, &selected).await;
}

#[tokio::test]
async fn query_token_reconnect_rejects_api_key_scope_reduction_and_revocation() {
    let mut state = state().await;
    query_store(&mut state).await;
    let selected = archive(&state).await;
    let key = state
        .db
        .oauth
        .create_api_key(
            serde_json::from_value(serde_json::json!({
                "name": "Query reconnect", "scope": "admin"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let app = middleware_router(state.clone());
    let token = mint_query_token(&app, &selected.0, &key.api_key).await;
    let cursor = query_replay_cursor(&state, &app, &selected, &token).await;
    sqlx::query("UPDATE api_key SET scope='read' WHERE id=$1")
        .bind(key.id)
        .execute(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(query_request(&selected.0, &token, Some(cursor)))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE api_key SET scope='admin' WHERE id=$1")
        .bind(key.id)
        .execute(&state.db.pool)
        .await
        .unwrap();
    assert!(state.db.oauth.revoke_api_key(key.id).await.unwrap());
    assert_eq!(
        app.oneshot(query_request(&selected.0, &token, Some(cursor)))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    remove_archive_fixture(&state, &selected).await;
}

#[cfg(feature = "hosted-auth")]
async fn changed_oidc_query_access(change: &str, pat: bool) {
    let (mut state, tenant) = oidc_state("admin").await;
    query_store(&mut state).await;
    let selected = archive(&state).await;
    let app = middleware_router(state.clone());
    let oidc_token = mint_query_token(&app, &selected.0, "synthetic-oidc").await;
    let stored = state
        .ingest_token_store
        .validate_event_stream(&oidc_token)
        .await
        .unwrap();
    let user = stored
        .event_identity
        .unwrap()
        .user_id
        .expect("stable app_user id");
    let (app, token, parent) = if pat {
        let repo = matric_db::PgPersonalAccessTokenRepository::new(
            state.db.pool.clone(),
            matric_db::PersonalAccessTokenCrypto::for_tests(),
        );
        let parent = repo
            .create(
                tenant,
                user,
                "Query reconnect",
                &["admin".into()],
                Utc::now() + chrono::Duration::minutes(5),
            )
            .await
            .unwrap();
        state.personal_access_tokens = Some(repo);
        let app = middleware_router(state.clone());
        let token = mint_query_token(&app, &selected.0, &parent.token).await;
        (app, token, Some(parent.record.id))
    } else {
        (app, oidc_token, None)
    };
    let cursor = query_replay_cursor(&state, &app, &selected, &token).await;
    let pending_replay = app
        .clone()
        .oneshot(query_request(&selected.0, &token, Some(cursor)))
        .await
        .unwrap();
    assert_eq!(pending_replay.status(), StatusCode::OK);
    let mut pending_replay = pending_replay.into_body().into_data_stream();
    let mut active = app
        .clone()
        .oneshot(query_request(&selected.0, &token, None))
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    match change {
        "disabled" => {
            state
                .app_users
                .set_status(tenant, user, "disabled")
                .await
                .unwrap()
                .unwrap();
        }
        "user_scopes" => {
            sqlx::query(
                "UPDATE app_user SET current_scopes=ARRAY['profile'] WHERE tenant_id=$1 AND id=$2",
            )
            .bind(tenant)
            .bind(user)
            .execute(&state.db.pool)
            .await
            .unwrap();
        }
        "parent_scopes" => {
            sqlx::query("UPDATE personal_access_token SET scopes=ARRAY['profile'] WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(parent.unwrap()).execute(&state.db.pool).await.unwrap();
        }
        "revoked" => {
            assert!(state
                .personal_access_tokens
                .as_ref()
                .unwrap()
                .revoke_for_admin(tenant, parent.unwrap())
                .await
                .unwrap());
        }
        _ => unreachable!(),
    }
    for cursor in [None, Some(cursor)] {
        let response = app
            .clone()
            .oneshot(query_request(&selected.0, &token, cursor))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{change}: cursor={cursor:?}"
        );
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), pending_replay.next())
            .await
            .unwrap()
            .is_none(),
        "a replay admitted before {change} must revalidate before releasing its frames"
    );
    emit(&state.event_bus, Some(&selected.0), None, Uuid::new_v4());
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), active.next())
            .await
            .unwrap()
            .is_none()
    );
    drop(active);
    remove_archive_fixture(&state, &selected).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn query_token_reconnect_rejects_disabled_oidc_user() {
    changed_oidc_query_access("disabled", false).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn query_token_reconnect_rejects_oidc_user_scope_reduction() {
    changed_oidc_query_access("user_scopes", false).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn query_token_reconnect_rejects_disabled_pat_user() {
    changed_oidc_query_access("disabled", true).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn query_token_reconnect_rejects_revoked_pat_parent() {
    changed_oidc_query_access("revoked", true).await;
}

#[cfg(feature = "hosted-auth")]
#[tokio::test]
async fn query_token_reconnect_rejects_pat_scope_reduction() {
    changed_oidc_query_access("parent_scopes", true).await;
    changed_oidc_query_access("user_scopes", true).await;
}

#[tokio::test]
async fn webhook_payload_preserves_named_single_tenant_archive_memory() {
    let state = state().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<serde_json::Value>(2);
    let app = Router::new()
        .route(
            "/",
            post(
                |State(sender): State<tokio::sync::mpsc::Sender<serde_json::Value>>,
                 Json(payload): Json<serde_json::Value>| async move {
                    sender.send(payload).await.unwrap();
                    StatusCode::NO_CONTENT
                },
            ),
        )
        .with_state(sender);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let webhook = state
        .db
        .webhooks
        .create(matric_core::CreateWebhookRequest {
            url: format!("http://{address}/"),
            secret: None,
            events: vec!["NoteDeleted".into()],
            max_retries: 0,
        })
        .await
        .unwrap();
    let dispatcher = tokio::spawn(webhook_dispatcher(
        state.event_bus.clone(),
        state.db.clone(),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.event_bus.subscriber_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let note = Uuid::new_v4();
    state.event_bus.emit_with_context(
        ServerEvent::NoteDeleted { note_id: note },
        event_context_for(&ArchiveContext {
            name: Some("research".into()),
            schema: "archive_research".into(),
            is_default: false,
        }),
    );
    let payload = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(payload["memory"], "research");
    assert_eq!(payload["archive_name"], "research");
    assert_eq!(payload["archive_schema"], "archive_research");
    assert_eq!(payload["payload"]["note_id"], note.to_string());
    dispatcher.abort();
    server.abort();
    state.db.webhooks.delete(webhook).await.unwrap();
}

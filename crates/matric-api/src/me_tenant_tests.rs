//! `/me` shares verified tenant ownership with production live and replay SSE.
use super::*;
use crate::sse_tests::{assert_me_tenant_matches_events, next_frame};
use futures::StreamExt;

fn pat_repo(pool: &sqlx::PgPool) -> matric_db::PgPersonalAccessTokenRepository {
    matric_db::PgPersonalAccessTokenRepository::new(
        pool.clone(),
        matric_db::PersonalAccessTokenCrypto::for_tests(),
    )
}

fn bearer_request(path: &str, token: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn create_pat(
    admin: &sqlx::PgPool,
    state: &AppState,
    oidc_token: &str,
) -> matric_db::CreatedPersonalAccessToken {
    let identity = validate_bearer_identity(state, oidc_token, None)
        .await
        .unwrap();
    let principal = identity.request_principal.unwrap();
    // Only fixture provisioning uses the owner pool; authentication uses runtime.
    pat_repo(admin)
        .create(
            principal.tenant_id.unwrap(),
            principal.user_id.unwrap(),
            "Hosted PAT fixture",
            &["admin".into()],
            Utc::now() + chrono::Duration::minutes(5),
        )
        .await
        .unwrap()
}

async fn setup(
    admin: &sqlx::PgPool,
    hosted: bool,
) -> (
    AppState,
    String,
    Uuid,
    Arc<external_oidc_test_issuer::Issuer>,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let tenant = Uuid::new_v4();
    if hosted {
        matric_api::admin_bootstrap::bootstrap_tenant(
            admin,
            &matric_api::admin_bootstrap::BootstrapRequest {
                slug: format!("me-tenant-{tenant}"),
                display_name: "Me tenant fixture".into(),
                tenant_id: tenant,
                reactivate: false,
            },
            false,
            "fortemi:tenant_id",
        )
        .await
        .unwrap();
    }
    let url = runtime_url(admin, tenant).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        if hosted {
            startup::Tenancy::Hosted
        } else {
            startup::Tenancy::External(tenant)
        },
    )
    .await
    .unwrap();
    let mut state = super::super::super::tests::build_call_api_test_state(db, &url).await;
    state.require_auth = true;
    state.multi_tenant = hosted;
    state.auth_mode = if hosted {
        AuthMode::HostedMultiTenant
    } else {
        AuthMode::ExternalOidc
    };
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    if hosted {
        let redis = std::env::var("REDIS_URL").expect("explicit disposable Redis");
        state.hosted_quota = Some(Arc::new(
            RedisRequestQuotaGate::connect(
                &redis,
                RequestQuotaPolicy {
                    id: format!("me-tenant-{tenant}"),
                    version: 1,
                    limit: 600,
                    window: std::time::Duration::from_secs(60),
                },
            )
            .await
            .unwrap(),
        ));
    }
    let issuer = external_oidc_test_issuer::issuer();
    state.oidc_issuer = Some(issuer.url.clone());
    let mut config = issuer.config(tenant);
    if hosted {
        config.default_tenant_id = None;
    } else {
        state.external_oidc = Some(ExternalOidcConfig {
            audiences: ["https://api.example".into()].into_iter().collect(),
            default_tenant_id: tenant,
            allow_multi_audience: false,
            allow_token_scopes: false,
            allow_legacy_tokens: true,
            legacy_scope_ceiling: ["read".into(), "write".into(), "admin".into(), "mcp".into()]
                .into_iter()
                .collect(),
            max_token_lifetime_seconds: 3600,
        });
    }
    state.hosted_auth = Some(
        matric_api::hosted_auth::build_clerk_authenticator_with_claim_policy_options(
            &config,
            state.db.pool.clone(),
            &matric_api::hosted_claim_policy::ClaimPolicyLoadOptions {
                required: true,
                allow_token_scopes: false,
                forbidden_audiences: &Default::default(),
            },
        )
        .unwrap(),
    );
    let token = issuer.token(if hosted {
        serde_json::json!({"fortemi:tenant_id": tenant})
    } else {
        serde_json::json!({})
    });
    validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    (state, token, tenant, issuer)
}

#[sqlx::test(migrations = false)]
async fn me_hosted_tenant_matches_live_and_replay(admin: sqlx::PgPool) {
    let (state, token, tenant, _issuer) = setup(&admin, true).await;
    let app = crate::production_router(state.clone(), None);
    let body = assert_me_tenant_matches_events(&app, &state.event_bus, &token, tenant).await;
    assert_eq!(body["credential_class"], "oidc");
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_hosted_pat_tenant_matches_live_and_replay(admin: sqlx::PgPool) {
    let (mut state, token, tenant, _issuer) = setup(&admin, true).await;
    let restricted: bool = sqlx::query_scalar(
        "SELECT NOT rolsuper AND NOT rolbypassrls AND NOT rolcreatedb AND NOT rolcreaterole
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&state.db.pool)
    .await
    .unwrap();
    assert!(
        restricted,
        "production requests must use the restricted runtime role"
    );
    let bound: Option<String> =
        sqlx::query_scalar("SELECT current_setting('app.current_tenant', true)")
            .fetch_one(&state.db.pool)
            .await
            .unwrap();
    assert!(bound.is_none_or(|value| value.is_empty()));
    let pat = create_pat(&admin, &state, &token).await;
    state.personal_access_tokens = Some(pat_repo(&state.db.pool));
    let app = crate::production_router(state.clone(), None);
    let mut statuses = Vec::new();
    for path in ["/api/v1/me", "/api/v1/events"] {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .header(header::AUTHORIZATION, format!("Bearer {}", pat.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        eprintln!(
            "hosted PAT restricted runtime {path}: {}",
            response.status()
        );
        statuses.push(response.status());
    }
    assert_eq!(statuses, vec![StatusCode::OK, StatusCode::OK]);
    let body = assert_me_tenant_matches_events(&app, &state.event_bus, &pat.token, tenant).await;
    assert_eq!(body["credential_class"], "pat");
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_hosted_pat_lookup_is_narrow_and_runtime_only(admin: sqlx::PgPool) {
    let (mut state, jwt, tenant, _issuer) = setup(&admin, true).await;
    let pat = create_pat(&admin, &state, &jwt).await;
    state.personal_access_tokens = Some(pat_repo(&state.db.pool));
    let hardened: bool = sqlx::query_scalar(
        "SELECT prosecdef AND proisstrict
            AND proconfig @> ARRAY['search_path=pg_catalog', 'row_security=off']
            AND NOT EXISTS (
                SELECT 1 FROM aclexplode(COALESCE(proacl, acldefault('f', proowner)))
                WHERE grantee=0 AND privilege_type='EXECUTE')
         FROM pg_proc WHERE oid='public.resolve_pat_tenant(text)'::regprocedure",
    )
    .fetch_one(&admin)
    .await
    .unwrap();
    assert!(hardened);
    let stranger = format!("pat_stranger_{}", tenant.simple());
    sqlx::query(&format!("CREATE ROLE {stranger} NOLOGIN"))
        .execute(&admin)
        .await
        .unwrap();
    let stranger_can_execute: bool = sqlx::query_scalar(
        "SELECT has_function_privilege($1, 'public.resolve_pat_tenant(text)', 'EXECUTE')",
    )
    .bind(&stranger)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert!(!stranger_can_execute);
    let result_shape: String = sqlx::query_scalar(
        "SELECT pg_get_function_result('public.resolve_pat_tenant(text)'::regprocedure)",
    )
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(result_shape, "TABLE(tenant_id uuid, active boolean)");
    let hash: String =
        sqlx::query_scalar("SELECT token_hash FROM public.personal_access_token WHERE id=$1")
            .bind(pat.record.id)
            .fetch_one(&admin)
            .await
            .unwrap();
    let owner: (Uuid, bool) = sqlx::query_as("SELECT * FROM public.resolve_pat_tenant($1)")
        .bind(hash)
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(owner, (tenant, true));
    let unknown: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT * FROM public.resolve_pat_tenant($1)")
            .bind("0".repeat(64))
            .fetch_optional(&state.db.pool)
            .await
            .unwrap();
    assert!(unknown.is_none());
    let foreign = Uuid::new_v4();
    // Even a bypass pool cannot make a mismatched explicit hint validate.
    for repo in [pat_repo(&admin), pat_repo(&state.db.pool)] {
        assert!(repo
            .validate_for_tenant(foreign, &pat.token, 30, None)
            .await
            .unwrap()
            .is_none());
    }
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_hosted_pat_invalid_credentials_are_indistinguishable(admin: sqlx::PgPool) {
    let (mut state, jwt, tenant, _issuer) = setup(&admin, true).await;
    let revoked = create_pat(&admin, &state, &jwt).await;
    assert!(pat_repo(&admin)
        .revoke_for_admin(tenant, revoked.record.id)
        .await
        .unwrap());
    let expired = create_pat(&admin, &state, &jwt).await;
    sqlx::query("UPDATE public.personal_access_token SET expires_at=now()-interval '1 second' WHERE tenant_id=$1 AND id=$2")
        .bind(tenant).bind(expired.record.id).execute(&admin).await.unwrap();
    let suspended = create_pat(&admin, &state, &jwt).await;
    sqlx::query(
        "UPDATE public.personal_access_token SET status='suspended' WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant)
    .bind(suspended.record.id)
    .execute(&admin)
    .await
    .unwrap();
    let valid = create_pat(&admin, &state, &jwt).await;
    state.personal_access_tokens = Some(pat_repo(&state.db.pool));
    let app = crate::production_router(state.clone(), None);
    let unknown = matric_db::PgPersonalAccessTokenRepository::generate_token();
    let mut expected = None;
    for token in [&revoked.token, &expired.token, &suspended.token, &unknown] {
        for path in ["/api/v1/me", "/api/v1/events"] {
            let response = app
                .clone()
                .oneshot(bearer_request(path, token))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            let problem = (
                body["type"].clone(),
                body["title"].clone(),
                body["detail"].clone(),
            );
            if let Some(ref expected) = expected {
                assert_eq!(&problem, expected);
            } else {
                expected = Some(problem);
            }
        }
    }
    // Tenant activity is resolved before any PAT/user data is read.
    sqlx::query("UPDATE public.tenant_registry SET status='suspended' WHERE id=$1")
        .bind(tenant)
        .execute(&admin)
        .await
        .unwrap();
    for path in ["/api/v1/me", "/api/v1/events"] {
        assert_eq!(
            app.clone()
                .oneshot(bearer_request(path, &valid.token))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_hosted_pat_cannot_read_another_tenants_live_or_replayed_events(admin: sqlx::PgPool) {
    let (mut state, jwt, tenant, issuer) = setup(&admin, true).await;
    let pat = create_pat(&admin, &state, &jwt).await;
    let foreign = Uuid::new_v4();
    matric_api::admin_bootstrap::bootstrap_tenant(
        &admin,
        &matric_api::admin_bootstrap::BootstrapRequest {
            slug: format!("me-foreign-{foreign}"),
            display_name: "Foreign PAT tenant".into(),
            tenant_id: foreign,
            reactivate: false,
        },
        false,
        "fortemi:tenant_id",
    )
    .await
    .unwrap();
    let foreign_jwt = issuer.token(serde_json::json!({"fortemi:tenant_id": foreign}));
    let foreign_pat = create_pat(&admin, &state, &foreign_jwt).await;
    state.personal_access_tokens = Some(pat_repo(&state.db.pool));
    let app = crate::production_router(state.clone(), None);
    let stream_request = |token: &str, cursor: Option<&str>| {
        let mut request = axum::http::Request::builder()
            .uri(format!(
                "/api/v1/events?memory=public&types=note.created&tenant_id={foreign}"
            ))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header("X-Tenant-ID", foreign.to_string());
        if let Some(cursor) = cursor {
            request = request.header("Last-Event-ID", cursor);
        }
        request.body(Body::empty()).unwrap()
    };
    let response = app
        .clone()
        .oneshot(stream_request(&pat.token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut live = response.into_body().into_data_stream();
    let response = app
        .clone()
        .oneshot(stream_request(&foreign_pat.token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut foreign_live = response.into_body().into_data_stream();
    let mut frames = Vec::new();
    let mut foreign_frames = Vec::new();
    for _ in 0..2 {
        for token in [&foreign_pat.token, &pat.token] {
            let response = app.clone().oneshot(
                axum::http::Request::builder().method(Method::POST).uri("/api/v1/notes")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"content":"PAT isolation fixture","revision_mode":"none","pipeline":[]}"#)).unwrap()
            ).await.unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let created: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            let frame = if token == &pat.token {
                next_frame(&mut live).await
            } else {
                next_frame(&mut foreign_live).await
            };
            assert_eq!(frame["payload"]["note_id"], created["id"]);
            assert_eq!(
                frame["tenant_id"],
                if token == &pat.token { tenant } else { foreign }.to_string()
            );
            if token == &pat.token {
                frames.push(frame);
            } else {
                foreign_frames.push(frame);
            }
        }
    }
    drop(live);
    let response = app
        .clone()
        .oneshot(stream_request(&pat.token, frames[0]["event_id"].as_str()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut replay = response.into_body().into_data_stream();
    assert_eq!(next_frame(&mut replay).await, frames[1]);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), replay.next())
            .await
            .is_err()
    );
    // Even with A's cursor, B receives only its own later event.
    let response = app
        .oneshot(stream_request(
            &foreign_pat.token,
            frames[0]["event_id"].as_str(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut foreign_replay = response.into_body().into_data_stream();
    assert_eq!(next_frame(&mut foreign_replay).await, foreign_frames[1]);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), foreign_replay.next())
            .await
            .is_err()
    );
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_external_oidc_jwt_tenant_matches_live_and_replay(admin: sqlx::PgPool) {
    let (state, token, tenant, issuer) = setup(&admin, false).await;
    let app = crate::production_router(state.clone(), None);
    let body = assert_me_tenant_matches_events(&app, &state.event_bus, &token, tenant).await;
    assert_eq!(body["credential_class"], "oidc");
    // A signed conflicting claim is rejected and cannot disclose another owner.
    let foreign = issuer.token(serde_json::json!({"fortemi:tenant_id": Uuid::new_v4()}));
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/me")
                .header(header::AUTHORIZATION, format!("Bearer {foreign}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_external_oidc_pat_tenant_matches_live_and_replay(admin: sqlx::PgPool) {
    let (mut state, token, tenant, _issuer) = setup(&admin, false).await;
    let identity = validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    let user = identity.request_principal.unwrap().user_id.unwrap();
    let repo = matric_db::PgPersonalAccessTokenRepository::new(
        state.db.pool.clone(),
        matric_db::PersonalAccessTokenCrypto::for_tests(),
    );
    let pat = repo
        .create(
            tenant,
            user,
            "Me tenant fixture",
            &["admin".into()],
            Utc::now() + chrono::Duration::minutes(5),
        )
        .await
        .unwrap();
    state.personal_access_tokens = Some(repo);
    let app = crate::production_router(state.clone(), None);
    let body = assert_me_tenant_matches_events(&app, &state.event_bus, &pat.token, tenant).await;
    assert_eq!(body["credential_class"], "pat");
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn me_external_oidc_legacy_tenant_matches_live_and_replay(admin: sqlx::PgPool) {
    let (state, _, tenant, _issuer) = setup(&admin, false).await;
    let key = state
        .db
        .oauth
        .create_api_key(
            serde_json::from_value(serde_json::json!({"name":"Me tenant fixture","scope":"admin"}))
                .unwrap(),
        )
        .await
        .unwrap();
    let app = crate::production_router(state.clone(), None);
    let body = assert_me_tenant_matches_events(&app, &state.event_bus, &key.api_key, tenant).await;
    assert_eq!(body["credential_class"], "legacy");
    state.db.pool.close().await;
}

//! Community startup and HTTP cloning of the shipped archive layout.
use super::*;
use sqlx::ConnectOptions;
use tower::ServiceExt;

use super::legacy_archive;

#[sqlx::test(migrations = false)]
async fn community_startup_clones_release_archives_without_copying_shared_identity_tables(
    admin: sqlx::PgPool,
) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let release = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter()
                .filter(|m| m.version < 20261014000000)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    release.run(&admin).await.unwrap();
    let url = admin.connect_options().to_url_lossy();
    let local = matric_db::create_pool(url.as_str()).await.unwrap();
    let user = Uuid::new_v4();
    let pat = Uuid::new_v4();
    let mut sources = Vec::new();
    for populated in [false, true] {
        let name = if populated {
            "community_populated_legacy"
        } else {
            "community_empty_legacy"
        };
        let source = legacy_archive::PgArchiveRepository::new(local.clone())
            .create_archive_schema(name, None)
            .await
            .unwrap();
        if populated {
            sqlx::query(&format!("INSERT INTO {}.app_user(id,iss,sub,kind,status) VALUES($1,'https://legacy.example','community-clone','user','disabled')", source.schema_name))
            .bind(user).execute(&local).await.unwrap();
            sqlx::query(&format!("INSERT INTO {}.personal_access_token(id,user_id,token_hash,token_prefix,token_last4,name,status,expires_at) VALUES($1,$2,repeat('b',64),'mm_pat_fixture','1234','Community legacy','suspended','2030-01-01')", source.schema_name))
            .bind(pat).bind(user).execute(&local).await.unwrap();
        }
        let notes = matric_db::PgNoteRepository::new(local.clone());
        let note = Database::new(local.clone())
            .for_schema(&source.schema_name)
            .unwrap()
            .execute(move |tx| {
                Box::pin(async move {
                    notes
                        .insert_tx(
                            tx,
                            CreateNoteRequest {
                                content: "Community legacy content survives cloning".into(),
                                format: "markdown".into(),
                                source: "release-fixture".into(),
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
        sources.push((source, note, populated));
    }
    let db = startup::database(url.as_str(), None, startup::Tenancy::Community)
        .await
        .unwrap();
    let state =
        super::super::tests::build_call_api_test_state(db.clone(), "community-upgrade").await;
    let app = super::production_router(state, None);
    for (source, note, populated) in &sources {
        let target = if *populated {
            "community_populated_clone"
        } else {
            "community_empty_clone"
        };
        let response = app
            .clone()
            .oneshot(http_request(
                Method::POST,
                &format!("/api/v1/archives/{}/clone", source.name),
                serde_json::json!({"new_name": target}),
                false,
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = json_body(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let schema = body["schema_name"].as_str().unwrap();
        let copied: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {schema}.note WHERE id=$1)"
        ))
        .bind(note)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert!(copied, "legacy note must be copied with its UUID");
        for table in ["app_user", "personal_access_token"] {
            let source_exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
                .bind(format!("{}.{table}", source.schema_name))
                .fetch_one(&admin)
                .await
                .unwrap();
            assert!(source_exists, "community must preserve legacy {table}");
            let target_absent: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NULL")
                .bind(format!("{schema}.{table}"))
                .fetch_one(&admin)
                .await
                .unwrap();
            assert!(target_absent, "shared {table} must not be cloned");
        }
    }
    let populated_schema = &sources[1].0.schema_name;
    let preserved: bool = sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {populated_schema}.app_user WHERE id=$1 AND status='disabled') AND EXISTS(SELECT 1 FROM {populated_schema}.personal_access_token WHERE id=$2 AND user_id=$1 AND status='suspended')"))
        .bind(user).bind(pat).fetch_one(&admin).await.unwrap();
    assert!(
        preserved,
        "cloning must leave legacy identities and credentials intact"
    );
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM external_oidc_ownership")
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(receipts, 0, "community startup must not adopt content");
    drop(app);
    db.pool.close().await;
    local.close().await;
}

/// #1201 keeps the local AllowAllPolicy management surface unauthenticated.
#[sqlx::test(migrations = false)]
async fn community_archive_management_without_authentication(admin: sqlx::PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let url = admin.connect_options().to_url_lossy();
    let db = startup::database(url.as_str(), None, startup::Tenancy::Community)
        .await
        .unwrap();
    let state = super::super::tests::build_call_api_test_state(db, "local-management-1201").await;
    assert!(!state.require_auth && !state.multi_tenant);
    assert_eq!(state.authorization_policy.policy_id(), "allow_all");
    let app = super::production_router(state, None);
    for (method, path, body, expected) in [
        (
            Method::POST,
            "/api/v1/archives",
            serde_json::json!({"name":"local_management"}),
            StatusCode::CREATED,
        ),
        (
            Method::PATCH,
            "/api/v1/archives/local_management",
            serde_json::json!({"description":"local owner"}),
            StatusCode::NO_CONTENT,
        ),
        (
            Method::POST,
            "/api/v1/archives/local_management/clone",
            serde_json::json!({"new_name":"local_clone"}),
            StatusCode::CREATED,
        ),
        (
            Method::POST,
            "/api/v1/archives/local_management/set-default",
            serde_json::Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            Method::POST,
            "/api/v1/archives/public/set-default",
            serde_json::Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            Method::DELETE,
            "/api/v1/archives/local_management",
            serde_json::Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            Method::DELETE,
            "/api/v1/archives/local_clone",
            serde_json::Value::Null,
            StatusCode::NO_CONTENT,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(http_request(method, path, body, false))
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&body));
    }
}

/// A broad operator subscription must consult grants for each live/replay event.
#[sqlx::test(migrations = false)]
async fn archive_policy_filters_broad_sse_live_and_replay(admin: sqlx::PgPool) {
    use matric_core::{Action, AuthzContext, AuthzError, Resource};
    struct Policy {
        default: Arc<dyn AuthorizationPolicy>,
        denied_schema: String,
        denied_calls: Arc<AtomicUsize>,
    }
    #[async_trait::async_trait]
    impl AuthorizationPolicy for Policy {
        async fn authorize(
            &self,
            principal: &AuthPrincipal,
            action: &Action,
            resource: &Resource,
            ctx: &AuthzContext,
        ) -> Result<Decision, AuthzError> {
            self.default
                .authorize(principal, action, resource, ctx)
                .await
        }
        async fn may_select_archive(
            &self,
            _: &AuthPrincipal,
            archive: &Resource,
            _: &AuthzContext,
        ) -> Result<bool, AuthzError> {
            let allowed = archive.attrs["archive_schema"] != self.denied_schema;
            if !allowed {
                self.denied_calls.fetch_add(1, Ordering::Relaxed);
            }
            Ok(allowed)
        }
        fn policy_id(&self) -> &'static str {
            "test_broad_archive_grants"
        }
        fn policy_version(&self) -> &'static str {
            "1201"
        }
    }
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let url = admin.connect_options().to_url_lossy();
    let db = startup::database(url.as_str(), None, startup::Tenancy::Community)
        .await
        .unwrap();
    let mut state = super::super::tests::build_call_api_test_state(db, "broad-sse-1201").await;
    let a = state
        .db
        .archives
        .create_archive_schema("broad_a", None)
        .await
        .unwrap();
    let b = state
        .db
        .archives
        .create_archive_schema("broad_b", None)
        .await
        .unwrap();
    // A request-name collision must not relabel an event's canonical schema.
    state
        .db
        .archives
        .create_archive_schema(&b.schema_name, None)
        .await
        .unwrap();
    let key = state
        .db
        .oauth
        .create_api_key(
            serde_json::from_value(
                serde_json::json!({"name":"Broad archive policy fixture","scope":"admin"}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let denied_calls = Arc::new(AtomicUsize::new(0));
    let calls = denied_calls.clone();
    let denied_schema = b.schema_name.clone();
    state.authorization_policy = matric_api::authorization_policy::select_policy_with(
        matric_api::authorization_policy::PolicyMode {
            multi_tenant: false,
            external_oidc: false,
        },
        Some(&move |_, default| {
            Arc::new(Policy {
                default,
                denied_schema: denied_schema.clone(),
                denied_calls: calls.clone(),
            })
        }),
    );
    let app = super::production_router(state.clone(), None);
    let mut events = state.event_bus.subscribe();
    let emit = |archive: &matric_core::ArchiveInfo, id| {
        state.event_bus.emit_with_context(
            ServerEvent::NoteDeleted { note_id: id },
            EventContext {
                memory: Some(archive.name.clone()),
                archive_name: Some(archive.name.clone()),
                archive_schema: Some(archive.schema_name.clone()),
                tenant_id: Some(Uuid::nil().to_string()),
                ..Default::default()
            },
        )
    };
    let emit_schema_only_b = || {
        state.event_bus.emit_with_context(
            ServerEvent::NoteDeleted {
                note_id: Uuid::new_v4(),
            },
            EventContext {
                memory: Some(b.schema_name.clone()),
                tenant_id: Some(Uuid::nil().to_string()),
                ..Default::default()
            },
        )
    };
    let emit_global = || {
        state.event_bus.emit_with_context(
            ServerEvent::JobsPaused {
                scope: "global".into(),
            },
            EventContext::default(),
        )
    };
    emit(&a, Uuid::new_v4());
    let cursor = events.recv().await.unwrap().event_id;
    let replay = Uuid::new_v4();
    emit(&b, Uuid::new_v4());
    emit_schema_only_b();
    emit_global();
    emit(&a, replay);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/events?types=note.deleted,jobs.paused")
                .header(header::AUTHORIZATION, format!("Bearer {}", key.api_key))
                .header("last-event-id", cursor.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    assert_eq!(
        crate::sse_tests::next_frame(&mut stream).await["payload"]["note_id"],
        replay.to_string()
    );
    let live = Uuid::new_v4();
    emit(&b, Uuid::new_v4());
    emit_schema_only_b();
    emit_global();
    emit(&a, live);
    assert_eq!(
        crate::sse_tests::next_frame(&mut stream).await["payload"]["note_id"],
        live.to_string()
    );
    assert_eq!(
        denied_calls.load(Ordering::Relaxed),
        6,
        "both paths must check named B, schema-only B, and global inventory frames"
    );
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(150),
        futures::StreamExt::next(&mut stream)
    )
    .await
    .is_err());
}

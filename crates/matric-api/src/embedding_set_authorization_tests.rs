//! Live hosted authorization coverage for embedding-set route normalization.
//!
//! This intentionally requires the runner-owned ephemeral PostgreSQL fixture.
//! It must never fall back to a workstation database or silently skip.

use super::*;
use axum::routing::any;
use matric_jobs::{JobContext, JobHandler, JobResult};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

#[derive(Clone)]
struct FixtureIdentity {
    tenant_id: Uuid,
}

async fn inject_fixture_identity(
    Extension(identity): Extension<FixtureIdentity>,
    mut request: axum::http::Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let scope = request
        .headers()
        .get("x-fixture-scope")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("read write admin")
        .to_owned();
    request
        .extensions_mut()
        .insert(VerifiedRequestTenant::from_verified(identity.tenant_id).expect("fixture tenant"));
    request.extensions_mut().insert(TenantScopeRequired);
    request.extensions_mut().insert(Auth {
        principal: AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: scope.to_string(),
        },
    });
    let input = route_policy::authorization_input_for_request(
        request.method(),
        request.uri().path(),
        Some(&identity.tenant_id.to_string()),
    )
    .expect("embedding authorization route has policy input");
    request.extensions_mut().insert(input);
    next.run(request).await
}

fn embedding_authorization_router(state: AppState, pool: sqlx::PgPool, tenant: Uuid) -> Router {
    Router::new()
        .route(
            "/api/v1/notes/{id}/similar",
            get(find_similar_note_entities),
        )
        .route(
            "/api/v1/entities/similar",
            get(find_similar_external_entities),
        )
        .route(
            "/api/v1/embedding-sets/{slug}",
            get(get_embedding_set)
                .patch(update_embedding_set)
                .delete(delete_embedding_set),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/members",
            get(list_embedding_set_members).post(add_embedding_set_members),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/members/{note_id}",
            delete(remove_embedding_set_member),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/build-index",
            post(build_embedding_set_index),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/index-health",
            get(get_embedding_set_index_health),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/runs",
            get(handlers::vector_import::list_embedding_runs)
                .post(handlers::vector_import::import_embedding_run),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/runs/{run_id}",
            get(handlers::vector_import::get_embedding_run),
        )
        .route(
            "/api/v1/embedding-sets/{slug}/refresh",
            post(refresh_embedding_set),
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
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_fixture_identity))
        .layer(Extension(FixtureIdentity { tenant_id: tenant }))
        .with_state(state)
}

#[derive(Clone)]
struct ClosedRouteProbe {
    hits: Arc<AtomicUsize>,
}

async fn closed_route_probe(Extension(probe): Extension<ClosedRouteProbe>) -> StatusCode {
    probe.hits.fetch_add(1, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}

/// This router deliberately has a counting probe in place of each real handler.
/// It still uses the production identity, tenant scope, archive routing, and
/// authorization middleware, so any admission regression is observable without
/// invoking a mutating closed-route handler.
fn closed_route_authorization_router(
    state: AppState,
    pool: sqlx::PgPool,
    tenant: Uuid,
    probe: ClosedRouteProbe,
) -> Router {
    Router::new()
        .route("/api/v1/calls/{id}", any(closed_route_probe))
        .route("/api/v1/graph/{id}", any(closed_route_probe))
        .route("/api/v1/notes/{id}/concepts", any(closed_route_probe))
        .route(
            "/api/v1/notes/{id}/concepts/{concept_id}",
            any(closed_route_probe),
        )
        .route(
            "/api/v1/notes/{id}/memory-provenance",
            any(closed_route_probe),
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
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_fixture_identity))
        .layer(Extension(FixtureIdentity { tenant_id: tenant }))
        .layer(Extension(probe))
        .with_state(state)
}

/// #1340: the real note, concepts and provenance handlers behind the production
/// identity, tenant-scope, archive-routing and authorization middleware.
fn note_scoped_read_router(state: AppState, pool: sqlx::PgPool, tenant: Uuid) -> Router {
    Router::new()
        .route("/api/v1/notes/{id}", get(get_note))
        .route("/api/v1/notes/{id}/concepts", get(get_note_concepts))
        .route("/api/v1/notes/{id}/provenance", get(get_note_provenance))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_fixture_identity))
        .layer(Extension(FixtureIdentity { tenant_id: tenant }))
        .with_state(state)
}

async fn bootstrap_tenant(admin: &sqlx::PgPool, tenant: Uuid, label: &str) {
    // Archive creation seeds its archive-local default set from the tenant's
    // default embedding configuration. Provision the same complete baseline as
    // the hosted archive fixtures, rather than bypassing its RLS-scoped setup.
    let request =
        matric_api::admin_bootstrap::BootstrapRequest::new(label, Some(tenant), None, false)
            .expect("build synthetic fixture tenant bootstrap");
    matric_api::admin_bootstrap::bootstrap_tenant(admin, &request, false, "fortemi:tenant_id")
        .await
        .expect("bootstrap synthetic fixture tenant");
}

async fn seed_embedding_set(admin: &sqlx::PgPool, schema: &str, tenant: Uuid, slug: &str) -> Uuid {
    matric_db::validate_schema_name(schema).expect("fixture archive schema");
    let id = Uuid::new_v4();
    sqlx::query(&format!(
        "INSERT INTO \"{schema}\".embedding_set (
             id, tenant_id, name, slug, set_type, mode, criteria,
             auto_embed_rules, vector_source, defer_index_build, agent_metadata
         ) VALUES (
             $1, $2, $3, $4, 'full'::embedding_set_type,
             'manual'::embedding_set_mode, '{{}}'::jsonb, '{{}}'::jsonb,
             'external', TRUE, '{{}}'::jsonb
         )"
    ))
    .bind(id)
    .bind(tenant)
    .bind(format!("fixture {slug}"))
    .bind(slug)
    .execute(admin)
    .await
    .expect("seed synthetic fixture embedding set");
    id
}

async fn seed_note(admin: &sqlx::PgPool, schema: &str, tenant: Uuid) -> Uuid {
    matric_db::validate_schema_name(schema).expect("fixture archive schema");
    let id = Uuid::new_v4();
    sqlx::query(&format!(
        "INSERT INTO \"{schema}\".note \
         (id, tenant_id, format, source, created_at_utc, updated_at_utc) \
         VALUES ($1, $2, 'markdown', 'closed-route-fixture', now(), now())"
    ))
    .bind(id)
    .bind(tenant)
    .execute(admin)
    .await
    .expect("seed valid in-tenant note for closed-route proof");
    id
}

fn request(method: Method, path: &str, archive: &str, scope: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header(middleware::archive_routing::MEMORY_HEADER, archive)
        .header("x-fixture-scope", scope)
        .body(Body::empty())
        .expect("build fixture request")
}

async fn assert_not_policy_forbidden(
    app: Router,
    request: axum::http::Request<Body>,
    route: &str,
) -> StatusCode {
    let response = app.oneshot(request).await.expect("fixture response");
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "production normalizer dispatch must admit the authorized route before its handler: {route}"
    );
    response.status()
}

#[tokio::test]
async fn closed_route_exceptions_keep_the_prior_role_based_forbidden_response_byte_identical() {
    let auth = Auth {
        principal: AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read write admin".to_string(),
        },
    };

    for exception in route_policy::CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS {
        for method_name in exception.methods {
            let method = Method::from_bytes(method_name.as_bytes()).expect("inventory method");
            let path = exception
                .path
                .replace("{id}", &Uuid::new_v4().to_string())
                .replace("{concept_id}", &Uuid::new_v4().to_string());
            let input =
                route_policy::authorization_input_for_request(&method, &path, Some("tenant-a"))
                    .expect("closed route has policy input");
            assert_eq!(
                input.resource.attrs["requires_backing_resource_normalization"], false,
                "{method} {} must be an inventory exception",
                exception.path
            );
            assert!(route_policy::role_based_route_is_intentionally_closed(
                &input
            ));

            // This represents the exact pre-#1278 generic-normalization gate:
            // the same role-based helper returned this response while the flag
            // was true.  The closure registry must preserve it byte-for-byte.
            let mut pre_exception_input = input.clone();
            pre_exception_input.resource.attrs.insert(
                "requires_backing_resource_normalization".to_string(),
                serde_json::json!(true),
            );
            let expected =
                authorize_policy_input(&RoleBasedPolicy, &TracingSink, &auth, &pre_exception_input)
                    .await
                    .expect_err("pre-exception role-based normalization gate must deny");
            let actual = authorize_policy_input(&RoleBasedPolicy, &TracingSink, &auth, &input)
                .await
                .expect_err("closed-route exception must remain role-based denied");
            let expected_status = expected.status();
            let actual_status = actual.status();
            let expected_body = axum::body::to_bytes(expected.into_body(), usize::MAX)
                .await
                .expect("read pre-exception denial");
            let actual_body = axum::body::to_bytes(actual.into_body(), usize::MAX)
                .await
                .expect("read closed-route denial");
            assert_eq!(
                actual_status, expected_status,
                "{method} {}",
                exception.path
            );
            assert_eq!(actual_body, expected_body, "{method} {}", exception.path);
        }
    }
}

#[tokio::test]
async fn embedding_config_id_is_admin_operator_not_tenant_object() {
    let mut input = route_policy::authorization_input_for_request(
        &Method::GET,
        &format!("/api/v1/embedding-configs/{}", Uuid::new_v4()),
        Some("tenant-a"),
    )
    .expect("embedding config route has policy input");
    assert_eq!(input.policy.class, route_policy::PolicyClass::AdminOperator);
    assert!(!route_policy::role_based_route_is_intentionally_closed(
        &input
    ));

    // A tenant-scoped read/write principal cannot reach the AdminOperator
    // family even after the generic normalization gate has been satisfied.
    route_policy::mark_resource_id_normalized(&mut input);
    let tenant_auth = Auth {
        principal: AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read write".to_string(),
        },
    };
    let response = authorize_policy_input(&RoleBasedPolicy, &TracingSink, &tenant_auth, &input)
        .await
        .expect_err("tenant role must not reach embedding configuration handlers");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Requires the runner-owned `with-ephemeral-postgres.mjs` wrapper.  The
/// wrapper sets both values; missing either is an infrastructure failure, not a
/// skipped isolation test.
#[tokio::test]
async fn embedding_set_authorization_live_db_http_inventory_and_isolation() {
    assert_eq!(
        std::env::var("FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS").as_deref(),
        Ok("1"),
        "run this required test through with-ephemeral-postgres.mjs"
    );
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be supplied by the runner-owned ephemeral PostgreSQL fixture");
    let admin = matric_db::create_pool(&database_url)
        .await
        .expect("connect runner-owned PostgreSQL fixture");
    let admin_db = Database::new(admin.clone());
    admin_db
        .migrate()
        .await
        .expect("migrate runner-owned PostgreSQL fixture");

    let role = format!("embset_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' \
         NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .expect("create powerless fixture runtime role");
    for grant in [
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role}"),
        format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"),
    ] {
        sqlx::query(&grant)
            .execute(&admin)
            .await
            .expect("grant fixture runtime role");
    }
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            database_url
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("fixture PostgreSQL URL")
                .username(&role)
                .password(&password),
        )
        .await
        .expect("connect powerless fixture runtime role");
    let safe_runtime: bool = sqlx::query_scalar(
        "SELECT NOT rolsuper AND NOT rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&runtime)
    .await
    .expect("verify fixture runtime role");
    assert!(
        safe_runtime,
        "fixture runtime role must enforce row security"
    );

    // The admin pool is intentionally personal-mode scoped for migrations and
    // fixture setup. Application state must instead use the same powerless,
    // hosted-unscoped runtime pool as the request middleware.
    let db = Database::new(runtime.clone());

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    bootstrap_tenant(&admin, tenant_a, &format!("embset-a-{tenant_a}")).await;
    bootstrap_tenant(&admin, tenant_b, &format!("embset-b-{tenant_b}")).await;
    let archives = matric_db::administrative::AdministrativeExecutor::new(admin.clone(), &runtime)
        .await
        .expect("create tenant-aware archive executor");
    let archive_a = archives
        .create_archive(
            tenant_a,
            &format!("embset-a-{}", Uuid::new_v4().simple()),
            None,
        )
        .await
        .expect("create synthetic tenant A archive");
    let archive_a_other = archives
        .create_archive(
            tenant_a,
            &format!("embset-a-other-{}", Uuid::new_v4().simple()),
            None,
        )
        .await
        .expect("create second synthetic tenant A archive");
    let archive_b = archives
        .create_archive(
            tenant_b,
            &format!("embset-b-{}", Uuid::new_v4().simple()),
            None,
        )
        .await
        .expect("create synthetic tenant B archive");

    let slug = format!("fixture-set-{}", Uuid::new_v4().simple());
    let set_id = seed_embedding_set(&admin, &archive_a.schema_name, tenant_a, &slug).await;
    let note_id = seed_note(&admin, &archive_a.schema_name, tenant_a).await;
    let call_id = admin_db
        .call_sessions
        .create_session(matric_core::CreateCallSessionRequest {
            provider: "closed-route-fixture".to_string(),
            provider_call_id: format!("closed-route-{}", Uuid::new_v4()),
            started_at: None,
            asr_backend: None,
            remote_party: None,
            archive_id: None,
            metadata: serde_json::json!({"fixture": "closed-route"}),
        })
        .await
        .expect("seed valid call fixture for closed-route proof")
        .call_id;
    seed_embedding_set(
        &admin,
        &archive_a_other.schema_name,
        tenant_a,
        &format!("other-{slug}"),
    )
    .await;
    seed_embedding_set(&admin, &archive_b.schema_name, tenant_b, &slug).await;

    let mut state = super::tests::build_call_api_test_state(db.clone(), &database_url).await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = std::sync::Arc::new(RoleBasedPolicy);
    let closed_probe = ClosedRouteProbe {
        hits: Arc::new(AtomicUsize::new(0)),
    };
    let closed_app = closed_route_authorization_router(
        state.clone(),
        runtime.clone(),
        tenant_a,
        closed_probe.clone(),
    );
    profile_similarity_http_matrix(
        state.clone(),
        &admin,
        &runtime,
        tenant_a,
        tenant_b,
        SimilarityArchives {
            archive: &archive_a,
            other_archive: &archive_a_other,
            foreign_archive: &archive_b,
        },
    )
    .await;
    let app = embedding_authorization_router(state.clone(), runtime.clone(), tenant_a);
    let archive = archive_a.name.as_str();

    assert_eq!(
        assert_not_policy_forbidden(
            app.clone(),
            request(
                Method::GET,
                &format!("/api/v1/embedding-sets/{slug}"),
                archive,
                "read",
            ),
            "known slug",
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        assert_not_policy_forbidden(
            app.clone(),
            request(
                Method::GET,
                &format!("/api/v1/embedding-sets/{set_id}"),
                archive,
                "read",
            ),
            "known UUID",
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        assert_not_policy_forbidden(
            app.clone(),
            request(
                Method::GET,
                &format!("/api/v1/embedding-sets/{slug}/runs"),
                archive,
                "read",
            ),
            "runs collection",
        )
        .await,
        StatusCode::OK
    );
    assert_ne!(
        assert_not_policy_forbidden(
            app.clone(),
            request(
                Method::POST,
                &format!("/api/v1/embedding-sets/{slug}/build-index"),
                archive,
                "read write admin",
            ),
            "build-index",
        )
        .await,
        StatusCode::FORBIDDEN
    );
    for (method, path, label) in [
        (
            Method::DELETE,
            format!("/api/v1/embedding-sets/{slug}/members/{}", Uuid::new_v4()),
            "nested note ID",
        ),
        (
            Method::GET,
            format!("/api/v1/embedding-sets/{slug}/runs/fixture-run-id"),
            "nested run ID",
        ),
    ] {
        assert_eq!(
            assert_not_policy_forbidden(
                app.clone(),
                request(method, &path, archive, "read write admin"),
                label,
            )
            .await,
            StatusCode::NOT_FOUND,
            "the embedding-set normalizer must select the outer slug, not {label}"
        );
    }
    for (selected_archive, selected_slug, label) in [
        (archive, "missing-fixture-set", "missing"),
        (
            archive_a_other.name.as_str(),
            slug.as_str(),
            "cross archive",
        ),
        (
            archive_b.name.as_str(),
            slug.as_str(),
            "cross tenant archive",
        ),
    ] {
        assert_eq!(
            assert_not_policy_forbidden(
                app.clone(),
                request(
                    Method::GET,
                    &format!("/api/v1/embedding-sets/{selected_slug}"),
                    selected_archive,
                    "read",
                ),
                label,
            )
            .await,
            StatusCode::NOT_FOUND,
            "{label} must be indistinguishable from a missing set"
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request(
                Method::GET,
                &format!("/api/v1/embedding-sets/{slug}"),
                archive,
                "mcp",
            ))
            .await
            .expect("scope-denial response")
            .status(),
        StatusCode::FORBIDDEN,
        "missing-resource handling must not weaken the role-based scope guard"
    );

    // #1340: concepts and provenance reads authorize through their backing
    // note exactly like GET /notes/{id}. A same-schema row owned by another
    // tenant and a random id are both invisible under tenant-predicated lookup.
    let foreign_note_id = seed_note(&admin, &archive_a.schema_name, tenant_b).await;
    let missing_note_id = Uuid::new_v4();
    let note_scoped_app = note_scoped_read_router(state.clone(), runtime.clone(), tenant_a);
    for suffix in ["/concepts", "/provenance"] {
        let owned = note_scoped_app
            .clone()
            .oneshot(request(
                Method::GET,
                &format!("/api/v1/notes/{note_id}{suffix}"),
                archive,
                "read",
            ))
            .await
            .expect("owned note-scoped response");
        assert_eq!(owned.status(), StatusCode::OK, "owned note {suffix}");
        for (label, id) in [
            ("cross-tenant", foreign_note_id),
            ("missing", missing_note_id),
        ] {
            let note_status = note_scoped_app
                .clone()
                .oneshot(request(
                    Method::GET,
                    &format!("/api/v1/notes/{id}"),
                    archive,
                    "read",
                ))
                .await
                .expect("note baseline response")
                .status();
            let status = note_scoped_app
                .clone()
                .oneshot(request(
                    Method::GET,
                    &format!("/api/v1/notes/{id}{suffix}"),
                    archive,
                    "read",
                ))
                .await
                .expect("note-scoped denial response")
                .status();
            assert_eq!(status, StatusCode::FORBIDDEN, "{label} note {suffix}");
            assert_eq!(
                status, note_status,
                "{label} {suffix} must match /notes/{{id}}"
            );
        }
    }
    drop(note_scoped_app);

    // Every closed exception is dispatched through the production middleware
    // with an existing fixture object.  The in-tenant note is used by graph,
    // concepts, and provenance cases; the call route
    // uses a real call-session row.  The counting handler proves none reaches
    // dispatch after the exception's false normalization flag.
    for exception in route_policy::CLOSED_ROUTE_NORMALIZATION_EXCEPTIONS {
        for method_name in exception.methods {
            let method = Method::from_bytes(method_name.as_bytes()).expect("inventory method");
            let primary_id = if exception.path == "/api/v1/calls/{id}" {
                call_id
            } else {
                note_id
            };
            let path = exception
                .path
                .replace("{id}", &primary_id.to_string())
                .replace("{concept_id}", &Uuid::new_v4().to_string());
            let response = closed_app
                .clone()
                .oneshot(request(method.clone(), &path, archive, "read write admin"))
                .await
                .expect("closed-route fixture response");
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "{method} {} must retain the pre-handler role-based denial",
                exception.path
            );
            assert_eq!(
                closed_probe.hits.load(Ordering::SeqCst),
                0,
                "{method} {} reached its handler despite the closed-route guard",
                exception.path
            );
        }
    }

    // Every embedding route/method is executed through the production
    // middleware with a fresh selected-archive row.  Invalid handler payloads
    // are acceptable; a role-based 403 is not, because it means production
    // backing normalization was disconnected before dispatch.
    for policy in route_policy::ROUTE_POLICY_INVENTORY
        .iter()
        .filter(|policy| {
            policy.action_family == "embedding_control" && policy.path.contains("{slug}")
        })
    {
        for method_name in policy.methods {
            let method = Method::from_bytes(method_name.as_bytes()).expect("inventory method");
            let inventory_slug = format!("inventory-{}", Uuid::new_v4().simple());
            seed_embedding_set(&admin, &archive_a.schema_name, tenant_a, &inventory_slug).await;
            let path = policy
                .path
                .replace("{slug}", &inventory_slug)
                .replace("{note_id}", &Uuid::new_v4().to_string())
                .replace("{run_id}", "fixture-run-id");
            let mut request = request(method.clone(), &path, archive, "read write admin");
            if method == Method::POST && path.ends_with("/runs") {
                request.headers_mut().insert(
                    header::CONTENT_TYPE,
                    "text/plain".parse().expect("content type"),
                );
            } else if matches!(method, Method::POST | Method::PATCH) {
                request.headers_mut().insert(
                    header::CONTENT_TYPE,
                    "application/json".parse().expect("content type"),
                );
                *request.body_mut() = Body::from("{}");
            }
            assert_not_policy_forbidden(app.clone(), request, policy.path).await;
        }
    }

    drop(closed_app);
    drop(app);
    archives
        .drop_archive(tenant_a, &archive_a.name)
        .await
        .expect("drop tenant A archive");
    archives
        .drop_archive(tenant_a, &archive_a_other.name)
        .await
        .expect("drop tenant A second archive");
    archives
        .drop_archive(tenant_b, &archive_b.name)
        .await
        .expect("drop tenant B archive");
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .expect("drop fixture runtime grants");
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .expect("drop fixture runtime role");
    admin.close().await;
}

struct SimilarityArchiveOnly(String);

#[async_trait::async_trait]
impl matric_core::AuthorizationPolicy for SimilarityArchiveOnly {
    async fn authorize(
        &self,
        principal: &AuthPrincipal,
        action: &matric_core::Action,
        resource: &matric_core::Resource,
        context: &matric_core::AuthzContext,
    ) -> Result<matric_core::Decision, matric_core::AuthzError> {
        RoleBasedPolicy
            .authorize(principal, action, resource, context)
            .await
    }

    async fn may_select_archive(
        &self,
        _: &AuthPrincipal,
        archive: &matric_core::Resource,
        _: &matric_core::AuthzContext,
    ) -> Result<bool, matric_core::AuthzError> {
        Ok(archive.attrs["archive_schema"] == self.0)
    }

    // Retain the production role-policy normalization guard while narrowing
    // only archive selection. The decision engine remains RoleBasedPolicy.
    fn policy_id(&self) -> &'static str {
        "role_based"
    }
    fn policy_version(&self) -> &'static str {
        "1292"
    }
}

struct SimilarityArchives<'a> {
    archive: &'a matric_core::ArchiveInfo,
    other_archive: &'a matric_core::ArchiveInfo,
    foreign_archive: &'a matric_core::ArchiveInfo,
}

/// Runs as part of the required live HTTP inventory, against the same private
/// migrated fixture and powerless runtime role. A one-connection pool makes an
/// independent pool lookup inside a scoped callback observable as a timeout.
async fn profile_similarity_http_matrix(
    mut state: AppState,
    admin: &sqlx::PgPool,
    runtime: &sqlx::PgPool,
    tenant: Uuid,
    foreign_tenant: Uuid,
    archives: SimilarityArchives<'_>,
) {
    let SimilarityArchives {
        archive,
        other_archive,
        foreign_archive,
    } = archives;
    let singleton = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*runtime.connect_options()).clone())
        .await
        .unwrap();
    let initial_scope = similarity_pool_scope(&singleton).await;
    assert!(initial_scope.2.is_none(), "runtime starts without a tenant");
    state.db = Database::new(singleton.clone());
    let slug = format!("similarity-{}", Uuid::new_v4().simple());
    let set = seed_embedding_set(admin, &archive.schema_name, tenant, &slug).await;
    let other_set = seed_embedding_set(admin, &other_archive.schema_name, tenant, &slug).await;
    let foreign_set = seed_embedding_set(
        admin,
        &archive.schema_name,
        foreign_tenant,
        &format!("foreign-{slug}"),
    )
    .await;
    // Match imported full-set profiles without materialized auto-membership.
    // Disable refresh before creating notes so triggers cannot add memberships.
    sqlx::query(&format!(
        "UPDATE {}.embedding_set
        SET set_type='full', vector_source='external', is_active=TRUE,
            mode='auto', criteria='{{\"include_all\":true,\"exclude_archived\":true}}'::jsonb,
            auto_refresh=FALSE
        WHERE tenant_id=$1 AND id=$2",
        archive.schema_name
    ))
    .bind(tenant)
    .bind(set)
    .execute(admin)
    .await
    .unwrap();
    let root = seed_note(admin, &archive.schema_name, tenant).await;
    let near = seed_note(admin, &archive.schema_name, tenant).await;
    let foreign_root = seed_note(admin, &archive.schema_name, foreign_tenant).await;
    let deleted_root = seed_note(admin, &archive.schema_name, tenant).await;
    let empty_root = seed_note(admin, &archive.schema_name, tenant).await;
    sqlx::query(&format!(
        "UPDATE {}.note SET deleted_at=now() WHERE id=$1",
        archive.schema_name
    ))
    .bind(deleted_root)
    .execute(admin)
    .await
    .unwrap();
    // Duplicate the exact root UUID in a second archive. It has no local vector;
    // it must never inherit the first archive's root vector or neighbours.
    sqlx::query(&format!(
        "INSERT INTO {}.note
        (id,tenant_id,format,source,created_at_utc,updated_at_utc)
        VALUES ($1,$2,'markdown','similarity-duplicate',now(),now())",
        other_archive.schema_name
    ))
    .bind(root)
    .bind(tenant)
    .execute(admin)
    .await
    .unwrap();
    for (schema, owner, id) in [
        (&archive.schema_name, tenant, set),
        (&other_archive.schema_name, tenant, other_set),
        (&archive.schema_name, foreign_tenant, foreign_set),
    ] {
        sqlx::query(&format!("UPDATE {schema}.embedding_set SET embedding_config_id=(
            SELECT id FROM public.embedding_config WHERE tenant_id=$1 AND is_default IS TRUE LIMIT 1)
            WHERE id=$2 AND tenant_id=$1"))
            .bind(owner).bind(id).execute(admin).await.unwrap();
    }
    let dimension: i32 = sqlx::query_scalar(
        "SELECT dimension FROM public.embedding_config
        WHERE tenant_id=$1 AND is_default IS TRUE LIMIT 1",
    )
    .bind(tenant)
    .fetch_one(admin)
    .await
    .unwrap();
    for (id, second) in [(root, 0.0_f32), (near, 0.75)] {
        let mut values = vec![0.0_f32; dimension as usize];
        values[0] = 1.0;
        values[1] = second;
        sqlx::query(&format!(
            "INSERT INTO {}.embedding
            (id,tenant_id,note_id,embedding_set_id,chunk_index,text,vector,model,
             vector_kind,template_version,profile_hash,profile_text)
            VALUES ($1,$2,$3,$4,-1,'profile',$5,'fixture','profile','v1','hash','profile')",
            archive.schema_name
        ))
        .bind(Uuid::new_v4())
        .bind(tenant)
        .bind(id)
        .bind(set)
        .bind(pgvector::Vector::from(values))
        .execute(admin)
        .await
        .unwrap();
        sqlx::query(&format!(
            "DELETE FROM {}.embedding_set_member
            WHERE tenant_id=$1 AND note_id=$2 AND embedding_set_id=$3",
            archive.schema_name
        ))
        .bind(tenant)
        .bind(id)
        .bind(set)
        .execute(admin)
        .await
        .unwrap();
    }
    // These are real, nearer vectors (cosine 1), not post-query exclusions.
    // Tenant-qualified FKs require a foreign tenant's own set. Note UUIDs are
    // unique within a schema, so duplicate UUIDs belong in separate schemas.
    // These external sets defer index creation: the real untyped vector column
    // can hold a wrong shape without violating an expression index's cast.
    let wrong_set = seed_embedding_set(
        admin,
        &archive.schema_name,
        tenant,
        &format!("wrong-{slug}"),
    )
    .await;
    let foreign_archive_set =
        seed_embedding_set(admin, &foreign_archive.schema_name, foreign_tenant, &slug).await;
    let mut forbidden_ids = Vec::new();
    let mut forbidden_markers = Vec::new();
    for reason in ["deleted", "archived", "set", "kind", "dimension", "tenant"] {
        let id = Uuid::new_v4();
        let owner = if reason == "tenant" {
            foreign_tenant
        } else {
            tenant
        };
        let candidate_set = match reason {
            "tenant" => foreign_set,
            "set" => wrong_set,
            _ => set,
        };
        let marker = format!("forbidden-{reason}-{id}");
        seed_similarity_competitor(
            admin,
            &archive.schema_name,
            owner,
            id,
            SimilarityVectorFixture {
                set: candidate_set,
                dimension: if reason == "dimension" {
                    dimension + 1
                } else {
                    dimension
                },
                kind: if reason == "kind" {
                    "body_chunk"
                } else {
                    "profile"
                },
            },
            true,
            &marker,
        )
        .await;
        if matches!(reason, "deleted" | "archived") {
            sqlx::query(&format!(
                "UPDATE {}.note SET archived=$2, deleted_at=CASE WHEN $3 THEN now() END WHERE id=$1",
                archive.schema_name
            )).bind(id).bind(reason == "archived").bind(reason == "deleted")
                .execute(admin).await.unwrap();
        }
        if reason == "set" {
            // Membership in the requested set must not admit another set's vector.
            sqlx::query(&format!(
                "INSERT INTO {}.embedding_set_member (tenant_id,note_id,embedding_set_id) VALUES ($1,$2,$3)",
                archive.schema_name
            )).bind(tenant).bind(id).bind(set).execute(admin).await.unwrap();
        }
        forbidden_ids.push(id);
        forbidden_markers.push(marker);
    }
    for (schema, owner, candidate_set, reason) in [
        (
            &other_archive.schema_name,
            tenant,
            other_set,
            "other-archive",
        ),
        (
            &foreign_archive.schema_name,
            foreign_tenant,
            foreign_archive_set,
            "foreign-archive",
        ),
    ] {
        // Duplicate the permitted neighbour UUID, including under another tenant.
        let marker = format!("forbidden-{reason}-{near}");
        seed_similarity_competitor(
            admin,
            schema,
            owner,
            near,
            SimilarityVectorFixture {
                set: candidate_set,
                dimension,
                kind: "profile",
            },
            true,
            &marker,
        )
        .await;
        forbidden_markers.push(marker);
        let id = Uuid::new_v4();
        let marker = format!("forbidden-{reason}-{id}");
        seed_similarity_competitor(
            admin,
            schema,
            owner,
            id,
            SimilarityVectorFixture {
                set: candidate_set,
                dimension,
                kind: "profile",
            },
            true,
            &marker,
        )
        .await;
        forbidden_ids.push(id);
        forbidden_markers.push(marker);
    }
    let tag = format!("similarity-tag-{}", Uuid::new_v4().simple());
    let foreign_tag = format!("foreign-{tag}");
    for (owner, name) in [(tenant, &tag), (foreign_tenant, &foreign_tag)] {
        sqlx::query(&format!(
            "INSERT INTO {}.tag (tenant_id,name,created_at_utc) VALUES ($1,$2,now())",
            archive.schema_name
        ))
        .bind(owner)
        .bind(name)
        .execute(admin)
        .await
        .unwrap();
    }
    sqlx::query(&format!(
        "INSERT INTO {}.note_tag (tenant_id,note_id,tag_name) VALUES ($1,$2,$3)",
        archive.schema_name
    ))
    .bind(tenant)
    .bind(near)
    .bind(&tag)
    .execute(admin)
    .await
    .unwrap();
    let memberships: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.embedding_set_member
        WHERE tenant_id=$1 AND embedding_set_id=$2 AND note_id=ANY($3)",
        archive.schema_name
    ))
    .bind(tenant)
    .bind(set)
    .bind(vec![root, near])
    .fetch_one(admin)
    .await
    .unwrap();
    assert_eq!(memberships, 0, "imported root and near have no memberships");
    state.authorization_policy = Arc::new(SimilarityArchiveOnly(archive.schema_name.clone()));
    let app = embedding_authorization_router(state.clone(), singleton.clone(), tenant);
    let path = format!("/api/v1/notes/{root}/similar?set={set}&k=1");
    let (status, body) = similarity_http_call(&app, &path, &archive.name, "read").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["query_note_id"], root.to_string());
    assert_eq!(body["set_id"], set.to_string());
    assert_eq!(body["kind"], "profile");
    assert_eq!(body["results"].as_array().unwrap().len(), 1);
    assert_eq!(body["results"][0]["note_id"], near.to_string());
    assert!((body["results"][0]["score"].as_f64().unwrap() - 0.8).abs() < 0.001);
    let (status, projected) = similarity_http_call(
        &app,
        &format!("{path}&metadata=fixture_marker"),
        &archive.name,
        "read",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{projected}");
    assert_eq!(projected["results"].as_array().unwrap().len(), 1);
    assert_eq!(projected["results"][0]["note_id"], near.to_string());
    assert!((projected["results"][0]["score"].as_f64().unwrap() - 0.8).abs() < 0.001);
    for response in [&body, &projected] {
        let serialized = response.to_string();
        for id in &forbidden_ids {
            assert!(
                !serialized.contains(&id.to_string()),
                "forbidden candidate {id}"
            );
        }
        for marker in &forbidden_markers {
            assert!(
                !serialized.contains(marker),
                "forbidden metadata/identity {marker}"
            );
        }
        assert!(response["results"][0]["source_namespace"].is_null());
        assert!(response["results"][0]["external_source_id"].is_null());
        assert_eq!(response["results"][0]["metadata"], serde_json::json!({}));
    }
    for (name, expected) in [(&tag, 1), (&foreign_tag, 0)] {
        let strict = serde_json::json!({"any_tags":[name]}).to_string();
        let (status, body) = similarity_http_call(
            &app,
            &format!("{path}&strict_filter={}", urlencoding::encode(&strict)),
            &archive.name,
            "read",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["results"].as_array().unwrap().len(), expected);
    }
    assert_eq!(
        similarity_http_call(&app, &path, &archive.name, "mcp")
            .await
            .0,
        StatusCode::FORBIDDEN,
        "read scope remains required"
    );
    let missing = similarity_http_call(
        &app,
        &format!("/api/v1/notes/{}/similar?set={set}", Uuid::new_v4()),
        &archive.name,
        "read",
    )
    .await;
    assert_eq!(missing.0, StatusCode::FORBIDDEN);
    for id in [foreign_root, deleted_root] {
        let denied = similarity_http_call(
            &app,
            &format!("/api/v1/notes/{id}/similar?set={set}"),
            &archive.name,
            "read",
        )
        .await;
        assert_eq!(denied, missing, "non-enumerating root denial");
    }
    let unknown_set = similarity_http_call(
        &app,
        &format!("/api/v1/notes/{root}/similar?set={}", Uuid::new_v4()),
        &archive.name,
        "read",
    )
    .await;
    assert_eq!(unknown_set.0, StatusCode::NOT_FOUND);
    for id in [foreign_set, other_set] {
        assert_eq!(
            similarity_http_call(
                &app,
                &format!("/api/v1/notes/{root}/similar?set={id}"),
                &archive.name,
                "read"
            )
            .await,
            unknown_set
        );
    }
    for name in [
        other_archive.name.as_str(),
        foreign_archive.name.as_str(),
        "missing-archive",
    ] {
        assert_eq!(
            similarity_http_call(&app, &path, name, "read").await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        similarity_http_call(
            &app,
            &format!("/api/v1/entities/similar?source=fixture&external_id=missing&set={set}"),
            &archive.name,
            "read"
        )
        .await
        .0,
        StatusCode::FORBIDDEN,
        "catalog guard remains"
    );
    let response = app
        .clone()
        .oneshot(request(Method::HEAD, &path, &archive.name, "read"))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "HEAD retains catalog admission"
    );
    for k in ["0", "-1", "9223372036854775807"] {
        let (status, body) = similarity_http_call(
            &app,
            &format!("/api/v1/notes/{root}/similar?set={set}&k={k}"),
            &archive.name,
            "read",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["results"].as_array().unwrap().len(), 1);
    }
    assert_eq!(
        similarity_http_call(
            &app,
            &format!("/api/v1/notes/{root}/similar?set={set}&k=invalid"),
            &archive.name,
            "read"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, body) = similarity_http_call(
        &app,
        &format!("/api/v1/notes/{empty_root}/similar?set={set}"),
        &archive.name,
        "read",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].as_array().unwrap().is_empty());
    profile_similarity_http_relation_probes(
        &app,
        admin,
        &singleton,
        SimilarityProbeFixture {
            archive,
            tenant,
            root,
            near,
            set,
            dimension,
        },
        &path,
        &missing,
    )
    .await;
    state.authorization_policy = Arc::new(SimilarityArchiveOnly(other_archive.schema_name.clone()));
    let other_app = embedding_authorization_router(state, singleton.clone(), tenant);
    let (status, body) = similarity_http_call(
        &other_app,
        &format!("/api/v1/notes/{root}/similar?set={other_set}"),
        &other_archive.name,
        "read",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].as_array().unwrap().is_empty());
    // Reuse the same physical pool after successful and denied requests.
    assert_eq!(
        similarity_http_call(&app, &path, &archive.name, "read")
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        similarity_pool_scope(&singleton).await,
        initial_scope,
        "all HTTP outcomes restore the original physical connection scope"
    );
    drop(other_app);
    drop(app);
    singleton.close().await;
}

struct SimilarityVectorFixture<'a> {
    set: Uuid,
    dimension: i32,
    kind: &'a str,
}

/// Admin-only fixture writes still obey the migrated PKs, composite FKs and
/// profile/body constraints. Each forbidden row has independently visible
/// title, projected metadata, and source identity canaries.
async fn seed_similarity_competitor(
    admin: &sqlx::PgPool,
    schema: &str,
    tenant: Uuid,
    note: Uuid,
    vector: SimilarityVectorFixture<'_>,
    member: bool,
    marker: &str,
) {
    let SimilarityVectorFixture {
        set,
        dimension,
        kind,
    } = vector;
    matric_db::validate_schema_name(schema).unwrap();
    sqlx::query(&format!(
        "INSERT INTO {schema}.note
        (id,tenant_id,format,source,created_at_utc,updated_at_utc,title,metadata)
        VALUES ($1,$2,'markdown','similarity-adversary',now(),now(),$3,$4)"
    ))
    .bind(note)
    .bind(tenant)
    .bind(marker)
    .bind(serde_json::json!({"fixture_marker": marker}))
    .execute(admin)
    .await
    .unwrap();
    let mut values = vec![0.0_f32; dimension as usize];
    values[0] = 1.0;
    sqlx::query(&format!(
        "INSERT INTO {schema}.embedding
        (id,tenant_id,note_id,embedding_set_id,chunk_index,text,vector,model,
         vector_kind,template_version,profile_hash,profile_text)
        VALUES ($1,$2,$3,$4,$5,'profile',$6,'fixture',$7,'v1','hash','profile')"
    ))
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(note)
    .bind(set)
    .bind(if kind == "profile" { -1_i32 } else { 0_i32 })
    .bind(pgvector::Vector::from(values))
    .bind(kind)
    .execute(admin)
    .await
    .unwrap();
    if member {
        sqlx::query(&format!(
            "INSERT INTO {schema}.embedding_set_member (tenant_id,note_id,embedding_set_id)
            VALUES ($1,$2,$3) ON CONFLICT DO NOTHING"
        ))
        .bind(tenant)
        .bind(note)
        .bind(set)
        .execute(admin)
        .await
        .unwrap();
    } else {
        sqlx::query(&format!(
            "DELETE FROM {schema}.embedding_set_member
            WHERE tenant_id=$1 AND note_id=$2 AND embedding_set_id=$3"
        ))
        .bind(tenant)
        .bind(note)
        .bind(set)
        .execute(admin)
        .await
        .unwrap();
    }
    sqlx::query(&format!(
        "INSERT INTO {schema}.source_identity
        (tenant_id,note_id,source_namespace,external_id,source_id,
         source_schema_version,content_digest,import_run_id)
        VALUES ($1,$2,$3,$3,$3,'1.0.0',$4,'similarity-fixture')"
    ))
    .bind(tenant)
    .bind(note)
    .bind(marker)
    .bind(format!("sha256:{}", "0".repeat(64)))
    .execute(admin)
    .await
    .unwrap();
}

async fn similarity_pool_scope(pool: &sqlx::PgPool) -> (i32, String, Option<String>) {
    sqlx::query_as(
        "SELECT pg_backend_pid(), current_setting('search_path'),
         NULLIF(current_setting('app.current_tenant', true),'')",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

struct SimilarityProbeFixture<'a> {
    archive: &'a matric_core::ArchiveInfo,
    tenant: Uuid,
    root: Uuid,
    near: Uuid,
    set: Uuid,
    dimension: i32,
}

/// Exercise the real HTTP handler with committed DDL visible to its runtime
/// connection. Join errors capture assertion/timeout panics until restoration
/// and public-decoy cleanup finish; no security constraints are relaxed.
async fn profile_similarity_http_relation_probes(
    app: &Router,
    admin: &sqlx::PgPool,
    singleton: &sqlx::PgPool,
    fixture: SimilarityProbeFixture<'_>,
    path: &str,
    missing_root: &(StatusCode, serde_json::Value),
) {
    let SimilarityProbeFixture {
        archive,
        tenant,
        root,
        near,
        set,
        dimension,
    } = fixture;
    let schema = &archive.schema_name;
    let marker = format!("public-decoy-{}", Uuid::new_v4());
    // Public decoys match the archive's tenant, set and note UUIDs. If a
    // relation falls back, identity canaries or cosine 1 replace local data.
    sqlx::query(&format!(
        "INSERT INTO public.embedding_set
        (id,tenant_id,name,slug,set_type,mode,criteria,auto_embed_rules,
         vector_source,defer_index_build,agent_metadata,embedding_config_id)
        SELECT id,tenant_id,name,slug,set_type,mode,criteria,auto_embed_rules,
         vector_source,defer_index_build,agent_metadata,embedding_config_id
        FROM {schema}.embedding_set WHERE id=$1 AND tenant_id=$2"
    ))
    .bind(set)
    .bind(tenant)
    .execute(admin)
    .await
    .unwrap();
    for id in [root, near] {
        seed_similarity_competitor(
            admin,
            "public",
            tenant,
            id,
            SimilarityVectorFixture {
                set,
                dimension,
                kind: "profile",
            },
            true,
            &format!("{marker}-{id}"),
        )
        .await;
    }
    let baseline = similarity_http_call(app, path, &archive.name, "read").await;
    let baseline_scope = similarity_pool_scope(singleton).await;
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(singleton)
        .await
        .unwrap();
    let role = format!("\"{}\"", role.replace('"', "\"\""));
    let hidden_schema = format!("hidden_{}", Uuid::new_v4().simple());
    let mut faults = Vec::new();
    for relation in ["source_identity", "embedding", "embedding_set_member"] {
        faults.push((
            format!("missing {relation}"),
            format!("ALTER TABLE {schema}.{relation} RENAME TO similarity_hidden_relation"),
            format!("ALTER TABLE {schema}.similarity_hidden_relation RENAME TO {relation}"),
            false,
        ));
    }
    faults.push((
        "SELECT denied".to_string(),
        format!("REVOKE SELECT ON {schema}.source_identity FROM {role}"),
        format!("GRANT SELECT ON {schema}.source_identity TO {role}"),
        false,
    ));
    faults.push((
        "USAGE denied".to_string(),
        format!("REVOKE USAGE ON SCHEMA {schema} FROM {role}"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        true,
    ));
    faults.push((
        "missing schema".to_string(),
        format!("ALTER SCHEMA {schema} RENAME TO {hidden_schema}"),
        format!("ALTER SCHEMA {hidden_schema} RENAME TO {schema}"),
        true,
    ));
    let mut observations = Vec::new();
    for (label, remove, restore, admission_failure) in faults {
        sqlx::query(&remove).execute(admin).await.unwrap();
        let probe_app = app.clone();
        let probe_path = path.to_string();
        let probe_archive = archive.name.clone();
        let failed = tokio::spawn(async move {
            similarity_http_call(&probe_app, &probe_path, &probe_archive, "read").await
        })
        .await;
        // Restore before interpreting the HTTP result, including task panics.
        let restored = sqlx::query(&restore).execute(admin).await;
        if let Err(error) = restored {
            // Retry restoration before reporting an infrastructure failure.
            sqlx::query(&restore)
                .execute(admin)
                .await
                .unwrap_or_else(|_| panic!("fixture restoration failed: {error}"));
        }
        let after_failure = similarity_pool_scope(singleton).await;
        let probe_app = app.clone();
        let probe_path = path.to_string();
        let probe_archive = archive.name.clone();
        let recovered = tokio::spawn(async move {
            similarity_http_call(&probe_app, &probe_path, &probe_archive, "read").await
        })
        .await;
        let after_success = similarity_pool_scope(singleton).await;
        observations.push((
            label,
            admission_failure,
            failed,
            recovered,
            after_failure,
            after_success,
        ));
    }
    // Delete only this probe's public rows, before asserting observations.
    sqlx::query("DELETE FROM public.note WHERE tenant_id=$1 AND id=ANY($2)")
        .bind(tenant)
        .bind(vec![root, near])
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("DELETE FROM public.embedding_set WHERE tenant_id=$1 AND id=$2")
        .bind(tenant)
        .bind(set)
        .execute(admin)
        .await
        .unwrap();
    assert_eq!(baseline.0, StatusCode::OK);
    assert_eq!(baseline.1["results"][0]["note_id"], near.to_string());
    assert!((baseline.1["results"][0]["score"].as_f64().unwrap() - 0.8).abs() < 0.001);
    assert!(!baseline.1.to_string().contains(&marker));
    for (label, admission_failure, failed, recovered, after_failure, after_success) in observations
    {
        let failed = failed.expect("HTTP failure probe must complete without panic");
        if admission_failure {
            assert_eq!(&failed, missing_root, "private admission failure: {label}");
        } else {
            assert_eq!(
                failed.0,
                StatusCode::INTERNAL_SERVER_ERROR,
                "{label}: {}",
                failed.1
            );
            assert_eq!(failed.1["detail"], "An internal error occurred.", "{label}");
        }
        assert!(failed.1.get("results").is_none(), "{label}");
        let serialized = failed.1.to_string();
        for private in [
            schema.as_str(),
            hidden_schema.as_str(),
            marker.as_str(),
            &root.to_string(),
            &near.to_string(),
            &set.to_string(),
            &tenant.to_string(),
        ] {
            assert!(
                !serialized.contains(private),
                "private failure detail: {label}"
            );
        }
        assert_eq!(
            recovered.expect("restored HTTP probe must complete"),
            baseline,
            "{label}"
        );
        assert_eq!(
            after_failure, baseline_scope,
            "failed request scope reset: {label}"
        );
        assert_eq!(
            after_success, baseline_scope,
            "successful request scope reset: {label}"
        );
    }
}

async fn similarity_http_call(
    app: &Router,
    path: &str,
    archive: &str,
    scope: &str,
) -> (StatusCode, serde_json::Value) {
    let mut req = request(Method::GET, path, archive, scope);
    req.headers_mut().insert(
        "x-request-id",
        "00000000-0000-4000-8000-000000000001".parse().unwrap(),
    );
    let response =
        tokio::time::timeout(std::time::Duration::from_secs(12), app.clone().oneshot(req))
            .await
            .expect("single-connection request must not deadlock")
            .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&body).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&body).into_owned())
        }),
    )
}

/// #1342/#1280: the real archive job-control, note purge and note move handlers
/// behind the production identity, tenant-scope, archive-routing and
/// authorization middleware.
fn archive_job_note_router(state: AppState, pool: sqlx::PgPool, tenant: Uuid) -> Router {
    Router::new()
        .route("/api/v1/archives/{name}", get(get_archive))
        .route("/api/v1/notes/{id}", get(get_note))
        .route("/api/v1/notes/{id}/purge", post(purge_note))
        .route("/api/v1/notes/{id}/move", post(move_note_to_collection))
        .route("/api/v1/jobs/pause/{archive}", post(pause_jobs_archive))
        .route("/api/v1/jobs/resume/{archive}", post(resume_jobs_archive))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_fixture_identity))
        .layer(Extension(FixtureIdentity { tenant_id: tenant }))
        .with_state(state)
}

async fn seed_collection(admin: &sqlx::PgPool, schema: &str, tenant: Uuid) -> Uuid {
    matric_db::validate_schema_name(schema).expect("fixture archive schema");
    let id = Uuid::new_v4();
    sqlx::query(&format!(
        "INSERT INTO \"{schema}\".collection (id, name, created_at_utc, tenant_id) \
         VALUES ($1, $2, now(), $3)"
    ))
    .bind(id)
    .bind(format!("fixture-{id}"))
    .bind(tenant)
    .execute(admin)
    .await
    .expect("seed synthetic fixture collection");
    id
}

fn json_request(
    method: Method,
    path: &str,
    archive: &str,
    scope: &str,
    body: &str,
) -> axum::http::Request<Body> {
    let mut request = request(method, path, archive, scope);
    request.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/json".parse().expect("content type"),
    );
    *request.body_mut() = Body::from(body.to_owned());
    request
}

#[tokio::test]
async fn hosted_archive_job_control_purge_and_note_move_live_db() {
    assert_eq!(
        std::env::var("FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS").as_deref(),
        Ok("1"),
        "run this required test through with-ephemeral-postgres.mjs"
    );
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be supplied by the runner-owned ephemeral PostgreSQL fixture");
    let admin = matric_db::create_pool(&database_url)
        .await
        .expect("connect runner-owned PostgreSQL fixture");
    let admin_db = Database::new(admin.clone());
    admin_db
        .migrate()
        .await
        .expect("migrate runner-owned PostgreSQL fixture");

    let role = format!("jobmove_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' \
         NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .expect("create powerless fixture runtime role");
    for grant in [
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role}"),
        format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"),
    ] {
        sqlx::query(&grant)
            .execute(&admin)
            .await
            .expect("grant fixture runtime role");
    }
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            database_url
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("fixture PostgreSQL URL")
                .username(&role)
                .password(&password),
        )
        .await
        .expect("connect powerless fixture runtime role");
    let tenant_a = Uuid::new_v4();
    bootstrap_tenant(&admin, tenant_a, &format!("jobmove-a-{tenant_a}")).await;
    let archives = matric_db::administrative::AdministrativeExecutor::new(admin.clone(), &runtime)
        .await
        .expect("create tenant-aware archive executor");
    let archive_a = archives
        .create_archive(
            tenant_a,
            &format!("jobmove-a-{}", Uuid::new_v4().simple()),
            None,
        )
        .await
        .expect("create synthetic tenant A archive");
    let archive_other = archives
        .create_archive(
            tenant_a,
            &format!("jobmove-o-{}", Uuid::new_v4().simple()),
            None,
        )
        .await
        .expect("create second synthetic tenant A archive");
    assert_ne!(archive_a.schema_name, "public");

    let note_id = seed_note(&admin, &archive_a.schema_name, tenant_a).await;
    let purge_note_id = seed_note(&admin, &archive_a.schema_name, tenant_a).await;
    let same_collection = seed_collection(&admin, &archive_a.schema_name, tenant_a).await;
    let other_collection = seed_collection(&admin, &archive_other.schema_name, tenant_a).await;

    // Single-tenant dedicated deployments run application state on a pool whose
    // connections are bound to the deployment tenant, while the per-request
    // middleware uses the unscoped runtime pool for its tenant transactions.
    let (_, credentials_and_host) = database_url
        .split_once('@')
        .expect("fixture PostgreSQL URL has a user");
    let tenant_pool = matric_db::create_pool_with_config(
        &format!("postgresql://{role}:{password}@{credentials_and_host}"),
        matric_db::PoolConfig::new()
            .max_connections(3)
            .single_tenant(tenant_a),
    )
    .await
    .expect("connect tenant-bound fixture runtime pool");
    let db = Database::new(tenant_pool.clone());
    let mut state = super::tests::build_call_api_test_state(db.clone(), &database_url).await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = std::sync::Arc::new(RoleBasedPolicy);
    let pause_state = matric_jobs::PauseState::load(tenant_pool.clone())
        .await
        .expect("load pause state");
    state.pause_state = Some(pause_state.clone());
    let app = archive_job_note_router(state.clone(), runtime.clone(), tenant_a);
    let archive = archive_a.name.as_str();
    let admin_scope = "read write admin";

    // Archive pause/resume as admin on an existing non-public archive.
    let paused = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/v1/jobs/pause/{archive}"),
            archive,
            admin_scope,
        ))
        .await
        .expect("pause response");
    assert_eq!(paused.status(), StatusCode::OK, "admin archive pause");
    assert!(pause_state.is_archive_paused(&archive_a.schema_name).await);
    let readback = pause_state.state(None).await;
    assert_eq!(
        readback
            .archives
            .get(&archive_a.schema_name)
            .map(String::as_str),
        Some("paused")
    );
    let resumed = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/v1/jobs/resume/{archive}"),
            archive,
            admin_scope,
        ))
        .await
        .expect("resume response");
    assert_eq!(resumed.status(), StatusCode::OK, "admin archive resume");
    assert!(!pause_state.is_archive_paused(&archive_a.schema_name).await);
    assert!(!pause_state
        .state(None)
        .await
        .archives
        .contains_key(&archive_a.schema_name));

    // A missing archive is refused exactly like GET /archives/{name}.
    let missing = "no-such-archive";
    let baseline = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/api/v1/archives/{missing}"),
            archive,
            admin_scope,
        ))
        .await
        .expect("archive baseline response")
        .status();
    assert!(
        matches!(baseline, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "baseline {baseline}"
    );
    for action in ["pause", "resume"] {
        let status = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/v1/jobs/{action}/{missing}"),
                archive,
                admin_scope,
            ))
            .await
            .expect("missing archive response")
            .status();
        assert_eq!(
            status, baseline,
            "{action} missing archive must match /archives/{{name}}"
        );
    }

    // A non-admin principal is refused even for an existing archive.
    for action in ["pause", "resume"] {
        let status = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/v1/jobs/{action}/{archive}"),
                archive,
                "read write",
            ))
            .await
            .expect("non-admin response")
            .status();
        assert_eq!(status, StatusCode::FORBIDDEN, "non-admin {action}");
    }
    assert!(!pause_state.is_archive_paused(&archive_a.schema_name).await);

    // Purge of a note in a non-public archive: the queued job carries the
    // archive schema, succeeds, and the note is gone.
    let purged = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/v1/notes/{purge_note_id}/purge"),
            archive,
            admin_scope,
        ))
        .await
        .expect("purge response");
    assert_eq!(purged.status(), StatusCode::OK, "purge enqueue");
    let job_row: (Uuid, serde_json::Value) = sqlx::query_as(
        "SELECT id, payload FROM job_queue \
         WHERE note_id = $1 AND job_type = 'purge_note'::job_type",
    )
    .bind(purge_note_id)
    .fetch_one(&admin)
    .await
    .expect("queued purge job");
    assert_eq!(
        job_row.1["schema"],
        serde_json::json!(archive_a.schema_name),
        "purge job must carry the request archive schema"
    );
    let job = admin_db
        .jobs
        .get(job_row.0)
        .await
        .expect("load purge job")
        .expect("purge job exists");
    let result = PurgeNoteHandler::new(admin_db.clone())
        .execute(JobContext::new(job))
        .await;
    assert!(
        matches!(result, JobResult::Success(_)),
        "purge job must succeed: {result:?}"
    );
    let remaining: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM \"{}\".note WHERE id = $1",
        archive_a.schema_name
    ))
    .bind(purge_note_id)
    .fetch_one(&admin)
    .await
    .expect("count purged note");
    assert_eq!(remaining, 0, "purged note must be gone from its archive");

    // Move into a same-archive collection is admitted and applied.
    let moved = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/notes/{note_id}/move"),
            archive,
            "read write",
            &serde_json::json!({ "collection_id": same_collection }).to_string(),
        ))
        .await
        .expect("move response");
    assert_eq!(moved.status(), StatusCode::NO_CONTENT, "same-archive move");
    let placed: Option<Uuid> = sqlx::query_scalar(&format!(
        "SELECT collection_id FROM \"{}\".note WHERE id = $1",
        archive_a.schema_name
    ))
    .bind(note_id)
    .fetch_one(&admin)
    .await
    .expect("read back moved note");
    assert_eq!(placed, Some(same_collection));

    // A collection in another archive, or one that does not exist, is refused
    // and leaves the membership unchanged.
    for (label, target) in [
        ("cross-archive", other_collection),
        ("missing", Uuid::new_v4()),
    ] {
        let status = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                &format!("/api/v1/notes/{note_id}/move"),
                archive,
                "read write",
                &serde_json::json!({ "collection_id": target }).to_string(),
            ))
            .await
            .expect("refused move response")
            .status();
        assert!(
            status.is_client_error(),
            "{label} collection move must be refused with 4xx, got {status}"
        );
        let still: Option<Uuid> = sqlx::query_scalar(&format!(
            "SELECT collection_id FROM \"{}\".note WHERE id = $1",
            archive_a.schema_name
        ))
        .bind(note_id)
        .fetch_one(&admin)
        .await
        .expect("read back note after refused move");
        assert_eq!(
            still,
            Some(same_collection),
            "{label} move changed membership"
        );
    }

    // A missing note and a read-only principal are refused before the handler.
    let missing_note = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/notes/{}/move", Uuid::new_v4()),
            archive,
            "read write",
            &serde_json::json!({ "collection_id": same_collection }).to_string(),
        ))
        .await
        .expect("missing note move response")
        .status();
    assert_eq!(missing_note, StatusCode::FORBIDDEN);
    let read_only = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/notes/{note_id}/move"),
            archive,
            "read",
            &serde_json::json!({ "collection_id": same_collection }).to_string(),
        ))
        .await
        .expect("read-only move response")
        .status();
    assert_eq!(read_only, StatusCode::FORBIDDEN);

    drop(app);
    archives
        .drop_archive(tenant_a, &archive_a.name)
        .await
        .expect("drop archive");
    archives
        .drop_archive(tenant_a, &archive_other.name)
        .await
        .expect("drop second archive");
    tenant_pool.close().await;
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .expect("drop fixture runtime grants");
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .expect("drop fixture runtime role");
    admin.close().await;
}

/// #1342 follow-up: real backup-file and embedding-config handlers behind the
/// production identity, tenant-scope, archive-routing and authorization middleware.
fn admin_resource_router(state: AppState, pool: sqlx::PgPool, tenant: Uuid) -> Router {
    Router::new()
        .route("/api/v1/backup/list/{filename}", get(get_backup_info))
        .route(
            "/api/v1/backup/metadata/{filename}",
            get(get_backup_metadata).put(update_backup_metadata),
        )
        .route(
            "/api/v1/backup/knowledge-archive/{filename}",
            get(knowledge_archive_download),
        )
        .route(
            "/api/v1/embedding-configs/{id}",
            get(get_embedding_config).delete(delete_embedding_config),
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
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_fixture_identity))
        .layer(Extension(FixtureIdentity { tenant_id: tenant }))
        .with_state(state)
}

struct BackupDestGuard(Option<std::ffi::OsString>);

impl Drop for BackupDestGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("BACKUP_DEST", value),
            None => std::env::remove_var("BACKUP_DEST"),
        }
    }
}

#[tokio::test]
async fn hosted_backup_file_and_embedding_config_routes_live_db() {
    assert_eq!(
        std::env::var("FORTEMI_REQUIRE_LIVE_POSTGRES_TESTS").as_deref(),
        Ok("1"),
        "run this required test through with-ephemeral-postgres.mjs"
    );
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be supplied by the runner-owned ephemeral PostgreSQL fixture");
    let admin = matric_db::create_pool(&database_url)
        .await
        .expect("connect runner-owned PostgreSQL fixture");
    Database::new(admin.clone())
        .migrate()
        .await
        .expect("migrate runner-owned PostgreSQL fixture");

    let role = format!("adminres_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' \
         NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .expect("create powerless fixture runtime role");
    for grant in [
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role}"),
        format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"),
    ] {
        sqlx::query(&grant)
            .execute(&admin)
            .await
            .expect("grant fixture runtime role");
    }
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            database_url
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("fixture PostgreSQL URL")
                .username(&role)
                .password(&password),
        )
        .await
        .expect("connect powerless fixture runtime role");

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    bootstrap_tenant(&admin, tenant_a, &format!("adminres-a-{tenant_a}")).await;
    bootstrap_tenant(&admin, tenant_b, &format!("adminres-b-{tenant_b}")).await;
    let archives = matric_db::administrative::AdministrativeExecutor::new(admin.clone(), &runtime)
        .await
        .expect("create tenant-aware archive executor");
    let archive_a = archives
        .create_archive(
            tenant_a,
            &format!("adminres-a-{}", Uuid::new_v4().simple()),
            None,
        )
        .await
        .expect("create synthetic tenant A archive");
    let config_a: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_config WHERE tenant_id = $1 AND is_default LIMIT 1",
    )
    .bind(tenant_a)
    .fetch_one(&admin)
    .await
    .expect("tenant A default embedding config");
    let config_b: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_config WHERE tenant_id = $1 AND is_default LIMIT 1",
    )
    .bind(tenant_b)
    .fetch_one(&admin)
    .await
    .expect("tenant B default embedding config");

    let (_, credentials_and_host) = database_url
        .split_once('@')
        .expect("fixture PostgreSQL URL has a user");
    let tenant_pool = matric_db::create_pool_with_config(
        &format!("postgresql://{role}:{password}@{credentials_and_host}"),
        matric_db::PoolConfig::new()
            .max_connections(3)
            .single_tenant(tenant_a),
    )
    .await
    .expect("connect tenant-bound fixture runtime pool");
    let db = Database::new(tenant_pool.clone());
    let mut state = super::tests::build_call_api_test_state(db, &database_url).await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = std::sync::Arc::new(RoleBasedPolicy);
    let app = admin_resource_router(state, runtime.clone(), tenant_a);
    let archive = archive_a.name.as_str();
    let admin_scope = "read write admin";

    // The backup directory is a child of a scratch directory that also holds a
    // secret sibling, so any escape would be observable.
    let scratch = tempfile::tempdir().expect("scratch dir");
    let backups = scratch.path().join("backups");
    std::fs::create_dir(&backups).expect("backup dir");
    std::fs::write(backups.join("snap_live.sql.gz"), b"backup-bytes").expect("backup file");
    std::fs::write(scratch.path().join("secret.txt"), b"SECRET-OUTSIDE").expect("secret file");
    let _restore = BackupDestGuard(std::env::var_os("BACKUP_DEST"));
    std::env::set_var("BACKUP_DEST", &backups);

    async fn status(
        app: &Router,
        method: Method,
        path: &str,
        archive: &str,
        scope: &str,
    ) -> StatusCode {
        let mut request = request(method.clone(), path, archive, scope);
        if method == Method::PUT {
            request.headers_mut().insert(
                header::CONTENT_TYPE,
                "application/json".parse().expect("content type"),
            );
            *request.body_mut() = Body::from(r#"{"title":"Live title"}"#);
        }
        app.clone()
            .oneshot(request)
            .await
            .expect("response")
            .status()
    }

    // An existing backup file is admitted on every filename route.
    for path in [
        "/api/v1/backup/list/snap_live.sql.gz",
        "/api/v1/backup/metadata/snap_live.sql.gz",
        "/api/v1/backup/knowledge-archive/snap_live.sql.gz",
    ] {
        assert_eq!(
            status(&app, Method::GET, path, archive, admin_scope).await,
            StatusCode::OK,
            "admin GET {path}"
        );
        // The same route is refused for a non-admin principal.
        assert_eq!(
            status(&app, Method::GET, path, archive, "read write").await,
            StatusCode::FORBIDDEN,
            "non-admin GET {path}"
        );
    }
    assert_eq!(
        status(
            &app,
            Method::PUT,
            "/api/v1/backup/metadata/snap_live.sql.gz",
            archive,
            admin_scope
        )
        .await,
        StatusCode::OK,
        "admin PUT metadata"
    );
    assert!(backups.join("snap_live.sql.gz.meta.json").exists());

    // Missing files and traversal attempts never reach a handler.
    for path in [
        "/api/v1/backup/list/missing.sql.gz",
        "/api/v1/backup/metadata/missing.sql.gz",
        "/api/v1/backup/knowledge-archive/missing.sql.gz",
        "/api/v1/backup/list/..%2Fsecret.txt",
        "/api/v1/backup/metadata/..%2Fsecret.txt",
        "/api/v1/backup/knowledge-archive/..%2Fsecret.txt",
        "/api/v1/backup/knowledge-archive/%2e%2e%2fsecret.txt",
        "/api/v1/backup/list/%2e%2e",
        "/api/v1/backup/metadata/snap_live.sql.gz%2f..%2f..%2fsecret.txt",
        "/api/v1/backup/list/..%5csecret.txt",
        "/api/v1/backup/list/%2Fetc%2Fpasswd",
    ] {
        for method in [Method::GET, Method::PUT] {
            if method == Method::PUT && !path.contains("/metadata/") {
                continue;
            }
            assert_eq!(
                status(&app, method.clone(), path, archive, admin_scope).await,
                StatusCode::FORBIDDEN,
                "{method} {path} must fail closed before the handler"
            );
        }
    }
    assert!(
        !scratch.path().join("secret.txt.meta.json").exists(),
        "traversal must not create metadata outside the backup directory"
    );
    assert_eq!(
        std::fs::read(scratch.path().join("secret.txt")).unwrap(),
        b"SECRET-OUTSIDE"
    );

    // Embedding configs: existing -> admitted, missing/foreign -> 403, non-admin -> 403.
    let own = format!("/api/v1/embedding-configs/{config_a}");
    assert_eq!(
        status(&app, Method::GET, &own, archive, admin_scope).await,
        StatusCode::OK,
        "admin GET own embedding config"
    );
    assert_eq!(
        status(&app, Method::GET, &own, archive, "read write").await,
        StatusCode::FORBIDDEN,
        "non-admin GET embedding config"
    );
    for (label, id) in [("missing", Uuid::new_v4()), ("foreign-tenant", config_b)] {
        for method in [Method::GET, Method::DELETE] {
            assert_eq!(
                status(
                    &app,
                    method.clone(),
                    &format!("/api/v1/embedding-configs/{id}"),
                    archive,
                    admin_scope
                )
                .await,
                StatusCode::FORBIDDEN,
                "{label} embedding config {method}"
            );
        }
    }
    let foreign_still_there: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM embedding_config WHERE id = $1)")
            .bind(config_b)
            .fetch_one(&admin)
            .await
            .expect("foreign config readback");
    assert!(
        foreign_still_there,
        "foreign tenant config must be untouched"
    );

    drop(app);
    archives
        .drop_archive(tenant_a, &archive_a.name)
        .await
        .expect("drop archive");
    tenant_pool.close().await;
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .expect("drop fixture runtime grants");
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .expect("drop fixture runtime role");
    admin.close().await;
}

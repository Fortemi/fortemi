//! Live hosted authorization coverage for embedding-set route normalization.
//!
//! This intentionally requires the runner-owned ephemeral PostgreSQL fixture.
//! It must never fall back to a workstation database or silently skip.

use super::*;
use axum::routing::any;
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
        .route("/api/v1/notes/{id}/provenance", any(closed_route_probe))
        .route("/api/v1/notes/{id}/move", any(closed_route_probe))
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
    let app = embedding_authorization_router(state, runtime.clone(), tenant_a);
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

    // Every closed exception is dispatched through the production middleware
    // with an existing fixture object.  The in-tenant note is used by graph,
    // concepts, provenance, and the explicit POST /move case; the call route
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

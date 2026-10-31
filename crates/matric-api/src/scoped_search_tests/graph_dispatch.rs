//! #1280: actual middleware, backing normalization, handler and FORCE RLS.
use super::*;
mod fixtures;
mod policy;
use fixtures::*;

#[sqlx::test(migrations = false)]
async fn hosted_graph_dispatch_qualifies_live_root_and_all_graph_rows(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    for tenant in [a, b] {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(tenant).bind(format!("graph-{tenant}")).execute(&admin).await.unwrap();
    }
    // A missing archive relation must not resolve to public through search_path.
    let mut fallback = admin.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA graph_empty_fixture")
        .execute(&mut *fallback)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('search_path','graph_empty_fixture, public',true)")
        .execute(&mut *fallback)
        .await
        .unwrap();
    assert!(matric_db::PgLinkRepository::require_graph_archive_tx(
        &mut fallback,
        "graph_empty_fixture"
    )
    .await
    .is_err());
    fallback.rollback().await.unwrap();
    let provision = sqlx::postgres::PgPoolOptions::new().max_connections(1)
        .after_connect(|c,_| Box::pin(async move {
            sqlx::query("SELECT set_config('app.current_tenant','00000000-0000-0000-0000-000000000000',false)").execute(c).await?;
            Ok(())
        })).connect_with((*admin.connect_options()).clone()).await.unwrap();
    let db = Database::new(provision.clone());
    let archive = db
        .archives
        .create_archive_schema("graph_fixture", None)
        .await
        .unwrap();
    sqlx::query("UPDATE public.archive_registry SET tenant_id=$1 WHERE id=$2")
        .bind(a)
        .bind(archive.id)
        .execute(&admin)
        .await
        .unwrap();
    let public = seed_graph(&db, "public", a).await;
    let foreign = seed_graph(&db, "public", b).await;
    let archived = seed_graph(&db, &archive.schema_name, a).await;
    provision.close().await;
    let role = format!("graph_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"))
        .execute(&admin).await.unwrap();
    for schema in ["public", archive.schema_name.as_str()] {
        for grant in [
            format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
            format!("GRANT SELECT ON ALL TABLES IN SCHEMA {schema} TO {role}"),
        ] {
            sqlx::query(&grant).execute(&admin).await.unwrap();
        }
    }
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*admin.connect_options())
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user"
    )
    .fetch_one(&runtime)
    .await
    .unwrap());
    let mut state =
        tests::build_call_api_test_state(Database::new(runtime.clone()), "fixture").await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.hosted_auth = Some(Arc::new(GraphIdentity(a, b)));
    let router = app(state.clone(), runtime.clone());
    for (token, memory, ids) in [
        ("fixture-a", "public", public),
        ("fixture-b", "public", foreign),
        ("service-read", "graph_fixture", archived),
        ("service-write", "public", public),
    ] {
        let (status, body) = request(
            &router,
            token,
            memory,
            &ids[0].to_string(),
            Method::GET,
            "depth=10&max_nodes=1000&include_structural=false",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let returned: Vec<_> = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(returned.len(), 2, "deleted bridges must not be traversed");
        assert!(returned.contains(&ids[0].to_string()) && returned.contains(&ids[1].to_string()));
        assert_eq!(body["edges"].as_array().unwrap().len(), 1);
        for edge in body["edges"].as_array().unwrap() {
            assert!(returned.contains(&edge["source"].as_str().unwrap().to_owned()));
            assert!(returned.contains(&edge["target"].as_str().unwrap().to_owned()));
        }
    }
    let hidden_ids = [Uuid::new_v4(), foreign[0], archived[0], public[2]];
    let mut missing_body = None;
    for token in ["fixture-a", "service-read"] {
        for id in hidden_ids {
            let (status, bytes) =
                request_raw(&router, token, "public", &id.to_string(), Method::GET, "").await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            // No request-id middleware is installed here: compare actual bytes.
            if let Some(previous) = &missing_body {
                assert_eq!(previous, &bytes);
            } else {
                missing_body = Some(bytes.clone());
            }
            let text = String::from_utf8(bytes).unwrap();
            for secret in [
                id.to_string(),
                a.to_string(),
                b.to_string(),
                archive.schema_name.clone(),
            ] {
                assert!(
                    !text.contains(&secret),
                    "not-found must not expose resource fields"
                );
            }
        }
    }
    for token in ["service-none", "fixture-denied"] {
        let mut denied_body = None;
        for id in std::iter::once(public[0]).chain(hidden_ids) {
            let (status, bytes) =
                request_raw(&router, token, "public", &id.to_string(), Method::GET, "").await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            if let Some(previous) = &denied_body {
                assert_eq!(previous, &bytes);
            } else {
                denied_body = Some(bytes);
            }
        }
    }
    for (token, id, method) in [
        ("service-none", public[0].to_string(), Method::GET),
        ("service-read", "bad-id".into(), Method::GET),
        ("service-read", public[0].to_string(), Method::HEAD),
    ] {
        assert!(!request(&router, token, "public", &id, method, "")
            .await
            .0
            .is_success());
    }
    let (status, limited) = request(
        &router,
        "service-read",
        "public",
        &public[0].to_string(),
        Method::GET,
        "depth=-1&max_nodes=0&max_edges_per_node=0",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(limited["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(limited["nodes"][0]["id"], public[0].to_string());
    assert_eq!(limited["meta"]["effective_depth"], 0);
    assert!(limited["edges"].as_array().unwrap().is_empty());
    let stale = Router::new()
        .route("/api/v1/graph/{id}", get(stale_graph))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            runtime.clone(),
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());
    sqlx::query(&format!(
        "GRANT UPDATE(deleted_at) ON public.note TO {role}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let (status, bytes) = request_raw(
        &stale,
        "service-read",
        "public",
        &public[0].to_string(),
        Method::GET,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(Some(bytes), missing_body);
    // A distinct owner/visibility policy must still deny the CompleteCatalog route.
    let mut restricted = state.clone();
    restricted.authorization_policy = Arc::new(DenyNoteReads);
    assert!(!request(
        &app(restricted, runtime.clone()),
        "service-read",
        "public",
        &public[0].to_string(),
        Method::GET,
        ""
    )
    .await
    .0
    .is_success());
    // Missing scope cannot silently fall back to an unbound hosted pool.
    let unscoped = Router::new()
        .route("/api/v1/graph/{id}", get(explore_graph))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state);
    assert!(!unscoped
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/api/v1/graph/{}", public[0]))
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap()
        .status()
        .is_success());
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
}

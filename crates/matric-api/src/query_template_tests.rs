use super::*;
use matric_core::NoteRepository;
use serde_json::{json, Value};
use tower::ServiceExt;

fn profile(provider: &str, model: &str) -> EmbeddingConfigProfile {
    serde_json::from_value(json!({
        "id": Uuid::new_v4(), "name": "synthetic query config",
        "model": model, "dimension": 3, "vector_type": "vector",
        "chunk_size": 512, "chunk_overlap": 0, "is_default": false,
        "created_at": Utc::now(), "updated_at": Utc::now(),
        "supports_mrl": true, "provider": "custom",
        "provider_config": {"provider_id": provider}, "content_types": ["text"]
    }))
    .unwrap()
}

pub(super) fn registry(provider: &str, url: &str) -> matric_inference::ProviderRegistry {
    let mut registry = matric_inference::ProviderRegistry::new(provider.to_string());
    registry.register(matric_inference::ProviderConfig {
        id: provider.to_string(),
        base_url: url.to_string(),
        api_key: None,
        capabilities: vec![matric_inference::ProviderCapability::Embedding],
        timeout: std::time::Duration::from_secs(2),
        is_default: true,
        health: matric_inference::ProviderHealth::Healthy,
        http_referer: None,
        x_title: None,
    });
    registry
}

#[tokio::test]
async fn registry_providers_receive_one_wrapped_query_and_normalize_mrl() {
    let received = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let requests = received.clone();
    let route = post(move |Json(body): Json<Value>| {
        let requests = requests.clone();
        async move {
            let model = body["model"].clone();
            requests.lock().unwrap().push(body);
            Json(json!({
                "model": model, "embeddings": [[3.0, 4.0, 12.0]],
                "data": [{"index": 0, "embedding": [3.0, 4.0, 12.0]}],
                "usage": {"prompt_tokens": 1, "total_tokens": 1}
            }))
        }
    });
    let app = Router::new()
        .route("/embeddings", route.clone())
        .route("/api/embed", route);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    for provider in ["openai", "llamacpp", "openrouter", "ollama"] {
        let registry = registry(provider, &url);
        // E5 has a built-in Ollama query prefix; the set template takes precedence.
        let mut profile = profile(provider, "e5-large-v2");
        profile.provider_dimension = Some(3);
        for template in [None, Some("Instruct: Find companies\nQuery:{text}")] {
            let vector = generate_search_query_embedding(
                &registry,
                Some(&profile),
                Some(2),
                Some(Uuid::new_v4()),
                template,
                "synthetic company",
            )
            .await
            .unwrap();
            assert_eq!(vector.as_slice(), &[0.6, 0.8]);
            let expected =
                matric_core::apply_query_template(template, "synthetic company").unwrap();
            let request = received.lock().unwrap().pop().unwrap();
            assert_eq!(request["input"], json!([expected]), "{provider}");
        }
    }
    server.abort();
}

pub(super) async fn response(
    app: &Router,
    method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1048576)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[sqlx::test(migrations = false)]
async fn set_templates_persist_validate_as_problem_and_guard_degrades_to_fts(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let pool = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    db.migrate().await.unwrap();
    let state = tests::build_call_api_test_state(db.clone(), "fixture").await;
    state.inference_runtime.write().unwrap().provider_registry =
        Arc::new(registry("openai", "http://127.0.0.1:1"));
    let app = Router::new()
        .route("/api/v1/embedding-sets", post(create_embedding_set))
        .route(
            "/api/v1/embedding-sets/{slug}",
            get(get_embedding_set).patch(update_embedding_set),
        )
        .route("/api/v1/search", get(search_notes))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state);
    for template in [
        "".to_string(),
        "{text}{text}".to_string(),
        format!("{}{text}", "a".repeat(2048), text = "{text}"),
    ] {
        for (method, uri) in [
            (Method::POST, "/api/v1/embedding-sets"),
            (Method::PATCH, "/api/v1/embedding-sets/missing"),
        ] {
            let (status, body) = response(
                &app,
                method,
                uri,
                json!({"name":"synthetic", "query_template":template}),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(body["status"], 400);
            assert!(body["type"].is_string());
        }
    }

    let config = profile("openai", "synthetic");
    sqlx::query("INSERT INTO embedding_config (id,name,model,dimension,provider,provider_config) VALUES ($1,$2,$3,3,'custom',$4)")
        .bind(config.id).bind(&config.name).bind(&config.model).bind(&config.provider_config)
        .execute(&pool).await.unwrap();
    let (status, created) = response(&app, Method::POST, "/api/v1/embedding-sets", json!({
        "name":"Synthetic companies", "slug":"synthetic-companies", "set_type":"full",
        "embedding_config_id":config.id, "defer_index_build":true, "query_template":"Query:{text}"
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["query_template"], "Query:{text}");
    let uri = "/api/v1/embedding-sets/synthetic-companies";
    let (status, updated) = response(
        &app,
        Method::PATCH,
        uri,
        json!({"query_template":"Instruct: Find companies\nQuery:{text}"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let (_, read) = response(&app, Method::GET, uri, Value::Null).await;
    assert_eq!(read["query_template"], updated["query_template"]);
    let (_, unchanged) =
        response(&app, Method::PATCH, uri, json!({"description":"synthetic"})).await;
    assert_eq!(unchanged["query_template"], updated["query_template"]);

    let note_id = db
        .notes
        .insert(matric_core::CreateNoteRequest {
            content: "Synthetic company profile".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            document_type_id: None,
            title: Some("Synthetic company".to_string()),
            tags: None,
            metadata: None,
        })
        .await
        .unwrap();
    db.embedding_sets
        .add_members(
            "synthetic-companies",
            matric_core::AddMembersRequest {
                note_ids: vec![note_id],
                added_by: None,
            },
        )
        .await
        .unwrap();

    // Simulate a legacy or corrupt instruction that cannot be applied. The provider
    // URL is unreachable, so the stable preparation code also proves no raw call.
    sqlx::query(
        "UPDATE embedding_set SET query_template='unusable' WHERE slug='synthetic-companies'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = response(
        &app,
        Method::GET,
        "/api/v1/search?q=synthetic&mode=semantic&set=synthetic-companies",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["degraded"], true);
    assert_eq!(
        body["degradation"]["code"],
        "embedding_query_template_unsupported"
    );
    assert_eq!(body["degradation"]["effective_mode"], "fts");
    assert_eq!(body["results"][0]["note_id"], note_id.to_string(), "{body}");

    sqlx::query("UPDATE embedding_set SET embedding_config_id=$1, query_template='unusable' WHERE slug='default'")
        .bind(config.id).execute(&pool).await.unwrap();
    let (status, body) = response(
        &app,
        Method::GET,
        "/api/v1/search?q=synthetic&mode=hybrid",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["degraded"], true);
    assert_eq!(
        body["degradation"]["code"],
        "embedding_query_template_unsupported"
    );
    assert_eq!(body["results"][0]["note_id"], note_id.to_string(), "{body}");
    pool.close().await;
}

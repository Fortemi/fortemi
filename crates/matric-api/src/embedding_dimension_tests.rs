use super::*;
use query_template_tests::{registry, response};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(super) async fn database(admin: sqlx::PgPool) -> Database {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let pool = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let db = Database::new(pool);
    db.migrate().await.unwrap();
    db
}

#[sqlx::test(migrations = false)]
async fn query_template_external_mrl_import_and_provider_search_use_effective_dimension(
    admin: sqlx::PgPool,
) {
    let db = database(admin).await;
    let received = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let requests = received.clone();
    let mock = Router::new().route(
        "/embeddings",
        post(move |Json(body): Json<Value>| {
            let requests = requests.clone();
            async move {
                requests.lock().unwrap().push(body.clone());
                let raw = if body["model"] == "Qwen/Qwen3-Embedding-8B" {
                    4096
                } else {
                    8
                };
                let scale = if body["input"][0].as_str().unwrap().contains("scaled") {
                    7.0
                } else {
                    1.0
                };
                let mut vector = vec![0.0; raw];
                vector[0] = 3.0 * scale;
                vector[1] = 4.0 * scale;
                vector[raw - 1] = 12.0 * scale;
                Json(
                    json!({"model":body["model"],"data":[{"index":0,"embedding":vector}],
                    "usage":{"prompt_tokens":1,"total_tokens":1}}),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let state = tests::build_call_api_test_state(db.clone(), "fixture").await;
    state.inference_runtime.write().unwrap().provider_registry = Arc::new(registry("openai", &url));
    let app = Router::new()
        .route("/api/v1/search", get(search_notes))
        .route("/api/v1/embedding-configs", post(create_embedding_config))
        .route(
            "/api/v1/embedding-configs/{id}",
            get(get_embedding_config).patch(update_embedding_config),
        )
        .route("/api/v1/embedding-sets", post(create_embedding_set))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state.clone());
    for (slug, dimension, provider_dimension, model) in [
        ("mrl-external", 1024, Some(4096), "Qwen/Qwen3-Embedding-8B"),
        ("native-external", 8, None, "raw-8"),
    ] {
        let template =
            provider_dimension.map(|_| "Instruct: Find synthetic companies\nQuery:{text}");
        let space_contract = json!({"provider":"synthetic","model":model,"normalization":"l2"});
        let space_id = matric_core::embedding_space_id(&space_contract);
        let config_body = json!({"name":slug,"model":model,"dimension":dimension,
            "provider_dimension":provider_dimension,"provider":"custom",
            "provider_config":{"provider_id":"openai"},"space_contract":space_contract});
        for invalid in [Some(dimension as i32 - 1), Some(16385)] {
            let mut invalid_body = config_body.clone();
            invalid_body["provider_dimension"] = json!(invalid);
            let (status, body) = response(
                &app,
                Method::POST,
                "/api/v1/embedding-configs",
                invalid_body,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        }
        let (status, created) =
            response(&app, Method::POST, "/api/v1/embedding-configs", config_body).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["provider_dimension"], json!(provider_dimension));
        let config_id: Uuid = serde_json::from_value(created["id"].clone()).unwrap();
        let uri = format!("/api/v1/embedding-configs/{config_id}");
        let (_, read) = response(&app, Method::GET, &uri, Value::Null).await;
        assert_eq!(read["provider_dimension"], json!(provider_dimension));
        let (status, _) = response(
            &app,
            Method::PATCH,
            &uri,
            json!({"provider_dimension":16385}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        if provider_dimension.is_some() {
            let (status, updated) = response(
                &app,
                Method::PATCH,
                &uri,
                json!({"provider_dimension":4096}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{updated}");
            let (status, _) = response(&app, Method::PATCH, &uri, json!({"dimension":2001})).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let (status, set) = response(
            &app,
            Method::POST,
            "/api/v1/embedding-sets",
            json!({
                "name":slug,"slug":slug,"set_type":"full","mode":"manual",
                "embedding_config_id":config_id,"query_template":template,
                "vector_source":"external","defer_index_build":true
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{set}");
        let set_id: Uuid = serde_json::from_value(set["id"].clone()).unwrap();
        let mut expected_vector = vec![0.0; dimension];
        expected_vector[0] = 0.6;
        expected_vector[1] = 0.8;
        if provider_dimension.is_none() {
            expected_vector[0] = 3.0 / 13.0;
            expected_vector[1] = 4.0 / 13.0;
            expected_vector[7] = 12.0 / 13.0;
        }
        let profile = db
            .embedding_sets
            .get_config(config_id)
            .await
            .unwrap()
            .unwrap();
        let query = generate_search_query_embedding(
            &registry("openai", &url),
            Some(&profile),
            None,
            Some(set_id),
            template,
            "synthetic",
        )
        .await
        .unwrap();
        assert_eq!(query.as_slice().len(), dimension);
        if provider_dimension.is_some() {
            assert_eq!(query.as_slice(), expected_vector);
        } else {
            assert_eq!(&query.as_slice()[..2], &[3., 4.]);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("profiles")).unwrap();
        let mut other_vector = vec![0.0; dimension];
        other_vector[0] = -1.0;
        let rows = [("expected", expected_vector), ("other", other_vector)].into_iter().map(|(entity, vector)| {
            json!({"entity_id":format!("{slug}-{entity}"),"source":"synthetic-1217",
                "profile_text":entity,"profile_hash":hex::encode(Sha256::digest(format!("profile-v1\n{entity}"))),
                "template_version":"profile-v1","space_id":space_id,"dims":dimension,"embedding":vector})
        }).collect::<Vec<_>>();
        let body = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(root.path().join("profiles/vectors.jsonl"), &body).unwrap();
        std::fs::write(root.path().join("manifest.json"), json!({
            "run_id":slug,"space_id":space_id,"created_at":"2026-10-15T00:00:00Z",
            "counts":{"profiles":2,"deletions":0},"template_version":"profile-v1",
            "files":[{"path":"profiles/vectors.jsonl","rows":2,"sha256":hex::encode(Sha256::digest(&body))}]
        }).to_string()).unwrap();
        let report = matric_db::PgEmbeddingImportRepository::new(db.pool.clone())
            .import_run_folder("public", slug, root.path(), None, None)
            .await
            .unwrap();
        assert_eq!(report.inserted, 2, "{report:?}");
        assert_eq!(report.rejected.total, 0);
        // Membership is independent of importing entity profile vectors.
        sqlx::query(
            "INSERT INTO embedding_set_member (embedding_set_id,note_id,membership_type)
            SELECT $1,note_id,'include' FROM embedding WHERE embedding_set_id=$1
            ON CONFLICT DO NOTHING",
        )
        .bind(set_id)
        .execute(&db.pool)
        .await
        .unwrap();
        let expected_note: Uuid = sqlx::query_scalar(
            "SELECT note_id FROM embedding WHERE embedding_set_id=$1 AND profile_text='expected'",
        )
        .bind(set_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        for set_query in [format!("&set={slug}"), String::new()] {
            if set_query.is_empty() {
                sqlx::query("UPDATE embedding_set SET slug='previous-default',is_system=false WHERE slug='default'")
                    .execute(&db.pool).await.unwrap();
                sqlx::query("UPDATE embedding_set SET slug='default',is_system=true WHERE id=$1")
                    .bind(set_id)
                    .execute(&db.pool)
                    .await
                    .unwrap();
            }
            let (status, body) = response(
                &app,
                Method::GET,
                &format!("/api/v1/search?q={slug}&mode=semantic{set_query}"),
                Value::Null,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["degraded"], false, "{body}");
            assert_eq!(
                body["results"][0]["note_id"],
                expected_note.to_string(),
                "{body}"
            );
            let request = received.lock().unwrap().pop().unwrap();
            let expected_query = if template.is_some() {
                format!("Instruct: Find synthetic companies\nQuery:{slug}")
            } else {
                slug.to_string()
            };
            assert_eq!(request["input"], json!([expected_query]));
            if set_query.is_empty() {
                sqlx::query("UPDATE embedding_set SET slug=$2,is_system=false WHERE id=$1")
                    .bind(set_id)
                    .bind(slug)
                    .execute(&db.pool)
                    .await
                    .unwrap();
                sqlx::query("UPDATE embedding_set SET slug='default',is_system=true WHERE slug='previous-default'")
                    .execute(&db.pool).await.unwrap();
            }
        }
        let search_uri = format!("/api/v1/search?q={slug}&mode=semantic&set={slug}");
        let (_, baseline) = response(&app, Method::GET, &search_uri, Value::Null).await;
        let (_, scaled_query) = response(
            &app,
            Method::GET,
            &search_uri.replace("q=", "q=scaled-"),
            Value::Null,
        )
        .await;
        assert_rankings_equal(&baseline, &scaled_query);
        sqlx::query("UPDATE embedding SET vector=(SELECT array_agg(v*11)::vector FROM unnest(vector::real[]) v)
            WHERE embedding_set_id=$1").bind(set_id).execute(&db.pool).await.unwrap();
        let (_, scaled_stored) = response(&app, Method::GET, &search_uri, Value::Null).await;
        assert_rankings_equal(&baseline, &scaled_stored);
        if provider_dimension.is_some() {
            super::shard_query_template_tests::assert_external_restore_search(
                &state,
                &url,
                set_id,
                expected_note,
                &baseline,
            )
            .await;
        }
        sqlx::query("UPDATE embedding_set SET defer_index_build=false WHERE id=$1")
            .bind(set_id)
            .execute(&db.pool)
            .await
            .unwrap();
        matric_db::vector_index::enqueue_build_for_config(
            &db.pool, "public", config_id, false, "mrl_test",
        )
        .await
        .unwrap();
        let health = matric_db::index_health::set_index_health(&db.pool, "public", set_id, None)
            .await
            .unwrap();
        assert_eq!(health.dimension as usize, dimension);
        assert_eq!(
            health.index_name,
            format!("idx_embedding_hnsw_vector_{dimension}")
        );
        let queued: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM job_queue
            WHERE job_type='build_set_index' AND (payload->>'dimension')::int=$1)",
        )
        .bind(dimension as i32)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(queued);
        let mut tx = db.pool.begin().await.unwrap();
        let manual_job =
            matric_db::vector_index::clear_defer_and_enqueue_for_set_tx(&mut tx, "public", set_id)
                .await
                .unwrap()
                .unwrap();
        tx.commit().await.unwrap();
        let payload: Value = sqlx::query_scalar("SELECT payload FROM job_queue WHERE id=$1")
            .bind(manual_job)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(payload["dimension"], dimension);
        matric_db::vector_index::build_vector_indexes(
            &db.pool,
            serde_json::from_value(payload).unwrap(),
        )
        .await
        .unwrap();
        let health = matric_db::index_health::set_index_health(&db.pool, "public", set_id, None)
            .await
            .unwrap();
        assert_eq!(health.index_valid, Some(true));
        let definition: String = sqlx::query_scalar(
            "SELECT indexdef FROM pg_indexes WHERE schemaname='public' AND indexname=$1",
        )
        .bind(&health.index_name)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(definition.contains("vector_cosine_ops"), "{definition}");
    }
    server.abort();
    db.pool.close().await;
}

pub(super) fn assert_rankings_equal(left: &Value, right: &Value) {
    assert_eq!(right["degraded"], false, "{right}");
    let left = left["results"].as_array().unwrap();
    let right = right["results"].as_array().unwrap();
    assert!(!left.is_empty());
    assert_eq!(left.len(), right.len());
    for (a, b) in left.iter().zip(right) {
        assert_eq!(a["note_id"], b["note_id"]);
        assert!((a["score"].as_f64().unwrap() - b["score"].as_f64().unwrap()).abs() < 1e-5);
    }
}

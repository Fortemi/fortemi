use super::*;
use embedding_dimension_tests::database;
use query_template_tests::{registry, response};
use serde_json::{json, Value};

pub(super) async fn assert_external_writes_suppressed(
    db: &Database,
    schema: &str,
    set_id: Uuid,
    note_id: Uuid,
) {
    use matric_jobs::{JobContext, JobHandler, JobResult};
    let ctx = db.for_schema(schema).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let before: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id,vector::text FROM embedding WHERE embedding_set_id=$1 ORDER BY id",
    )
    .bind(set_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert!(!before.is_empty());
    tx.commit().await.unwrap();
    let job: matric_core::Job = serde_json::from_value(json!({
        "id":Uuid::new_v4(),"note_id":note_id,"job_type":"embedding","status":"running",
        "priority":1,"payload":{"embedding_set_id":set_id,"schema":schema},"result":null,
        "error_message":null,"progress_percent":0,"progress_message":null,"retry_count":0,
        "max_retries":1,"created_at":Utc::now(),"started_at":Utc::now(),"completed_at":null,
        "cost_tier":null,"initiated_by_user_id":null
    }))
    .unwrap();
    let handler = handlers::EmbeddingHandler::new(
        db.clone(),
        Arc::new(registry("openai", "http://127.0.0.1:1")),
        Arc::new(matric_core::InMemoryMeter::default()),
    );
    assert!(matches!(
        handler.execute(JobContext::new(job.clone())).await,
        JobResult::Failed(_)
    ));
    let mut refresh = job;
    refresh.job_type = matric_core::JobType::RefreshEmbeddingSet;
    refresh.payload.as_mut().unwrap()["set_id"] = json!(set_id);
    let handler = handlers::RefreshEmbeddingSetHandler::new(db.clone());
    assert!(matches!(
        handler.execute(JobContext::new(refresh)).await,
        JobResult::Success(_)
    ));
    let mut tx = ctx.begin_tx().await.unwrap();
    let after: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id,vector::text FROM embedding WHERE embedding_set_id=$1 ORDER BY id",
    )
    .bind(set_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(before, after);
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.job_queue WHERE job_type='embedding'
         AND payload->>'embedding_set_id'=$1",
    )
    .bind(set_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(queued, 0);
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn ownership_change_keeps_existing_template_less_body_chunks_searchable(admin: sqlx::PgPool) {
    let db = database(admin).await;
    let state = tests::build_call_api_test_state(db.clone(), "fixture").await;
    let config_id = Uuid::new_v4();
    sqlx::query("INSERT INTO embedding_config(id,name,model,dimension) VALUES($1,'ownership','synthetic',3)")
        .bind(config_id).execute(&db.pool).await.unwrap();
    let app = Router::new()
        .route("/api/v1/embedding-sets", post(create_embedding_set))
        .route("/api/v1/embedding-sets/{slug}", patch(update_embedding_set))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state);
    let (status, set) = response(
        &app,
        Method::POST,
        "/api/v1/embedding-sets",
        json!({
            "name":"ownership","slug":"ownership","set_type":"full","mode":"manual",
            "embedding_config_id":config_id,"defer_index_build":true
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{set}");
    let set_id: Uuid = serde_json::from_value(set["id"].clone()).unwrap();
    use matric_core::NoteRepository;
    for vector in [vec![3., 4., 1.], vec![1., 0., 0.]] {
        let request = matric_core::CreateNoteRequest {
            content: "synthetic body".into(),
            format: "markdown".into(),
            source: "test".into(),
            collection_id: None,
            document_type_id: None,
            title: None,
            tags: None,
            metadata: None,
        };
        let note = db.notes.insert(request).await.unwrap();
        sqlx::query(
            "INSERT INTO embedding(id,note_id,embedding_set_id,chunk_index,text,vector,model)
            VALUES($1,$2,$3,0,'body',$4,'synthetic')",
        )
        .bind(Uuid::new_v4())
        .bind(note)
        .bind(set_id)
        .bind(matric_core::Vector::from(vector))
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO embedding_set_member(embedding_set_id,note_id,membership_type) VALUES($1,$2,'include')")
            .bind(set_id).bind(note).execute(&db.pool).await.unwrap();
    }
    let scope = matric_db::search_candidates::SearchCandidateScope {
        embedding_set_id: Some(set_id),
        ..Default::default()
    };
    let mut connection = db.pool.acquire().await.unwrap();
    let query = matric_core::Vector::from(vec![3., 4., 1.]);
    let before =
        matric_db::search_candidates::vector_on_connection(&mut connection, &query, 10, &scope)
            .await
            .unwrap();
    assert_eq!(before.len(), 2);
    let (status, updated) = response(
        &app,
        Method::PATCH,
        "/api/v1/embedding-sets/ownership",
        json!({"vector_source":"external"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let after =
        matric_db::search_candidates::vector_on_connection(&mut connection, &query, 10, &scope)
            .await
            .unwrap();
    assert_eq!(
        before
            .iter()
            .map(|hit| (hit.note_id, hit.score))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|hit| (hit.note_id, hit.score))
            .collect::<Vec<_>>()
    );
    drop(connection);
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn generated_documents_opt_into_raw_dimension_and_legacy_bytes_stay_native(
    admin: sqlx::PgPool,
) {
    use matric_core::NoteRepository;
    use matric_jobs::{JobContext, JobHandler, JobResult};
    let db = database(admin).await;
    let received = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let requests = received.clone();
    let mock = Router::new().route("/embeddings", post(move |Json(body): Json<Value>| {
        let requests = requests.clone();
        async move {
            requests.lock().unwrap().push(body.clone());
            let raw = if body["model"] == "raw-4096" {4096} else {2};
            let data = body["input"].as_array().unwrap().iter().enumerate().map(|(index,_)| {
                let mut vector = vec![0.;raw]; vector[0]=3.; vector[1]=4.;
                if raw > 2 { vector[4095]=12.; }
                json!({"index":index,"embedding":vector})
            }).collect::<Vec<_>>();
            Json(json!({"model":body["model"],"data":data,"usage":{"prompt_tokens":1,"total_tokens":1}}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let registry = Arc::new(registry("openai", &url));
    for (dimension, provider_dimension, model) in
        [(1024, Some(4096), "raw-4096"), (2, None, "legacy-2")]
    {
        let config_id = Uuid::new_v4();
        let set_id = Uuid::new_v4();
        let truncate_dimension = provider_dimension.is_none().then_some(2);
        let config_dimension = if truncate_dimension.is_some() {
            8
        } else {
            dimension
        };
        sqlx::query("INSERT INTO embedding_config(id,name,model,dimension,provider_dimension,provider,provider_config,supports_mrl,matryoshka_dims)
            VALUES($1,$2,$2,$3,$4,'custom','{\"provider_id\":\"openai\"}',$5,$6)")
            .bind(config_id).bind(model).bind(config_dimension).bind(provider_dimension)
            .bind(truncate_dimension.is_some()).bind(truncate_dimension.map(|_| vec![8,2]))
            .execute(&db.pool).await.unwrap();
        sqlx::query("INSERT INTO embedding_set(id,name,slug,set_type,mode,embedding_config_id,defer_index_build,truncate_dim)
            VALUES($1,$2,$2,'full','manual',$3,true,$4)").bind(set_id).bind(model).bind(config_id)
            .bind(truncate_dimension).execute(&db.pool).await.unwrap();
        let note = db
            .notes
            .insert(matric_core::CreateNoteRequest {
                content: "Synthetic document embedding".into(),
                format: "markdown".into(),
                source: "test".into(),
                collection_id: None,
                document_type_id: None,
                title: None,
                tags: None,
                metadata: None,
            })
            .await
            .unwrap();
        sqlx::query("INSERT INTO embedding_set_member(embedding_set_id,note_id,membership_type) VALUES($1,$2,'include')")
            .bind(set_id).bind(note).execute(&db.pool).await.unwrap();
        let job: matric_core::Job = serde_json::from_value(json!({
            "id":Uuid::new_v4(),"note_id":note,"job_type":"embedding","status":"running",
            "priority":1,"payload":{"embedding_set_id":set_id},"result":null,"error_message":null,
            "progress_percent":0,"progress_message":null,"retry_count":0,"max_retries":1,
            "created_at":Utc::now(),"started_at":Utc::now(),"completed_at":null,"cost_tier":null,"initiated_by_user_id":null
        })).unwrap();
        let handler = handlers::EmbeddingHandler::new(
            db.clone(),
            registry.clone(),
            Arc::new(matric_core::InMemoryMeter::default()),
        );
        let result = handler.execute(JobContext::new(job)).await;
        assert!(matches!(result, JobResult::Success(_)), "{result:?}");
        let (vector, fingerprint): (matric_core::Vector, String) = sqlx::query_as(
            "SELECT vector,contract_fingerprint FROM embedding WHERE embedding_set_id=$1 LIMIT 1",
        )
        .bind(set_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(vector.as_slice().len(), dimension as usize);
        assert_eq!(
            &vector.as_slice()[..2],
            if provider_dimension.is_some() {
                &[0.6, 0.8]
            } else {
                &[3., 4.]
            }
        );
        let expected =
            matric_core::EmbeddingContract::new("openai", model, dimension as usize, Some(set_id))
                .unwrap()
                .with_provider_dimension(provider_dimension.map(|d| d as usize))
                .unwrap();
        assert_eq!(fingerprint, expected.fingerprint());
        let profile = db
            .embedding_sets
            .get_config(config_id)
            .await
            .unwrap()
            .unwrap();
        let query = generate_search_query_embedding(
            &registry,
            Some(&profile),
            truncate_dimension,
            Some(set_id),
            None,
            "synthetic",
        )
        .await
        .unwrap();
        assert_eq!(query.as_slice(), vector.as_slice());
        // The actual freshness path must recognize the stored fingerprint.
        sqlx::query(
            "DELETE FROM job_queue WHERE job_type='embedding' AND payload->>'embedding_set_id'=$1",
        )
        .bind(set_id.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
        let refresh: matric_core::Job = serde_json::from_value(json!({
            "id":Uuid::new_v4(),"note_id":null,"job_type":"refresh_embedding_set","status":"running",
            "priority":1,"payload":{"set_id":set_id,"refresh_stale":true},"result":null,"error_message":null,
            "progress_percent":0,"progress_message":null,"retry_count":0,"max_retries":1,
            "created_at":Utc::now(),"started_at":Utc::now(),"completed_at":null,"cost_tier":null,"initiated_by_user_id":null
        })).unwrap();
        let result = handlers::RefreshEmbeddingSetHandler::new(db.clone())
            .execute(JobContext::new(refresh))
            .await;
        assert!(matches!(result, JobResult::Success(_)), "{result:?}");
        let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE job_type='embedding' AND payload->>'embedding_set_id'=$1")
            .bind(set_id.to_string()).fetch_one(&db.pool).await.unwrap();
        assert_eq!(
            queued, 0,
            "current fingerprint should not queue regeneration"
        );
    }
    assert!(!received.lock().unwrap().is_empty());
    server.abort();
    db.pool.close().await;
}

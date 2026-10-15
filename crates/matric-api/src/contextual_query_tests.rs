use super::*;
use matric_core::ArchiveRepository;
use matric_jobs::{JobContext, JobHandler, JobResult};
use query_template_tests::registry;
use serde_json::{json, Value};

#[sqlx::test(migrations = false)]
async fn contextual_job_uses_default_set_template_once_and_searches_stored_dimension(
    admin: sqlx::PgPool,
) {
    let db = embedding_dimension_tests::database(admin).await;
    let embeddings = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let generations = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let raw_dimension = Arc::new(std::sync::atomic::AtomicUsize::new(8));
    let recorded = embeddings.clone();
    let raw = raw_dimension.clone();
    let generated = generations.clone();
    let mock = Router::new()
        .route("/embeddings", post(move |Json(body): Json<Value>| {
            let recorded = recorded.clone();
            let raw = raw.clone();
            async move {
                recorded.lock().unwrap().push(body.clone());
                let mut vector = vec![0.;raw.load(std::sync::atomic::Ordering::Relaxed)];
                vector[0] = 3.; vector[1] = 4.;
                *vector.last_mut().unwrap() = 12.;
                Json(json!({"model":body["model"],"data":[{"index":0,"embedding":vector}],
                    "usage":{"prompt_tokens":1,"total_tokens":1}}))
            }
        }))
        .route("/api/ps", get(|| async { Json(json!({"models":[]})) }))
        .route("/api/chat", post(move |Json(body): Json<Value>| {
            let generated = generated.clone();
            async move {
                generated.lock().unwrap().push(body);
                Json(json!({"message":{"role":"assistant","content":"Synthetic contextual revision."}}))
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let handler = handlers::AiRevisionContextualHandler::new(
        db.clone(),
        OllamaBackend::with_config(url.clone(), "unused".into(), "synthetic".into(), 4),
        Arc::new(registry("openai", &url)),
    );
    let archive = db
        .archives
        .create_archive_schema("contextual-query", None)
        .await
        .unwrap();
    let template = "Instruct: Find contextual notes\nQuery:{text}";
    for (schema, filtered) in [("public", false), (archive.schema_name.as_str(), true)] {
        let context = db.for_schema(schema).unwrap();
        let mut tx = context.begin_tx().await.unwrap();
        let config = Uuid::new_v4();
        sqlx::query("INSERT INTO embedding_config(id,name,model,dimension,provider_dimension,provider,provider_config)
            VALUES($1,$2,'synthetic-8',4,8,'custom','{\"provider_id\":\"openai\"}')")
            .bind(config).bind(format!("contextual-{schema}")).execute(&mut *tx).await.unwrap();
        let set: Uuid = sqlx::query_scalar(
            "UPDATE embedding_set SET embedding_config_id=$1,query_template=$2
            WHERE slug='default' RETURNING id",
        )
        .bind(config)
        .bind(template)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let mut ids = Vec::new();
        for content in [
            "Synthetic Phase 1 {text}",
            "Related synthetic context",
            "Unrelated synthetic context",
        ] {
            ids.push(
                db.notes
                    .insert_tx(
                        &mut tx,
                        matric_core::CreateNoteRequest {
                            content: content.into(),
                            format: "markdown".into(),
                            source: "test".into(),
                            collection_id: None,
                            document_type_id: None,
                            title: None,
                            tags: None,
                            metadata: None,
                        },
                    )
                    .await
                    .unwrap(),
            );
        }
        db.notes
            .update_revised_tx(&mut tx, ids[0], "Synthetic Phase 1 {text}", None)
            .await
            .unwrap();
        for (id, vector) in [
            (ids[1], vec![0.6, 0.8, 0., 0.]),
            (ids[2], vec![0.8, -0.6, 0., 0.]),
        ] {
            sqlx::query(
                "INSERT INTO embedding(id,note_id,embedding_set_id,chunk_index,text,vector,model)
                VALUES($1,$2,$3,0,'context',$4,'synthetic-8')",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(set)
            .bind(matric_core::Vector::from(vector))
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        let filter = matric_core::StrictTagFilter::default().exclude_concept(Uuid::new_v4());
        let job: matric_core::Job = serde_json::from_value(json!({
            "id":Uuid::new_v4(),"note_id":ids[0],"job_type":"ai_revision_contextual","status":"running",
            "priority":1,"payload":{"schema":schema,"context_filter":filtered.then_some(filter)},
            "result":null,"error_message":null,"progress_percent":0,"progress_message":null,
            "retry_count":0,"max_retries":1,"created_at":Utc::now(),"started_at":Utc::now(),
            "completed_at":null,"cost_tier":null,"initiated_by_user_id":null
        })).unwrap();
        let result = handler.execute(JobContext::new(job.clone())).await;
        let JobResult::Success(Some(result)) = result else {
            panic!("{result:?}")
        };
        assert_eq!(result["related_notes_used"], 1, "{result}");
        assert_eq!(result["context_filtered"], filtered);
        assert_eq!(
            embeddings.lock().unwrap().pop().unwrap()["input"],
            json!(["Instruct: Find contextual notes\nQuery:Synthetic Phase 1 {text}"])
        );
        assert!(
            embeddings.lock().unwrap().is_empty(),
            "one provider request per job"
        );
        let generated = generations.lock().unwrap().pop().unwrap().to_string();
        assert!(generated.contains("Related synthetic context"));
        assert!(!generated.contains("Unrelated synthetic context"));

        for (invalid_template, raw, reason) in [
            ("invalid", 8, "embedding_query_template_unsupported"),
            (template, 7, "embedding_dimension_mismatch"),
        ] {
            let mut tx = context.begin_tx().await.unwrap();
            sqlx::query("UPDATE embedding_set SET query_template=$1 WHERE id=$2")
                .bind(invalid_template)
                .bind(set)
                .execute(&mut *tx)
                .await
                .unwrap();
            db.notes
                .update_revised_tx(&mut tx, ids[0], "Synthetic Phase 1 {text}", None)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            raw_dimension.store(raw, std::sync::atomic::Ordering::Relaxed);
            let result = handler.execute(JobContext::new(job.clone())).await;
            let JobResult::Success(Some(result)) = result else {
                panic!("{result:?}")
            };
            assert_eq!(result["reason"], reason, "{result}");
            assert_eq!(result["phase1_preserved"], true);
            let mut tx = context.begin_tx().await.unwrap();
            assert_eq!(
                db.notes
                    .fetch_tx(&mut tx, ids[0])
                    .await
                    .unwrap()
                    .revised
                    .content,
                "Synthetic Phase 1 {text}"
            );
            tx.rollback().await.unwrap();
            assert!(generations.lock().unwrap().is_empty());
            embeddings.lock().unwrap().clear();
        }
        raw_dimension.store(8, std::sync::atomic::Ordering::Relaxed);
    }
    server.abort();
    db.pool.close().await;
}

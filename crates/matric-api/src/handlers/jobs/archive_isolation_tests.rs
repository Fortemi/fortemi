//! Execute real context/linking workers with colliding A/public note identities.
use super::*;
use axum::{routing::post, Json, Router};
use matric_core::{ArchiveRepository, CreateNoteRequest, Job, JobStatus};
use serde_json::json;
use sqlx::ConnectOptions;
use uuid::Uuid;

fn job(note: Uuid, kind: JobType, schema: &str) -> JobContext {
    JobContext::new(Job {
        id: Uuid::now_v7(),
        note_id: Some(note),
        job_type: kind,
        status: JobStatus::Pending,
        priority: 1,
        payload: Some(json!({"schema":schema})),
        result: None,
        error_message: None,
        progress_percent: 0,
        progress_message: None,
        retry_count: 0,
        max_retries: 3,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
        cost_tier: None,
        initiated_by_user_id: None,
    })
}

async fn seed(db: &Database, ctx: &SchemaContext, title: &str, content: &str) -> Uuid {
    let mut tx = ctx.begin_tx().await.unwrap();
    let id = db
        .notes
        .insert_tx(
            &mut tx,
            CreateNoteRequest {
                content: content.into(),
                format: "markdown".into(),
                source: "isolation".into(),
                collection_id: None,
                tags: None,
                metadata: None,
                document_type_id: None,
                title: Some(title.into()),
            },
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // Identical UUIDs and relational shapes; the content and titles differ.
    for (table, key) in [
        ("note", "id"),
        ("note_original", "note_id"),
        ("note_revised_current", "note_id"),
    ] {
        let columns: String = sqlx::query_scalar(
            "SELECT string_agg(quote_ident(column_name), ',' ORDER BY ordinal_position) FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND is_generated='NEVER'"
        ).bind(table).fetch_one(&db.pool).await.unwrap();
        sqlx::query(&format!(
            "INSERT INTO public.{table} ({columns}) SELECT {columns} FROM {}.{table} WHERE {key}=$1",
            ctx.schema()
        ))
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE public.note SET title='Denied public title' WHERE id=$1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE public.note_original SET content='DENIED-PUBLIC-CONTENT' WHERE note_id=$1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE public.note_revised_current SET content='DENIED-PUBLIC-CONTENT' WHERE note_id=$1",
    )
    .bind(id)
    .execute(&db.pool)
    .await
    .unwrap();
    id
}

#[sqlx::test(migrations = false)]
async fn context_and_wiki_link_workers_never_read_or_write_colliding_public_notes(
    admin: sqlx::PgPool,
) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let db = Database::connect(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let archive = db
        .archives
        .create_archive_schema("worker-isolation", None)
        .await
        .unwrap();
    let ctx = db.for_schema(&archive.schema_name).unwrap();
    let source = seed(&db, &ctx, "Source", "Source refers to [[Scopedtarget]]").await;
    let target = seed(&db, &ctx, "Scopedtarget", "AUTHORIZED-A-CONTENT").await;
    let linking = LinkingHandler::new(db.clone());
    assert_eq!(
        linking.resolve_wiki_link("Scopedtarget", &ctx).await,
        Some(target)
    );
    assert_eq!(
        linking.resolve_wiki_link("Denied public title", &ctx).await,
        None
    );
    let result = linking
        .execute(job(source, JobType::Linking, ctx.schema()))
        .await;
    assert!(matches!(result, JobResult::Success(_)), "{result:?}");
    let mut tx = ctx.begin_tx().await.unwrap();
    let links = db.links.get_outgoing_tx(&mut tx, source).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].to_note_id, Some(target));
    tx.commit().await.unwrap();
    let public_links: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public.link WHERE from_note_id=$1")
            .bind(source)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(public_links, 0, "wiki-link writes belong to A");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mock = Router::new().route("/api/chat", post(move |Json(body): Json<serde_json::Value>| async move {
        tx.send(body).unwrap();
        Json(json!({"message":{"role":"assistant","content":"Source\n\n## Related Context\nAUTHORIZED-A-CONTENT"}}))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, mock).await.unwrap();
    });
    let handler = ContextUpdateHandler::new(
        db.clone(),
        OllamaBackend::with_config(endpoint, "test".into(), "test".into(), 768),
        Arc::new(ProviderRegistry::new("ollama".into())),
    );
    let result = handler
        .execute(job(source, JobType::ContextUpdate, ctx.schema()))
        .await;
    assert!(matches!(result, JobResult::Success(_)), "{result:?}");
    let input = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap()
        .to_string();
    assert!(input.contains("AUTHORIZED-A-CONTENT"), "{input}");
    assert!(!input.contains("DENIED-PUBLIC-CONTENT"), "{input}");
    let mut tx = ctx.begin_tx().await.unwrap();
    let note = db.notes.fetch_tx(&mut tx, source).await.unwrap();
    assert!(note.revised.content.contains("AUTHORIZED-A-CONTENT"));
    tx.commit().await.unwrap();
    assert_eq!(
        db.notes.fetch(source).await.unwrap().revised.content,
        "DENIED-PUBLIC-CONTENT"
    );
    server.abort();
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn linking_concept_boost_reads_archive_tags_not_public_tags(admin: sqlx::PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let db = Database::connect(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let archive = db
        .archives
        .create_archive_schema("concept-isolation", None)
        .await
        .unwrap();
    let ctx = db.for_schema(&archive.schema_name).unwrap();
    let source = seed(&db, &ctx, "Source", "Source").await;
    let candidate = seed(&db, &ctx, "Candidate", "Candidate").await;
    let concept = Uuid::new_v4();
    let mut tx = ctx.begin_tx().await.unwrap();
    sqlx::query("INSERT INTO skos_concept(id,notation,primary_scheme_id) SELECT $1,'isolation-concept',id FROM skos_concept_scheme ORDER BY created_at LIMIT 1")
        .bind(concept)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO note_skos_concept(note_id,concept_id) VALUES($1,$3),($2,$3)")
        .bind(source)
        .bind(candidate)
        .bind(concept)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let handler = LinkingHandler::new(db.clone());
    let concepts = handler
        .archive_concepts(&ctx, &[source, candidate])
        .await
        .unwrap();
    assert_eq!(concepts[&source], HashSet::from([concept]));
    assert_eq!(concepts[&candidate], HashSet::from([concept]));
    let hit = matric_core::SearchHit {
        note_id: candidate,
        score: 0.5,
        snippet: None,
        title: None,
        tags: vec![],
        embedding_status: None,
        evidence: None,
    };
    let mut candidates = vec![(hit, pgvector::Vector::from(vec![1.0_f32, 0.0]))];
    handler
        .apply_tag_boost(source, &mut candidates, 0.5, &ctx)
        .await;
    assert_eq!(
        candidates[0].0.score, 0.75,
        "both linking strategies share the scoped concept lookup"
    );
    db.pool.close().await;
}

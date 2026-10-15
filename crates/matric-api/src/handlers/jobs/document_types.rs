use matric_core::{DocumentType, Error, Result};
use matric_db::{PgDocumentTypeRepository, SchemaContext};
use uuid::Uuid;

/// Legacy workers already resolve a committed note in their archive. Use that
/// note's stored tenant for configuration, never the worker pool's ambient tenant.
pub(super) async fn note_document_type(
    context: &SchemaContext,
    note_id: Uuid,
    document_type_id: Option<Uuid>,
    preview: Option<String>,
) -> Result<Option<DocumentType>> {
    context
        .execute(move |tx| {
            Box::pin(async move {
                let tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM note WHERE id=$1")
                    .bind(note_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| Error::NotFound("Note not found".into()))?;
                sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
                    .bind(tenant.to_string())
                    .execute(&mut **tx)
                    .await?;
                let id = match document_type_id {
                    Some(id) => Some(id),
                    None => PgDocumentTypeRepository::detect_tx(tx, None, preview.as_deref(), None)
                        .await?
                        .map(|result| result.document_type.id),
                };
                match id {
                    Some(id) => PgDocumentTypeRepository::get_tx(tx, id).await,
                    None => Ok(None),
                }
            })
        })
        .await
}

#[cfg(all(test, feature = "hosted-auth"))]
mod tests {
    use super::*;
    use crate::handlers::jobs::{build_type_aware_prompt, EmbeddingHandler};
    use async_trait::async_trait;
    use matric_core::{EmbeddingBackend, NoteRepository, RevisionMode};
    use matric_db::{Database, PoolConfig};
    use matric_jobs::{JobContext, JobHandler, JobResult};
    use std::sync::{Arc, Mutex};

    struct Recorder(Arc<Mutex<Vec<String>>>);
    #[async_trait]
    impl EmbeddingBackend for Recorder {
        async fn embed_texts(&self, texts: &[String]) -> Result<Vec<matric_core::Vector>> {
            self.0.lock().unwrap().extend_from_slice(texts);
            Ok(texts
                .iter()
                .map(|_| matric_core::Vector::from(vec![0.25; 768]))
                .collect())
        }
        fn dimension(&self) -> usize {
            768
        }
        fn model_name(&self) -> &str {
            "synthetic-tenant-embedding"
        }
    }

    #[sqlx::test(migrations = false)]
    async fn jobs_resolve_owner_prompts_fallback_context_linking_and_chunk_sizes(
        admin: sqlx::PgPool,
    ) {
        use sqlx::ConnectOptions;
        sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
            .execute(&admin)
            .await
            .unwrap();
        Database::new(admin.clone()).migrate().await.unwrap();
        let url = admin.connect_options().to_url_lossy().to_string();
        let local = matric_db::create_pool(&url).await.unwrap();
        let local_context = SchemaContext::new(local.clone(), "public").unwrap();
        let mut ids = Vec::new();
        let mut chunk_counts = Vec::new();
        for (size, role, category) in [
            (180, "Tenant A editor", "code"),
            (540, "Tenant B editor", "prose"),
        ] {
            let tenant = Uuid::new_v4();
            crate::external_oidc::ensure_default_tenant(&admin, tenant)
                .await
                .unwrap();
            let type_id = Uuid::new_v4();
            let agentic = serde_json::json!({"generation_prompt":role,"required_sections":["Decisions"],"revision_chunking":{"max_chars":size}});
            sqlx::query("INSERT INTO document_type(id,name,display_name,category,chunk_size_default,chunk_overlap_default,magic_patterns,agentic_config,tenant_id) VALUES($1,'owner-specific','Owner specific',$2::document_category,$3,0,ARRAY['Owner specific'], $4,$5)")
                .bind(type_id).bind(category).bind(size).bind(agentic).bind(tenant).execute(&admin).await.unwrap();
            let runtime =
                matric_db::create_pool_with_config(&url, PoolConfig::new().single_tenant(tenant))
                    .await
                    .unwrap();
            let db = Database::new(runtime.clone());
            let note_id = db
                .notes
                .insert(matric_core::CreateNoteRequest {
                    content: format!(
                        "Owner specific {}",
                        (0..80)
                            .map(|index| format!("Synthetic segment {index:03} for chunking."))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                    format: "markdown".into(),
                    source: "synthetic".into(),
                    collection_id: None,
                    tags: None,
                    metadata: None,
                    document_type_id: Some(type_id),
                    title: None,
                })
                .await
                .unwrap();
            // Even a legacy local pool resolves the committed owner's config.
            let doc_type = note_document_type(&local_context, note_id, Some(type_id), None)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(doc_type.chunk_size_default, size);
            assert_eq!(
                doc_type.agentic_config.generation_prompt.as_deref(),
                Some(role)
            );
            assert_eq!(
                doc_type
                    .agentic_config
                    .revision_chunking
                    .as_ref()
                    .unwrap()
                    .max_chars,
                Some(size as usize)
            );
            let prompt = build_type_aware_prompt(
                Some(&doc_type),
                RevisionMode::Standard,
                "Synthetic",
                "",
                0,
                1,
                false,
            );
            assert!(prompt.contains(role));
            let detected = note_document_type(
                &local_context,
                note_id,
                None,
                Some("Owner specific synthetic".into()),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(detected.id, type_id);
            // Contextual ID hints and linking category resolution share this lookup.
            assert_eq!(doc_type.name, "owner-specific");
            assert_eq!(doc_type.category.to_string(), category);
            if let Some(foreign_id) = ids.first() {
                assert!(
                    note_document_type(&local_context, note_id, Some(*foreign_id), None)
                        .await
                        .unwrap()
                        .is_none()
                );
            }
            ids.push(type_id);
            let inputs = Arc::new(Mutex::new(Vec::new()));
            let handler = EmbeddingHandler::new(
                db.clone(),
                Arc::new(matric_inference::ProviderRegistry::from_env()),
                Arc::new(matric_core::InMemoryMeter::default()),
            )
            .with_backend_override(Arc::new(Recorder(inputs.clone())));
            let now = chrono::Utc::now();
            let job = matric_core::Job {
                id: Uuid::now_v7(),
                note_id: Some(note_id),
                job_type: matric_core::JobType::Embedding,
                status: matric_core::JobStatus::Running,
                priority: 1,
                payload: Some(serde_json::json!({"schema":"public","tenant_id":tenant})),
                result: None,
                error_message: None,
                progress_percent: 0,
                progress_message: None,
                retry_count: 0,
                max_retries: 1,
                created_at: now,
                started_at: Some(now),
                completed_at: None,
                cost_tier: None,
                initiated_by_user_id: None,
            };
            let result = handler.execute(JobContext::new(job)).await;
            assert!(matches!(result, JobResult::Success(_)), "{result:?}");
            {
                let recorded = inputs.lock().unwrap();
                assert!(!recorded.is_empty());
                assert!(recorded.iter().all(|chunk| chunk.len() <= size as usize));
                chunk_counts.push(recorded.len());
            }
            let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM embedding e JOIN embedding_set s ON s.id=e.embedding_set_id WHERE e.note_id=$1 AND e.tenant_id=$2 AND s.tenant_id=$2")
                .bind(note_id).bind(tenant).fetch_one(&admin).await.unwrap();
            assert_eq!(stored as usize, *chunk_counts.last().unwrap());
            runtime.close().await;
        }
        assert!(chunk_counts[0] > chunk_counts[1]);
        local.close().await;
    }
}

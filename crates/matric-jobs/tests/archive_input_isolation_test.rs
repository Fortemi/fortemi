//! Worker defense against legacy internal overrides and cross-schema attachment reads.
use async_trait::async_trait;
use matric_core::{
    ArchiveRepository, ExtractionAdapter, ExtractionResult, ExtractionStrategy, Job, JobRepository,
    JobStatus, JobType,
};
use matric_db::Database;
use matric_inference::{DiarizationBackend, DiarizationResult};
use matric_jobs::{
    extraction::ExtractionRegistry, ExtractionHandler, JobContext, JobHandler, JobResult,
    SpeakerDiarizationHandler,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use uuid::Uuid;

fn job(note: Option<Uuid>, kind: JobType, payload: serde_json::Value) -> JobContext {
    JobContext::new(Job {
        id: Uuid::now_v7(),
        note_id: note,
        job_type: kind,
        status: JobStatus::Pending,
        priority: 1,
        payload: Some(payload),
        result: None,
        error_message: None,
        progress_percent: 0,
        progress_message: None,
        retry_count: 0,
        max_retries: 3,
        created_at: chrono::Utc::now(),
        started_at: None,
        completed_at: None,
        cost_tier: None,
        initiated_by_user_id: None,
    })
}

struct InspectAdapter(Arc<AtomicBool>);
#[async_trait]
impl ExtractionAdapter for InspectAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::AudioTranscribe
    }
    async fn extract(
        &self,
        data: &[u8],
        _: &str,
        _: &str,
        config: &serde_json::Value,
    ) -> matric_core::Result<ExtractionResult> {
        assert_eq!(data, b"authorized inline bytes");
        for key in [
            "_source_path",
            "_checkpoint",
            "_skip_transcode",
            "_future_control",
        ] {
            assert!(config.get(key).is_none(), "untrusted {key} reached adapter");
        }
        assert_eq!(config["language"], "en");
        self.0.store(true, Ordering::Relaxed);
        Ok(ExtractionResult::default())
    }
    async fn health_check(&self) -> matric_core::Result<bool> {
        Ok(true)
    }
    fn name(&self) -> &str {
        "inspect-authorized-input"
    }
}

#[tokio::test]
async fn legacy_extraction_payload_cannot_supply_internal_adapter_paths() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let checked = Arc::new(AtomicBool::new(false));
    let mut registry = ExtractionRegistry::new();
    registry.register(Arc::new(InspectAdapter(checked.clone())));
    let handler = ExtractionHandler::new(db.clone(), Arc::new(registry));
    let result = handler.execute(job(None, JobType::Extraction, json!({
        "strategy":"audio_transcribe", "data":"authorized inline bytes",
        "config":{"_source_path":"/denied/archive/audio.wav", "_checkpoint":{"completed_frames":[0]},
        "_skip_transcode":true, "_future_control":true, "language":"en"}
    }))).await;
    assert!(matches!(result, JobResult::Success(_)), "{result:?}");
    assert!(checked.load(Ordering::Relaxed));
    db.pool.close().await;
}

struct InspectAudio(Arc<AtomicBool>);
#[async_trait]
impl DiarizationBackend for InspectAudio {
    async fn diarize(
        &self,
        path: &std::path::Path,
        _: Option<usize>,
        _: Option<usize>,
    ) -> matric_core::Result<DiarizationResult> {
        assert_eq!(std::fs::read(path).unwrap(), b"archive-A-inline-audio");
        self.0.store(true, Ordering::Relaxed);
        Ok(DiarizationResult {
            segments: vec![],
            num_speakers: 0,
        })
    }
    async fn health_check(&self) -> matric_core::Result<bool> {
        Ok(true)
    }
    fn model_name(&self) -> &str {
        "inspect-scoped-audio"
    }
}

#[tokio::test]
async fn diarization_inline_read_uses_archive_even_with_same_id_in_public() {
    let storage = tempfile::tempdir().unwrap();
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap()
        .with_filesystem_storage(storage.path().to_str().unwrap(), 0);
    let name = format!("diarization-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&name, None)
        .await
        .unwrap();
    let ctx = db.for_schema(&archive.schema_name).unwrap();
    let note = Uuid::now_v7();
    let attachment = Uuid::now_v7();
    for (schema, bytes) in [
        ("public", b"denied-public-audio".as_slice()),
        (
            archive.schema_name.as_str(),
            b"archive-A-inline-audio".as_slice(),
        ),
    ] {
        let schema_ctx = db.for_schema(schema).unwrap();
        let mut tx = schema_ctx.begin_tx().await.unwrap();
        sqlx::query("INSERT INTO note(id,format,source,created_at_utc,updated_at_utc) VALUES($1,'markdown','isolation-test',now(),now())")
            .bind(note).execute(&mut *tx).await.unwrap();
        let blob = Uuid::now_v7();
        sqlx::query("INSERT INTO attachment_blob(id,content_hash,content_type,size_bytes,storage_backend,data) VALUES($1,$2,'audio/wav',$3,'database',$4)")
            .bind(blob).bind(format!("isolated-{blob}")).bind(bytes.len() as i64).bind(bytes).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment(id,note_id,blob_id,filename,extracted_metadata,virus_scan_status,virus_scan_blob_hash) VALUES($1,$2,$3,'audio.wav',$4,'clean',$5)")
            .bind(attachment).bind(note).bind(blob).bind(json!({"transcript_segments":[{"start_secs":0.0,"end_secs":1.0,"text":"A transcript"}]})).bind(format!("isolated-{blob}")).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    let checked = Arc::new(AtomicBool::new(false));
    let handler =
        SpeakerDiarizationHandler::new(db.clone(), Arc::new(InspectAudio(checked.clone())));
    let result = handler
        .execute(job(
            Some(note),
            JobType::SpeakerDiarization,
            json!({"schema":ctx.schema(),"attachment_id":attachment}),
        ))
        .await;
    assert!(matches!(result, JobResult::Success(_)), "{result:?}");
    assert!(checked.load(Ordering::Relaxed));
    db.archives.drop_archive_schema(&name).await.unwrap();
    sqlx::query("DELETE FROM public.attachment WHERE id=$1")
        .bind(attachment)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM public.note WHERE id=$1")
        .bind(note)
        .execute(&db.pool)
        .await
        .unwrap();
    db.pool.close().await;
}

// Verify the tenant predicate independently of RLS, for every queue identity.
#[tokio::test]
async fn job_identity_includes_tenant_schema_and_optional_note() {
    let admin = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    for tenant in [tenant_a, tenant_b] {
        sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'dedup-test','active')")
            .bind(tenant).bind(format!("dedup-{tenant}")).execute(&admin.pool).await.unwrap();
    }
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant',$1,false)")
                    .bind(tenant_a.to_string())
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with((*admin.pool.connect_options()).clone())
        .await
        .unwrap();
    let db = Database::new(pool);
    let bypass: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(
        bypass,
        "test must exercise the tenant predicate independently of RLS"
    );
    for note in [None, Some(Uuid::now_v7())] {
        // Same schema, type and optional note in B must not suppress A.
        sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,status,payload) VALUES($1,$2,$3,'graph_maintenance','pending',$4)")
            .bind(Uuid::now_v7()).bind(tenant_b).bind(note).bind(json!({"schema":"archive_a"})).execute(&db.pool).await.unwrap();
        let queued = db
            .jobs
            .queue_deduplicated(
                note,
                JobType::GraphMaintenance,
                1,
                Some(json!({"schema":"archive_a"})),
                None,
            )
            .await
            .unwrap();
        let id = queued.expect("foreign tenant cannot suppress this job");
        let duplicate = db
            .jobs
            .queue_deduplicated(
                note,
                JobType::GraphMaintenance,
                1,
                Some(json!({"schema":"archive_a"})),
                None,
            )
            .await
            .unwrap();
        assert!(
            duplicate.is_none(),
            "same-archive deduplication is preserved"
        );
        assert!(db
            .jobs
            .queue_deduplicated(
                note,
                JobType::GraphMaintenance,
                1,
                Some(json!({"schema":"archive_b"})),
                None
            )
            .await
            .unwrap()
            .is_some());
        sqlx::query("UPDATE job_queue SET status='running' WHERE id=$1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(db
            .jobs
            .queue_deduplicated(
                note,
                JobType::GraphMaintenance,
                1,
                Some(json!({"schema":"archive_a"})),
                None
            )
            .await
            .unwrap()
            .is_none());
        // Legacy missing/empty schema and explicit public share one identity.
        assert!(db
            .jobs
            .queue_deduplicated(note, JobType::GraphMaintenance, 1, None, None)
            .await
            .unwrap()
            .is_some());
        for payload in [json!({"schema":"public"}), json!({"schema":""})] {
            assert!(db
                .jobs
                .queue_deduplicated(note, JobType::GraphMaintenance, 1, Some(payload), None)
                .await
                .unwrap()
                .is_none());
        }
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE tenant_id=$1")
        .bind(tenant_a)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 6);
    sqlx::query("DELETE FROM job_queue WHERE tenant_id=ANY($1)")
        .bind(vec![tenant_a, tenant_b])
        .execute(&admin.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenant_registry WHERE id=ANY($1)")
        .bind(vec![tenant_a, tenant_b])
        .execute(&admin.pool)
        .await
        .unwrap();
    db.pool.close().await;
    admin.pool.close().await;
}

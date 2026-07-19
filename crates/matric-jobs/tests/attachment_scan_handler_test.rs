use async_trait::async_trait;
use matric_core::{AttachmentScanStatus, Job, JobRepository, JobStatus, JobType};
use matric_db::Database;
use matric_jobs::{
    AttachmentScanFailure, AttachmentScanHandler, AttachmentScanMetrics, AttachmentScanOutcome,
    AttachmentScanner, JobContext, JobHandler, JobResult,
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

#[derive(Clone, Copy)]
enum MockResult {
    Clean,
    Infected,
    Timeout,
}

struct MockScanner {
    result: MockResult,
}

#[async_trait]
impl AttachmentScanner for MockScanner {
    fn backend_name(&self) -> &'static str {
        "mock-scanner"
    }

    fn engine_version(&self) -> Option<&str> {
        Some("1.0")
    }

    fn signature_version(&self) -> Option<&str> {
        Some("42")
    }

    fn max_bytes(&self) -> usize {
        1024
    }

    async fn health_check(&self) -> Result<(), AttachmentScanFailure> {
        Ok(())
    }

    async fn scan(&self, _data: &[u8]) -> Result<AttachmentScanOutcome, AttachmentScanFailure> {
        match self.result {
            MockResult::Clean => Ok(AttachmentScanOutcome::Clean),
            MockResult::Infected => Ok(AttachmentScanOutcome::Infected),
            MockResult::Timeout => Err(AttachmentScanFailure::timed_out()),
        }
    }
}

async fn setup_attachment() -> (Database, TempDir, Uuid, Uuid) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let temp_dir = TempDir::new().unwrap();
    let db = Database::new(pool.clone())
        .with_filesystem_storage(temp_dir.path().to_str().unwrap(), 10_485_760);
    let note_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO note (id, format, source, created_at_utc, updated_at_utc)
           VALUES ($1, 'markdown', 'attachment-scan-test', NOW(), NOW())"#,
    )
    .bind(note_id)
    .execute(&pool)
    .await
    .unwrap();
    let file_data = format!("bounded test bytes {note_id}");
    let attachment = db
        .file_storage
        .as_ref()
        .unwrap()
        .store_file(note_id, "scan.txt", "text/plain", file_data.as_bytes())
        .await
        .unwrap();
    (db, temp_dir, note_id, attachment.id)
}

fn scan_context(note_id: Uuid, attachment_id: Uuid, with_downstream: bool) -> JobContext {
    let downstream_jobs = if with_downstream {
        vec![json!({
            "job_type": "exif_extraction",
            "payload": {
                "attachment_id": attachment_id.to_string(),
            },
        })]
    } else {
        Vec::new()
    };
    JobContext::new(Job {
        id: Uuid::now_v7(),
        note_id: Some(note_id),
        job_type: JobType::AttachmentVirusScan,
        status: JobStatus::Pending,
        priority: JobType::AttachmentVirusScan.default_priority(),
        payload: Some(json!({
            "attachment_id": attachment_id.to_string(),
            "downstream_jobs": downstream_jobs,
        })),
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
    })
}

#[tokio::test]
async fn clean_scan_releases_download_and_downstream_job() {
    let (db, _temp_dir, note_id, attachment_id) = setup_attachment().await;
    let metrics = Arc::new(AttachmentScanMetrics::default());
    let handler = AttachmentScanHandler::new(
        db.clone(),
        Arc::new(MockScanner {
            result: MockResult::Clean,
        }),
        metrics.clone(),
    );

    let result = handler
        .execute(scan_context(note_id, attachment_id, true))
        .await;
    assert!(matches!(result, JobResult::Success(_)), "{result:?}");
    let attachment = db
        .file_storage
        .as_ref()
        .unwrap()
        .get(attachment_id)
        .await
        .unwrap();
    assert_eq!(attachment.virus_scan_status, AttachmentScanStatus::Clean);
    db.file_storage
        .as_ref()
        .unwrap()
        .download_file(attachment_id)
        .await
        .expect("clean attachment should be downloadable");
    let downstream = db.jobs.get_for_note(note_id).await.unwrap();
    assert!(downstream
        .iter()
        .any(|job| job.job_type == JobType::ExifExtraction));
    let retry_result = handler
        .execute(scan_context(note_id, attachment_id, true))
        .await;
    assert!(
        matches!(retry_result, JobResult::Success(_)),
        "{retry_result:?}"
    );
    let downstream = db.jobs.get_for_note(note_id).await.unwrap();
    assert_eq!(
        downstream
            .iter()
            .filter(|job| job.job_type == JobType::ExifExtraction)
            .count(),
        1,
        "a retried scan must not duplicate downstream work"
    );
    assert_eq!(metrics.snapshot().clean, 2);
}

#[tokio::test]
async fn infected_scan_quarantines_and_suppresses_downstream_job() {
    let (db, _temp_dir, note_id, attachment_id) = setup_attachment().await;
    let handler = AttachmentScanHandler::new(
        db.clone(),
        Arc::new(MockScanner {
            result: MockResult::Infected,
        }),
        Arc::new(AttachmentScanMetrics::default()),
    );

    let result = handler
        .execute(scan_context(note_id, attachment_id, true))
        .await;
    assert!(matches!(result, JobResult::Success(_)), "{result:?}");
    let attachment = db
        .file_storage
        .as_ref()
        .unwrap()
        .get(attachment_id)
        .await
        .unwrap();
    assert_eq!(attachment.virus_scan_status, AttachmentScanStatus::Infected);
    assert_eq!(
        attachment.status,
        matric_core::AttachmentStatus::Quarantined
    );
    assert!(db.jobs.get_for_note(note_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn scanner_timeout_is_retryable_and_remains_quarantined() {
    let (db, _temp_dir, note_id, attachment_id) = setup_attachment().await;
    let handler = AttachmentScanHandler::new(
        db.clone(),
        Arc::new(MockScanner {
            result: MockResult::Timeout,
        }),
        Arc::new(AttachmentScanMetrics::default()),
    );

    let result = handler
        .execute(scan_context(note_id, attachment_id, false))
        .await;
    assert!(
        matches!(result, JobResult::Retry(ref code) if code == "scanner_timeout"),
        "{result:?}"
    );
    let attachment = db
        .file_storage
        .as_ref()
        .unwrap()
        .get(attachment_id)
        .await
        .unwrap();
    assert_eq!(attachment.virus_scan_status, AttachmentScanStatus::Error);
    assert_eq!(
        attachment.status,
        matric_core::AttachmentStatus::Quarantined
    );
}

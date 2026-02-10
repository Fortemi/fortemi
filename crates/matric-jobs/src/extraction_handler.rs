//! ExtractionHandler — dispatches upload → extract → chunk → embed pipeline.

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use matric_core::{ExtractionStrategy, JobType};
use matric_db::Database;

use crate::extraction::ExtractionRegistry;
use crate::handler::{JobContext, JobHandler, JobResult};

pub struct ExtractionHandler {
    db: Database,
    registry: Arc<ExtractionRegistry>,
}

impl ExtractionHandler {
    pub fn new(db: Database, registry: Arc<ExtractionRegistry>) -> Self {
        Self { db, registry }
    }
}

#[async_trait]
impl JobHandler for ExtractionHandler {
    fn job_type(&self) -> JobType {
        JobType::Extraction
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        // Parse payload: { strategy, filename, mime_type, data, config }
        let payload = match ctx.payload() {
            Some(p) => p.clone(),
            None => return JobResult::Failed("Missing extraction job payload".into()),
        };

        let strategy_str = payload
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("text_native");
        let strategy: ExtractionStrategy = match strategy_str.parse() {
            Ok(s) => s,
            Err(e) => return JobResult::Failed(format!("Invalid extraction strategy: {}", e)),
        };

        let filename = payload
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let mime_type = payload
            .get("mime_type")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream");
        let config = payload.get("config").cloned().unwrap_or_else(|| json!({}));

        // Get data: prefer attachment_id (fetch from file storage), fall back to inline data
        let data = if let Some(attachment_id_str) =
            payload.get("attachment_id").and_then(|v| v.as_str())
        {
            let attachment_id: Uuid = match attachment_id_str.parse() {
                Ok(id) => id,
                Err(e) => {
                    return JobResult::Failed(format!("Invalid attachment_id UUID: {}", e))
                }
            };

            let file_storage = match self.db.file_storage.as_ref() {
                Some(fs) => fs,
                None => return JobResult::Failed("File storage not configured".into()),
            };

            match file_storage.download_file(attachment_id).await {
                Ok((file_data, _content_type, _filename)) => file_data,
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to download attachment {}: {}",
                        attachment_id, e
                    ))
                }
            }
        } else if let Some(data_str) = payload.get("data").and_then(|v| v.as_str()) {
            data_str.as_bytes().to_vec()
        } else {
            return JobResult::Failed(
                "No data provided (expected 'attachment_id' or 'data' field)".into(),
            );
        };

        ctx.report_progress(10, Some("Starting extraction"));

        // Check adapter availability
        if !self.registry.has_adapter(strategy) {
            return JobResult::Failed(format!(
                "No adapter registered for strategy: {:?}",
                strategy
            ));
        }

        ctx.report_progress(20, Some("Extracting content"));

        // Run extraction
        match self
            .registry
            .extract(strategy, &data, filename, mime_type, &config)
            .await
        {
            Ok(result) => {
                ctx.report_progress(80, Some("Extraction complete"));

                let result_json = json!({
                    "strategy": strategy.to_string(),
                    "has_text": result.extracted_text.is_some(),
                    "text_length": result.extracted_text.as_ref().map(|t| t.len()).unwrap_or(0),
                    "has_description": result.ai_description.is_some(),
                    "metadata": result.metadata,
                });

                info!(
                    strategy = %strategy,
                    filename,
                    text_len = result.extracted_text.as_ref().map(|t| t.len()).unwrap_or(0),
                    "Extraction completed successfully"
                );

                ctx.report_progress(100, Some("Done"));
                JobResult::Success(Some(result_json))
            }
            Err(e) => {
                error!(strategy = %strategy, filename, error = %e, "Extraction failed");
                JobResult::Failed(format!("Extraction failed: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TextNativeAdapter;
    use chrono::Utc;
    use matric_core::{Job, JobStatus};
    use serde_json::json;
    use uuid::Uuid;

    fn test_db() -> Database {
        let pool =
            sqlx::Pool::<sqlx::Postgres>::connect_lazy("postgres://test:test@localhost/test")
                .expect("lazy pool");
        Database::new(pool)
    }

    fn create_test_job(payload: Option<serde_json::Value>) -> Job {
        Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::Extraction,
            status: JobStatus::Pending,
            priority: 7,
            payload,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_job_type() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);
        assert_eq!(handler.job_type(), JobType::Extraction);
    }

    #[tokio::test]
    async fn test_extraction_handler_can_handle() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);
        assert!(handler.can_handle(JobType::Extraction));
        assert!(!handler.can_handle(JobType::Embedding));
        assert!(!handler.can_handle(JobType::Linking));
    }

    #[tokio::test]
    async fn test_extraction_handler_missing_payload() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);

        let job = create_test_job(None);
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("Missing extraction job payload"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_invalid_strategy() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);

        let payload = json!({
            "strategy": "invalid_strategy_name",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "test content"
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("Invalid extraction strategy"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_missing_adapter() {
        let registry = Arc::new(ExtractionRegistry::new());
        let handler = ExtractionHandler::new(test_db(), registry);

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "test content"
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("No adapter registered for strategy"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_missing_data() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain"
            // Missing "data" field
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Failed(msg) => {
                assert!(msg.contains("No data provided"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_success() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "hello world",
            "config": {}
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        match result {
            JobResult::Success(Some(result_json)) => {
                assert_eq!(result_json["strategy"], "text_native");
                assert_eq!(result_json["has_text"], true);
                assert_eq!(result_json["text_length"], 11);
            }
            _ => panic!("Expected Success result with data"),
        }
    }

    #[tokio::test]
    async fn test_extraction_handler_with_progress_tracking() {
        use std::sync::{Arc as StdArc, Mutex};

        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        let payload = json!({
            "strategy": "text_native",
            "filename": "test.txt",
            "mime_type": "text/plain",
            "data": "test content"
        });

        let job = create_test_job(Some(payload));

        let progress_log = StdArc::new(Mutex::new(Vec::new()));
        let progress_log_clone = progress_log.clone();

        let ctx = JobContext::new(job).with_progress_callback(move |percent, message| {
            progress_log_clone
                .lock()
                .unwrap()
                .push((percent, message.map(String::from)));
        });

        let result = handler.execute(ctx).await;
        assert!(matches!(result, JobResult::Success(_)));

        let log = progress_log.lock().unwrap();
        assert!(log.len() >= 4); // At least: 10%, 20%, 80%, 100%
        assert!(log.iter().any(|(p, _)| *p == 10));
        assert!(log.iter().any(|(p, _)| *p == 20));
        assert!(log.iter().any(|(p, _)| *p == 80));
        assert!(log.iter().any(|(p, _)| *p == 100));
    }

    #[tokio::test]
    async fn test_extraction_handler_default_values() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));
        let handler = ExtractionHandler::new(test_db(), Arc::new(registry));

        // Minimal payload with defaults
        let payload = json!({
            "data": "test"
        });

        let job = create_test_job(Some(payload));
        let ctx = JobContext::new(job);

        let result = handler.execute(ctx).await;
        // Should use default strategy "text_native", filename "unknown", etc.
        assert!(matches!(result, JobResult::Success(_)));
    }
}

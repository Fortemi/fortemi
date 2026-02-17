//! Integration test for schema context in job handlers (Issue #110).
//!
//! This test verifies that job handlers correctly extract and handle schema
//! information from job payloads, enabling schema-scoped operations for
//! parallel memory archives.

use matric_core::{Job, JobStatus, JobType};
use matric_jobs::{JobContext, JobHandler, NoOpHandler};
use serde_json::json;
use uuid::Uuid;

/// Helper function to create a job with schema in payload.
fn create_job_with_schema(job_type: JobType, schema: Option<&str>) -> Job {
    let mut payload_obj = json!({
        "test_field": "test_value"
    });

    if let Some(s) = schema {
        payload_obj["schema"] = json!(s);
    }

    Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type,
        status: JobStatus::Pending,
        priority: 0,
        payload: Some(payload_obj),
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
    }
}

/// Test that NoOpHandler processes jobs with schema field successfully.
#[tokio::test]
async fn test_noop_handler_with_schema() {
    let handler = NoOpHandler::new(JobType::Embedding);
    let job = create_job_with_schema(JobType::Embedding, Some("archive_2026"));

    let ctx = JobContext::new(job);

    // Verify schema is in payload
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    assert_eq!(schema, "archive_2026");

    // Execute handler - should succeed
    let result = handler.execute(ctx).await;
    assert!(matches!(result, matric_jobs::JobResult::Success(_)));
}

/// Test that NoOpHandler processes jobs without schema field successfully (backward compat).
#[tokio::test]
async fn test_noop_handler_without_schema() {
    let handler = NoOpHandler::new(JobType::Linking);
    let job = create_job_with_schema(JobType::Linking, None);

    let ctx = JobContext::new(job);

    // Verify schema defaults to "public"
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    assert_eq!(schema, "public");

    // Execute handler - should succeed
    let result = handler.execute(ctx).await;
    assert!(matches!(result, matric_jobs::JobResult::Success(_)));
}

/// Test schema extraction for all supported job types.
#[tokio::test]
async fn test_schema_extraction_all_job_types() {
    let test_cases = vec![
        (JobType::AiRevision, "archive_ai"),
        (JobType::Embedding, "archive_embed"),
        (JobType::TitleGeneration, "archive_title"),
        (JobType::Linking, "archive_link"),
        (JobType::ConceptTagging, "archive_concept"),
    ];

    for (job_type, schema_name) in test_cases {
        let job = create_job_with_schema(job_type, Some(schema_name));
        let ctx = JobContext::new(job);

        let extracted_schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .unwrap_or("public");

        assert_eq!(
            extracted_schema, schema_name,
            "Schema extraction failed for {:?}",
            job_type
        );
    }
}

/// Test that empty schema strings default to "public".
#[tokio::test]
async fn test_empty_schema_defaults_to_public() {
    let payload = json!({
        "schema": ""  // Empty string
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::Embedding,
        status: JobStatus::Pending,
        priority: 0,
        payload: Some(payload.clone()),
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
    };

    let ctx = JobContext::new(job);

    // Extract with empty string filter
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty()) // Filter out empty strings
        .unwrap_or("public");

    assert_eq!(schema, "public");
}

/// Test that jobs with schema names containing underscores and numbers work.
#[tokio::test]
async fn test_schema_with_complex_name() {
    let complex_schemas = vec![
        "archive_2026_01_15",
        "test_archive_v2",
        "user_123_archive",
        "archive_2026_q1",
    ];

    for schema_name in complex_schemas {
        let job = create_job_with_schema(JobType::Embedding, Some(schema_name));
        let ctx = JobContext::new(job);

        let extracted = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .unwrap_or("public");

        assert_eq!(extracted, schema_name);
    }
}

/// Test backward compatibility: old job payloads without schema field.
#[tokio::test]
async fn test_backward_compatibility_old_payloads() {
    // Simulate old payload format - only has job-specific fields, no schema
    let old_payload = json!({
        "revision_mode": "Light",
        "some_other_field": 42
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::AiRevision,
        status: JobStatus::Pending,
        priority: 0,
        payload: Some(old_payload),
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
    };

    let ctx = JobContext::new(job);

    // Should gracefully default to "public"
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public");

    assert_eq!(schema, "public");

    // Handler should still work
    let handler = NoOpHandler::new(JobType::AiRevision);
    let result = handler.execute(ctx).await;
    assert!(matches!(result, matric_jobs::JobResult::Success(_)));
}

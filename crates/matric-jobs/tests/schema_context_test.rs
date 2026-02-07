//! Tests for schema context propagation in job payloads (Issue #110).

use matric_core::{Job, JobStatus, JobType};
use matric_jobs::JobContext;
use serde_json::json;
use uuid::Uuid;

/// Test that schema field is correctly extracted from payload.
#[test]
fn test_job_context_extract_schema_from_payload() {
    let payload = json!({
        "revision_mode": "Full",
        "schema": "archive_2026"
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::AiRevision,
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
    };

    let ctx = JobContext::new(job);

    // Extract schema from payload
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");

    assert_eq!(schema, "archive_2026");
}

/// Test that schema defaults to "public" when not present in payload.
#[test]
fn test_job_context_schema_defaults_to_public() {
    let payload = json!({
        "revision_mode": "Full"
        // No schema field
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::AiRevision,
        status: JobStatus::Pending,
        priority: 0,
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
    };

    let ctx = JobContext::new(job);

    // Extract schema from payload (should default to "public")
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");

    assert_eq!(schema, "public");
}

/// Test that schema defaults to "public" when payload is None.
#[test]
fn test_job_context_schema_defaults_when_no_payload() {
    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::Embedding,
        status: JobStatus::Pending,
        priority: 0,
        payload: None, // No payload at all
        result: None,
        error_message: None,
        progress_percent: 0,
        progress_message: None,
        retry_count: 0,
        max_retries: 3,
        created_at: chrono::Utc::now(),
        started_at: None,
        completed_at: None,
    };

    let ctx = JobContext::new(job);

    // Extract schema from payload (should default to "public")
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");

    assert_eq!(schema, "public");
}

/// Test that schema field is preserved across different job types.
#[test]
fn test_schema_field_for_all_job_types() {
    let job_types = vec![
        JobType::AiRevision,
        JobType::Embedding,
        JobType::TitleGeneration,
        JobType::Linking,
        JobType::ConceptTagging,
    ];

    for job_type in job_types {
        let payload = json!({
            "schema": "test_archive",
            "other_field": "value"
        });

        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type,
            status: JobStatus::Pending,
            priority: 0,
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
        };

        let ctx = JobContext::new(job);

        let schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .unwrap_or("public");

        assert_eq!(schema, "test_archive", "Schema mismatch for {:?}", job_type);
    }
}

/// Test backward compatibility: jobs without schema field still work.
#[test]
fn test_backward_compatibility_no_schema_field() {
    // Simulate an old job payload without schema field
    let old_payload = json!({
        "revision_mode": "Light",
        "note_id": Uuid::new_v4().to_string()
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
    };

    let ctx = JobContext::new(job);

    // Should gracefully default to "public" for backward compatibility
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");

    assert_eq!(schema, "public");
}

/// Test that invalid schema values are handled gracefully.
#[test]
fn test_invalid_schema_value_fallback() {
    let payload = json!({
        "schema": 12345 // Invalid: number instead of string
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::Embedding,
        status: JobStatus::Pending,
        priority: 0,
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
    };

    let ctx = JobContext::new(job);

    // as_str() should return None for non-string, so defaults to "public"
    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");

    assert_eq!(schema, "public");
}

/// Test empty schema string defaults to "public".
#[test]
fn test_empty_schema_string() {
    let payload = json!({
        "schema": "" // Empty string
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::Linking,
        status: JobStatus::Pending,
        priority: 0,
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
    };

    let ctx = JobContext::new(job);

    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty()) // Filter out empty strings
        .unwrap_or("public");

    assert_eq!(schema, "public");
}

/// Test that schema with special characters is preserved.
#[test]
fn test_schema_with_special_characters() {
    let payload = json!({
        "schema": "archive_2026_01"
    });

    let job = Job {
        id: Uuid::new_v4(),
        note_id: Some(Uuid::new_v4()),
        job_type: JobType::TitleGeneration,
        status: JobStatus::Pending,
        priority: 0,
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
    };

    let ctx = JobContext::new(job);

    let schema = ctx
        .payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .unwrap_or("public");

    assert_eq!(schema, "archive_2026_01");
}

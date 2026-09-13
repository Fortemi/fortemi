use matric_core::{Error, Result};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use super::PgDocumentTypeRepository;
use crate::TenantScopedConn;

impl PgDocumentTypeRepository {
    /// Classify and record a job's result atomically on an admitted content scope.
    /// The caller must hold the committed job's attempt fence (HostedClaim).
    pub async fn infer_note_scoped(
        scope: &mut TenantScopedConn<'_>,
        job_id: Uuid,
        note_id: Uuid,
    ) -> Result<Value> {
        let note = sqlx::query(
            "SELECT document_type_id, metadata FROM note
             WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(note_id)
        .fetch_optional(scope.executor())
        .await?
        .ok_or_else(|| Error::NotFound("Document type inference note unavailable".into()))?;

        // A durable activity shares the globally unique job ID. Replays of the
        // same job return its committed result, including empty/no-match outcomes.
        let prior = sqlx::query(
            "SELECT note_id, activity_type, ended_at IS NOT NULL AS complete, metadata
             FROM provenance_activity WHERE id=$1",
        )
        .bind(job_id)
        .fetch_optional(scope.executor())
        .await?;
        if let Some(prior) = prior {
            if prior.get::<Uuid, _>("note_id") != note_id
                || prior.get::<String, _>("activity_type") != "document_type_inference"
                || !prior.get::<bool, _>("complete")
            {
                return Err(Error::InvalidInput("Hosted job receipt conflict".into()));
            }
            return prior
                .get::<Option<Value>, _>("metadata")
                .filter(Value::is_object)
                .ok_or_else(|| Error::InvalidInput("Hosted job receipt invalid".into()));
        }

        // Keep content rows locked through assignment. This also fences editors
        // which update the original/revised row without first locking note.
        let original = sqlx::query("SELECT content FROM note_original WHERE note_id=$1 FOR UPDATE")
            .bind(note_id)
            .fetch_one(scope.executor())
            .await?;
        let revised = sqlx::query(
            "SELECT content, last_revision_id FROM note_revised_current WHERE note_id=$1 FOR UPDATE",
        )
        .bind(note_id)
        .fetch_one(scope.executor())
        .await?;
        let original: String = original.get("content");
        let revised_content: String = revised.get("content");
        let (content, revision_id) = if revised_content.is_empty() {
            (original.as_str(), None)
        } else {
            (
                revised_content.as_str(),
                revised.get::<Option<Uuid>, _>("last_revision_id"),
            )
        };

        let result = if note.get::<Option<Uuid>, _>("document_type_id").is_some() {
            json!({"skipped":true,"reason":"document_type_already_assigned"})
        } else if content.trim().is_empty() {
            json!({"detected":false,"reason":"empty_content"})
        } else {
            let metadata: Value = note
                .get::<Option<Value>, _>("metadata")
                .unwrap_or_else(|| json!({}));
            let filename = metadata.get("source_file").and_then(Value::as_str);
            let preview: String = content.chars().take(1000).collect();
            match Self::detect_scoped(scope, filename, Some(&preview), None).await? {
                None => json!({"detected":false,"reason":"no_match"}),
                Some(detection) => {
                    // The selected shared configuration must remain active and
                    // visible until commit, even if an operator edits it in flight.
                    let active: Option<Uuid> = sqlx::query_scalar(
                        "SELECT id FROM public.document_type WHERE id=$1 AND is_active FOR SHARE",
                    )
                    .bind(detection.document_type.id)
                    .fetch_optional(scope.executor())
                    .await?;
                    if active.is_none() {
                        return Err(Error::InvalidInput(
                            "Document type no longer available".into(),
                        ));
                    }
                    let changed = sqlx::query(
                        "UPDATE note SET document_type_id=$1
                         WHERE id=$2 AND document_type_id IS NULL AND deleted_at IS NULL",
                    )
                    .bind(detection.document_type.id)
                    .bind(note_id)
                    .execute(scope.executor())
                    .await?
                    .rows_affected();
                    if changed != 1 {
                        return Err(Error::InvalidInput("Document type assignment lost".into()));
                    }
                    json!({
                        "detected":true,
                        "document_type_id_present":true,
                        "document_type_id_len":detection.document_type.id.to_string().chars().count(),
                        "detection_method_len":detection.detection_method.chars().count(),
                        "confidence":detection.confidence,
                    })
                }
            }
        };

        // Preserve the legacy read's access accounting, but make both the access
        // receipt and completed provenance mandatory members of this transaction.
        sqlx::query(
            "UPDATE note SET last_accessed_at=NOW(), access_count=access_count+1 WHERE id=$1",
        )
        .bind(note_id)
        .execute(scope.executor())
        .await?;
        sqlx::query(
            "INSERT INTO note_access_log(note_id,accessed_at,access_type)
             VALUES($1,NOW(),'direct_get'::public.note_access_type)",
        )
        .bind(note_id)
        .execute(scope.executor())
        .await?;
        sqlx::query(
            "INSERT INTO provenance_activity(id,note_id,revision_id,activity_type,ended_at,metadata)
             VALUES($1,$2,$3,'document_type_inference',clock_timestamp(),$4)",
        )
        .bind(job_id)
        .bind(note_id)
        .bind(revision_id)
        .bind(&result)
        .execute(scope.executor())
        .await?;
        Ok(result)
    }
}

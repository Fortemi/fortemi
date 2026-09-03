//! PostgreSQL implementation of the source-addressed note upsert contract.

use matric_core::{
    new_v7, source_content_digest, source_identity_digest, source_request_digest,
    CreateNoteRequest, Error, Result, SourceUpsertBatchOutcome, SourceUpsertCounts,
    SourceUpsertItemOutcome, SourceUpsertItemResult, SourceUpsertPolicy, SourceUpsertRequest,
    SourceUpsertResponse, SOURCE_UPSERT_CONTRACT_VERSION, SOURCE_UPSERT_MAX_ITEMS,
};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::PgNoteRepository;

const MAX_CONTENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 256 * 1024;
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024;

pub struct PgSourceUpsertRepository {
    notes: PgNoteRepository,
}

#[derive(Clone)]
struct ExistingIdentity {
    note_id: Uuid,
    content_digest: String,
}

impl PgSourceUpsertRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            notes: PgNoteRepository::new(pool),
        }
    }

    /// Apply one bounded batch inside the caller's tenant- and memory-scoped
    /// transaction. Any database error rolls the complete batch back.
    pub async fn upsert_tx(
        &self,
        tx: &mut PgConnection,
        request: SourceUpsertRequest,
    ) -> Result<SourceUpsertResponse> {
        let tenant_id: Uuid =
            sqlx::query_scalar("SELECT current_setting('app.current_tenant')::uuid")
                .fetch_one(&mut *tx)
                .await
                .map_err(Error::Database)?;
        let memory_name: String = sqlx::query_scalar("SELECT current_schema()::text")
            .fetch_one(&mut *tx)
            .await
            .map_err(Error::Database)?;
        let request_digest = source_request_digest(&request);
        let batch_id = request
            .batch_id
            .clone()
            .unwrap_or_else(|| request_digest.clone());

        if let Some(reason_code) = validate_request(&request, &batch_id) {
            return Ok(rejected_response(
                &request,
                &batch_id,
                tenant_id,
                &memory_name,
                reason_code,
            ));
        }

        if !request.dry_run {
            if let Some(row) = sqlx::query(
                r#"
                SELECT request_digest, receipt
                  FROM source_import_batch
                 WHERE source_namespace = $1
                   AND import_run_id = $2
                   AND batch_id = $3
                 FOR UPDATE
                "#,
            )
            .bind(&request.source_namespace)
            .bind(&request.import_run_id)
            .bind(&batch_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(Error::Database)?
            {
                let stored_digest: String = row.get("request_digest");
                if stored_digest != request_digest {
                    return Ok(rejected_response(
                        &request,
                        &batch_id,
                        tenant_id,
                        &memory_name,
                        "batch_id_reused_with_different_request",
                    ));
                }
                let receipt: serde_json::Value = row.get("receipt");
                let response: SourceUpsertResponse =
                    serde_json::from_value(receipt).map_err(|_| {
                        Error::Config("source upsert receipt could not be decoded".to_string())
                    })?;
                return Ok(as_duplicate(response));
            }
        }

        // Serialize missing-row races without emitting raw source keys in logs.
        let mut lock_keys: Vec<i64> = request
            .items
            .iter()
            .map(|item| advisory_lock_key(&request.source_namespace, &item.external_id))
            .collect();
        lock_keys.sort_unstable();
        lock_keys.dedup();
        for lock_key in lock_keys {
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(lock_key)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        let mut existing = HashMap::new();
        for item in &request.items {
            if let Some(row) = sqlx::query(
                r#"
                SELECT si.note_id, si.content_digest
                  FROM source_identity si
                  JOIN note n ON n.id = si.note_id AND n.deleted_at IS NULL
                 WHERE si.source_namespace = $1 AND si.external_id = $2
                 FOR UPDATE OF si, n
                "#,
            )
            .bind(&request.source_namespace)
            .bind(&item.external_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(Error::Database)?
            {
                existing.insert(
                    item.external_id.clone(),
                    ExistingIdentity {
                        note_id: row.get("note_id"),
                        content_digest: row.get("content_digest"),
                    },
                );
            }
        }

        for item in &request.items {
            if let Some(caller_id) = item.caller_stable_id {
                let incompatible = existing
                    .get(&item.external_id)
                    .map(|identity| identity.note_id != caller_id)
                    .unwrap_or(false);
                let occupied: bool = if existing.contains_key(&item.external_id) {
                    false
                } else {
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id = $1)")
                        .bind(caller_id)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(Error::Database)?
                };
                if incompatible || occupied {
                    return Ok(rejected_response(
                        &request,
                        &batch_id,
                        tenant_id,
                        &memory_name,
                        "caller_stable_id_conflict",
                    ));
                }
            }
        }

        let mut results = Vec::with_capacity(request.items.len());
        let mut counts = SourceUpsertCounts::default();

        for (index, item) in request.items.iter().enumerate() {
            let computed_digest = source_content_digest(&item.content);
            let external_id_hash = source_identity_digest(
                tenant_id,
                &memory_name,
                &request.source_namespace,
                &item.external_id,
            );
            let policy = item.policy.unwrap_or(request.policy);

            let (outcome, note_id) = match existing.get(&item.external_id) {
                Some(identity) if identity.content_digest == computed_digest => {
                    (SourceUpsertItemOutcome::Unchanged, Some(identity.note_id))
                }
                Some(identity) if policy == SourceUpsertPolicy::Conflict => {
                    (SourceUpsertItemOutcome::Conflict, Some(identity.note_id))
                }
                Some(identity) if request.dry_run => (
                    match policy {
                        SourceUpsertPolicy::Replace => SourceUpsertItemOutcome::Replaced,
                        SourceUpsertPolicy::Version => SourceUpsertItemOutcome::Versioned,
                        SourceUpsertPolicy::Conflict => unreachable!(),
                    },
                    Some(identity.note_id),
                ),
                Some(identity) => {
                    match policy {
                        SourceUpsertPolicy::Version => {
                            self.notes
                                .update_revised_tx(
                                    tx,
                                    identity.note_id,
                                    &item.content,
                                    Some("externally managed source changed"),
                                )
                                .await?;
                            update_note_metadata(tx, identity.note_id, item).await?;
                        }
                        SourceUpsertPolicy::Replace => {
                            replace_note_content(tx, identity.note_id, item, &computed_digest)
                                .await?;
                        }
                        SourceUpsertPolicy::Conflict => unreachable!(),
                    }
                    sqlx::query(
                        r#"
                        UPDATE source_identity
                           SET source_id = $1,
                               source_schema_version = $2,
                               content_digest = $3,
                               import_run_id = $4,
                               updated_at = now()
                         WHERE source_namespace = $5 AND external_id = $6
                        "#,
                    )
                    .bind(&request.source_id)
                    .bind(&request.source_schema_version)
                    .bind(&computed_digest)
                    .bind(&request.import_run_id)
                    .bind(&request.source_namespace)
                    .bind(&item.external_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(Error::Database)?;
                    (
                        match policy {
                            SourceUpsertPolicy::Version => SourceUpsertItemOutcome::Versioned,
                            SourceUpsertPolicy::Replace => SourceUpsertItemOutcome::Replaced,
                            SourceUpsertPolicy::Conflict => unreachable!(),
                        },
                        Some(identity.note_id),
                    )
                }
                None if request.dry_run => {
                    (SourceUpsertItemOutcome::Inserted, item.caller_stable_id)
                }
                None => {
                    let note_id = item.caller_stable_id.unwrap_or_else(new_v7);
                    let metadata = if item.metadata.is_null() {
                        serde_json::json!({})
                    } else {
                        item.metadata.clone()
                    };
                    self.notes
                        .insert_with_id_tx(
                            tx,
                            note_id,
                            CreateNoteRequest {
                                content: item.content.clone(),
                                format: item.format.clone(),
                                source: "external-managed".to_string(),
                                collection_id: None,
                                tags: None,
                                metadata: Some(metadata),
                                document_type_id: None,
                                title: item.title.clone(),
                            },
                        )
                        .await?;
                    sqlx::query(
                        r#"
                        INSERT INTO source_identity (
                            source_namespace, external_id, note_id, source_id,
                            source_schema_version, content_digest, import_run_id,
                            caller_stable_id
                        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                        "#,
                    )
                    .bind(&request.source_namespace)
                    .bind(&item.external_id)
                    .bind(note_id)
                    .bind(&request.source_id)
                    .bind(&request.source_schema_version)
                    .bind(&computed_digest)
                    .bind(&request.import_run_id)
                    .bind(item.caller_stable_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(Error::Database)?;
                    (SourceUpsertItemOutcome::Inserted, Some(note_id))
                }
            };

            counts.observe(outcome);
            results.push(SourceUpsertItemResult {
                index,
                outcome,
                note_id,
                external_id_hash,
                content_digest: computed_digest,
                reason_code: None,
            });
        }

        let mut response = SourceUpsertResponse {
            contract_version: SOURCE_UPSERT_CONTRACT_VERSION.to_string(),
            import_run_id: request.import_run_id.clone(),
            batch_id: batch_id.clone(),
            dry_run: request.dry_run,
            outcome: if request.dry_run {
                SourceUpsertBatchOutcome::Preview
            } else {
                SourceUpsertBatchOutcome::Committed
            },
            checkpoint: request.checkpoint.clone(),
            counts,
            items: results,
        };

        if request.dry_run {
            return Ok(response);
        }

        sqlx::query(
            r#"
            INSERT INTO source_import_run (
                source_namespace, import_run_id, source_id,
                source_schema_version, workspace_id, checkpoint
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, source_namespace, import_run_id)
            DO UPDATE SET source_id = EXCLUDED.source_id,
                          source_schema_version = EXCLUDED.source_schema_version,
                          workspace_id = EXCLUDED.workspace_id,
                          checkpoint = EXCLUDED.checkpoint,
                          updated_at = now()
            "#,
        )
        .bind(&request.source_namespace)
        .bind(&request.import_run_id)
        .bind(&request.source_id)
        .bind(&request.source_schema_version)
        .bind(&request.workspace_id)
        .bind(&request.checkpoint)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        let receipt = serde_json::to_value(&response)
            .map_err(|_| Error::Config("source upsert receipt could not be encoded".to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO source_import_batch (
                source_namespace, import_run_id, batch_id,
                request_digest, receipt, checkpoint
            ) VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&request.source_namespace)
        .bind(&request.import_run_id)
        .bind(&batch_id)
        .bind(&request_digest)
        .bind(receipt)
        .bind(&request.checkpoint)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        response.outcome = SourceUpsertBatchOutcome::Committed;
        Ok(response)
    }
}

fn validate_request(request: &SourceUpsertRequest, batch_id: &str) -> Option<&'static str> {
    if !valid_identifier(&request.source_namespace, 200)
        || !valid_identifier(&request.source_schema_version, 100)
        || !valid_identifier(&request.import_run_id, 200)
        || !valid_identifier(batch_id, 200)
        || request
            .source_id
            .as_ref()
            .is_some_and(|value| !valid_identifier(value, 500))
        || request
            .workspace_id
            .as_ref()
            .is_some_and(|value| !valid_identifier(value, 500))
    {
        return Some("invalid_batch_metadata");
    }
    if request.items.is_empty() || request.items.len() > SOURCE_UPSERT_MAX_ITEMS {
        return Some("batch_size_out_of_bounds");
    }
    if request
        .checkpoint
        .as_ref()
        .is_some_and(|value| serialized_len(value) > MAX_CHECKPOINT_BYTES)
    {
        return Some("checkpoint_too_large");
    }
    let mut keys = HashSet::with_capacity(request.items.len());
    for item in &request.items {
        if !valid_identifier(&item.external_id, 1000)
            || item.content.is_empty()
            || item.content.len() > MAX_CONTENT_BYTES
            || item.format.is_empty()
            || item.format.len() > 100
            || item.title.as_ref().is_some_and(|value| value.len() > 2000)
            || serialized_len(&item.metadata) > MAX_METADATA_BYTES
            || item.caller_stable_id.is_some_and(|value| value.is_nil())
        {
            return Some("invalid_item");
        }
        if !keys.insert(item.external_id.as_str()) {
            return Some("duplicate_external_id_in_batch");
        }
        if item
            .content_digest
            .as_ref()
            .is_some_and(|digest| digest != &source_content_digest(&item.content))
        {
            return Some("content_digest_mismatch");
        }
    }
    None
}

fn valid_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && !value.chars().any(char::is_control)
        && value.trim() == value
}

fn serialized_len(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn advisory_lock_key(namespace: &str, external_id: &str) -> i64 {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(external_id.as_bytes());
    let digest = hasher.finalize();
    i64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
}

fn rejected_response(
    request: &SourceUpsertRequest,
    batch_id: &str,
    tenant_id: Uuid,
    memory_name: &str,
    reason_code: &str,
) -> SourceUpsertResponse {
    let items = request
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| SourceUpsertItemResult {
            index,
            outcome: SourceUpsertItemOutcome::Rejected,
            note_id: None,
            external_id_hash: source_identity_digest(
                tenant_id,
                memory_name,
                &request.source_namespace,
                &item.external_id,
            ),
            content_digest: source_content_digest(&item.content),
            reason_code: Some(reason_code.to_string()),
        })
        .collect::<Vec<_>>();
    SourceUpsertResponse {
        contract_version: SOURCE_UPSERT_CONTRACT_VERSION.to_string(),
        import_run_id: request.import_run_id.clone(),
        batch_id: batch_id.to_string(),
        dry_run: request.dry_run,
        outcome: SourceUpsertBatchOutcome::Rejected,
        checkpoint: request.checkpoint.clone(),
        counts: SourceUpsertCounts {
            rejected: items.len(),
            ..SourceUpsertCounts::default()
        },
        items,
    }
}

fn as_duplicate(mut response: SourceUpsertResponse) -> SourceUpsertResponse {
    response.outcome = SourceUpsertBatchOutcome::Duplicate;
    response.dry_run = false;
    response.counts = SourceUpsertCounts::default();
    for item in &mut response.items {
        if matches!(
            item.outcome,
            SourceUpsertItemOutcome::Inserted
                | SourceUpsertItemOutcome::Versioned
                | SourceUpsertItemOutcome::Replaced
        ) {
            item.outcome = SourceUpsertItemOutcome::Unchanged;
        }
        response.counts.observe(item.outcome);
    }
    response
}

async fn update_note_metadata(
    tx: &mut PgConnection,
    note_id: Uuid,
    item: &matric_core::SourceUpsertItem,
) -> Result<()> {
    let metadata = if item.metadata.is_null() {
        serde_json::json!({})
    } else {
        item.metadata.clone()
    };
    sqlx::query(
        "UPDATE note SET format = $1, title = $2, metadata = $3, updated_at_utc = now() WHERE id = $4",
    )
    .bind(&item.format)
    .bind(&item.title)
    .bind(metadata)
    .bind(note_id)
    .execute(&mut *tx)
    .await
    .map_err(Error::Database)?;
    Ok(())
}

async fn replace_note_content(
    tx: &mut PgConnection,
    note_id: Uuid,
    item: &matric_core::SourceUpsertItem,
    digest: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE note_original SET content = $1, hash = $2, user_last_edited_at = now() WHERE note_id = $3",
    )
    .bind(&item.content)
    .bind(digest)
    .bind(note_id)
    .execute(&mut *tx)
    .await
    .map_err(Error::Database)?;
    sqlx::query(
        "UPDATE note_revised_current SET content = $1, last_revision_id = NULL WHERE note_id = $2",
    )
    .bind(&item.content)
    .bind(note_id)
    .execute(&mut *tx)
    .await
    .map_err(Error::Database)?;
    update_note_metadata(tx, note_id, item).await?;
    sqlx::query(
        r#"
        INSERT INTO activity_log (id, at_utc, actor, action, note_id, meta)
        VALUES ($1, now(), 'system', 'external_replace', $2, '{"policy":"replace"}'::jsonb)
        "#,
    )
    .bind(new_v7())
    .bind(note_id)
    .execute(&mut *tx)
    .await
    .map_err(Error::Database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SourceUpsertRequest {
        SourceUpsertRequest {
            source_namespace: "example.sync".to_string(),
            source_id: Some("source".to_string()),
            source_schema_version: "1".to_string(),
            import_run_id: "run-1".to_string(),
            batch_id: Some("batch-1".to_string()),
            workspace_id: None,
            checkpoint: None,
            dry_run: false,
            policy: SourceUpsertPolicy::Version,
            items: vec![matric_core::SourceUpsertItem {
                external_id: "external-1".to_string(),
                content: "content".to_string(),
                content_digest: None,
                caller_stable_id: None,
                title: None,
                format: "markdown".to_string(),
                metadata: serde_json::json!({}),
                policy: None,
            }],
        }
    }

    #[test]
    fn validation_rejects_digest_mismatch_and_duplicate_keys() {
        let mut mismatch = request();
        mismatch.items[0].content_digest = Some(source_content_digest("other"));
        assert_eq!(
            validate_request(&mismatch, "batch-1"),
            Some("content_digest_mismatch")
        );

        let mut duplicate = request();
        duplicate.items.push(duplicate.items[0].clone());
        assert_eq!(
            validate_request(&duplicate, "batch-1"),
            Some("duplicate_external_id_in_batch")
        );
    }
}

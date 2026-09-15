//! PostgreSQL authority for previewable, resumable lifecycle purge.
//!
//! Callers provide an already tenant- and memory-scoped connection. Relational
//! deletion is atomic; filesystem and search cleanup are durable follow-up work
//! that must complete before a content-free receipt is issued.

use chrono::{DateTime, Utc};
use matric_core::{
    lifecycle_purge_selector_fingerprint, new_v7, DeletionReceipt, Error, PurgeCounts,
    PurgeOutcome, PurgePreview, PurgeReceiptPolicy, PurgeRequest, PurgeSelector, PurgeStatus,
    Result, LIFECYCLE_PURGE_CONTRACT_VERSION, LIFECYCLE_PURGE_PREVIEW_TTL_SECONDS,
};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// Private cleanup work. Its custom debug form deliberately excludes the path.
#[derive(Clone)]
pub struct LifecyclePurgeBlobCleanup {
    pub operation_id: Uuid,
    pub blob_id: Uuid,
    storage_path: String,
}

impl std::fmt::Debug for LifecyclePurgeBlobCleanup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LifecyclePurgeBlobCleanup")
            .field("operation_id_present", &true)
            .field("blob_id_present", &true)
            .field("storage_path_len", &self.storage_path.chars().count())
            .finish()
    }
}

impl LifecyclePurgeBlobCleanup {
    pub(crate) fn storage_path(&self) -> &str {
        &self.storage_path
    }
}

#[derive(Clone, Default)]
pub struct PgLifecyclePurgeRepository;

impl PgLifecyclePurgeRepository {
    pub fn new() -> Self {
        Self
    }

    /// Persist a bounded snapshot without returning selector material.
    pub async fn preview_tx(
        &self,
        tx: &mut PgConnection,
        selector: PurgeSelector,
    ) -> Result<PurgePreview> {
        selector.validate()?;
        sqlx::query(
            "DELETE FROM lifecycle_purge_preview \
             WHERE consumed_by IS NULL AND expires_at <= now()",
        )
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        let fingerprint = lifecycle_purge_selector_fingerprint(&selector)?;
        let note_ids = selected_note_ids(tx, &selector, false).await?;
        let counts = count_selected(tx, &note_ids).await?;
        let preview_id = new_v7();

        let expires_at: DateTime<Utc> = sqlx::query_scalar(
            r#"
            INSERT INTO lifecycle_purge_preview (
                id, selector_fingerprint, selector, selected_note_ids, counts, expires_at
            ) VALUES ($1, $2, $3, $4, $5,
                      now() + ($6::double precision * interval '1 second'))
            RETURNING expires_at
            "#,
        )
        .bind(preview_id)
        .bind(fingerprint)
        .bind(serde_json::to_value(&selector)?)
        .bind(&note_ids)
        .bind(serde_json::to_value(&counts)?)
        .bind(LIFECYCLE_PURGE_PREVIEW_TTL_SECONDS)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;

        Ok(PurgePreview {
            contract_version: LIFECYCLE_PURGE_CONTRACT_VERSION.to_string(),
            preview_id,
            counts,
            expires_at,
        })
    }

    /// Atomically consume an exact preview and remove all relational content.
    /// A replay of the same operation is harmless and returns current status.
    pub async fn begin_tx(
        &self,
        tx: &mut PgConnection,
        request: PurgeRequest,
    ) -> Result<PurgeStatus> {
        advisory_lock(tx, request.operation_id).await?;
        if let Some(status) = self.status_tx(tx, request.operation_id).await? {
            let stored_preview: Uuid = sqlx::query_scalar(
                "SELECT preview_id FROM lifecycle_purge_operation WHERE id = $1",
            )
            .bind(request.operation_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(Error::Database)?;
            if stored_preview != request.preview_id {
                return Err(Error::InvalidInput(
                    "Purge operation ID was already used for another preview.".to_string(),
                ));
            }
            return Ok(status);
        }

        let preview = sqlx::query(
            r#"
            SELECT selector_fingerprint, selector, selected_note_ids, counts,
                   expires_at <= now() AS expired, consumed_by
              FROM lifecycle_purge_preview
             WHERE id = $1
             FOR UPDATE
            "#,
        )
        .bind(request.preview_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| Error::NotFound("Lifecycle purge preview was not found.".to_string()))?;

        let consumed_by: Option<Uuid> = preview.get("consumed_by");
        if consumed_by.is_some() {
            return Err(Error::InvalidInput(
                "Lifecycle purge preview was already consumed.".to_string(),
            ));
        }
        if preview.get::<bool, _>("expired") {
            return Err(Error::InvalidInput(
                "Lifecycle purge preview expired; create a new preview.".to_string(),
            ));
        }

        let selector_value: Value = preview.get("selector");
        let selector: PurgeSelector = serde_json::from_value(selector_value)?;
        selector.validate()?;
        let stored_fingerprint: String = preview.get("selector_fingerprint");
        if lifecycle_purge_selector_fingerprint(&selector)? != stored_fingerprint {
            return Err(Error::Config(
                "Lifecycle purge preview integrity check failed.".to_string(),
            ));
        }
        let preview_ids: Vec<Uuid> = preview.get("selected_note_ids");
        let preview_counts: PurgeCounts = serde_json::from_value(preview.get("counts"))?;

        // Lock selected notes, then recompute the complete snapshot so a stale
        // preview cannot silently delete a different graph.
        let selected_ids = selected_note_ids(tx, &selector, true).await?;
        let current_counts = count_selected(tx, &selected_ids).await?;
        if selected_ids != preview_ids || current_counts != preview_counts {
            return Err(Error::InvalidInput(
                "Lifecycle purge preview is stale; create a new preview.".to_string(),
            ));
        }

        let blob_rows = orphan_blob_rows(tx, &selected_ids).await?;
        let search_cleanup_complete = selected_ids.is_empty();
        if blob_rows
            .iter()
            .any(|row| row.requires_path && row.storage_path.is_none())
        {
            return Err(Error::Config(
                "Lifecycle purge found a filesystem blob without a storage path.".to_string(),
            ));
        }
        let has_blob_cleanup = blob_rows.iter().any(|row| row.storage_path.is_some());

        sqlx::query(
            r#"
            INSERT INTO lifecycle_purge_operation (
                id, preview_id, selector_fingerprint, state, counts, search_cleanup_complete
            ) VALUES ($1, $2, $3, 'cleanup_pending', $4, $5)
            "#,
        )
        .bind(request.operation_id)
        .bind(request.preview_id)
        .bind(&stored_fingerprint)
        .bind(serde_json::to_value(&current_counts)?)
        .bind(search_cleanup_complete)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        for note_id in &selected_ids {
            sqlx::query(
                "INSERT INTO lifecycle_purge_erasure_target (operation_id, note_id) VALUES ($1, $2)",
            )
            .bind(request.operation_id)
            .bind(note_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }
        for blob in &blob_rows {
            if let Some(path) = &blob.storage_path {
                sqlx::query(
                    r#"
                    INSERT INTO lifecycle_purge_blob_cleanup (
                        operation_id, blob_id, storage_path
                    ) VALUES ($1, $2, $3)
                    "#,
                )
                .bind(request.operation_id)
                .bind(blob.id)
                .bind(path)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
            }
        }

        delete_selected(tx, &selected_ids, &selector).await?;
        let blob_ids = blob_rows.iter().map(|row| row.id).collect::<Vec<_>>();
        if !blob_ids.is_empty() {
            sqlx::query("DELETE FROM attachment_blob WHERE id = ANY($1)")
                .bind(&blob_ids)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        // Retain only the non-reversible fingerprint on the operation and erase
        // the sensitive selector snapshot immediately after successful use.
        sqlx::query(
            r#"
            UPDATE lifecycle_purge_preview
               SET consumed_by = $2,
                   selector = '{}'::jsonb,
                   selected_note_ids = ARRAY[]::uuid[]
             WHERE id = $1
            "#,
        )
        .bind(request.preview_id)
        .bind(request.operation_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        if search_cleanup_complete && !has_blob_cleanup {
            self.finalize_tx(tx, request.operation_id).await?;
        }
        self.status_tx(tx, request.operation_id)
            .await?
            .ok_or_else(|| {
                Error::Internal("Lifecycle purge operation was not created.".to_string())
            })
    }

    pub async fn status_tx(
        &self,
        tx: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<Option<PurgeStatus>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT o.state, o.counts, o.search_cleanup_complete, o.completed_at,
                   r.outcome AS receipt_outcome, r.counts AS receipt_counts,
                   r.completed_at AS receipt_completed_at, r.policy AS receipt_policy,
                   (SELECT COUNT(*) FROM lifecycle_purge_blob_cleanup b
                     WHERE b.operation_id = o.id AND b.state = 'pending') AS blob_pending
              FROM lifecycle_purge_operation o
              LEFT JOIN deletion_receipt r ON r.operation_id = o.id
             WHERE o.id = $1
            "#,
        )
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?
        else {
            return Ok(None);
        };

        let counts: PurgeCounts = serde_json::from_value(row.get("counts"))?;
        let state: String = row.get("state");
        let outcome = if state == "completed" {
            PurgeOutcome::Completed
        } else {
            PurgeOutcome::CleanupPending
        };
        let receipt = if let Some(completed_at) =
            row.try_get::<Option<DateTime<Utc>>, _>("receipt_completed_at")?
        {
            Some(DeletionReceipt {
                contract_version: LIFECYCLE_PURGE_CONTRACT_VERSION.to_string(),
                operation_id,
                outcome: PurgeOutcome::Completed,
                counts: serde_json::from_value(row.get("receipt_counts"))?,
                completed_at,
                policy: serde_json::from_value(row.get("receipt_policy"))?,
            })
        } else {
            None
        };

        Ok(Some(PurgeStatus {
            contract_version: LIFECYCLE_PURGE_CONTRACT_VERSION.to_string(),
            operation_id,
            outcome,
            counts,
            blob_cleanup_pending: row.get::<i64, _>("blob_pending") as u64,
            search_cleanup_pending: !row.get::<bool, _>("search_cleanup_complete"),
            receipt,
        }))
    }

    pub async fn pending_blob_cleanup_tx(
        &self,
        tx: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<Vec<LifecyclePurgeBlobCleanup>> {
        advisory_lock(tx, operation_id).await?;
        let rows = sqlx::query(
            r#"
            SELECT operation_id, blob_id, storage_path
              FROM lifecycle_purge_blob_cleanup
             WHERE operation_id = $1 AND state = 'pending'
             ORDER BY blob_id
             FOR UPDATE
            "#,
        )
        .bind(operation_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;
        Ok(rows
            .into_iter()
            .map(|row| LifecyclePurgeBlobCleanup {
                operation_id: row.get("operation_id"),
                blob_id: row.get("blob_id"),
                storage_path: row.get("storage_path"),
            })
            .collect())
    }

    /// Re-erase note identities resurrected by a restore before they can be
    /// queued for indexing. The original operation returns to cleanup-pending
    /// until any restored sidecars and the search cache are cleared again.
    pub async fn reerase_restored_tx(
        &self,
        tx: &mut PgConnection,
    ) -> Result<(Vec<Uuid>, Vec<Uuid>)> {
        let operation_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT operation_id
              FROM (
                    SELECT operation.id AS operation_id
                      FROM lifecycle_purge_operation operation
                     WHERE operation.state = 'cleanup_pending'
                    UNION
                    SELECT target.operation_id
                      FROM lifecycle_purge_erasure_target target
                      JOIN note restored ON restored.id = target.note_id
                   ) resumable
             ORDER BY operation_id
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;

        let mut reerased_note_ids = Vec::new();
        for operation_id in &operation_ids {
            advisory_lock(tx, *operation_id).await?;
            let note_ids: Vec<Uuid> = sqlx::query_scalar(
                r#"
                SELECT target.note_id
                  FROM lifecycle_purge_erasure_target target
                  JOIN note restored ON restored.id = target.note_id
                 WHERE target.operation_id = $1
                 ORDER BY target.note_id
                 FOR UPDATE OF restored
                "#,
            )
            .bind(operation_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(Error::Database)?;
            if note_ids.is_empty() {
                continue;
            }
            reerased_note_ids.extend_from_slice(&note_ids);

            let blob_rows = orphan_blob_rows(tx, &note_ids).await?;
            if blob_rows
                .iter()
                .any(|row| row.requires_path && row.storage_path.is_none())
            {
                return Err(Error::Config(
                    "Restore re-erasure found a filesystem blob without a storage path."
                        .to_string(),
                ));
            }
            for blob in &blob_rows {
                if let Some(path) = &blob.storage_path {
                    sqlx::query(
                        r#"
                        INSERT INTO lifecycle_purge_blob_cleanup (
                            operation_id, blob_id, storage_path, state, attempt_count,
                            last_failure_class, completed_at
                        ) VALUES ($1, $2, $3, 'pending', 0, NULL, NULL)
                        ON CONFLICT (operation_id, blob_id) DO UPDATE
                           SET storage_path = EXCLUDED.storage_path,
                               state = 'pending', attempt_count = 0,
                               last_failure_class = NULL, completed_at = NULL
                        "#,
                    )
                    .bind(operation_id)
                    .bind(blob.id)
                    .bind(path)
                    .execute(&mut *tx)
                    .await
                    .map_err(Error::Database)?;
                }
            }

            // The prior receipt described the prior terminal state. Remove it
            // atomically with resurrection erasure and reissue one only after
            // the new external cleanup reaches terminal state.
            sqlx::query("DELETE FROM deletion_receipt WHERE operation_id = $1")
                .bind(operation_id)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
            sqlx::query(
                r#"
                UPDATE lifecycle_purge_operation
                   SET state = 'cleanup_pending', search_cleanup_complete = FALSE,
                       reerasure_count = reerasure_count + 1,
                       last_failure_class = NULL, completed_at = NULL, updated_at = now()
                 WHERE id = $1
                "#,
            )
            .bind(operation_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            delete_selected(tx, &note_ids, &PurgeSelector::default()).await?;
            let blob_ids = blob_rows.iter().map(|row| row.id).collect::<Vec<_>>();
            if !blob_ids.is_empty() {
                sqlx::query("DELETE FROM attachment_blob WHERE id = ANY($1)")
                    .bind(&blob_ids)
                    .execute(&mut *tx)
                    .await
                    .map_err(Error::Database)?;
            }
        }
        reerased_note_ids.sort_unstable();
        reerased_note_ids.dedup();
        Ok((operation_ids, reerased_note_ids))
    }

    pub async fn mark_blob_cleanup_complete_tx(
        &self,
        tx: &mut PgConnection,
        operation_id: Uuid,
        blob_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE lifecycle_purge_blob_cleanup
               SET state = 'completed', storage_path = NULL, attempt_count = attempt_count + 1,
                   last_failure_class = NULL, completed_at = now()
             WHERE operation_id = $1 AND blob_id = $2 AND state = 'pending'
            "#,
        )
        .bind(operation_id)
        .bind(blob_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    pub async fn record_blob_cleanup_failure_tx(
        &self,
        tx: &mut PgConnection,
        operation_id: Uuid,
        blob_id: Uuid,
        failure_class: &'static str,
    ) -> Result<()> {
        if !matches!(failure_class, "filesystem" | "invalid_storage_path") {
            return Err(Error::InvalidInput(
                "Unsupported lifecycle purge failure class.".to_string(),
            ));
        }
        sqlx::query(
            r#"
            UPDATE lifecycle_purge_blob_cleanup
               SET attempt_count = attempt_count + 1, last_failure_class = $3
             WHERE operation_id = $1 AND blob_id = $2 AND state = 'pending'
            "#,
        )
        .bind(operation_id)
        .bind(blob_id)
        .bind(failure_class)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        sqlx::query(
            "UPDATE lifecycle_purge_operation SET last_failure_class = $2, updated_at = now() WHERE id = $1",
        )
        .bind(operation_id)
        .bind(failure_class)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    pub async fn mark_search_cleanup_complete_tx(
        &self,
        tx: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE lifecycle_purge_operation
               SET search_cleanup_complete = TRUE, last_failure_class = NULL, updated_at = now()
             WHERE id = $1 AND state = 'cleanup_pending'
            "#,
        )
        .bind(operation_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    /// Issue exactly one terminal receipt after all external effects completed.
    pub async fn finalize_tx(
        &self,
        tx: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<Option<DeletionReceipt>> {
        advisory_lock(tx, operation_id).await?;
        let row = sqlx::query(
            r#"
            SELECT state, counts, search_cleanup_complete,
                   EXISTS (
                       SELECT 1 FROM lifecycle_purge_blob_cleanup
                        WHERE operation_id = $1 AND state = 'pending'
                   ) AS blob_pending
              FROM lifecycle_purge_operation
             WHERE id = $1
             FOR UPDATE
            "#,
        )
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        let Some(row) = row else { return Ok(None) };

        if row.get::<String, _>("state") != "completed"
            && (!row.get::<bool, _>("search_cleanup_complete")
                || row.get::<bool, _>("blob_pending"))
        {
            return Ok(None);
        }

        let counts: PurgeCounts = serde_json::from_value(row.get("counts"))?;
        let policy = PurgeReceiptPolicy::default();
        let completed_at: DateTime<Utc> = sqlx::query_scalar(
            r#"
            INSERT INTO deletion_receipt (operation_id, outcome, counts, completed_at, policy)
            VALUES ($1, 'completed', $2, now(), $3)
            ON CONFLICT (operation_id) DO UPDATE
               SET operation_id = EXCLUDED.operation_id
            RETURNING completed_at
            "#,
        )
        .bind(operation_id)
        .bind(serde_json::to_value(&counts)?)
        .bind(serde_json::to_value(&policy)?)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;
        sqlx::query(
            r#"
            UPDATE lifecycle_purge_operation
               SET state = 'completed', completed_at = $2, updated_at = $2
             WHERE id = $1
            "#,
        )
        .bind(operation_id)
        .bind(completed_at)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        Ok(Some(DeletionReceipt {
            contract_version: LIFECYCLE_PURGE_CONTRACT_VERSION.to_string(),
            operation_id,
            outcome: PurgeOutcome::Completed,
            counts,
            completed_at,
            policy,
        }))
    }
}

#[derive(Clone)]
struct OrphanBlobRow {
    id: Uuid,
    storage_path: Option<String>,
    requires_path: bool,
}

async fn selected_note_ids(
    tx: &mut PgConnection,
    selector: &PurgeSelector,
    lock: bool,
) -> Result<Vec<Uuid>> {
    let source_namespace = selector
        .source
        .as_ref()
        .map(|source| source.namespace.as_str());
    let external_id = selector
        .source
        .as_ref()
        .and_then(|source| source.external_id.as_deref());
    let lock_clause = if lock { " FOR UPDATE OF n" } else { "" };
    let query = format!(
        r#"
        SELECT n.id
          FROM note n
         WHERE (cardinality($1::uuid[]) = 0 OR n.id = ANY($1))
           AND ($2::text IS NULL OR EXISTS (
               SELECT 1 FROM source_identity si
                WHERE si.note_id = n.id
                  AND si.source_namespace = $2
                  AND ($3::text IS NULL OR si.external_id = $3)
           ))
         ORDER BY n.id{lock_clause}
        "#
    );
    sqlx::query_scalar(&query)
        .bind(&selector.note_ids)
        .bind(source_namespace)
        .bind(external_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)
}

async fn count_selected(tx: &mut PgConnection, ids: &[Uuid]) -> Result<PurgeCounts> {
    if ids.is_empty() {
        return Ok(PurgeCounts::default());
    }
    let row = sqlx::query(
        r#"
        SELECT
          (SELECT COUNT(*) FROM note WHERE id = ANY($1)) AS notes,
          ((SELECT COUNT(*) FROM note_revision WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM note_original_history WHERE note_id = ANY($1))) AS revisions,
          (SELECT COUNT(*) FROM link WHERE from_note_id = ANY($1) OR to_note_id = ANY($1)) AS links,
          ((SELECT COUNT(*) FROM note_tag WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM note_skos_concept WHERE note_id = ANY($1))) AS tags,
          ((SELECT COUNT(*) FROM embedding WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM note_token_embeddings WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM embedding_coarse WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM note_graph_embedding WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM embedding_set_member WHERE note_id = ANY($1))) AS embeddings,
          (SELECT COUNT(*) FROM attachment WHERE note_id = ANY($1)) AS attachments,
          (SELECT COUNT(DISTINCT candidate.blob_id)
             FROM (
               SELECT blob_id FROM attachment WHERE note_id = ANY($1)
               UNION
               SELECT preview_blob_id FROM attachment
                WHERE note_id = ANY($1) AND preview_blob_id IS NOT NULL
             ) candidate
            WHERE NOT EXISTS (
              SELECT 1 FROM attachment survivor
               WHERE survivor.note_id <> ALL($1)
                 AND (survivor.blob_id = candidate.blob_id OR survivor.preview_blob_id = candidate.blob_id)
            )) AS blobs,
          (SELECT COUNT(*) FROM graph_edge_artifact
            WHERE from_note_id = ANY($1) OR to_note_id = ANY($1)) AS graph_edges,
          ((SELECT COUNT(*) FROM provenance_edge pe
             WHERE pe.source_note_id = ANY($1)
                OR pe.revision_id IN (SELECT id FROM note_revision WHERE note_id = ANY($1))) +
           (SELECT COUNT(*) FROM provenance_activity WHERE note_id = ANY($1)) +
           (SELECT COUNT(*) FROM provenance p
             WHERE p.note_id = ANY($1)
                OR p.attachment_id IN (SELECT id FROM attachment WHERE note_id = ANY($1)))) AS provenance_edges,
          (SELECT COUNT(*) FROM source_identity WHERE note_id = ANY($1)) AS source_identities
        "#,
    )
    .bind(ids)
    .fetch_one(&mut *tx)
    .await
    .map_err(Error::Database)?;
    Ok(PurgeCounts {
        notes: row.get::<i64, _>("notes") as u64,
        revisions: row.get::<i64, _>("revisions") as u64,
        links: row.get::<i64, _>("links") as u64,
        tags: row.get::<i64, _>("tags") as u64,
        embeddings: row.get::<i64, _>("embeddings") as u64,
        attachments: row.get::<i64, _>("attachments") as u64,
        blobs: row.get::<i64, _>("blobs") as u64,
        graph_edges: row.get::<i64, _>("graph_edges") as u64,
        provenance_edges: row.get::<i64, _>("provenance_edges") as u64,
        source_identities: row.get::<i64, _>("source_identities") as u64,
    })
}

async fn orphan_blob_rows(tx: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<OrphanBlobRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT blob.id, blob.storage_backend = 'filesystem' AS requires_path,
               CASE WHEN blob.storage_backend = 'filesystem' THEN blob.storage_path END AS storage_path
          FROM attachment_blob blob
          JOIN (
              SELECT blob_id FROM attachment WHERE note_id = ANY($1)
              UNION
              SELECT preview_blob_id FROM attachment
               WHERE note_id = ANY($1) AND preview_blob_id IS NOT NULL
          ) candidate ON candidate.blob_id = blob.id
         WHERE NOT EXISTS (
             SELECT 1 FROM attachment survivor
              WHERE survivor.note_id <> ALL($1)
                AND (survivor.blob_id = blob.id OR survivor.preview_blob_id = blob.id)
         )
         ORDER BY blob.id
         FOR UPDATE OF blob
        "#,
    )
    .bind(ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(Error::Database)?;
    Ok(rows
        .into_iter()
        .map(|row| OrphanBlobRow {
            id: row.get("id"),
            storage_path: row.get("storage_path"),
            requires_path: row.get("requires_path"),
        })
        .collect())
}

async fn delete_selected(
    tx: &mut PgConnection,
    ids: &[Uuid],
    selector: &PurgeSelector,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let attachment_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM attachment WHERE note_id = ANY($1) ORDER BY id")
            .bind(ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(Error::Database)?;

    // These FKs intentionally use SET NULL or are shared outboxes, so remove
    // content-bearing rows explicitly before the note cascade.
    sqlx::query("DELETE FROM activity_log WHERE note_id = ANY($1)")
        .bind(ids)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
    sqlx::query(
        "DELETE FROM provenance_edge WHERE source_note_id = ANY($1) OR revision_id IN (SELECT id FROM note_revision WHERE note_id = ANY($1))",
    )
    .bind(ids)
    .execute(&mut *tx)
    .await
    .map_err(Error::Database)?;
    sqlx::query("DELETE FROM public.event_outbox WHERE entity_id = ANY($1) OR entity_id = ANY($2)")
        .bind(ids)
        .bind(&attachment_ids)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
    sqlx::query("DELETE FROM public.job_queue WHERE note_id = ANY($1)")
        .bind(ids)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
    sqlx::query("DELETE FROM note WHERE id = ANY($1)")
        .bind(ids)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

    // A whole-source purge also removes checkpoints and idempotency journals
    // once no live identity remains. A single external ID cannot safely own a
    // source-wide journal, so those rows are retained in that narrower mode.
    if let Some(source) = &selector.source {
        if source.external_id.is_none() {
            sqlx::query(
                r#"
                DELETE FROM source_import_run run
                 WHERE run.source_namespace = $1
                   AND NOT EXISTS (
                       SELECT 1 FROM source_identity identity
                        WHERE identity.source_namespace = run.source_namespace
                   )
                "#,
            )
            .bind(&source.namespace)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }
    }
    Ok(())
}

async fn advisory_lock(tx: &mut PgConnection, operation_id: Uuid) -> Result<()> {
    let bytes = operation_id.as_bytes();
    let key = i64::from_be_bytes(bytes[0..8].try_into().expect("UUID prefix has eight bytes"));
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(key)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_debug_is_content_free() {
        let cleanup = LifecyclePurgeBlobCleanup {
            operation_id: Uuid::now_v7(),
            blob_id: Uuid::now_v7(),
            storage_path: "blobs/private/provider-secret.bin".to_string(),
        };
        let debug = format!("{cleanup:?}");
        assert!(!debug.contains("private"));
        assert!(!debug.contains("provider-secret"));
    }
}

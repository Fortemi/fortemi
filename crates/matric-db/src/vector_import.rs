//! Entity-keyed import of externally generated profile embeddings.

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
};
use uuid::Uuid;

use matric_core::{
    check_row_space, validate_embedding_values, EmbeddingVectorSource, Error, Result,
    SourceUpsertItem, SourceUpsertItemOutcome, SourceUpsertPolicy, SourceUpsertRequest,
};

use crate::{
    delete_profile_vector_tx, upsert_profile_vectors_tx, EntityProfileVectorBatchOutcome,
    EntityProfileVectorRow, PgSourceUpsertRepository,
};

const IMPORT_BATCH_ROWS: usize = 1_000;
const SOURCE_UPSERT_BATCH_ROWS: usize = matric_core::SOURCE_UPSERT_MAX_ITEMS;
const REJECTION_SAMPLE_LIMIT: usize = 100;
pub const DEFAULT_VECTOR_IMPORT_MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingImportManifest {
    pub run_id: String,
    pub previous_run_id: Option<String>,
    pub space_id: String,
    pub created_at: DateTime<Utc>,
    pub counts: EmbeddingImportManifestCounts,
    #[serde(default)]
    pub files: Vec<EmbeddingImportManifestFile>,
    pub template_version: String,
    #[serde(default)]
    pub body_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct EmbeddingImportManifestCounts {
    pub profiles: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingImportManifestFile {
    pub path: String,
    pub sha256: String,
    pub rows: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct EmbeddingImportRejectedRow {
    pub entity_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct EmbeddingImportRejected {
    pub total: usize,
    pub rows: Vec<EmbeddingImportRejectedRow>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct EmbeddingImportRunReport {
    pub run_id: String,
    pub status: String,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub deleted: usize,
    pub rejected: EmbeddingImportRejected,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct EmbeddingImportRunRecord {
    pub run_id: String,
    pub set_id: Uuid,
    pub previous_run_id: Option<String>,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub report: EmbeddingImportRunReport,
}

#[derive(Debug, Clone)]
struct ImportSetInfo {
    id: Uuid,
    vector_source: EmbeddingVectorSource,
    dimension: usize,
    space_id: Option<String>,
    space_contract: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ProfileInputRow {
    entity_id: String,
    source: String,
    profile_hash: String,
    profile_text: Option<String>,
    template_version: String,
    space_id: String,
    dims: usize,
    embedding: Vec<f32>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Deserialize)]
struct DeletionInputRow {
    entity_id: String,
    source: String,
    run_id: String,
}

struct ValidProfileRow {
    entity_id: String,
    source: String,
    profile_hash: String,
    profile_text: String,
    template_version: String,
    embedding: Vec<f32>,
    metadata: Value,
}

struct ValidDeletionRow {
    entity_id: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct UploadManifestLine {
    #[serde(rename = "type")]
    line_type: String,
    #[serde(flatten)]
    manifest: EmbeddingImportManifest,
}

enum UploadRow {
    Profile(ProfileInputRow),
    Deletion(DeletionInputRow),
}

struct NdjsonLineReader<S> {
    stream: S,
    buffer: Vec<u8>,
    max_line_bytes: usize,
    finished: bool,
}

impl<S> NdjsonLineReader<S> {
    fn new(stream: S, max_line_bytes: usize) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            max_line_bytes,
            finished: false,
        }
    }
}

impl<S, B, E> NdjsonLineReader<S>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: fmt::Display,
{
    async fn next_line(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            if let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
                let mut line = self.buffer.drain(..=position).collect::<Vec<_>>();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(Some(line));
            }

            if self.finished {
                if self.buffer.is_empty() {
                    return Ok(None);
                }
                let mut line = std::mem::take(&mut self.buffer);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(Some(line));
            }

            let Some(chunk) = self.stream.next().await else {
                self.finished = true;
                continue;
            };
            let chunk = chunk.map_err(|error| {
                Error::InvalidInput(format!("could not read embedding import upload: {error}"))
            })?;
            let bytes = chunk.as_ref();
            if self.buffer.len().saturating_add(bytes.len()) > self.max_line_bytes
                && !bytes.contains(&b'\n')
            {
                return Err(Error::InvalidInput(format!(
                    "embedding import upload line exceeds {} byte limit",
                    self.max_line_bytes
                )));
            }
            self.buffer.extend_from_slice(bytes);
            if self
                .buffer
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(self.buffer.len())
                > self.max_line_bytes
            {
                return Err(Error::InvalidInput(format!(
                    "embedding import upload line exceeds {} byte limit",
                    self.max_line_bytes
                )));
            }
        }
    }
}

pub struct PgEmbeddingImportRepository {
    pool: PgPool,
}

impl PgEmbeddingImportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn import_run_folder(
        &self,
        schema: &str,
        set_ref: &str,
        run_dir: impl AsRef<Path>,
    ) -> Result<EmbeddingImportRunReport> {
        let run_dir = run_dir.as_ref();
        let manifest_path = run_dir.join("manifest.json");
        let manifest = read_manifest(&manifest_path).await?;
        let set = self.load_set(schema, set_ref).await?;
        if !set.vector_source.is_external() {
            return Err(Error::InvalidInput(
                "embedding import requires an external embedding set".to_string(),
            ));
        }
        check_row_space(set.space_id.as_deref(), &manifest.space_id)
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        reject_parquet_files(&manifest)?;
        verify_files(run_dir, &manifest).await?;

        let prior = self
            .existing_run_report(schema, set.id, &manifest.run_id)
            .await?;
        if let Some(mut record) = prior {
            if record.status == "applied" {
                record.report.status = "already_applied".to_string();
                return Ok(record.report);
            }
        }
        self.ensure_run_order(schema, set.id, manifest.previous_run_id.as_deref())
            .await?;
        self.start_run(schema, set.id, &manifest).await?;

        let result = self.apply_run(run_dir, schema, &set, &manifest).await;
        match result {
            Ok(mut report) => {
                report.status = "applied".to_string();
                self.finish_run(schema, set.id, &manifest.run_id, "applied", &report)
                    .await?;
                Ok(report)
            }
            Err(error) => {
                let report = EmbeddingImportRunReport {
                    run_id: manifest.run_id.clone(),
                    status: "failed".to_string(),
                    ..EmbeddingImportRunReport::default()
                };
                let _ = self
                    .finish_run(schema, set.id, &manifest.run_id, "failed", &report)
                    .await;
                Err(error)
            }
        }
    }

    pub async fn import_run_ndjson<S, B, E>(
        &self,
        schema: &str,
        set_ref: &str,
        stream: S,
        max_line_bytes: usize,
    ) -> Result<EmbeddingImportRunReport>
    where
        S: Stream<Item = std::result::Result<B, E>> + Unpin,
        B: AsRef<[u8]>,
        E: fmt::Display,
    {
        let mut lines = NdjsonLineReader::new(stream, max_line_bytes);
        let manifest_line = lines.next_line().await?.ok_or_else(|| {
            Error::InvalidInput("embedding import upload is missing manifest line".to_string())
        })?;
        let manifest = parse_upload_manifest(&manifest_line)?;
        let set = self.load_set(schema, set_ref).await?;
        if !set.vector_source.is_external() {
            return Err(Error::InvalidInput(
                "embedding import requires an external embedding set".to_string(),
            ));
        }
        check_row_space(set.space_id.as_deref(), &manifest.space_id)
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        if !manifest.files.is_empty() {
            return Err(Error::InvalidInput(
                "streamed embedding import manifest must not declare files".to_string(),
            ));
        }
        let expected_body_sha = manifest.body_sha256.as_deref().ok_or_else(|| {
            Error::InvalidInput(
                "streamed embedding import manifest requires body_sha256".to_string(),
            )
        })?;
        let expected_body_sha = normalize_body_sha256(expected_body_sha)?;

        let prior = self
            .existing_run_report(schema, set.id, &manifest.run_id)
            .await?;
        if let Some(mut record) = prior {
            if record.status == "applied" {
                record.report.status = "already_applied".to_string();
                return Ok(record.report);
            }
        }
        self.ensure_run_order(schema, set.id, manifest.previous_run_id.as_deref())
            .await?;
        self.start_run(schema, set.id, &manifest).await?;

        let result = self
            .apply_upload_stream(schema, &set, &manifest, &expected_body_sha, &mut lines)
            .await;
        match result {
            Ok(mut report) => {
                report.status = "applied".to_string();
                self.finish_run(schema, set.id, &manifest.run_id, "applied", &report)
                    .await?;
                Ok(report)
            }
            Err(error) => {
                let report = EmbeddingImportRunReport {
                    run_id: manifest.run_id.clone(),
                    status: "failed".to_string(),
                    ..EmbeddingImportRunReport::default()
                };
                let _ = self
                    .finish_run(schema, set.id, &manifest.run_id, "failed", &report)
                    .await;
                Err(error)
            }
        }
    }

    pub async fn list_runs(
        &self,
        schema: &str,
        set_ref: &str,
    ) -> Result<Vec<EmbeddingImportRunRecord>> {
        let set = self.load_set(schema, set_ref).await?;
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        let rows = sqlx::query(
            r#"
            SELECT run_id, set_id, previous_run_id, status, started_at, finished_at, report
            FROM embedding_import_run
            WHERE set_id = $1
            ORDER BY started_at DESC, run_id DESC
            "#,
        )
        .bind(set.id)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        rows.into_iter().map(run_record_from_row).collect()
    }

    pub async fn get_run(
        &self,
        schema: &str,
        set_ref: &str,
        run_id: &str,
    ) -> Result<Option<EmbeddingImportRunRecord>> {
        let set = self.load_set(schema, set_ref).await?;
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        let row = sqlx::query(
            r#"
            SELECT run_id, set_id, previous_run_id, status, started_at, finished_at, report
            FROM embedding_import_run
            WHERE set_id = $1 AND run_id = $2
            "#,
        )
        .bind(set.id)
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        row.map(run_record_from_row).transpose()
    }

    async fn apply_run(
        &self,
        run_dir: &Path,
        schema: &str,
        set: &ImportSetInfo,
        manifest: &EmbeddingImportManifest,
    ) -> Result<EmbeddingImportRunReport> {
        let mut report = EmbeddingImportRunReport {
            run_id: manifest.run_id.clone(),
            status: "applying".to_string(),
            ..EmbeddingImportRunReport::default()
        };
        let mut observed_profiles = 0usize;
        let mut observed_deletions = 0usize;

        for file in &manifest.files {
            if is_profile_path(&file.path) {
                let path = safe_join(run_dir, &file.path)?;
                let input = File::open(&path).await.map_err(|error| {
                    Error::InvalidInput(format!("could not read profile file: {error}"))
                })?;
                let mut lines = BufReader::new(input).lines();
                let mut batch = Vec::with_capacity(IMPORT_BATCH_ROWS);
                while let Some(line) = lines.next_line().await.map_err(|error| {
                    Error::InvalidInput(format!("could not read profile file: {error}"))
                })? {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let row: ProfileInputRow = serde_json::from_str(&line).map_err(|error| {
                        Error::InvalidInput(format!("invalid profile JSONL row: {error}"))
                    })?;
                    observed_profiles += 1;
                    match validate_profile_row(row, set, manifest) {
                        Ok(valid) => batch.push(valid),
                        Err((entity_id, reason)) => push_rejection(&mut report, entity_id, reason),
                    }
                    if batch.len() >= IMPORT_BATCH_ROWS {
                        self.apply_profile_batch(schema, set.id, manifest, &mut report, &mut batch)
                            .await?;
                    }
                }
                self.apply_profile_batch(schema, set.id, manifest, &mut report, &mut batch)
                    .await?;
            } else if is_deletion_path(&file.path) {
                let path = safe_join(run_dir, &file.path)?;
                let input = File::open(&path).await.map_err(|error| {
                    Error::InvalidInput(format!("could not read deletion file: {error}"))
                })?;
                let mut lines = BufReader::new(input).lines();
                let mut batch = Vec::with_capacity(IMPORT_BATCH_ROWS);
                while let Some(line) = lines.next_line().await.map_err(|error| {
                    Error::InvalidInput(format!("could not read deletion file: {error}"))
                })? {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let row: DeletionInputRow = serde_json::from_str(&line).map_err(|error| {
                        Error::InvalidInput(format!("invalid deletion JSONL row: {error}"))
                    })?;
                    observed_deletions += 1;
                    if row.run_id != manifest.run_id {
                        push_rejection(
                            &mut report,
                            row.entity_id,
                            "deletion run_id does not match manifest".to_string(),
                        );
                    } else {
                        batch.push(ValidDeletionRow {
                            entity_id: row.entity_id,
                            source: row.source,
                        });
                    }
                    if batch.len() >= IMPORT_BATCH_ROWS {
                        self.apply_deletion_batch(schema, set.id, &mut report, &mut batch)
                            .await?;
                    }
                }
                self.apply_deletion_batch(schema, set.id, &mut report, &mut batch)
                    .await?;
            }
        }

        if observed_profiles != manifest.counts.profiles {
            return Err(Error::InvalidInput(format!(
                "manifest profile count mismatch: observed {observed_profiles}, expected {}",
                manifest.counts.profiles
            )));
        }
        if observed_deletions != manifest.counts.deletions {
            return Err(Error::InvalidInput(format!(
                "manifest deletion count mismatch: observed {observed_deletions}, expected {}",
                manifest.counts.deletions
            )));
        }

        // Post-batch maintenance hook (#1181): call schedule_post_batch_maintenance(set_id, changed_rows)
        // here when it is available on this branch. changed_rows is the sum of material vector changes.
        Ok(report)
    }

    async fn apply_upload_stream<S, B, E>(
        &self,
        schema: &str,
        set: &ImportSetInfo,
        manifest: &EmbeddingImportManifest,
        expected_body_sha: &str,
        lines: &mut NdjsonLineReader<S>,
    ) -> Result<EmbeddingImportRunReport>
    where
        S: Stream<Item = std::result::Result<B, E>> + Unpin,
        B: AsRef<[u8]>,
        E: fmt::Display,
    {
        let mut report = EmbeddingImportRunReport {
            run_id: manifest.run_id.clone(),
            status: "applying".to_string(),
            ..EmbeddingImportRunReport::default()
        };
        let mut observed_profiles = 0usize;
        let mut observed_deletions = 0usize;
        let mut profile_batch = Vec::with_capacity(IMPORT_BATCH_ROWS);
        let mut deletion_batch = Vec::with_capacity(IMPORT_BATCH_ROWS);
        let mut hasher = Sha256::new();

        while let Some(line) = lines.next_line().await? {
            if line.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            hasher.update(&line);
            hasher.update(b"\n");
            match parse_upload_row(&line)? {
                UploadRow::Profile(row) => {
                    if !deletion_batch.is_empty() {
                        self.apply_deletion_batch(schema, set.id, &mut report, &mut deletion_batch)
                            .await?;
                    }
                    observed_profiles += 1;
                    match validate_profile_row(row, set, manifest) {
                        Ok(valid) => profile_batch.push(valid),
                        Err((entity_id, reason)) => push_rejection(&mut report, entity_id, reason),
                    }
                    if profile_batch.len() >= IMPORT_BATCH_ROWS {
                        self.apply_profile_batch(
                            schema,
                            set.id,
                            manifest,
                            &mut report,
                            &mut profile_batch,
                        )
                        .await?;
                    }
                }
                UploadRow::Deletion(row) => {
                    if !profile_batch.is_empty() {
                        self.apply_profile_batch(
                            schema,
                            set.id,
                            manifest,
                            &mut report,
                            &mut profile_batch,
                        )
                        .await?;
                    }
                    observed_deletions += 1;
                    if row.run_id != manifest.run_id {
                        push_rejection(
                            &mut report,
                            row.entity_id,
                            "deletion run_id does not match manifest".to_string(),
                        );
                    } else {
                        deletion_batch.push(ValidDeletionRow {
                            entity_id: row.entity_id,
                            source: row.source,
                        });
                    }
                    if deletion_batch.len() >= IMPORT_BATCH_ROWS {
                        self.apply_deletion_batch(schema, set.id, &mut report, &mut deletion_batch)
                            .await?;
                    }
                }
            }
        }
        self.apply_profile_batch(schema, set.id, manifest, &mut report, &mut profile_batch)
            .await?;
        self.apply_deletion_batch(schema, set.id, &mut report, &mut deletion_batch)
            .await?;

        let actual_body_sha = hex::encode(hasher.finalize());
        if actual_body_sha != expected_body_sha {
            return Err(Error::InvalidInput(
                "streamed embedding import body_sha256 mismatch".to_string(),
            ));
        }
        if observed_profiles != manifest.counts.profiles {
            return Err(Error::InvalidInput(format!(
                "manifest profile count mismatch: observed {observed_profiles}, expected {}",
                manifest.counts.profiles
            )));
        }
        if observed_deletions != manifest.counts.deletions {
            return Err(Error::InvalidInput(format!(
                "manifest deletion count mismatch: observed {observed_deletions}, expected {}",
                manifest.counts.deletions
            )));
        }

        Ok(report)
    }

    async fn apply_profile_batch(
        &self,
        schema: &str,
        set_id: Uuid,
        manifest: &EmbeddingImportManifest,
        report: &mut EmbeddingImportRunReport,
        batch: &mut Vec<ValidProfileRow>,
    ) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let rows = std::mem::take(batch);
        let source_upserts = PgSourceUpsertRepository::new(self.pool.clone());
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        let mut vector_rows = Vec::with_capacity(rows.len());
        let mut grouped: HashMap<String, Vec<(usize, ValidProfileRow)>> = HashMap::new();
        for (index, row) in rows.into_iter().enumerate() {
            grouped
                .entry(row.source.clone())
                .or_default()
                .push((index, row));
        }

        for (source, group) in grouped {
            for chunk in group.chunks(SOURCE_UPSERT_BATCH_ROWS) {
                let request = SourceUpsertRequest {
                    source_namespace: source.clone(),
                    source_id: Some(source.clone()),
                    source_schema_version: "embedding-profile-run-v1".to_string(),
                    import_run_id: manifest.run_id.clone(),
                    batch_id: Some(format!(
                        "{}:{}:{}",
                        manifest.run_id,
                        source,
                        chunk.first().map(|(index, _)| *index).unwrap_or(0)
                    )),
                    workspace_id: None,
                    checkpoint: None,
                    dry_run: false,
                    policy: SourceUpsertPolicy::Replace,
                    items: chunk
                        .iter()
                        .map(|(_, row)| SourceUpsertItem {
                            external_id: row.entity_id.clone(),
                            content: source_upsert_content(row),
                            content_digest: None,
                            caller_stable_id: None,
                            title: None,
                            format: "markdown".to_string(),
                            metadata: source_upsert_metadata(row),
                            policy: None,
                        })
                        .collect(),
                };
                let receipt = source_upserts.upsert_tx(&mut tx, request).await?;
                for item in receipt.items {
                    let row = &chunk[item.index].1;
                    match item.outcome {
                        SourceUpsertItemOutcome::Conflict | SourceUpsertItemOutcome::Rejected => {
                            push_rejection(
                                report,
                                row.entity_id.clone(),
                                item.reason_code
                                    .unwrap_or_else(|| "source upsert rejected row".to_string()),
                            );
                        }
                        _ => {
                            if let Some(note_id) = item.note_id {
                                vector_rows.push(EntityProfileVectorRow {
                                    note_id,
                                    profile_hash: row.profile_hash.clone(),
                                    template_version: row.template_version.clone(),
                                    profile_text: row.profile_text.clone(),
                                    vector: Vector::from(row.embedding.clone()),
                                });
                            } else {
                                push_rejection(
                                    report,
                                    row.entity_id.clone(),
                                    "source upsert did not return a note id".to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }

        let outcome = upsert_profile_vectors_tx(&mut tx, set_id, vector_rows).await?;
        observe_outcome(report, outcome);
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn apply_deletion_batch(
        &self,
        schema: &str,
        set_id: Uuid,
        report: &mut EmbeddingImportRunReport,
        batch: &mut Vec<ValidDeletionRow>,
    ) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let rows = std::mem::take(batch);
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        for row in rows {
            let note_id =
                crate::note_id_for_source_identity_tx(&mut tx, &row.source, &row.entity_id).await;
            match note_id {
                Ok(note_id) => {
                    let outcome = delete_profile_vector_tx(&mut tx, set_id, note_id).await?;
                    observe_outcome(report, outcome);
                }
                Err(_) => push_rejection(
                    report,
                    row.entity_id,
                    "source entity not found for deletion".to_string(),
                ),
            }
        }
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn load_set(&self, schema: &str, set_ref: &str) -> Result<ImportSetInfo> {
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        let row = if let Ok(id) = Uuid::parse_str(set_ref) {
            sqlx::query(
                r#"
                SELECT es.id, es.vector_source, ec.dimension, ec.space_id, ec.space_contract
                FROM embedding_set es
                JOIN embedding_config ec ON ec.id = es.embedding_config_id
                WHERE es.id = $1
                "#,
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(Error::Database)?
        } else {
            sqlx::query(
                r#"
                SELECT es.id, es.vector_source, ec.dimension, ec.space_id, ec.space_contract
                FROM embedding_set es
                JOIN embedding_config ec ON ec.id = es.embedding_config_id
                WHERE es.slug = $1
                "#,
            )
            .bind(set_ref)
            .fetch_optional(&mut *tx)
            .await
            .map_err(Error::Database)?
        };
        tx.commit().await.map_err(Error::Database)?;
        let row = row.ok_or_else(|| Error::NotFound("Embedding set not found".to_string()))?;
        let vector_source = row
            .get::<Option<String>, _>("vector_source")
            .and_then(|value| value.parse().ok())
            .unwrap_or_default();
        let dimension: i32 = row.get("dimension");
        let dimension = usize::try_from(dimension)
            .map_err(|_| Error::InvalidInput("embedding dimension must be positive".to_string()))?;
        Ok(ImportSetInfo {
            id: row.get("id"),
            vector_source,
            dimension,
            space_id: row.get("space_id"),
            space_contract: row.get("space_contract"),
        })
    }

    async fn existing_run_report(
        &self,
        schema: &str,
        set_id: Uuid,
        run_id: &str,
    ) -> Result<Option<EmbeddingImportRunRecord>> {
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        let row = sqlx::query(
            r#"
            SELECT run_id, set_id, previous_run_id, status, started_at, finished_at, report
            FROM embedding_import_run
            WHERE set_id = $1 AND run_id = $2
            "#,
        )
        .bind(set_id)
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        row.map(run_record_from_row).transpose()
    }

    async fn ensure_run_order(
        &self,
        schema: &str,
        set_id: Uuid,
        previous_run_id: Option<&str>,
    ) -> Result<()> {
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        let last_run: Option<String> = sqlx::query_scalar(
            r#"
            SELECT run_id
            FROM embedding_import_run
            WHERE set_id = $1 AND status = 'applied'
            ORDER BY finished_at DESC NULLS LAST, started_at DESC
            LIMIT 1
            "#,
        )
        .bind(set_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        if last_run.as_deref() == previous_run_id {
            Ok(())
        } else {
            Err(Error::InvalidInput(
                "embedding import run is out of order for this set".to_string(),
            ))
        }
    }

    async fn start_run(
        &self,
        schema: &str,
        set_id: Uuid,
        manifest: &EmbeddingImportManifest,
    ) -> Result<()> {
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        sqlx::query(
            r#"
            INSERT INTO embedding_import_run (
                set_id, run_id, previous_run_id, status, manifest,
                profiles_count, deletions_count
            )
            VALUES ($1, $2, $3, 'applying', $4, $5, $6)
            "#,
        )
        .bind(set_id)
        .bind(&manifest.run_id)
        .bind(&manifest.previous_run_id)
        .bind(serde_json::to_value(manifest).unwrap_or(Value::Null))
        .bind(i32::try_from(manifest.counts.profiles).unwrap_or(i32::MAX))
        .bind(i32::try_from(manifest.counts.deletions).unwrap_or(i32::MAX))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn finish_run(
        &self,
        schema: &str,
        set_id: Uuid,
        run_id: &str,
        status: &str,
        report: &EmbeddingImportRunReport,
    ) -> Result<()> {
        let mut tx = begin_schema_tx(&self.pool, schema).await?;
        sqlx::query(
            r#"
            UPDATE embedding_import_run
               SET status = $1,
                   finished_at = now(),
                   report = $2,
                   inserted_count = $3,
                   updated_count = $4,
                   unchanged_count = $5,
                   deleted_count = $6,
                   rejected_count = $7
             WHERE set_id = $8 AND run_id = $9
            "#,
        )
        .bind(status)
        .bind(serde_json::to_value(report).unwrap_or(Value::Null))
        .bind(i32::try_from(report.inserted).unwrap_or(i32::MAX))
        .bind(i32::try_from(report.updated).unwrap_or(i32::MAX))
        .bind(i32::try_from(report.unchanged).unwrap_or(i32::MAX))
        .bind(i32::try_from(report.deleted).unwrap_or(i32::MAX))
        .bind(i32::try_from(report.rejected.total).unwrap_or(i32::MAX))
        .bind(set_id)
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }
}

async fn begin_schema_tx<'pool>(
    pool: &'pool PgPool,
    schema: &str,
) -> Result<sqlx::Transaction<'pool, sqlx::Postgres>> {
    crate::validate_schema_name(schema)?;
    let mut tx = pool.begin().await.map_err(Error::Database)?;
    let set_search_path = format!("SET LOCAL search_path TO {schema}, public");
    sqlx::query(&set_search_path)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
    Ok(tx)
}

async fn read_manifest(path: &Path) -> Result<EmbeddingImportManifest> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| Error::InvalidInput(format!("could not read import manifest: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| Error::InvalidInput(format!("invalid import manifest: {error}")))
}

fn parse_upload_manifest(line: &[u8]) -> Result<EmbeddingImportManifest> {
    let typed: UploadManifestLine = serde_json::from_slice(line)
        .map_err(|error| Error::InvalidInput(format!("invalid upload manifest: {error}")))?;
    if typed.line_type != "manifest" {
        return Err(Error::InvalidInput(
            "embedding import upload first line must have type \"manifest\"".to_string(),
        ));
    }
    Ok(typed.manifest)
}

fn parse_upload_row(line: &[u8]) -> Result<UploadRow> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|error| Error::InvalidInput(format!("invalid upload JSON row: {error}")))?;
    let Some(line_type) = value.get("type").and_then(Value::as_str) else {
        return Err(Error::InvalidInput(
            "embedding import upload row is missing type".to_string(),
        ));
    };
    match line_type {
        "profile" => serde_json::from_value(value)
            .map(UploadRow::Profile)
            .map_err(|error| Error::InvalidInput(format!("invalid profile upload row: {error}"))),
        "deletion" => serde_json::from_value(value)
            .map(UploadRow::Deletion)
            .map_err(|error| Error::InvalidInput(format!("invalid deletion upload row: {error}"))),
        _ => Err(Error::InvalidInput(format!(
            "unsupported embedding import upload row type: {line_type}"
        ))),
    }
}

fn reject_parquet_files(manifest: &EmbeddingImportManifest) -> Result<()> {
    if manifest
        .files
        .iter()
        .any(|file| file.path.ends_with(".parquet"))
    {
        #[cfg(feature = "parquet-vector-import")]
        let message = "Parquet embedding import is reserved; convert Parquet to the streamed NDJSON upload contract before importing";
        #[cfg(not(feature = "parquet-vector-import"))]
        let message = "Parquet embedding import requires conversion to the streamed NDJSON upload contract before importing";
        return Err(Error::InvalidInput(message.to_string()));
    }
    Ok(())
}

async fn verify_files(run_dir: &Path, manifest: &EmbeddingImportManifest) -> Result<()> {
    for file in &manifest.files {
        if !is_profile_path(&file.path) && !is_deletion_path(&file.path) {
            return Err(Error::InvalidInput(format!(
                "unsupported import file path: {}",
                file.path
            )));
        }
        let path = safe_join(run_dir, &file.path)?;
        let actual = sha256_file(&path).await?;
        let expected = normalize_sha256(&file.sha256)?;
        if actual != expected {
            return Err(Error::InvalidInput(format!(
                "manifest sha256 mismatch for {}",
                file.path
            )));
        }
        let rows = count_jsonl_rows(&path).await?;
        if rows != file.rows {
            return Err(Error::InvalidInput(format!(
                "manifest row count mismatch for {}",
                file.path
            )));
        }
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.contains("..") {
        return Err(Error::InvalidInput("invalid import file path".to_string()));
    }
    Ok(root.join(path))
}

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .await
        .map_err(|error| Error::InvalidInput(format!("could not read import file: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| Error::InvalidInput(format!("could not hash import file: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn count_jsonl_rows(path: &Path) -> Result<usize> {
    let file = File::open(path)
        .await
        .map_err(|error| Error::InvalidInput(format!("could not read import file: {error}")))?;
    let mut lines = BufReader::new(file).lines();
    let mut count = 0usize;
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| Error::InvalidInput(format!("could not read import file: {error}")))?
    {
        if !line.trim().is_empty() {
            count += 1;
        }
    }
    Ok(count)
}

fn validate_profile_row(
    row: ProfileInputRow,
    set: &ImportSetInfo,
    manifest: &EmbeddingImportManifest,
) -> std::result::Result<ValidProfileRow, (String, String)> {
    let entity_id = row.entity_id.clone();
    if row.template_version != manifest.template_version {
        return Err((
            entity_id,
            "template_version does not match manifest".to_string(),
        ));
    }
    check_row_space(set.space_id.as_deref(), &row.space_id)
        .map_err(|error| (entity_id.clone(), error.to_string()))?;
    if row.dims != set.dimension {
        return Err((entity_id, "dims do not match embedding set".to_string()));
    }
    validate_embedding_values(&row.embedding, set.dimension)
        .map_err(|error| (entity_id.clone(), error.to_string()))?;
    if requires_l2_norm(set.space_contract.as_ref()) {
        let norm = l2_norm(&row.embedding);
        if (norm - 1.0).abs() > 1e-3 {
            return Err((
                entity_id,
                "embedding vector is not unit normalized".to_string(),
            ));
        }
    }
    if let Some(text) = row.profile_text.as_deref() {
        let expected = profile_text_hash(&row.template_version, text);
        if row.profile_hash != expected {
            return Err((
                entity_id,
                "profile_hash does not match profile_text".to_string(),
            ));
        }
    }
    Ok(ValidProfileRow {
        entity_id,
        source: row.source,
        profile_hash: row.profile_hash,
        profile_text: row.profile_text.unwrap_or_default(),
        template_version: row.template_version,
        embedding: row.embedding,
        metadata: row.metadata,
    })
}

fn requires_l2_norm(contract: Option<&Value>) -> bool {
    contract
        .and_then(|value| value.get("normalization"))
        .and_then(Value::as_str)
        .is_some_and(|value| matches!(value, "l2" | "l2-after-truncate"))
}

fn l2_norm(values: &[f32]) -> f32 {
    values.iter().map(|value| value * value).sum::<f32>().sqrt()
}

fn profile_text_hash(template_version: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template_version.as_bytes());
    hasher.update(b"\n");
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

fn source_upsert_content(row: &ValidProfileRow) -> String {
    if row.profile_text.trim().is_empty() {
        format!(
            "External profile entity\n\nsource: {}\nentity: {}",
            row.source, row.entity_id
        )
    } else {
        row.profile_text.clone()
    }
}

fn source_upsert_metadata(row: &ValidProfileRow) -> Value {
    let mut metadata = match &row.metadata {
        Value::Object(map) => Value::Object(map.clone()),
        _ => Value::Object(Default::default()),
    };
    if let Value::Object(map) = &mut metadata {
        map.insert(
            "embedding_import_source".to_string(),
            Value::String(row.source.clone()),
        );
        map.insert(
            "embedding_profile_hash".to_string(),
            Value::String(row.profile_hash.clone()),
        );
        map.insert(
            "embedding_template_version".to_string(),
            Value::String(row.template_version.clone()),
        );
    }
    metadata
}

fn push_rejection(report: &mut EmbeddingImportRunReport, entity_id: String, reason: String) {
    report.rejected.total += 1;
    if report.rejected.rows.len() < REJECTION_SAMPLE_LIMIT {
        report
            .rejected
            .rows
            .push(EmbeddingImportRejectedRow { entity_id, reason });
    }
}

fn observe_outcome(
    report: &mut EmbeddingImportRunReport,
    outcome: EntityProfileVectorBatchOutcome,
) {
    report.inserted += outcome.inserted;
    report.updated += outcome.updated;
    report.unchanged += outcome.unchanged;
    report.deleted += outcome.deleted;
}

fn is_profile_path(path: &str) -> bool {
    path.starts_with("profiles/") && (path.ends_with(".jsonl") || path.ends_with(".ndjson"))
}

fn is_deletion_path(path: &str) -> bool {
    matches!(path, "deletions.jsonl" | "deletions.ndjson")
}

fn normalize_sha256(value: &str) -> Result<String> {
    let trimmed = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
    if trimmed.len() == 64 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Err(Error::InvalidInput(
            "manifest file sha256 is invalid".to_string(),
        ))
    }
}

fn normalize_body_sha256(value: &str) -> Result<String> {
    normalize_sha256(value).map_err(|_| {
        Error::InvalidInput("streamed embedding import body_sha256 is invalid".to_string())
    })
}

fn run_record_from_row(row: sqlx::postgres::PgRow) -> Result<EmbeddingImportRunRecord> {
    let report: Value = row.try_get("report").map_err(Error::Database)?;
    Ok(EmbeddingImportRunRecord {
        run_id: row.try_get("run_id").map_err(Error::Database)?,
        set_id: row.try_get("set_id").map_err(Error::Database)?,
        previous_run_id: row.try_get("previous_run_id").map_err(Error::Database)?,
        status: row.try_get("status").map_err(Error::Database)?,
        started_at: row.try_get("started_at").map_err(Error::Database)?,
        finished_at: row.try_get("finished_at").map_err(Error::Database)?,
        report: serde_json::from_value(report).map_err(|_| {
            Error::Config("embedding import run report could not be decoded".to_string())
        })?,
    })
}

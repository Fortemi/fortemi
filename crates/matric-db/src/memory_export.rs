//! Point-in-time memory export queries (Fortemi #1157, ADR-109).
//!
//! The whole export — records, tombstones, and the high-water mark inputs — is
//! read by one SQL statement, so it observes a single MVCC snapshot under any
//! isolation level. Callers run it read-only inside their own transaction.

use chrono::Utc;
use serde_json::{Map, Value};
use sqlx::postgres::{PgConnection, PgPool};
use sqlx::Row;
use uuid::Uuid;

use matric_core::{
    sort_export_records, summarize_export_records, Error, ExportEntityType, ExportOp, ExportRecord,
    HighWaterMark, MemoryExport, MemoryExportManifest, MemoryExportMemory, MemoryExportProducer,
    MemoryExportRequest, MemoryExportSelection, MemoryExportWindow, Result, MEMORY_EXPORT_CONTRACT,
    MEMORY_EXPORT_VERSION,
};

/// Producer identity recorded in every manifest.
#[derive(Clone, Debug)]
pub struct MemoryExportContext {
    pub memory_name: Option<String>,
    pub schema: String,
    pub producer: MemoryExportProducer,
    pub schema_version: String,
}

const EXPORT_SQL: &str = r#"
WITH scope AS MATERIALIZED (
    SELECT current_setting('app.current_tenant')::uuid AS tenant_id,
           pg_snapshot_xmin(pg_current_snapshot()) AS snapshot_xmin
),
upserts AS (
    SELECT 'collection'::text AS entity, jsonb_build_array(c.id) AS entity_key,
           jsonb_build_object(
               'id', c.id, 'name', c.name, 'description', c.description,
               'parent_id', c.parent_id, 'created_at_utc', c.created_at_utc
           ) AS record
      FROM collection c, scope
     WHERE 'collection' = ANY($1) AND c.tenant_id = scope.tenant_id
       AND ($2::text IS NULL OR c.export_change_xid >= $2::text::xid8)
    UNION ALL
    SELECT 'note', jsonb_build_array(n.id),
           jsonb_build_object(
               'id', n.id, 'collection_id', n.collection_id, 'format', n.format,
               'source', n.source, 'title', n.title, 'metadata', n.metadata,
               'starred', n.starred, 'archived', n.archived,
               'visibility', n.visibility, 'document_type_id', n.document_type_id,
               'owner_id', n.owner_id, 'created_at_utc', n.created_at_utc,
               'updated_at_utc', n.updated_at_utc, 'deleted_at', n.deleted_at
           )
      FROM note n, scope
     WHERE 'note' = ANY($1) AND n.tenant_id = scope.tenant_id
       AND ($2::text IS NULL OR n.export_change_xid >= $2::text::xid8)
    UNION ALL
    SELECT 'note_original', jsonb_build_array(o.note_id),
           jsonb_build_object(
               'note_id', o.note_id, 'content', o.content, 'hash', o.hash,
               'version_number', o.version_number,
               'user_created_at', o.user_created_at,
               'user_last_edited_at', o.user_last_edited_at
           )
      FROM note_original o, scope
     WHERE 'note_original' = ANY($1) AND o.tenant_id = scope.tenant_id
       AND ($2::text IS NULL OR o.export_change_xid >= $2::text::xid8)
    UNION ALL
    SELECT 'note_revised_current', jsonb_build_array(r.note_id),
           jsonb_build_object(
               'note_id', r.note_id, 'content', r.content,
               'last_revision_id', r.last_revision_id, 'ai_metadata', r.ai_metadata
           )
      FROM note_revised_current r, scope
     WHERE 'note_revised_current' = ANY($1) AND r.tenant_id = scope.tenant_id
       AND ($2::text IS NULL OR r.export_change_xid >= $2::text::xid8)
    UNION ALL
    SELECT 'note_tag', jsonb_build_array(t.note_id, t.tag_name),
           jsonb_build_object('note_id', t.note_id, 'tag_name', t.tag_name, 'source', t.source)
      FROM note_tag t, scope
     WHERE 'note_tag' = ANY($1) AND t.tenant_id = scope.tenant_id
       AND ($2::text IS NULL OR t.export_change_xid >= $2::text::xid8)
    UNION ALL
    SELECT 'link', jsonb_build_array(l.id),
           jsonb_build_object(
               'id', l.id, 'from_note_id', l.from_note_id, 'to_note_id', l.to_note_id,
               'to_url', l.to_url, 'kind', l.kind, 'score', l.score,
               'metadata', l.metadata, 'created_at_utc', l.created_at_utc
           )
      FROM link l, scope
     WHERE 'link' = ANY($1) AND l.tenant_id = scope.tenant_id
       AND ($2::text IS NULL OR l.export_change_xid >= $2::text::xid8)
),
deletes AS (
    SELECT t.entity_type, t.entity_key
      FROM export_tombstone t, scope
     WHERE $2::text IS NOT NULL AND t.entity_type = ANY($1)
       AND t.tenant_id = scope.tenant_id AND t.change_xid >= $2::text::xid8
),
high AS (
    SELECT GREATEST(
        (SELECT max(x.export_change_xid) FROM collection x, scope WHERE x.tenant_id = scope.tenant_id),
        (SELECT max(x.export_change_xid) FROM note x, scope WHERE x.tenant_id = scope.tenant_id),
        (SELECT max(x.export_change_xid) FROM note_original x, scope WHERE x.tenant_id = scope.tenant_id),
        (SELECT max(x.export_change_xid) FROM note_revised_current x, scope WHERE x.tenant_id = scope.tenant_id),
        (SELECT max(x.export_change_xid) FROM note_tag x, scope WHERE x.tenant_id = scope.tenant_id),
        (SELECT max(x.export_change_xid) FROM link x, scope WHERE x.tenant_id = scope.tenant_id),
        (SELECT max(x.change_xid) FROM export_tombstone x, scope WHERE x.tenant_id = scope.tenant_id)
    ) AS max_xid
)
SELECT 'meta'::text AS kind, NULL::text AS entity, NULL::jsonb AS entity_key,
       NULL::jsonb AS record, scope.tenant_id::text AS tenant_id,
       scope.snapshot_xmin::text AS snapshot_xmin, high.max_xid::text AS max_xid
  FROM scope, high
UNION ALL
SELECT 'upsert', entity, entity_key, record, NULL, NULL, NULL FROM upserts
UNION ALL
SELECT 'delete', entity_type, entity_key, NULL, NULL, NULL, NULL FROM deletes
"#;

/// Derive the high-water mark from the snapshot and the visible stamps.
///
/// Every transaction below `snapshot_xmin` has finished, so no later commit can
/// carry a smaller stamp. When nothing newer than the latest visible stamp is
/// still running, `max_xid + 1` is also safe and depends only on the data,
/// which keeps repeated exports of unchanged data byte-identical.
pub fn derive_high_water_mark(snapshot_xmin: u64, max_xid: Option<u64>) -> HighWaterMark {
    let data_bound = max_xid.map_or(1, |value| value.saturating_add(1)).max(1);
    HighWaterMark(data_bound.min(snapshot_xmin))
}

fn parse_xid(text: &str) -> Result<u64> {
    text.parse::<u64>()
        .map_err(|_| Error::Internal("database returned a malformed xid8".to_string()))
}

fn project_fields(record: Value, fields: &[String]) -> Value {
    match record {
        Value::Object(map) => {
            let mut projected = Map::new();
            for (name, value) in map {
                if fields.iter().any(|field| field == &name) {
                    projected.insert(name, value);
                }
            }
            Value::Object(projected)
        }
        other => other,
    }
}

/// Stateless repository for memory exports.
#[derive(Clone, Copy, Debug, Default)]
pub struct PgMemoryExportRepository;

impl PgMemoryExportRepository {
    pub fn new() -> Self {
        Self
    }

    /// Export the memory selected by the connection's `search_path`.
    ///
    /// Marks the caller's transaction read-only before reading; the caller
    /// must not write in the same transaction afterwards.
    pub async fn export_tx(
        &self,
        connection: &mut PgConnection,
        context: &MemoryExportContext,
        request: &MemoryExportRequest,
    ) -> Result<MemoryExport> {
        let selection = request.selection()?;
        for statement in [
            "SET TRANSACTION READ ONLY",
            "SET LOCAL TimeZone = 'UTC'",
            "SET LOCAL extra_float_digits = 1",
        ] {
            sqlx::query(statement)
                .execute(&mut *connection)
                .await
                .map_err(Error::Database)?;
        }

        let entity_names: Vec<&str> = selection
            .entity_types
            .iter()
            .map(|entity| entity.as_str())
            .collect();
        let rows = sqlx::query(EXPORT_SQL)
            .bind(&entity_names)
            .bind(request.since.map(|mark| mark.0.to_string()))
            .fetch_all(&mut *connection)
            .await
            .map_err(Error::Database)?;

        let mut records = Vec::with_capacity(rows.len().saturating_sub(1));
        let mut meta: Option<(Uuid, u64, Option<u64>)> = None;
        for row in rows {
            let kind: String = row.get("kind");
            if kind == "meta" {
                let tenant: String = row.get("tenant_id");
                let tenant = Uuid::parse_str(&tenant).map_err(|_| {
                    Error::Internal("database tenant scope is not a UUID".to_string())
                })?;
                let snapshot_xmin = parse_xid(&row.get::<String, _>("snapshot_xmin"))?;
                let max_xid = row
                    .get::<Option<String>, _>("max_xid")
                    .map(|text| parse_xid(&text))
                    .transpose()?;
                meta = Some((tenant, snapshot_xmin, max_xid));
                continue;
            }
            let entity_name: String = row.get("entity");
            let entity = ExportEntityType::parse(&entity_name).ok_or_else(|| {
                Error::Internal("database returned an unknown export entity".to_string())
            })?;
            let key: Value = row.get("entity_key");
            let (op, record) = if kind == "upsert" {
                let record: Value = row.get("record");
                (
                    ExportOp::Upsert,
                    Some(project_fields(record, &selection.fields[&entity])),
                )
            } else {
                (ExportOp::Delete, None)
            };
            records.push(ExportRecord {
                entity,
                key,
                op,
                record,
            });
        }
        let (tenant_id, snapshot_xmin, max_xid) = meta.ok_or_else(|| {
            Error::Internal("memory export returned no snapshot metadata".to_string())
        })?;
        let high_water_mark = derive_high_water_mark(snapshot_xmin, max_xid);
        if let Some(since) = request.since {
            if since > high_water_mark {
                return Err(Error::InvalidInput(
                    "`since` is ahead of this memory's high-water mark".to_string(),
                ));
            }
        }

        sort_export_records(&mut records);
        Ok(build_export(
            context,
            request,
            selection,
            tenant_id,
            high_water_mark,
            records,
        ))
    }

    /// Community/personal path: run the export in its own
    /// `REPEATABLE READ READ ONLY` transaction scoped to `context.schema`.
    pub async fn export_snapshot(
        &self,
        pool: &PgPool,
        context: &MemoryExportContext,
        request: &MemoryExportRequest,
    ) -> Result<MemoryExport> {
        crate::validate_schema_name(&context.schema)?;
        let mut transaction = pool
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await
            .map_err(Error::Database)?;
        sqlx::query("SELECT set_config('search_path', $1, true)")
            .bind(format!("{}, public", context.schema))
            .execute(&mut *transaction)
            .await
            .map_err(Error::Database)?;
        let export = self.export_tx(&mut transaction, context, request).await?;
        transaction.commit().await.map_err(Error::Database)?;
        Ok(export)
    }
}

fn build_export(
    context: &MemoryExportContext,
    request: &MemoryExportRequest,
    selection: MemoryExportSelection,
    tenant_id: Uuid,
    high_water_mark: HighWaterMark,
    records: Vec<ExportRecord>,
) -> MemoryExport {
    let (entities, content_sha256) = summarize_export_records(&selection, &records);
    MemoryExport {
        manifest: MemoryExportManifest {
            contract: MEMORY_EXPORT_CONTRACT.to_string(),
            export_version: MEMORY_EXPORT_VERSION.to_string(),
            schema_version: context.schema_version.clone(),
            memory: MemoryExportMemory {
                name: context.memory_name.clone(),
                schema: context.schema.clone(),
                tenant_id,
            },
            mode: request.mode,
            since: request.since,
            high_water_mark,
            window: MemoryExportWindow {
                from: request.since,
                to: high_water_mark,
            },
            selection,
            entities,
            record_count: records.len() as u64,
            content_sha256,
            producer: context.producer.clone(),
            generated_at: Utc::now(),
        },
        records,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn high_water_mark_prefers_data_bound_until_an_older_writer_is_running() {
        // Quiescent: xmin is past every visible stamp, so the mark is data-derived.
        assert_eq!(derive_high_water_mark(900, Some(120)), HighWaterMark(121));
        // A transaction older than the newest visible stamp is still running.
        assert_eq!(derive_high_water_mark(100, Some(120)), HighWaterMark(100));
        // Empty memory or only pre-migration rows.
        assert_eq!(derive_high_water_mark(900, None), HighWaterMark(1));
        assert_eq!(derive_high_water_mark(900, Some(0)), HighWaterMark(1));
        assert_eq!(
            derive_high_water_mark(900, Some(u64::MAX)),
            HighWaterMark(900)
        );
    }

    #[test]
    fn field_projection_keeps_only_selected_fields() {
        let projected = project_fields(
            json!({"id": "a", "title": "t", "metadata": {}}),
            &["id".to_string(), "title".to_string()],
        );
        assert_eq!(projected, json!({"id": "a", "title": "t"}));
    }
}

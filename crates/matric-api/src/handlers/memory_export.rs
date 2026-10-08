//! Point-in-time memory export endpoint (Fortemi #1157, ADR-109).
//!
//! The export is read-only and requires the `read` scope. Hosted requests run
//! inside the verified tenant transaction; community requests open their own
//! `REPEATABLE READ READ ONLY` transaction.

use axum::{extract::State, http::header, response::IntoResponse, Extension, Json};
use matric_core::{MemoryExport, MemoryExportProducer, MemoryExportRequest};
use matric_db::{Database, MemoryExportContext, PgMemoryExportRepository};

use crate::middleware::{archive_routing::ArchiveContext, tenant_scope::TenantRequestScope};
use crate::{ApiError, AppState, Auth};

fn export_context(state: &AppState, archive: &ArchiveContext) -> MemoryExportContext {
    MemoryExportContext {
        memory_name: archive.name.clone(),
        schema: archive.schema.clone(),
        producer: MemoryExportProducer {
            name: "fortemi".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_sha: state.git_sha.clone(),
        },
        schema_version: Database::latest_migration_version().to_string(),
    }
}

/// Export the active memory from one consistent snapshot.
///
/// `full` returns every selected record; `incremental` returns records changed
/// at or after `since` plus delete tombstones. Apply semantics and the
/// high-water-mark guarantees are defined by `contracts/memory-export`.
#[utoipa::path(
    post,
    path = "/api/v1/memory/export",
    tag = "Archives",
    request_body = MemoryExportRequest,
    responses(
        (status = 200, description = "Deterministic export with manifest", body = MemoryExport),
        (status = 400, description = "Invalid selection or cursor"),
    )
)]
pub async fn export_memory(
    _auth: Auth,
    State(state): State<AppState>,
    scope: Option<Extension<TenantRequestScope>>,
    Extension(archive): Extension<ArchiveContext>,
    Json(request): Json<MemoryExportRequest>,
) -> Result<impl IntoResponse, ApiError> {
    request.selection()?;
    let context = export_context(&state, &archive);
    let repository = PgMemoryExportRepository::new();
    let export = match scope {
        Some(Extension(scope)) => {
            scope
                .with_schema_connection(archive.schema.clone(), move |connection| {
                    Box::pin(
                        async move { repository.export_tx(connection, &context, &request).await },
                    )
                })
                .await?
        }
        None if state.multi_tenant => {
            return Err(ApiError::OperationFailed {
                operation: "Memory export",
                detail: "Hosted memory transaction is unavailable.".into(),
            })
        }
        None => {
            repository
                .export_snapshot(&state.db.pool, &context, &request)
                .await?
        }
    };
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(export)))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use matric_core::{
        summarize_export_records, ExportEntityType, ExportOp, ExportRecord, HighWaterMark,
        MemoryExportManifest, MemoryExportMemory, MemoryExportMode, MemoryExportWindow,
        MEMORY_EXPORT_CONTRACT, MEMORY_EXPORT_VERSION,
    };
    use serde_json::{json, Value};

    use super::*;

    fn validator(schema: &str) -> jsonschema::Validator {
        let schema: Value = serde_json::from_str(schema).unwrap();
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .unwrap()
    }

    #[test]
    fn serialized_export_conforms_to_published_contract_schemas() {
        let request: MemoryExportRequest = serde_json::from_value(json!({
            "mode": "incremental", "since": "41", "entity_types": ["note", "note_tag"]
        }))
        .unwrap();
        let selection = request.selection().unwrap();
        let records = vec![
            ExportRecord {
                entity: ExportEntityType::Note,
                key: json!(["01900000-0000-7000-8000-000000000001"]),
                op: ExportOp::Upsert,
                record: Some(json!({"id": "01900000-0000-7000-8000-000000000001"})),
            },
            ExportRecord {
                entity: ExportEntityType::NoteTag,
                key: json!(["01900000-0000-7000-8000-000000000001", "topic/a"]),
                op: ExportOp::Delete,
                record: None,
            },
        ];
        let (entities, content_sha256) = summarize_export_records(&selection, &records);
        let export = MemoryExport {
            manifest: MemoryExportManifest {
                contract: MEMORY_EXPORT_CONTRACT.to_string(),
                export_version: MEMORY_EXPORT_VERSION.to_string(),
                schema_version: Database::latest_migration_version().to_string(),
                memory: MemoryExportMemory {
                    name: None,
                    schema: "public".to_string(),
                    tenant_id: uuid::Uuid::nil(),
                },
                mode: MemoryExportMode::Incremental,
                since: request.since,
                high_water_mark: HighWaterMark(57),
                window: MemoryExportWindow {
                    from: request.since,
                    to: HighWaterMark(57),
                },
                selection,
                entities,
                record_count: records.len() as u64,
                content_sha256,
                producer: MemoryExportProducer {
                    name: "fortemi".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    git_sha: "unknown".to_string(),
                },
                generated_at: Utc::now(),
            },
            records,
        };
        let value = serde_json::to_value(&export).unwrap();

        let manifest = validator(include_str!(
            "../../../../contracts/memory-export/1.0.0/manifest.schema.json"
        ));
        let record = validator(include_str!(
            "../../../../contracts/memory-export/1.0.0/record.schema.json"
        ));
        let request_schema = validator(include_str!(
            "../../../../contracts/memory-export/1.0.0/request.schema.json"
        ));
        assert!(
            manifest.is_valid(&value["manifest"]),
            "{}",
            value["manifest"]
        );
        for line in value["records"].as_array().unwrap() {
            assert!(record.is_valid(line), "{line}");
        }
        assert!(request_schema.is_valid(&serde_json::to_value(&request).unwrap()));
        assert!(!request_schema.is_valid(&json!({"mode": "incremental"})));
        assert!(!request_schema.is_valid(&json!({"mode": "full", "since": "3"})));
    }
}

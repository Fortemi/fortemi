use std::collections::{HashMap, HashSet};

use serde_json::Value;
use uuid::Uuid;

use super::{
    shard_component_filename, shard_operation_failed, shard_validation_failed,
    visit_shard_json_array_values_with_limits, visit_shard_jsonl_values_with_limits, ApiError,
    SHARD_MAX_RECORDS_PER_COMPONENT, SHARD_MAX_RECORD_BYTES,
};

const INVALID_REFERENCE: &str = "Knowledge shard references an unavailable destination note.";

// Called only after complete archive relationship validation. Archive note
// context is sufficient for selected notes, which apply restores or retains.
// Otherwise only references used by selected components require existing rows;
// unrelated notes carried in the archive must not constrain partial imports.
fn required_destination_notes(
    files: &HashMap<String, Vec<u8>>,
    selected: &HashSet<String>,
) -> Result<HashSet<Uuid>, ApiError> {
    let mut required = HashSet::new();
    if selected.contains("notes") {
        return Ok(required);
    }
    for (component, fields) in [
        ("links", &["from_note_id", "to_note_id"][..]),
        ("graph_edges", &["from_note_id", "to_note_id"][..]),
        ("community_assignments", &["note_id"][..]),
        ("note_originals", &["note_id"][..]),
        ("note_original_history", &["note_id"][..]),
        ("note_revised_current", &["note_id"][..]),
        ("note_revisions", &["note_id"][..]),
        ("provenance_edges", &["source_note_id"][..]),
        ("provenance_activities", &["note_id"][..]),
        ("provenance_records", &["note_id"][..]),
        ("embeddings", &["note_id"][..]),
        ("embedding_set_members", &["note_id"][..]),
        ("note_skos_tags", &["note_id"][..]),
        ("communities", &[][..]),
    ] {
        if !selected.contains(component) {
            continue;
        }
        let filename = shard_component_filename(component).expect("known shard component");
        let Some(data) = files.get(filename) else {
            continue;
        };
        let mut add = |value: &Value| -> Result<(), String> {
            if !value.is_null() {
                let id = value
                    .as_str()
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .ok_or_else(|| INVALID_REFERENCE.to_string())?;
                required.insert(id);
            }
            Ok(())
        };
        let mut visit = |record: Value| -> Result<(), String> {
            for field in fields {
                if let Some(value) = record.get(*field) {
                    add(value)?;
                }
            }
            if component == "communities" {
                if let Some(communities) = record.get("communities").and_then(Value::as_array) {
                    for community in communities {
                        if let Some(ids) = community
                            .get("representative_note_ids")
                            .and_then(Value::as_array)
                        {
                            for id in ids {
                                add(id)?;
                            }
                        }
                    }
                }
            }
            Ok(())
        };
        let result = if filename.ends_with(".jsonl") {
            visit_shard_jsonl_values_with_limits(
                data,
                SHARD_MAX_RECORDS_PER_COMPONENT,
                SHARD_MAX_RECORD_BYTES,
                &mut visit,
            )
        } else {
            visit_shard_json_array_values_with_limits(
                data,
                SHARD_MAX_RECORDS_PER_COMPONENT,
                SHARD_MAX_RECORD_BYTES,
                INVALID_REFERENCE,
                &mut visit,
            )
        };
        result.map_err(|_| shard_validation_failed(INVALID_REFERENCE))?;
    }
    Ok(required)
}

pub(super) async fn validate_destination_notes_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    files: &HashMap<String, Vec<u8>>,
    selected: &HashSet<String>,
    wipe_before_apply: bool,
) -> Result<(), ApiError> {
    let mut required = required_destination_notes(files, selected)?
        .into_iter()
        .collect::<Vec<_>>();
    if wipe_before_apply && !required.is_empty() {
        // A wipe removes existing notes even when notes are not selected.
        return Err(shard_validation_failed(INVALID_REFERENCE));
    }
    required.sort_unstable();
    for ids in required.chunks(1024) {
        // Use the same archive search_path, session tenant and RLS as apply.
        // The explicit tenant predicate also constrains privileged connections.
        // Locks retain checked parents until the import transaction completes.
        let visible = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM note WHERE id = ANY($1::uuid[])
             AND tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid
             ORDER BY id FOR KEY SHARE",
        )
        .bind(ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| shard_operation_failed("validate destination note references", error))?;
        if visible.len() != ids.len() {
            return Err(shard_validation_failed(INVALID_REFERENCE));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn graph_selection_requires_existing_endpoints_despite_archive_note_context() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let unrelated = Uuid::new_v4();
        let files = HashMap::from([
            (
                "notes.jsonl".into(),
                [a, b, unrelated]
                    .map(|id| json!({"id": id}).to_string())
                    .join("\n")
                    .into_bytes(),
            ),
            (
                "graph_edges.jsonl".into(),
                json!({"from_note_id": a, "to_note_id": b})
                    .to_string()
                    .into_bytes(),
            ),
            (
                "links.jsonl".into(),
                json!({"from_note_id": unrelated, "to_note_id": a})
                    .to_string()
                    .into_bytes(),
            ),
        ]);
        let mut selected = HashSet::from(["graph_edges".to_string()]);
        assert_eq!(
            required_destination_notes(&files, &selected).unwrap(),
            HashSet::from([a, b])
        );
        selected.insert("notes".into());
        assert!(required_destination_notes(&files, &selected)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn url_links_and_metadata_do_not_invent_note_references() {
        let a = Uuid::new_v4();
        let files = HashMap::from([(
            "links.jsonl".into(),
            json!({"from_note_id": a, "to_note_id": null,
                "to_url": "https://example.invalid/", "metadata": {"note_id": Uuid::new_v4()}})
            .to_string()
            .into_bytes(),
        )]);
        let selected = HashSet::from(["links".to_string()]);
        assert_eq!(
            required_destination_notes(&files, &selected).unwrap(),
            HashSet::from([a])
        );
    }

    #[test]
    fn community_representatives_and_assignments_require_destination_notes() {
        let representative = Uuid::new_v4();
        let assigned = Uuid::new_v4();
        let files = HashMap::from([
            (
                "communities.json".into(),
                serde_json::to_vec(&json!([{"communities": [
                    {"representative_note_ids": [representative]},
                    {"representative_note_ids": null}
                ]}]))
                .unwrap(),
            ),
            (
                "community_assignments.jsonl".into(),
                json!({"note_id": assigned}).to_string().into_bytes(),
            ),
        ]);
        let selected = HashSet::from([
            "communities".to_string(),
            "community_assignments".to_string(),
        ]);
        assert_eq!(
            required_destination_notes(&files, &selected).unwrap(),
            HashSet::from([representative, assigned])
        );
    }
}

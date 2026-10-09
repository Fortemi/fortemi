use std::collections::HashMap;

use matric_core::{validate_embedding_dimension, EmbeddingVectorType};
use serde_json::Value;
use uuid::Uuid;

pub const SHARD_SCHEMA_2_1_VERSION: &str = "2.1.0";

#[derive(Clone, Copy)]
struct ShardEmbeddingConfigContract {
    dimension: usize,
    vector_type: EmbeddingVectorType,
}

pub fn is_schema_2_1(version: &str) -> bool {
    version == SHARD_SCHEMA_2_1_VERSION
}

pub fn is_schema_2_family(version: &str) -> bool {
    matches!(version, "2.0.0" | SHARD_SCHEMA_2_1_VERSION)
}

pub fn manifest_schema(profile: &str) -> Option<&'static str> {
    match profile {
        "core-v1" => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/core-v1/manifest.schema.json"
        )),
        "record-v1" => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/record-v1/manifest.schema.json"
        )),
        "full-v1" => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/manifest.schema.json"
        )),
        _ => None,
    }
}

pub fn component_schema(profile: &str, component: &str) -> Option<&'static str> {
    match (profile, component) {
        ("core-v1", "notes") | ("full-v1", "notes") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/core-v1/note.schema.json"
        )),
        ("core-v1", "collections") | ("full-v1", "collections") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/core-v1/collection.schema.json"
        )),
        ("core-v1", "tags") | ("full-v1", "tags") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/core-v1/tag.schema.json"
        )),
        ("core-v1", "templates") | ("full-v1", "templates") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/core-v1/template.schema.json"
        )),
        ("core-v1", "links") | ("full-v1", "links") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/core-v1/link.schema.json"
        )),
        ("record-v1", "notes") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/record-v1/note.schema.json"
        )),
        ("record-v1", "collections") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/record-v1/collection.schema.json"
        )),
        ("record-v1", "tags") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/record-v1/tag.schema.json"
        )),
        ("record-v1", "links") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/record-v1/link.schema.json"
        )),
        ("full-v1", "embedding_configs") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/embedding-config.schema.json"
        )),
        ("full-v1", "embedding_sets") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/embedding-set.schema.json"
        )),
        ("full-v1", "embedding_set_members") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/embedding-set-member.schema.json"
        )),
        ("full-v1", "embeddings") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/embedding.schema.json"
        )),
        ("full-v1", "note_originals") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/note-original.schema.json"
        )),
        ("full-v1", "note_original_history") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/note-original-history.schema.json"
        )),
        ("full-v1", "note_revised_current") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/note-revised-current.schema.json"
        )),
        ("full-v1", "note_revisions") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/note-revision.schema.json"
        )),
        ("full-v1", "provenance_edges") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/provenance-edge.schema.json"
        )),
        ("full-v1", "provenance_activities") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/provenance-activity.schema.json"
        )),
        ("full-v1", "named_locations") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/named-location.schema.json"
        )),
        ("full-v1", "provenance_locations") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/provenance-location.schema.json"
        )),
        ("full-v1", "provenance_devices") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/provenance-device.schema.json"
        )),
        ("full-v1", "provenance_records") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/provenance-record.schema.json"
        )),
        ("full-v1", "skos_schemes") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-scheme.schema.json"
        )),
        ("full-v1", "skos_concepts") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-concept.schema.json"
        )),
        ("full-v1", "skos_labels") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-label.schema.json"
        )),
        ("full-v1", "skos_notes") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-note.schema.json"
        )),
        ("full-v1", "skos_relations") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-relation.schema.json"
        )),
        ("full-v1", "skos_mapping_relations") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-mapping-relation.schema.json"
        )),
        ("full-v1", "skos_scheme_memberships") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-scheme-membership.schema.json"
        )),
        ("full-v1", "note_skos_tags") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/note-skos-tag.schema.json"
        )),
        ("full-v1", "skos_collections") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-collection.schema.json"
        )),
        ("full-v1", "skos_collection_members") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/skos-collection-member.schema.json"
        )),
        ("full-v1", "graph_sources") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/graph-source.schema.json"
        )),
        ("full-v1", "graph_edges") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/graph-edge.schema.json"
        )),
        ("full-v1", "communities") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/community-set.schema.json"
        )),
        ("full-v1", "community_assignments") => Some(include_str!(
            "../../../contracts/knowledge-shard/2.1.0/full-v1/community-assignment.schema.json"
        )),
        _ => None,
    }
}

pub fn validate_schema_2_1_embedding_contract(
    files: &HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    let mut configs = HashMap::<Uuid, ShardEmbeddingConfigContract>::new();
    if let Some(data) = files.get("embedding_configs.json") {
        let values = serde_json::from_slice::<Vec<Value>>(data)
            .map_err(|_| "Knowledge shard embedding configs are invalid.".to_string())?;
        for value in values {
            let id = parse_uuid_field(&value, "id")
                .ok_or_else(|| "Knowledge shard embedding configs are invalid.".to_string())?;
            let dimension = value
                .get("dimension")
                .and_then(Value::as_u64)
                .and_then(|dimension| usize::try_from(dimension).ok())
                .ok_or_else(|| {
                    "Knowledge shard embedding config dimension is invalid.".to_string()
                })?;
            let vector_type = value
                .get("vector_type")
                .and_then(Value::as_str)
                .map(str::parse::<EmbeddingVectorType>)
                .transpose()
                .map_err(|_| {
                    "Knowledge shard embedding config vector type is invalid.".to_string()
                })?
                .ok_or_else(|| {
                    "Knowledge shard embedding config vector type is invalid.".to_string()
                })?;
            if validate_embedding_dimension(dimension, vector_type).is_err() {
                return Err(
                    "Knowledge shard embedding config dimension exceeds vector type limit."
                        .to_string(),
                );
            }
            configs.insert(
                id,
                ShardEmbeddingConfigContract {
                    dimension,
                    vector_type,
                },
            );
        }
    }

    let mut set_contracts = HashMap::<Uuid, ShardEmbeddingConfigContract>::new();
    if let Some(data) = files.get("embedding_sets.json") {
        let values = serde_json::from_slice::<Vec<Value>>(data)
            .map_err(|_| "Knowledge shard embedding sets are invalid.".to_string())?;
        for value in values {
            let Some(set_id) = parse_uuid_field(&value, "id") else {
                continue;
            };
            let Some(config_id) = parse_uuid_field(&value, "embedding_config_id") else {
                continue;
            };
            let Some(config) = configs.get(&config_id).copied() else {
                continue;
            };
            let dimension = value
                .get("truncate_dim")
                .and_then(Value::as_u64)
                .and_then(|dimension| usize::try_from(dimension).ok())
                .unwrap_or(config.dimension);
            if validate_embedding_dimension(dimension, config.vector_type).is_err() {
                return Err(
                    "Knowledge shard embedding set dimension exceeds vector type limit."
                        .to_string(),
                );
            }
            set_contracts.insert(
                set_id,
                ShardEmbeddingConfigContract {
                    dimension,
                    vector_type: config.vector_type,
                },
            );
        }
    }

    if let Some(data) = files.get("embeddings.jsonl") {
        let text = std::str::from_utf8(data)
            .map_err(|_| "Knowledge shard component is not valid UTF-8.".to_string())?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let value = serde_json::from_str::<Value>(line)
                .map_err(|_| "Knowledge shard embeddings are invalid.".to_string())?;
            let Some(set_id) = parse_uuid_field(&value, "embedding_set_id") else {
                continue;
            };
            let Some(vector) = value.get("vector").and_then(Value::as_array) else {
                continue;
            };
            let Some(contract) = set_contracts.get(&set_id) else {
                return Err("Knowledge shard embedding set dimension is required.".to_string());
            };
            if contract.dimension != vector.len() {
                return Err(
                    "Knowledge shard embedding vector length does not match declared dimension."
                        .to_string(),
                );
            }
        }
    }

    Ok(())
}

fn parse_uuid_field(value: &Value, field: &str) -> Option<Uuid> {
    value
        .get(field)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

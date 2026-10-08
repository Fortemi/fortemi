//! Point-in-time memory export contract (`memory-export/1.0.0`, ADR-109).
//!
//! An export is a canonical, deterministically ordered list of record lines
//! read from one database snapshot of one memory, plus a manifest carrying the
//! high-water mark that later incremental exports continue from.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{Error, Result};

pub const MEMORY_EXPORT_CONTRACT: &str = "memory-export";
pub const MEMORY_EXPORT_VERSION: &str = "1.0.0";

/// Exported entity types, in canonical line order.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExportEntityType {
    Collection,
    Note,
    NoteOriginal,
    NoteRevisedCurrent,
    NoteTag,
    Link,
}

impl ExportEntityType {
    pub const ALL: [ExportEntityType; 6] = [
        ExportEntityType::Collection,
        ExportEntityType::Note,
        ExportEntityType::NoteOriginal,
        ExportEntityType::NoteRevisedCurrent,
        ExportEntityType::NoteTag,
        ExportEntityType::Link,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Collection => "collection",
            Self::Note => "note",
            Self::NoteOriginal => "note_original",
            Self::NoteRevisedCurrent => "note_revised_current",
            Self::NoteTag => "note_tag",
            Self::Link => "link",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|entity| entity.as_str() == value)
    }

    /// Fields forming the stable key; always exported and used for ordering.
    pub fn key_fields(self) -> &'static [&'static str] {
        match self {
            Self::Collection | Self::Note | Self::Link => &["id"],
            Self::NoteOriginal | Self::NoteRevisedCurrent => &["note_id"],
            Self::NoteTag => &["note_id", "tag_name"],
        }
    }

    /// Every field a record of this type can carry.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Collection => &["id", "name", "description", "parent_id", "created_at_utc"],
            Self::Note => &[
                "id",
                "collection_id",
                "format",
                "source",
                "title",
                "metadata",
                "starred",
                "archived",
                "visibility",
                "document_type_id",
                "owner_id",
                "created_at_utc",
                "updated_at_utc",
                "deleted_at",
            ],
            Self::NoteOriginal => &[
                "note_id",
                "content",
                "hash",
                "version_number",
                "user_created_at",
                "user_last_edited_at",
            ],
            Self::NoteRevisedCurrent => &["note_id", "content", "last_revision_id", "ai_metadata"],
            Self::NoteTag => &["note_id", "tag_name", "source"],
            Self::Link => &[
                "id",
                "from_note_id",
                "to_note_id",
                "to_url",
                "kind",
                "score",
                "metadata",
                "created_at_utc",
            ],
        }
    }
}

impl fmt::Display for ExportEntityType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryExportMode {
    #[default]
    Full,
    Incremental,
}

/// Monotonic high-water mark: a 64-bit PostgreSQL transaction id (xid8).
///
/// Serialized as a decimal string so JavaScript consumers keep full precision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HighWaterMark(pub u64);

impl HighWaterMark {
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.len() > 20
            || !trimmed.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(Error::InvalidInput(
                "high-water mark must be a decimal transaction id".to_string(),
            ));
        }
        trimmed.parse::<u64>().map(Self).map_err(|_| {
            Error::InvalidInput("high-water mark is outside the xid8 range".to_string())
        })
    }
}

impl fmt::Display for HighWaterMark {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl Serialize for HighWaterMark {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for HighWaterMark {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        HighWaterMark::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl utoipa::PartialSchema for HighWaterMark {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .pattern(Some("^[0-9]{1,20}$"))
            .description(Some(
                "Monotonic xid8 high-water mark encoded as a decimal string.",
            ))
            .into()
    }
}

impl ToSchema for HighWaterMark {}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryExportRequest {
    #[serde(default)]
    pub mode: MemoryExportMode,
    /// Required for `incremental`; the `high_water_mark` of a previous export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<HighWaterMark>,
    /// Entity types to export. Defaults to every type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_types: Option<Vec<ExportEntityType>>,
    /// Optional per-type field subset. Key fields are always included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub fields: Option<BTreeMap<ExportEntityType, Vec<String>>>,
}

/// The validated selection an export ran with, echoed in the manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct MemoryExportSelection {
    pub entity_types: Vec<ExportEntityType>,
    #[schema(value_type = Object)]
    pub fields: BTreeMap<ExportEntityType, Vec<String>>,
}

impl MemoryExportRequest {
    /// Validate and normalize the request into a canonical selection.
    pub fn selection(&self) -> Result<MemoryExportSelection> {
        match (self.mode, self.since) {
            (MemoryExportMode::Incremental, None) => {
                return Err(Error::InvalidInput(
                    "incremental export requires `since`".to_string(),
                ))
            }
            (MemoryExportMode::Full, Some(_)) => {
                return Err(Error::InvalidInput(
                    "full export does not accept `since`".to_string(),
                ))
            }
            _ => {}
        }
        let entity_types: BTreeSet<ExportEntityType> = match &self.entity_types {
            Some(types) if types.is_empty() => {
                return Err(Error::InvalidInput(
                    "entity_types must not be empty".to_string(),
                ))
            }
            Some(types) => types.iter().copied().collect(),
            None => ExportEntityType::ALL.into_iter().collect(),
        };
        let mut fields = BTreeMap::new();
        for entity in &entity_types {
            let requested = self.fields.as_ref().and_then(|map| map.get(entity));
            let mut chosen: BTreeSet<&str> = entity.key_fields().iter().copied().collect();
            match requested {
                Some(names) => {
                    for name in names {
                        let known = entity.fields().iter().find(|field| **field == name);
                        let Some(field) = known else {
                            return Err(Error::InvalidInput(format!(
                                "unknown field `{name}` for entity type `{entity}`"
                            )));
                        };
                        chosen.insert(field);
                    }
                }
                None => chosen.extend(entity.fields().iter().copied()),
            }
            let ordered = entity
                .fields()
                .iter()
                .filter(|field| chosen.contains(*field))
                .map(|field| field.to_string())
                .collect();
            fields.insert(*entity, ordered);
        }
        if let Some(map) = &self.fields {
            if let Some(extra) = map.keys().find(|entity| !entity_types.contains(entity)) {
                return Err(Error::InvalidInput(format!(
                    "fields given for unselected entity type `{extra}`"
                )));
            }
        }
        Ok(MemoryExportSelection {
            entity_types: entity_types.into_iter().collect(),
            fields,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportOp {
    Upsert,
    Delete,
}

/// One export line. `record` is present for upserts and absent for deletes.
#[derive(Clone, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct ExportRecord {
    pub entity: ExportEntityType,
    /// JSON array of key field values, in `key_fields` order.
    #[schema(value_type = Vec<String>)]
    pub key: Value,
    pub op: ExportOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub record: Option<Value>,
}

impl fmt::Debug for ExportRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExportRecord")
            .field("entity", &self.entity)
            .field("op", &self.op)
            .field("record_present", &self.record.is_some())
            .finish()
    }
}

impl ExportRecord {
    /// Canonical JSON line (sorted keys, no insignificant whitespace).
    pub fn canonical_line(&self) -> String {
        let value = serde_json::to_value(self).expect("export record serializes");
        canonical_json(&value)
    }

    fn sort_key(&self) -> (ExportEntityType, String) {
        (self.entity, canonical_json(&self.key))
    }
}

/// Sort records into the contract's canonical order: entity type, then the
/// canonical key text compared bytewise. At most one line exists per key.
pub fn sort_export_records(records: &mut [ExportRecord]) {
    records.sort_by_cached_key(ExportRecord::sort_key);
}

/// Serialize JSON with lexicographically sorted object keys.
pub fn canonical_json(value: &Value) -> String {
    fn sort(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let sorted: BTreeMap<&String, Value> =
                    map.iter().map(|(key, value)| (key, sort(value))).collect();
                let mut out = Map::new();
                for (key, value) in sorted {
                    out.insert(key.clone(), value);
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(sort).collect()),
            other => other.clone(),
        }
    }
    // serde_json's default Map is ordered by key, and `sort` makes the order
    // explicit even if a dependency enables `preserve_order`.
    serde_json::to_string(&sort(value)).expect("JSON value serializes")
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct EntityExportStats {
    pub upserts: u64,
    pub deletes: u64,
    /// `sha256:` over this type's canonical lines, each followed by `\n`.
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct MemoryExportWindow {
    /// Inclusive lower bound (`since`); absent for full exports.
    #[serde(default)]
    pub from: Option<HighWaterMark>,
    /// Exclusive upper bound; equals `high_water_mark`.
    pub to: HighWaterMark,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct MemoryExportMemory {
    /// Archive name, or `null` for the unnamed default memory.
    #[serde(default)]
    pub name: Option<String>,
    pub schema: String,
    pub tenant_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct MemoryExportProducer {
    pub name: String,
    pub version: String,
    pub git_sha: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct MemoryExportManifest {
    pub contract: String,
    pub export_version: String,
    /// Latest database migration version the producer was built with.
    pub schema_version: String,
    pub memory: MemoryExportMemory,
    pub mode: MemoryExportMode,
    #[serde(default)]
    pub since: Option<HighWaterMark>,
    pub high_water_mark: HighWaterMark,
    pub window: MemoryExportWindow,
    pub selection: MemoryExportSelection,
    #[schema(value_type = Object)]
    pub entities: BTreeMap<ExportEntityType, EntityExportStats>,
    pub record_count: u64,
    /// `sha256:` over every canonical line in order, each followed by `\n`.
    pub content_sha256: String,
    pub producer: MemoryExportProducer,
    /// Wall-clock capture time; the only field excluded from determinism.
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct MemoryExport {
    pub manifest: MemoryExportManifest,
    pub records: Vec<ExportRecord>,
}

impl fmt::Debug for MemoryExport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryExport")
            .field("mode", &self.manifest.mode)
            .field("high_water_mark", &self.manifest.high_water_mark)
            .field("record_count", &self.records.len())
            .finish()
    }
}

fn sha256_prefixed(hasher: Sha256) -> String {
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Per-type statistics, total count, and content hash for sorted records.
pub fn summarize_export_records(
    selection: &MemoryExportSelection,
    records: &[ExportRecord],
) -> (BTreeMap<ExportEntityType, EntityExportStats>, String) {
    let mut total = Sha256::new();
    let mut per_type: BTreeMap<ExportEntityType, (EntityExportStats, Sha256)> = selection
        .entity_types
        .iter()
        .map(|entity| (*entity, (EntityExportStats::default(), Sha256::new())))
        .collect();
    for record in records {
        let line = record.canonical_line();
        total.update(line.as_bytes());
        total.update(b"\n");
        if let Some((stats, hasher)) = per_type.get_mut(&record.entity) {
            match record.op {
                ExportOp::Upsert => stats.upserts += 1,
                ExportOp::Delete => stats.deletes += 1,
            }
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
    }
    let entities = per_type
        .into_iter()
        .map(|(entity, (mut stats, hasher))| {
            stats.sha256 = sha256_prefixed(hasher);
            (entity, stats)
        })
        .collect();
    (entities, sha256_prefixed(total))
}

/// Materialized state keyed by `(entity, canonical key)`.
pub type ExportState = BTreeMap<(ExportEntityType, String), Value>;

/// Apply export lines to a materialized state.
///
/// Contract semantics: `upsert` replaces the whole record for its key;
/// `delete` removes the key and is a no-op when the key is absent. A full
/// export applied to an empty state reproduces that export; applying
/// incremental(since = previous high_water_mark) to the previous state yields
/// the state of a full export at the incremental's high_water_mark.
pub fn apply_export_records(state: &mut ExportState, records: &[ExportRecord]) {
    for record in records {
        let key = (record.entity, canonical_json(&record.key));
        match (record.op, &record.record) {
            (ExportOp::Upsert, Some(value)) => {
                state.insert(key, value.clone());
            }
            (ExportOp::Upsert, None) | (ExportOp::Delete, _) => {
                state.remove(&key);
            }
        }
    }
}

/// Render a materialized state back into canonical full-export records.
pub fn export_state_records(state: &ExportState) -> Vec<ExportRecord> {
    state
        .iter()
        .map(|((entity, _), value)| ExportRecord {
            entity: *entity,
            key: Value::Array(
                entity
                    .key_fields()
                    .iter()
                    .map(|field| value.get(*field).cloned().unwrap_or(Value::Null))
                    .collect(),
            ),
            op: ExportOp::Upsert,
            record: Some(value.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn upsert(entity: ExportEntityType, id: &str, title: &str) -> ExportRecord {
        ExportRecord {
            entity,
            key: json!([id]),
            op: ExportOp::Upsert,
            record: Some(json!({ "id": id, "title": title })),
        }
    }

    #[test]
    fn canonical_json_sorts_nested_keys() {
        let value = json!({"b": {"z": 1, "a": [ {"y": 2, "x": 1} ]}, "a": null});
        assert_eq!(
            canonical_json(&value),
            r#"{"a":null,"b":{"a":[{"x":1,"y":2}],"z":1}}"#
        );
    }

    #[test]
    fn high_water_mark_round_trips_as_decimal_string() {
        let mark = HighWaterMark::parse("18446744073709551615").unwrap();
        assert_eq!(
            serde_json::to_string(&mark).unwrap(),
            "\"18446744073709551615\""
        );
        assert_eq!(
            serde_json::from_str::<HighWaterMark>("\"42\"").unwrap(),
            HighWaterMark(42)
        );
        for bad in ["", "-1", "1e3", "18446744073709551616", "0x10"] {
            assert!(HighWaterMark::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn selection_requires_since_only_for_incremental_and_keeps_key_fields() {
        let request = MemoryExportRequest {
            mode: MemoryExportMode::Incremental,
            ..Default::default()
        };
        assert!(request.selection().is_err());
        let request = MemoryExportRequest {
            since: Some(HighWaterMark(3)),
            ..Default::default()
        };
        assert!(request.selection().is_err());

        let request: MemoryExportRequest = serde_json::from_value(json!({
            "entity_types": ["note_tag", "note"],
            "fields": {"note": ["title"]}
        }))
        .unwrap();
        let selection = request.selection().unwrap();
        assert_eq!(
            selection.entity_types,
            vec![ExportEntityType::Note, ExportEntityType::NoteTag]
        );
        assert_eq!(
            selection.fields[&ExportEntityType::Note],
            vec!["id", "title"]
        );
        assert_eq!(
            selection.fields[&ExportEntityType::NoteTag],
            vec!["note_id", "tag_name", "source"]
        );

        let unknown: MemoryExportRequest =
            serde_json::from_value(json!({"fields": {"note": ["tenant_id"]}})).unwrap();
        assert!(unknown.selection().is_err());
        let unselected: MemoryExportRequest =
            serde_json::from_value(json!({"entity_types": ["note"], "fields": {"link": ["kind"]}}))
                .unwrap();
        assert!(unselected.selection().is_err());
        assert!(serde_json::from_value::<MemoryExportRequest>(json!({"limit": 3})).is_err());
    }

    #[test]
    fn records_sort_by_entity_then_canonical_key() {
        let mut records = vec![
            upsert(ExportEntityType::Note, "b", "2"),
            upsert(ExportEntityType::Collection, "z", "c"),
            upsert(ExportEntityType::Note, "a", "1"),
        ];
        sort_export_records(&mut records);
        let order: Vec<_> = records
            .iter()
            .map(|record| (record.entity, record.key[0].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(
            order,
            vec![
                (ExportEntityType::Collection, "z".to_string()),
                (ExportEntityType::Note, "a".to_string()),
                (ExportEntityType::Note, "b".to_string()),
            ]
        );
    }

    #[test]
    fn apply_semantics_replace_delete_and_ignore_absent_deletes() {
        let mut state = ExportState::new();
        apply_export_records(
            &mut state,
            &[
                upsert(ExportEntityType::Note, "a", "1"),
                upsert(ExportEntityType::Note, "b", "1"),
            ],
        );
        apply_export_records(
            &mut state,
            &[
                upsert(ExportEntityType::Note, "a", "2"),
                ExportRecord {
                    entity: ExportEntityType::Note,
                    key: json!(["b"]),
                    op: ExportOp::Delete,
                    record: None,
                },
                ExportRecord {
                    entity: ExportEntityType::Note,
                    key: json!(["never-existed"]),
                    op: ExportOp::Delete,
                    record: None,
                },
            ],
        );
        let records = export_state_records(&state);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record.as_ref().unwrap()["title"], "2");
    }

    #[test]
    fn summaries_hash_canonical_lines_per_type_and_in_total() {
        let selection = MemoryExportRequest::default().selection().unwrap();
        let records = vec![upsert(ExportEntityType::Note, "a", "1")];
        let (entities, total) = summarize_export_records(&selection, &records);
        let line = r#"{"entity":"note","key":["a"],"op":"upsert","record":{"id":"a","title":"1"}}"#;
        let expected = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(format!("{line}\n").as_bytes()))
        );
        assert_eq!(entities[&ExportEntityType::Note].sha256, expected);
        assert_eq!(entities[&ExportEntityType::Note].upserts, 1);
        assert_eq!(total, expected);
        assert_eq!(entities.len(), ExportEntityType::ALL.len());
        assert_eq!(
            entities[&ExportEntityType::Link].sha256,
            format!("sha256:{}", hex::encode(Sha256::digest(b"")))
        );
    }
}

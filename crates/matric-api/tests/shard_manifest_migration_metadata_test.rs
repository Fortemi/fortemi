//! Integration tests for ShardManifest migration metadata (Issue #413).
//!
//! Tests verify that ShardManifest correctly handles migration metadata fields:
//! - min_reader_version
//! - migrated_from
//! - migration_history
//!
//! These fields support shard versioning and migration tracking for knowledge
//! shard export/import operations.

use serde::{Deserialize, Serialize};
use serde_json::json;

// Test structs mirror the implementation in main.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationHistoryEntry {
    from_version: String,
    to_version: String,
    migrated_at: chrono::DateTime<chrono::Utc>,
    migrated_by: String,
    changes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShardManifest {
    version: String,
    #[serde(default)]
    matric_version: Option<String>,
    format: String,
    created_at: chrono::DateTime<chrono::Utc>,
    components: Vec<String>,
    counts: ShardCounts,
    checksums: std::collections::HashMap<String, String>,

    // New migration metadata fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min_reader_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    migrated_from: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    migration_history: Vec<MigrationHistoryEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ShardCounts {
    notes: usize,
    collections: usize,
    tags: usize,
    templates: usize,
    links: usize,
    embedding_sets: usize,
    embedding_set_members: usize,
    embeddings: usize,
    embedding_configs: usize,
}

#[test]
fn test_new_manifest_includes_min_reader_version() {
    let manifest = ShardManifest {
        version: "1.0.0".to_string(),
        matric_version: Some("2026.1.12".to_string()),
        format: "matric-shard".to_string(),
        created_at: chrono::Utc::now(),
        components: vec!["notes".to_string()],
        counts: ShardCounts::default(),
        checksums: std::collections::HashMap::new(),
        min_reader_version: Some("1.0.0".to_string()),
        migrated_from: None,
        migration_history: vec![],
    };

    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: ShardManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.min_reader_version, Some("1.0.0".to_string()));
}

#[test]
fn test_old_manifest_parses_without_new_fields() {
    // Simulate an old manifest JSON without migration metadata
    let old_manifest_json = json!({
        "version": "1.0.0",
        "matric_version": "2026.1.0",
        "format": "matric-shard",
        "created_at": "2026-01-15T10:00:00Z",
        "components": ["notes", "tags"],
        "counts": {
            "notes": 10,
            "collections": 0,
            "tags": 5,
            "templates": 0,
            "links": 0,
            "embedding_sets": 0,
            "embedding_set_members": 0,
            "embeddings": 0,
            "embedding_configs": 0
        },
        "checksums": {}
    });

    let parsed: Result<ShardManifest, _> = serde_json::from_value(old_manifest_json);
    assert!(parsed.is_ok(), "Old manifest should parse successfully");

    let manifest = parsed.unwrap();
    assert_eq!(manifest.min_reader_version, None);
    assert_eq!(manifest.migrated_from, None);
    assert!(manifest.migration_history.is_empty());
}

#[test]
fn test_manifest_with_migration_history() {
    let history_entry = MigrationHistoryEntry {
        from_version: "1.0.0".to_string(),
        to_version: "1.1.0".to_string(),
        migrated_at: chrono::Utc::now(),
        migrated_by: "matric-memory/2026.1.12".to_string(),
        changes: vec![
            "Added embedding truncation support".to_string(),
            "Updated manifest schema".to_string(),
        ],
    };

    let manifest = ShardManifest {
        version: "1.1.0".to_string(),
        matric_version: Some("2026.1.12".to_string()),
        format: "matric-shard".to_string(),
        created_at: chrono::Utc::now(),
        components: vec!["notes".to_string()],
        counts: ShardCounts::default(),
        checksums: std::collections::HashMap::new(),
        min_reader_version: Some("1.1.0".to_string()),
        migrated_from: Some("1.0.0".to_string()),
        migration_history: vec![history_entry],
    };

    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: ShardManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.migrated_from, Some("1.0.0".to_string()));
    assert_eq!(parsed.migration_history.len(), 1);
    assert_eq!(parsed.migration_history[0].from_version, "1.0.0");
    assert_eq!(parsed.migration_history[0].to_version, "1.1.0");
    assert_eq!(parsed.migration_history[0].changes.len(), 2);
}

#[test]
fn test_migration_history_entry_serialization() {
    let entry = MigrationHistoryEntry {
        from_version: "1.0.0".to_string(),
        to_version: "1.2.0".to_string(),
        migrated_at: chrono::DateTime::parse_from_rfc3339("2026-02-01T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        migrated_by: "matric-memory/2026.2.0".to_string(),
        changes: vec!["Schema upgrade".to_string()],
    };

    let json = serde_json::to_string(&entry).unwrap();
    let parsed: MigrationHistoryEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.from_version, "1.0.0");
    assert_eq!(parsed.to_version, "1.2.0");
    assert_eq!(parsed.migrated_by, "matric-memory/2026.2.0");
    assert_eq!(parsed.changes.len(), 1);
}

#[test]
fn test_skip_serializing_empty_migration_history() {
    let manifest = ShardManifest {
        version: "1.0.0".to_string(),
        matric_version: Some("2026.1.12".to_string()),
        format: "matric-shard".to_string(),
        created_at: chrono::Utc::now(),
        components: vec!["notes".to_string()],
        counts: ShardCounts::default(),
        checksums: std::collections::HashMap::new(),
        min_reader_version: Some("1.0.0".to_string()),
        migrated_from: None,
        migration_history: vec![],
    };

    let json_value: serde_json::Value = serde_json::to_value(&manifest).unwrap();

    // Empty migration_history should not appear in serialized JSON
    assert!(json_value.get("migration_history").is_none());
}

#[test]
fn test_skip_serializing_none_min_reader_version() {
    let manifest = ShardManifest {
        version: "1.0.0".to_string(),
        matric_version: Some("2026.1.12".to_string()),
        format: "matric-shard".to_string(),
        created_at: chrono::Utc::now(),
        components: vec!["notes".to_string()],
        counts: ShardCounts::default(),
        checksums: std::collections::HashMap::new(),
        min_reader_version: None,
        migrated_from: None,
        migration_history: vec![],
    };

    let json_value: serde_json::Value = serde_json::to_value(&manifest).unwrap();

    // None fields should not appear in serialized JSON
    assert!(json_value.get("min_reader_version").is_none());
    assert!(json_value.get("migrated_from").is_none());
}

#[test]
fn test_multiple_migration_history_entries() {
    let entry1 = MigrationHistoryEntry {
        from_version: "1.0.0".to_string(),
        to_version: "1.1.0".to_string(),
        migrated_at: chrono::Utc::now(),
        migrated_by: "matric-memory/2026.1.10".to_string(),
        changes: vec!["First migration".to_string()],
    };

    let entry2 = MigrationHistoryEntry {
        from_version: "1.1.0".to_string(),
        to_version: "1.2.0".to_string(),
        migrated_at: chrono::Utc::now(),
        migrated_by: "matric-memory/2026.1.15".to_string(),
        changes: vec!["Second migration".to_string()],
    };

    let manifest = ShardManifest {
        version: "1.2.0".to_string(),
        matric_version: Some("2026.1.15".to_string()),
        format: "matric-shard".to_string(),
        created_at: chrono::Utc::now(),
        components: vec!["notes".to_string()],
        counts: ShardCounts::default(),
        checksums: std::collections::HashMap::new(),
        min_reader_version: Some("1.2.0".to_string()),
        migrated_from: Some("1.0.0".to_string()),
        migration_history: vec![entry1, entry2],
    };

    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: ShardManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.migration_history.len(), 2);
    assert_eq!(parsed.migration_history[0].to_version, "1.1.0");
    assert_eq!(parsed.migration_history[1].to_version, "1.2.0");
}

#[test]
fn test_backward_compatibility_roundtrip() {
    // Create a manifest with all fields
    let full_manifest = ShardManifest {
        version: "1.0.0".to_string(),
        matric_version: Some("2026.1.12".to_string()),
        format: "matric-shard".to_string(),
        created_at: chrono::Utc::now(),
        components: vec!["notes".to_string(), "tags".to_string()],
        counts: ShardCounts {
            notes: 42,
            tags: 10,
            ..Default::default()
        },
        checksums: std::collections::HashMap::new(),
        min_reader_version: Some("1.0.0".to_string()),
        migrated_from: None,
        migration_history: vec![],
    };

    // Serialize to JSON
    let json = serde_json::to_string(&full_manifest).unwrap();

    // Parse back
    let parsed: ShardManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.version, full_manifest.version);
    assert_eq!(parsed.matric_version, full_manifest.matric_version);
    assert_eq!(parsed.min_reader_version, full_manifest.min_reader_version);
    assert_eq!(parsed.counts.notes, 42);
    assert_eq!(parsed.counts.tags, 10);
}

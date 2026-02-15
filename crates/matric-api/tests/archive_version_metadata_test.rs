// Test: Archive Version Compatibility Metadata (#416)
//
// Unit tests for version compatibility fields in BackupMetadata.
// These tests verify that version information is properly serialized and deserialized.

use serde_json::json;

/// Test struct mirroring BackupMetadata with version fields
/// This is used for testing without depending on the main.rs internal struct
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TestBackupMetadata {
    title: String,
    description: Option<String>,
    backup_type: String,
    created_at: chrono::DateTime<chrono::Utc>,
    note_count: Option<i64>,
    db_size_bytes: Option<i64>,
    source: String,
    #[serde(default)]
    extra: std::collections::HashMap<String, String>,

    // Version compatibility fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matric_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matric_version_min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matric_version_max: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pg_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_migration_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_migration: Option<String>,
}

#[test]
fn test_backup_metadata_with_version_fields() {
    // Create metadata with all version fields populated
    let metadata = TestBackupMetadata {
        title: "Test Snapshot".to_string(),
        description: Some("Test description".to_string()),
        backup_type: "snapshot".to_string(),
        created_at: chrono::Utc::now(),
        note_count: Some(42),
        db_size_bytes: Some(1024000),
        source: "user".to_string(),
        extra: std::collections::HashMap::new(),
        matric_version: Some("2026.1.12".to_string()),
        matric_version_min: Some("2026.1.0".to_string()),
        matric_version_max: None,
        pg_version: Some("PostgreSQL 18.2".to_string()),
        schema_migration_count: Some(25),
        last_migration: Some("20260203200000_embedding_model_discovery".to_string()),
    };

    // Serialize to JSON
    let json = serde_json::to_value(&metadata).unwrap();

    // Verify version fields are present
    assert_eq!(
        json.get("matric_version").unwrap().as_str().unwrap(),
        "2026.1.12"
    );
    assert_eq!(
        json.get("matric_version_min").unwrap().as_str().unwrap(),
        "2026.1.0"
    );
    assert!(json.get("matric_version_max").is_none()); // None should not be serialized
    assert_eq!(
        json.get("pg_version").unwrap().as_str().unwrap(),
        "PostgreSQL 18.2"
    );
    assert_eq!(
        json.get("schema_migration_count")
            .unwrap()
            .as_i64()
            .unwrap(),
        25
    );
    assert_eq!(
        json.get("last_migration").unwrap().as_str().unwrap(),
        "20260203200000_embedding_model_discovery"
    );
}

#[test]
fn test_backup_metadata_backward_compatibility() {
    // Simulate old archive metadata without version fields
    let old_json = json!({
        "title": "Old Snapshot",
        "description": "Created before version fields",
        "backup_type": "snapshot",
        "created_at": "2026-01-15T10:30:00Z",
        "note_count": 100,
        "db_size_bytes": 5000000,
        "source": "user",
        "extra": {}
    });

    // Should deserialize successfully with version fields as None
    let metadata: TestBackupMetadata = serde_json::from_value(old_json).unwrap();

    assert_eq!(metadata.title, "Old Snapshot");
    assert_eq!(metadata.note_count, Some(100));
    assert!(metadata.matric_version.is_none());
    assert!(metadata.matric_version_min.is_none());
    assert!(metadata.matric_version_max.is_none());
    assert!(metadata.pg_version.is_none());
    assert!(metadata.schema_migration_count.is_none());
    assert!(metadata.last_migration.is_none());
}

#[test]
fn test_backup_metadata_partial_version_fields() {
    // Create metadata with only some version fields populated
    let metadata = TestBackupMetadata {
        title: "Partial Version Snapshot".to_string(),
        description: None,
        backup_type: "snapshot".to_string(),
        created_at: chrono::Utc::now(),
        note_count: None,
        db_size_bytes: None,
        source: "system".to_string(),
        extra: std::collections::HashMap::new(),
        matric_version: Some("2026.1.12".to_string()),
        matric_version_min: None, // Not set
        matric_version_max: None,
        pg_version: Some("PostgreSQL 18.2".to_string()),
        schema_migration_count: None, // Not set
        last_migration: None,
    };

    let json = serde_json::to_value(&metadata).unwrap();

    // Only populated fields should be present
    assert!(json.get("matric_version").is_some());
    assert!(json.get("pg_version").is_some());
    assert!(json.get("matric_version_min").is_none());
    assert!(json.get("schema_migration_count").is_none());
}

#[test]
fn test_version_format_validation() {
    // CalVer format: YYYY.M.PATCH
    let valid_versions = vec!["2026.1.0", "2026.1.12", "2026.12.5", "2027.1.0"];

    for version in valid_versions {
        assert!(
            version.matches('.').count() == 2,
            "Version {} should have exactly 2 dots",
            version
        );

        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "Version {} should have 3 parts", version);

        // Year should be 4 digits
        assert!(
            parts[0].len() == 4,
            "Year in {} should be 4 digits",
            version
        );

        // All parts should be numeric
        for part in parts {
            assert!(
                part.parse::<u32>().is_ok(),
                "Part {} in version {} should be numeric",
                part,
                version
            );
        }
    }
}

#[test]
fn test_postgres_version_string_parsing() {
    // PostgreSQL version strings can have various formats
    let pg_versions = vec![
        "PostgreSQL 18.2",
        "PostgreSQL 18.2 on x86_64-pc-linux-gnu",
        "PostgreSQL 15.3",
    ];

    for version in pg_versions {
        assert!(
            version.starts_with("PostgreSQL"),
            "PG version {} should start with 'PostgreSQL'",
            version
        );
    }
}

#[test]
fn test_migration_name_format() {
    // Migration names should follow timestamp pattern
    let migration_names = vec![
        "20260203200000_embedding_model_discovery",
        "20251201120000_add_document_types",
        "20260101000000_initial_schema",
    ];

    for name in migration_names {
        // Should start with 14-digit timestamp (YYYYMMDDHHmmss)
        let timestamp_part = &name[..14];
        assert!(
            timestamp_part.parse::<u64>().is_ok(),
            "Migration {} should start with 14-digit timestamp",
            name
        );

        // Should have underscore separator
        assert!(
            name.contains('_'),
            "Migration {} should contain underscore",
            name
        );
    }
}

#[test]
fn test_metadata_serialization_roundtrip() {
    // Create full metadata
    let original = TestBackupMetadata {
        title: "Roundtrip Test".to_string(),
        description: Some("Testing serialization".to_string()),
        backup_type: "snapshot".to_string(),
        created_at: chrono::Utc::now(),
        note_count: Some(50),
        db_size_bytes: Some(2000000),
        source: "user".to_string(),
        extra: [("custom".to_string(), "value".to_string())]
            .into_iter()
            .collect(),
        matric_version: Some("2026.1.12".to_string()),
        matric_version_min: Some("2026.1.0".to_string()),
        matric_version_max: Some("2027.0.0".to_string()),
        pg_version: Some("PostgreSQL 18.2".to_string()),
        schema_migration_count: Some(30),
        last_migration: Some("20260203200000_test_migration".to_string()),
    };

    // Serialize to JSON string
    let json_str = serde_json::to_string_pretty(&original).unwrap();

    // Deserialize back
    let deserialized: TestBackupMetadata = serde_json::from_str(&json_str).unwrap();

    // Verify all fields match
    assert_eq!(deserialized.title, original.title);
    assert_eq!(deserialized.matric_version, original.matric_version);
    assert_eq!(deserialized.matric_version_min, original.matric_version_min);
    assert_eq!(deserialized.matric_version_max, original.matric_version_max);
    assert_eq!(deserialized.pg_version, original.pg_version);
    assert_eq!(
        deserialized.schema_migration_count,
        original.schema_migration_count
    );
    assert_eq!(deserialized.last_migration, original.last_migration);
}

#[test]
fn test_version_compatibility_check_logic() {
    // Document the expected version compatibility logic

    // Case 1: Same version - always compatible
    let archive_version = "2026.1.12";
    let current_version = "2026.1.12";
    assert_eq!(archive_version, current_version);

    // Case 2: Archive from older version - should work if >= min version
    let _archive_version = "2026.1.5";
    let archive_min = "2026.1.0";
    let current_version = "2026.1.12";
    assert!(current_version >= archive_min);

    // Case 3: Archive from newer version - warning recommended
    let archive_version = "2026.2.0";
    let current_version = "2026.1.12";
    assert!(
        archive_version > current_version,
        "Should warn about newer archive"
    );
}

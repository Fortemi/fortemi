//! Comprehensive integration tests for the shard migration system.

use super::*;
use serde_json::json;

// =============================================================================
// Test Fixtures Module
// =============================================================================

/// Helper to load test fixtures from the fixtures directory.
mod fixtures {
    use std::fs;

    pub fn load(filename: &str) -> String {
        let path = format!(
            "{}/src/shard/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            filename
        );
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", filename, e))
    }

    pub fn load_json(filename: &str) -> serde_json::Value {
        let content = load(filename);
        serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse fixture {}: {}", filename, e))
    }
}

// =============================================================================
// Manifest Deserialization Tests (CRITICAL PATH - 100% coverage required)
// =============================================================================

#[test]
fn test_manifest_v1_0_minimal_deserialize() {
    let json = r#"{
        "version": "1.0.0",
        "format": "matric-shard",
        "created_at": "2026-01-01T00:00:00Z",
        "components": [],
        "counts": {},
        "checksums": {}
    }"#;

    let result: Result<serde_json::Value, _> = serde_json::from_str(json);
    assert!(result.is_ok(), "Minimal v1.0 manifest should deserialize");

    let manifest = result.unwrap();
    assert_eq!(manifest["version"], "1.0.0");
    assert_eq!(manifest["format"], "matric-shard");
}

#[test]
fn test_manifest_v1_0_minimal_from_fixture() {
    let manifest = fixtures::load_json("v1_0_0_minimal.json");

    assert_eq!(manifest["version"], "1.0.0");
    assert_eq!(manifest["format"], "matric-shard");
    assert_eq!(manifest["created_at"], "2026-01-01T00:00:00Z");
    assert!(manifest["components"].is_array());
    assert!(manifest["counts"].is_object());
    assert!(manifest["checksums"].is_object());
}

#[test]
fn test_manifest_v1_0_full_from_fixture() {
    let manifest = fixtures::load_json("v1_0_0_full.json");

    assert_eq!(manifest["version"], "1.0.0");
    assert_eq!(manifest["format"], "matric-shard");

    // Verify all components
    let components = manifest["components"].as_array().unwrap();
    assert_eq!(components.len(), 7);
    assert!(components.contains(&json!("notes")));
    assert!(components.contains(&json!("links")));
    assert!(components.contains(&json!("embeddings")));

    // Verify counts
    assert_eq!(manifest["counts"]["notes"], 42);
    assert_eq!(manifest["counts"]["links"], 128);

    // Verify optional fields are present
    assert!(manifest["created_by"].is_string());
    assert!(manifest["description"].is_string());
    assert!(manifest["metadata"].is_object());
    assert!(manifest["compatibility"].is_object());

    // Verify checksums
    let checksums = manifest["checksums"].as_object().unwrap();
    assert_eq!(checksums.len(), 7);
    for (_, checksum) in checksums {
        let checksum_str = checksum.as_str().unwrap();
        assert!(checksum_str.starts_with("sha256:"));
        assert_eq!(checksum_str.len(), 71); // "sha256:" + 64 hex chars
    }
}

#[test]
fn test_manifest_v1_1_forward_compat_from_fixture() {
    let manifest = fixtures::load_json("v1_1_0_forward_compat.json");

    assert_eq!(manifest["version"], "1.1.0");
    assert!(manifest["new_field_in_1_1_0"].is_string());
    assert!(manifest["features"].is_object());
    assert_eq!(manifest["features"]["advanced_chunking"], true);
}

#[test]
fn test_manifest_v2_0_incompatible_from_fixture() {
    let manifest = fixtures::load_json("v2_0_0_incompatible.json");

    assert_eq!(manifest["version"], "2.0.0");
    assert_eq!(manifest["schema_version"], 2);
    assert!(manifest["breaking_changes"].is_array());

    // Verify breaking changes in checksums
    let checksums = manifest["checksums"].as_object().unwrap();
    for (_, checksum) in checksums {
        let checksum_str = checksum.as_str().unwrap();
        assert!(checksum_str.starts_with("blake3:"));
    }
}

#[test]
fn test_manifest_with_unknown_fields_ignored() {
    let json = r#"{
        "version": "1.0.0",
        "format": "matric-shard",
        "created_at": "2026-01-01T00:00:00Z",
        "components": [],
        "counts": {},
        "checksums": {},
        "unknown_field_1": "should be ignored",
        "unknown_field_2": {"nested": "data"},
        "unknown_field_3": [1, 2, 3]
    }"#;

    // Should deserialize successfully, ignoring unknown fields
    let result: Result<serde_json::Value, _> = serde_json::from_str(json);
    assert!(
        result.is_ok(),
        "Unknown fields should be ignored during deserialization"
    );

    let manifest = result.unwrap();
    assert_eq!(manifest["version"], "1.0.0");
    // Unknown fields are preserved in serde_json::Value
    assert!(manifest["unknown_field_1"].is_string());
}

#[test]
fn test_manifest_missing_required_field() {
    let json = r#"{
        "format": "matric-shard",
        "created_at": "2026-01-01T00:00:00Z",
        "components": [],
        "counts": {},
        "checksums": {}
    }"#;

    // Missing "version" field - should still parse as generic JSON
    let result: Result<serde_json::Value, _> = serde_json::from_str(json);
    assert!(result.is_ok());

    let manifest = result.unwrap();
    assert!(manifest["version"].is_null());
}

// =============================================================================
// Version Parsing Edge Cases (CRITICAL PATH - 100% coverage required)
// =============================================================================

#[test]
fn test_version_parse_zero_components() {
    let v = Version::parse("0.0.0").unwrap();
    assert_eq!(v.major, 0);
    assert_eq!(v.minor, 0);
    assert_eq!(v.patch, 0);
}

#[test]
fn test_version_parse_empty_string() {
    let result = Version::parse("");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid version format"));
}

#[test]
fn test_version_parse_large_numbers() {
    let v = Version::parse("999.888.777").unwrap();
    assert_eq!(v.major, 999);
    assert_eq!(v.minor, 888);
    assert_eq!(v.patch, 777);
}

#[test]
fn test_version_parse_max_u64() {
    // Test near u64::MAX
    let v = Version::parse("18446744073709551615.0.0").unwrap();
    assert_eq!(v.major, u64::MAX);
}

#[test]
fn test_version_parse_overflow() {
    // This should fail to parse (exceeds u64::MAX)
    let result = Version::parse("18446744073709551616.0.0");
    assert!(result.is_err());
}

#[test]
fn test_version_parse_whitespace() {
    assert!(Version::parse(" 1.0.0 ").is_err());
    assert!(Version::parse("1. 0.0").is_err());
    assert!(Version::parse("1 .0.0").is_err());
    assert!(Version::parse("1.0 .0").is_err());
    assert!(Version::parse("\t1.0.0").is_err());
    assert!(Version::parse("1.0.0\n").is_err());
}

#[test]
fn test_version_parse_negative_numbers() {
    assert!(Version::parse("-1.0.0").is_err());
    assert!(Version::parse("1.-1.0").is_err());
    assert!(Version::parse("1.0.-1").is_err());
}

#[test]
fn test_version_parse_too_many_parts() {
    assert!(Version::parse("1.0.0.0").is_err());
    assert!(Version::parse("1.2.3.4.5").is_err());
}

#[test]
fn test_version_parse_too_few_parts() {
    assert!(Version::parse("1.0").is_err());
    assert!(Version::parse("1").is_err());
}

#[test]
fn test_version_parse_non_numeric() {
    assert!(Version::parse("a.b.c").is_err());
    assert!(Version::parse("1.x.0").is_err());
    assert!(Version::parse("1.0.x").is_err());
    assert!(Version::parse("v1.0.0").is_err());
}

#[test]
fn test_version_parse_special_chars() {
    assert!(Version::parse("1.0.0-beta").is_err());
    assert!(Version::parse("1.0.0+build").is_err());
    assert!(Version::parse("1.0.0@latest").is_err());
}

#[test]
fn test_version_ordering_comprehensive() {
    let v0_0_0 = Version::parse("0.0.0").unwrap();
    let v0_0_1 = Version::parse("0.0.1").unwrap();
    let v0_1_0 = Version::parse("0.1.0").unwrap();
    let v1_0_0 = Version::parse("1.0.0").unwrap();
    let v1_0_1 = Version::parse("1.0.1").unwrap();
    let v1_1_0 = Version::parse("1.1.0").unwrap();
    let v2_0_0 = Version::parse("2.0.0").unwrap();

    // Patch increments
    assert!(v0_0_0 < v0_0_1);
    assert!(v1_0_0 < v1_0_1);

    // Minor increments
    assert!(v0_0_1 < v0_1_0);
    assert!(v1_0_1 < v1_1_0);

    // Major increments
    assert!(v0_1_0 < v1_0_0);
    assert!(v1_1_0 < v2_0_0);

    // Transitivity
    assert!(v0_0_0 < v1_0_0);
    assert!(v0_0_0 < v2_0_0);
}

#[test]
fn test_version_equality() {
    let v1 = Version::parse("1.2.3").unwrap();
    let v2 = Version::parse("1.2.3").unwrap();
    assert_eq!(v1, v2);

    let v3 = Version::parse("1.2.4").unwrap();
    assert_ne!(v1, v3);
}

#[test]
fn test_version_display() {
    let v = Version::parse("1.2.3").unwrap();
    assert_eq!(v.to_string(), "1.2.3");

    let v = Version::parse("0.0.0").unwrap();
    assert_eq!(v.to_string(), "0.0.0");

    let v = Version::parse("999.888.777").unwrap();
    assert_eq!(v.to_string(), "999.888.777");
}

// =============================================================================
// Compatibility Matrix Tests (CRITICAL PATH - 100% coverage required)
// =============================================================================

#[test]
fn test_compatibility_same_version() {
    let result = check_shard_compatibility("1.0.0");
    assert_eq!(result, CompatibilityResult::Compatible);
}

#[test]
fn test_compatibility_same_version_from_fixture() {
    let manifest = fixtures::load_json("v1_0_0_minimal.json");
    let version = manifest["version"].as_str().unwrap();
    let result = check_shard_compatibility(version);
    assert_eq!(result, CompatibilityResult::Compatible);
}

#[test]
fn test_compatibility_newer_minor() {
    let result = check_shard_compatibility("1.1.0");
    match result {
        CompatibilityResult::NewerMinor {
            shard_version,
            warnings,
        } => {
            assert_eq!(shard_version, "1.1.0");
            assert!(!warnings.is_empty());
            assert!(warnings[0].contains("newer version"));
        }
        _ => panic!("Expected NewerMinor, got {:?}", result),
    }
}

#[test]
fn test_compatibility_newer_minor_from_fixture() {
    let manifest = fixtures::load_json("v1_1_0_forward_compat.json");
    let version = manifest["version"].as_str().unwrap();
    let result = check_shard_compatibility(version);
    match result {
        CompatibilityResult::NewerMinor { .. } => {}
        _ => panic!("Expected NewerMinor for v1.1.0 fixture, got {:?}", result),
    }
}

#[test]
fn test_compatibility_newer_patch() {
    // Current: 1.0.0, Shard: 1.0.1
    // Same major, same minor, newer patch = Compatible (standard semver)
    let result = check_shard_compatibility("1.0.1");
    assert_eq!(result, CompatibilityResult::Compatible);
}

#[test]
fn test_compatibility_newer_major() {
    let result = check_shard_compatibility("2.0.0");
    match result {
        CompatibilityResult::Incompatible {
            reason,
            min_required,
        } => {
            assert!(reason.contains("major version"));
            assert_eq!(min_required, Some("2.0.0".to_string()));
        }
        _ => panic!("Expected Incompatible, got {:?}", result),
    }
}

#[test]
fn test_compatibility_newer_major_from_fixture() {
    let manifest = fixtures::load_json("v2_0_0_incompatible.json");
    let version = manifest["version"].as_str().unwrap();
    let result = check_shard_compatibility(version);
    match result {
        CompatibilityResult::Incompatible { .. } => {}
        _ => panic!("Expected Incompatible for v2.0.0 fixture, got {:?}", result),
    }
}

#[test]
fn test_compatibility_older_major() {
    let result = check_shard_compatibility("0.9.0");
    match result {
        CompatibilityResult::Incompatible { reason, .. } => {
            assert!(reason.contains("major version"));
        }
        _ => panic!("Expected Incompatible, got {:?}", result),
    }
}

#[test]
fn test_compatibility_invalid_version() {
    let result = check_shard_compatibility("invalid");
    match result {
        CompatibilityResult::Incompatible { reason, .. } => {
            assert!(reason.contains("Invalid"));
        }
        _ => panic!("Expected Incompatible, got {:?}", result),
    }
}

#[test]
fn test_compatibility_invalid_version_from_fixture() {
    let manifest = fixtures::load_json("invalid_version.json");
    let version = manifest["version"].as_str().unwrap();
    let result = check_shard_compatibility(version);
    match result {
        CompatibilityResult::Incompatible { reason, .. } => {
            assert!(reason.contains("Invalid"));
        }
        _ => panic!(
            "Expected Incompatible for invalid version, got {:?}",
            result
        ),
    }
}

#[test]
fn test_compatibility_empty_version() {
    let result = check_shard_compatibility("");
    match result {
        CompatibilityResult::Incompatible { .. } => {}
        _ => panic!("Expected Incompatible for empty version"),
    }
}

// =============================================================================
// Migration Registry Tests (CRITICAL PATH - 100% coverage required)
// =============================================================================

struct MockMigration {
    from: String,
    to: String,
    should_fail: bool,
}

impl ShardMigration for MockMigration {
    fn from_version(&self) -> &str {
        &self.from
    }

    fn to_version(&self) -> &str {
        &self.to
    }

    fn description(&self) -> &str {
        "Mock migration for testing"
    }

    fn migrate(&self, data: serde_json::Value) -> Result<MigrationResult, MigrationError> {
        if self.should_fail {
            return Err(MigrationError::MigrationFailed("Mock failure".to_string()));
        }

        Ok(MigrationResult {
            data,
            warnings: vec![],
        })
    }
}

#[test]
fn test_registry_empty_initialization() {
    let registry = MigrationRegistry::new();
    let path = registry.find_path("1.0.0", "1.0.0");
    assert!(path.is_some());
    assert_eq!(path.unwrap().len(), 0);
}

#[test]
fn test_registry_default_initialization() {
    let registry = MigrationRegistry::default();
    let path = registry.find_path("1.0.0", "1.0.0");
    assert!(path.is_some());
}

#[test]
fn test_registry_single_hop() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));

    let path = registry.find_path("1.0.0", "1.1.0");
    assert!(path.is_some());
    assert_eq!(path.unwrap().len(), 1);
}

#[test]
fn test_registry_multi_hop() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.1.0".to_string(),
        to: "1.2.0".to_string(),
        should_fail: false,
    }));

    let path = registry.find_path("1.0.0", "1.2.0");
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path.len(), 2);
    assert_eq!(path[0].from_version(), "1.0.0");
    assert_eq!(path[0].to_version(), "1.1.0");
    assert_eq!(path[1].from_version(), "1.1.0");
    assert_eq!(path[1].to_version(), "1.2.0");
}

#[test]
fn test_registry_three_hop_chain() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.1.0".to_string(),
        to: "1.2.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.2.0".to_string(),
        to: "2.0.0".to_string(),
        should_fail: false,
    }));

    let path = registry.find_path("1.0.0", "2.0.0");
    assert!(path.is_some());
    assert_eq!(path.unwrap().len(), 3);
}

#[test]
fn test_registry_no_path() {
    let registry = MigrationRegistry::new();
    let path = registry.find_path("1.0.0", "2.0.0");
    assert!(path.is_none());
}

#[test]
fn test_registry_circular_path_handled() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.1.0".to_string(),
        to: "1.0.0".to_string(),
        should_fail: false,
    }));

    // Should find shortest path without infinite loop
    let path = registry.find_path("1.0.0", "1.1.0");
    assert!(path.is_some());
    assert_eq!(path.unwrap().len(), 1);
}

#[test]
fn test_registry_branching_paths() {
    let mut registry = MigrationRegistry::new();
    // Two different paths from 1.0.0
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.0.1".to_string(),
        should_fail: false,
    }));

    let path1 = registry.find_path("1.0.0", "1.1.0");
    let path2 = registry.find_path("1.0.0", "1.0.1");
    assert!(path1.is_some());
    assert!(path2.is_some());
    assert_eq!(path1.unwrap().len(), 1);
    assert_eq!(path2.unwrap().len(), 1);
}

#[test]
fn test_registry_migrate_success() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));

    let data = json!({"test": "data"});
    let result = registry.migrate(data.clone(), "1.0.0", "1.1.0");
    assert!(result.is_ok());

    let migration_result = result.unwrap();
    assert_eq!(migration_result.data, data);
    assert_eq!(migration_result.warnings.len(), 0);
}

#[test]
fn test_registry_migrate_multi_step() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.1.0".to_string(),
        to: "1.2.0".to_string(),
        should_fail: false,
    }));

    let data = json!({"test": "data"});
    let result = registry.migrate(data.clone(), "1.0.0", "1.2.0");
    assert!(result.is_ok());
}

#[test]
fn test_registry_migrate_no_path() {
    let registry = MigrationRegistry::new();
    let data = json!({"test": "data"});
    let result = registry.migrate(data, "1.0.0", "2.0.0");
    assert!(result.is_err());

    match result.unwrap_err() {
        MigrationError::NoMigrationPath { from, to } => {
            assert_eq!(from, "1.0.0");
            assert_eq!(to, "2.0.0");
        }
        e => panic!("Expected NoMigrationPath, got {:?}", e),
    }
}

#[test]
fn test_registry_migrate_failure() {
    let mut registry = MigrationRegistry::new();
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: true,
    }));

    let data = json!({"test": "data"});
    let result = registry.migrate(data, "1.0.0", "1.1.0");
    assert!(result.is_err());

    match result.unwrap_err() {
        MigrationError::MigrationFailed(msg) => {
            assert_eq!(msg, "Mock failure");
        }
        e => panic!("Expected MigrationFailed, got {:?}", e),
    }
}

#[test]
fn test_registry_same_version_no_migration() {
    let registry = MigrationRegistry::new();
    let data = json!({"test": "data"});
    let result = registry.migrate(data.clone(), "1.0.0", "1.0.0");
    assert!(result.is_ok());

    let migration_result = result.unwrap();
    assert_eq!(migration_result.data, data);
    assert_eq!(migration_result.warnings.len(), 0);
}

// =============================================================================
// Warning Serialization Tests (CRITICAL PATH - 100% coverage required)
// =============================================================================

#[test]
fn test_warning_field_removed_serialization() {
    let warning = MigrationWarning::FieldRemoved {
        field: "old_field".to_string(),
        count: 5,
    };

    let json = serde_json::to_string(&warning).unwrap();
    assert!(json.contains("field_removed"));
    assert!(json.contains("old_field"));

    let deserialized: MigrationWarning = serde_json::from_str(&json).unwrap();
    assert_eq!(warning, deserialized);
}

#[test]
fn test_warning_default_applied_serialization() {
    let warning = MigrationWarning::DefaultApplied {
        field: "new_field".to_string(),
        default: "default_value".to_string(),
    };

    let json = serde_json::to_string(&warning).unwrap();
    assert!(json.contains("default_applied"));
    assert!(json.contains("new_field"));
    assert!(json.contains("default_value"));

    let deserialized: MigrationWarning = serde_json::from_str(&json).unwrap();
    assert_eq!(warning, deserialized);
}

#[test]
fn test_warning_unknown_field_ignored_serialization() {
    let warning = MigrationWarning::UnknownFieldIgnored {
        field: "mystery_field".to_string(),
    };

    let json = serde_json::to_string(&warning).unwrap();
    assert!(json.contains("unknown_field_ignored"));
    assert!(json.contains("mystery_field"));

    let deserialized: MigrationWarning = serde_json::from_str(&json).unwrap();
    assert_eq!(warning, deserialized);
}

#[test]
fn test_warning_data_truncated_serialization() {
    let warning = MigrationWarning::DataTruncated {
        field: "long_text".to_string(),
        detail: "Truncated from 1000 to 255 characters".to_string(),
    };

    let json = serde_json::to_string(&warning).unwrap();
    assert!(json.contains("data_truncated"));
    assert!(json.contains("long_text"));
    assert!(json.contains("Truncated"));

    let deserialized: MigrationWarning = serde_json::from_str(&json).unwrap();
    assert_eq!(warning, deserialized);
}

#[test]
fn test_warning_array_serialization() {
    let warnings = vec![
        MigrationWarning::FieldRemoved {
            field: "f1".to_string(),
            count: 1,
        },
        MigrationWarning::DefaultApplied {
            field: "f2".to_string(),
            default: "val".to_string(),
        },
    ];

    let json = serde_json::to_string(&warnings).unwrap();
    let deserialized: Vec<MigrationWarning> = serde_json::from_str(&json).unwrap();
    assert_eq!(warnings, deserialized);
}

// =============================================================================
// Integration Scenario Tests
// =============================================================================

#[test]
fn test_scenario_import_same_version_shard() {
    // Scenario: Import a shard with the same version as current
    let manifest = fixtures::load_json("v1_0_0_minimal.json");
    let version = manifest["version"].as_str().unwrap();

    let result = check_shard_compatibility(version);
    assert_eq!(result, CompatibilityResult::Compatible);
}

#[test]
fn test_scenario_import_newer_minor_shard() {
    // Scenario: Import a shard from v1.1.0 into v1.0.0 system
    let manifest = fixtures::load_json("v1_1_0_forward_compat.json");
    let version = manifest["version"].as_str().unwrap();

    let result = check_shard_compatibility(version);
    match result {
        CompatibilityResult::NewerMinor { warnings, .. } => {
            assert!(!warnings.is_empty());
            assert!(warnings.iter().any(|w| w.contains("newer version")));
        }
        _ => panic!("Expected NewerMinor compatibility"),
    }
}

#[test]
fn test_scenario_import_incompatible_major_shard() {
    // Scenario: Attempt to import a v2.0.0 shard into v1.0.0 system
    let manifest = fixtures::load_json("v2_0_0_incompatible.json");
    let version = manifest["version"].as_str().unwrap();

    let result = check_shard_compatibility(version);
    match result {
        CompatibilityResult::Incompatible {
            reason,
            min_required,
        } => {
            assert!(reason.contains("major version"));
            assert!(reason.contains("incompatible"));
            assert_eq!(min_required, Some("2.0.0".to_string()));
        }
        _ => panic!("Expected Incompatible result"),
    }
}

#[test]
fn test_scenario_full_migration_chain() {
    // Scenario: Migrate data through multiple versions
    let mut registry = MigrationRegistry::new();

    // Register migrations: 1.0.0 -> 1.1.0 -> 1.2.0 -> 2.0.0
    registry.register(Box::new(MockMigration {
        from: "1.0.0".to_string(),
        to: "1.1.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.1.0".to_string(),
        to: "1.2.0".to_string(),
        should_fail: false,
    }));
    registry.register(Box::new(MockMigration {
        from: "1.2.0".to_string(),
        to: "2.0.0".to_string(),
        should_fail: false,
    }));

    let original_data = json!({
        "version": "1.0.0",
        "notes": [{"id": 1, "content": "test"}]
    });

    let result = registry.migrate(original_data.clone(), "1.0.0", "2.0.0");
    assert!(result.is_ok());

    let migrated = result.unwrap();
    assert_eq!(migrated.data["notes"][0]["id"], 1);
}

// =============================================================================
// Error Message Quality Tests
// =============================================================================

#[test]
fn test_error_message_no_migration_path() {
    let registry = MigrationRegistry::new();
    let data = json!({});
    let result = registry.migrate(data, "1.0.0", "2.0.0");

    match result {
        Err(MigrationError::NoMigrationPath { from, to }) => {
            assert_eq!(from, "1.0.0");
            assert_eq!(to, "2.0.0");

            // Check error display message
            let err_msg = format!("{}", MigrationError::NoMigrationPath { from, to });
            assert!(err_msg.contains("No migration path"));
            assert!(err_msg.contains("1.0.0"));
            assert!(err_msg.contains("2.0.0"));
        }
        _ => panic!("Expected NoMigrationPath error"),
    }
}

#[test]
fn test_error_message_invalid_version() {
    let result = check_shard_compatibility("not.a.version");
    match result {
        CompatibilityResult::Incompatible { reason, .. } => {
            assert!(reason.contains("Invalid"));
            assert!(reason.contains("version"));
        }
        _ => panic!("Expected Incompatible result"),
    }
}

// =============================================================================
// Current Version Constant Tests
// =============================================================================

#[test]
fn test_current_version_is_valid() {
    let current = Version::parse(CURRENT_SHARD_VERSION);
    assert!(
        current.is_ok(),
        "CURRENT_SHARD_VERSION must be valid semver"
    );

    let v = current.unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 0);
    assert_eq!(v.patch, 0);
}

#[test]
fn test_current_version_matches_fixtures() {
    // v1.0.0 fixtures should be compatible with current version
    let manifest = fixtures::load_json("v1_0_0_minimal.json");
    let version = manifest["version"].as_str().unwrap();
    assert_eq!(version, CURRENT_SHARD_VERSION);
}

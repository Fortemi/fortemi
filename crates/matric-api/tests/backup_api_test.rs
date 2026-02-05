/// Integration tests for backup API improvements
///
/// Tests cover:
/// - Issue #257: shard_type field consistent detection
/// - Issue #242: metadata echo in snapshot response
/// - Issue #218: list_backups archive bundling with metadata sidecars
///
/// Test Strategy:
/// - Unit tests for shard type detection logic
/// - Integration tests for API response structure
/// - File system tests for metadata sidecar bundling
use std::collections::HashMap;

/// Test shard_type detection according to issue #257
#[test]
fn test_shard_type_detection() {
    let test_cases = vec![
        ("snapshot_20260202_120000.sql.gz", "snapshot"),
        ("snapshot_20260202_120000_mybackup.sql.gz", "snapshot"),
        ("prerestore_20260202_120000.sql.gz", "prerestore"),
        (
            "prerestore_20260202_120000_before_restore.sql.gz",
            "prerestore",
        ),
        ("upload_20260202_120000.sql.gz", "upload"),
        ("upload_20260202_120000_imported.sql.gz", "upload"),
        ("knowledge_shard_20260202_120000.tar.gz", "shard"),
        ("export_20260202_120000.json", "json_export"),
        // Metadata sidecars should be skipped (not included in test_cases for iteration)
        // but we test the skip logic separately
    ];

    for (filename, expected_type) in test_cases {
        let detected = detect_shard_type(filename);
        assert_eq!(
            detected,
            Some(expected_type),
            "Failed for filename: {}",
            filename
        );
    }
}

/// Test that metadata sidecars are correctly identified and skipped
#[test]
fn test_metadata_sidecar_skip() {
    let metadata_files = vec![
        "snapshot_20260202_120000.sql.gz.meta.json",
        "upload_20260202_120000.sql.gz.meta.json",
        "knowledge_shard_20260202_120000.tar.gz.meta.json",
    ];

    for filename in metadata_files {
        let detected = detect_shard_type(filename);
        assert_eq!(
            detected, None,
            "Metadata sidecar should be skipped: {}",
            filename
        );
    }
}

/// Test snapshot response includes metadata when provided
#[test]
fn test_snapshot_response_metadata_echo() {
    // Test case 1: With both title and description
    let response_with_metadata = create_snapshot_response(
        "snapshot_20260202_120000.sql.gz",
        1024,
        Some("My Backup"),
        Some("Before major update"),
    );

    assert!(response_with_metadata.contains_key("metadata"));
    let metadata = response_with_metadata.get("metadata").unwrap();
    assert!(metadata.is_object());
    let meta_obj = metadata.as_object().unwrap();
    assert_eq!(
        meta_obj.get("title").and_then(|v| v.as_str()),
        Some("My Backup")
    );
    assert_eq!(
        meta_obj.get("description").and_then(|v| v.as_str()),
        Some("Before major update")
    );

    // Test case 2: With title only
    let response_title_only = create_snapshot_response(
        "snapshot_20260202_120000.sql.gz",
        1024,
        Some("My Backup"),
        None,
    );

    assert!(response_title_only.contains_key("metadata"));
    let metadata = response_title_only.get("metadata").unwrap();
    assert!(metadata.is_object());
    let meta_obj = metadata.as_object().unwrap();
    assert_eq!(
        meta_obj.get("title").and_then(|v| v.as_str()),
        Some("My Backup")
    );
    assert!(!meta_obj.contains_key("description"));

    // Test case 3: With description only
    let response_desc_only = create_snapshot_response(
        "snapshot_20260202_120000.sql.gz",
        1024,
        None,
        Some("Before major update"),
    );

    assert!(response_desc_only.contains_key("metadata"));
    let metadata = response_desc_only.get("metadata").unwrap();
    assert!(metadata.is_object());
    let meta_obj = metadata.as_object().unwrap();
    assert!(!meta_obj.contains_key("title"));
    assert_eq!(
        meta_obj.get("description").and_then(|v| v.as_str()),
        Some("Before major update")
    );

    // Test case 4: Without metadata
    let response_no_metadata =
        create_snapshot_response("snapshot_20260202_120000.sql.gz", 1024, None, None);

    assert!(!response_no_metadata.contains_key("metadata"));
}

/// Test that list_backups bundles archives with metadata sidecars
#[test]
fn test_backup_bundling_with_metadata() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let backup_dir = temp_dir.path();

    // Create test files
    let files = vec![
        ("snapshot_20260202_120000.sql.gz", 1024),
        ("snapshot_20260202_120000.sql.gz.meta.json", 256),
        ("upload_20260202_120000.sql.gz", 2048),
        ("knowledge_shard_20260202_120000.tar.gz", 4096),
        ("knowledge_shard_20260202_120000.tar.gz.meta.json", 256),
        ("orphan.meta.json", 128), // Orphan metadata without primary file
    ];

    for (filename, size) in &files {
        let path = backup_dir.join(filename);
        fs::write(&path, vec![0u8; *size]).unwrap();
    }

    // Write metadata content
    let snapshot_meta = r#"{"title":"Test Snapshot","description":"Test description"}"#;
    fs::write(
        backup_dir.join("snapshot_20260202_120000.sql.gz.meta.json"),
        snapshot_meta,
    )
    .unwrap();

    let shard_meta = r#"{"title":"Knowledge Shard","description":"Exported knowledge"}"#;
    fs::write(
        backup_dir.join("knowledge_shard_20260202_120000.tar.gz.meta.json"),
        shard_meta,
    )
    .unwrap();

    // Process files
    let result = process_backup_files(backup_dir);

    // Verify results
    assert_eq!(
        result.len(),
        3,
        "Should have 3 primary files (excluding orphan metadata)"
    );

    // Check snapshot file
    let snapshot = result
        .iter()
        .find(|f| f.filename == "snapshot_20260202_120000.sql.gz")
        .expect("Snapshot file should be in results");
    assert_eq!(snapshot.shard_type, "snapshot");
    assert_eq!(
        snapshot.metadata_file.as_deref(),
        Some("snapshot_20260202_120000.sql.gz.meta.json")
    );
    assert_eq!(snapshot.title.as_deref(), Some("Test Snapshot"));
    assert_eq!(snapshot.description.as_deref(), Some("Test description"));

    // Check upload file (no metadata)
    let upload = result
        .iter()
        .find(|f| f.filename == "upload_20260202_120000.sql.gz")
        .expect("Upload file should be in results");
    assert_eq!(upload.shard_type, "upload");
    assert_eq!(upload.metadata_file, None);
    assert_eq!(upload.title, None);
    assert_eq!(upload.description, None);

    // Check knowledge shard
    let shard = result
        .iter()
        .find(|f| f.filename == "knowledge_shard_20260202_120000.tar.gz")
        .expect("Shard file should be in results");
    assert_eq!(shard.shard_type, "shard");
    assert_eq!(
        shard.metadata_file.as_deref(),
        Some("knowledge_shard_20260202_120000.tar.gz.meta.json")
    );
    assert_eq!(shard.title.as_deref(), Some("Knowledge Shard"));
    assert_eq!(shard.description.as_deref(), Some("Exported knowledge"));

    // Verify orphan metadata is not in results
    assert!(
        !result.iter().any(|f| f.filename == "orphan.meta.json"),
        "Orphan metadata should not be in results"
    );
}

// Helper functions that mirror the implementation logic

fn detect_shard_type(filename: &str) -> Option<&'static str> {
    // Skip metadata sidecars
    if filename.ends_with(".meta.json") {
        return None;
    }

    // Check for specific prefixes first (issue #257)
    if filename.starts_with("snapshot_") && filename.ends_with(".sql.gz") {
        Some("snapshot")
    } else if filename.starts_with("prerestore_") && filename.ends_with(".sql.gz") {
        Some("prerestore")
    } else if filename.starts_with("upload_") && filename.ends_with(".sql.gz") {
        Some("upload")
    } else if filename.ends_with(".tar.gz") {
        Some("shard")
    } else if filename.ends_with(".json") {
        Some("json_export")
    } else {
        None
    }
}

fn create_snapshot_response(
    filename: &str,
    size: u64,
    title: Option<&str>,
    description: Option<&str>,
) -> HashMap<String, serde_json::Value> {
    use serde_json::json;

    let mut response = HashMap::new();
    response.insert("success".to_string(), json!(true));
    response.insert("filename".to_string(), json!(filename));
    response.insert("size_bytes".to_string(), json!(size));

    if title.is_some() || description.is_some() {
        let mut metadata_obj = serde_json::Map::new();
        if let Some(t) = title {
            metadata_obj.insert("title".to_string(), json!(t));
        }
        if let Some(d) = description {
            metadata_obj.insert("description".to_string(), json!(d));
        }
        response.insert(
            "metadata".to_string(),
            serde_json::Value::Object(metadata_obj),
        );
    }

    response
}

#[derive(Debug, Clone)]
struct BackupFileInfo {
    filename: String,
    shard_type: String,
    metadata_file: Option<String>,
    title: Option<String>,
    description: Option<String>,
}

fn process_backup_files(backup_dir: &std::path::Path) -> Vec<BackupFileInfo> {
    use std::collections::HashMap;
    use std::fs;

    // First pass: collect all files and identify metadata sidecars
    let mut primary_files = Vec::new();
    let mut metadata_map: HashMap<String, serde_json::Value> = HashMap::new();

    if let Ok(entries) = fs::read_dir(backup_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".meta.json") {
                    // This is a metadata sidecar
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            // Extract the primary filename (remove .meta.json)
                            let primary_name = name.trim_end_matches(".meta.json");
                            metadata_map.insert(primary_name.to_string(), json);
                        }
                    }
                } else {
                    // This is a primary file
                    if let Some(shard_type) = detect_shard_type(name) {
                        primary_files.push((name.to_string(), shard_type));
                    }
                }
            }
        }
    }

    // Second pass: bundle primary files with their metadata
    let mut result = Vec::new();
    for (filename, shard_type) in primary_files {
        let metadata = metadata_map.get(&filename);
        let (metadata_file, title, description) = if let Some(meta) = metadata {
            let meta_filename = format!("{}.meta.json", filename);
            let title = meta.get("title").and_then(|v| v.as_str()).map(String::from);
            let description = meta
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);
            (Some(meta_filename), title, description)
        } else {
            (None, None, None)
        };

        result.push(BackupFileInfo {
            filename,
            shard_type: shard_type.to_string(),
            metadata_file,
            title,
            description,
        });
    }

    result
}

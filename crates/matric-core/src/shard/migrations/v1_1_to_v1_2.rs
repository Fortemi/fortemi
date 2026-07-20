use serde_json::Value;

use super::super::{MigrationError, MigrationResult, ShardMigration};

/// Registers the compatible schema step that adds embedding contract lineage.
pub struct V1_1ToV1_2;

impl ShardMigration for V1_1ToV1_2 {
    fn from_version(&self) -> &str {
        "1.1.0"
    }

    fn to_version(&self) -> &str {
        "1.2.0"
    }

    fn description(&self) -> &str {
        "add nullable embedding contract fingerprints"
    }

    fn migrate(&self, data: Value) -> Result<MigrationResult, MigrationError> {
        if !data.is_object() {
            return Err(MigrationError::MigrationFailed(
                "knowledge shard record migration requires an object".to_string(),
            ));
        }
        Ok(MigrationResult {
            data,
            warnings: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_existing_records_for_the_compatible_minor_step() {
        let record = serde_json::json!({"id": "legacy", "deleted_at": null});
        let result = V1_1ToV1_2
            .migrate(record.clone())
            .expect("object record migrates");

        assert_eq!(result.data, record);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn rejects_non_object_records_without_echoing_data() {
        let error = V1_1ToV1_2
            .migrate(serde_json::json!("customer@example.com secret"))
            .expect_err("non-object must fail");
        let display = error.to_string();

        assert!(display.contains("message_len="));
        assert!(!display.contains("customer@example.com"));
        assert!(!display.contains("secret"));
    }
}

use serde_json::Value;

use super::super::{MigrationError, MigrationResult, MigrationWarning, ShardMigration};

/// Adds the optional core-v1 tombstone field introduced by schema 1.1.0.
pub struct V1_0ToV1_1;

impl ShardMigration for V1_0ToV1_1 {
    fn from_version(&self) -> &str {
        "1.0.0"
    }

    fn to_version(&self) -> &str {
        "1.1.0"
    }

    fn description(&self) -> &str {
        "add the core-v1 deleted_at tombstone field"
    }

    fn migrate(&self, mut data: Value) -> Result<MigrationResult, MigrationError> {
        let record = data.as_object_mut().ok_or_else(|| {
            MigrationError::MigrationFailed(
                "knowledge shard note migration requires an object".to_string(),
            )
        })?;
        let warnings = if record.contains_key("deleted_at") {
            Vec::new()
        } else {
            record.insert("deleted_at".to_string(), Value::Null);
            vec![MigrationWarning::DefaultApplied {
                field: "deleted_at".to_string(),
                default: "null".to_string(),
            }]
        };

        Ok(MigrationResult { data, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_explicit_active_state_to_legacy_note() {
        let result = V1_0ToV1_1
            .migrate(serde_json::json!({"id": "legacy"}))
            .expect("legacy note migrates");

        assert_eq!(result.data["deleted_at"], Value::Null);
        assert_eq!(
            result.warnings,
            vec![MigrationWarning::DefaultApplied {
                field: "deleted_at".to_string(),
                default: "null".to_string(),
            }]
        );
    }

    #[test]
    fn preserves_an_existing_tombstone_value_idempotently() {
        let deleted_at = "2026-07-18T00:00:00Z";
        let result = V1_0ToV1_1
            .migrate(serde_json::json!({"deleted_at": deleted_at}))
            .expect("current note remains valid");

        assert_eq!(result.data["deleted_at"], deleted_at);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn rejects_non_object_records_without_echoing_data() {
        let error = V1_0ToV1_1
            .migrate(serde_json::json!("customer@example.com secret"))
            .expect_err("non-object must fail");
        let display = error.to_string();

        assert!(display.contains("message_len="));
        assert!(!display.contains("customer@example.com"));
        assert!(!display.contains("secret"));
    }
}

//! Migration warnings for non-fatal migration issues.

use serde::{Deserialize, Serialize};

/// Warnings that can be emitted during shard migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MigrationWarning {
    /// A field was removed from the data during migration.
    FieldRemoved { field: String, count: usize },
    /// A default value was applied to a missing field.
    DefaultApplied { field: String, default: String },
    /// An unknown field was encountered and ignored.
    UnknownFieldIgnored { field: String },
    /// Data was truncated to fit new constraints.
    DataTruncated { field: String, detail: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warning_serialization() {
        let warning = MigrationWarning::FieldRemoved {
            field: "old_field".to_string(),
            count: 5,
        };
        let json = serde_json::to_string(&warning).unwrap();
        assert!(json.contains("field_removed"));
        assert!(json.contains("old_field"));
        assert!(json.contains("5"));

        let deserialized: MigrationWarning = serde_json::from_str(&json).unwrap();
        assert_eq!(warning, deserialized);
    }

    #[test]
    fn test_all_warning_types_serialize() {
        let warnings = vec![
            MigrationWarning::FieldRemoved {
                field: "foo".to_string(),
                count: 1,
            },
            MigrationWarning::DefaultApplied {
                field: "bar".to_string(),
                default: "baz".to_string(),
            },
            MigrationWarning::UnknownFieldIgnored {
                field: "qux".to_string(),
            },
            MigrationWarning::DataTruncated {
                field: "text".to_string(),
                detail: "truncated to 100 chars".to_string(),
            },
        ];

        for warning in warnings {
            let json = serde_json::to_string(&warning).unwrap();
            let deserialized: MigrationWarning = serde_json::from_str(&json).unwrap();
            assert_eq!(warning, deserialized);
        }
    }
}

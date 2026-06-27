//! Migration warnings for non-fatal migration issues.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Warnings that can be emitted during shard migration.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl fmt::Debug for MigrationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FieldRemoved { field, count } => f
                .debug_struct("MigrationWarning::FieldRemoved")
                .field("field_len", &field.chars().count())
                .field("count", count)
                .finish(),
            Self::DefaultApplied { field, default } => f
                .debug_struct("MigrationWarning::DefaultApplied")
                .field("field_len", &field.chars().count())
                .field("default_len", &default.chars().count())
                .finish(),
            Self::UnknownFieldIgnored { field } => f
                .debug_struct("MigrationWarning::UnknownFieldIgnored")
                .field("field_len", &field.chars().count())
                .finish(),
            Self::DataTruncated { field, detail } => f
                .debug_struct("MigrationWarning::DataTruncated")
                .field("field_len", &field.chars().count())
                .field("detail_len", &detail.chars().count())
                .finish(),
        }
    }
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

    #[test]
    fn migration_warning_debug_redacts_fields_defaults_and_details() {
        let warnings = vec![
            MigrationWarning::FieldRemoved {
                field: "customer@example.com_removed_sk-live-field".to_string(),
                count: 5,
            },
            MigrationWarning::DefaultApplied {
                field: "database_url".to_string(),
                default: "postgres://user:secret@db.internal/app".to_string(),
            },
            MigrationWarning::UnknownFieldIgnored {
                field: "token/sk-live-module".to_string(),
            },
            MigrationWarning::DataTruncated {
                field: "private/path/customer@example.com".to_string(),
                detail: "truncated bearer sk-live-detail from /srv/private/customer".to_string(),
            },
        ];

        for warning in warnings {
            let debug = format!("{warning:?}");

            assert!(debug.contains("MigrationWarning::"));
            assert!(debug.contains("field_len"));
            assert!(!debug.contains("customer@example.com"), "{debug}");
            assert!(!debug.contains("sk-live"), "{debug}");
            assert!(!debug.contains("postgres://"), "{debug}");
            assert!(!debug.contains("db.internal"), "{debug}");
            assert!(!debug.contains("database_url"), "{debug}");
            assert!(!debug.contains("private/path"), "{debug}");
            assert!(!debug.contains("/srv/private"), "{debug}");
            assert!(!debug.contains("bearer"), "{debug}");
        }
    }
}

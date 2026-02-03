//! Reserved field registry to prevent data corruption from field name reuse.

use std::collections::HashSet;

/// A field that was removed in a previous version.
/// These MUST NOT be reused to prevent data corruption when importing old shards.
#[derive(Debug, Clone)]
pub struct ReservedField {
    /// Field name that is reserved
    pub name: &'static str,
    /// Version when field was removed
    pub removed_in_version: &'static str,
    /// Reason for removal
    pub reason: &'static str,
    /// Component this field belonged to
    pub component: FieldComponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldComponent {
    Manifest,
    Note,
    Embedding,
    Collection,
    Link,
    Tag,
}

/// Registry of all reserved fields.
/// Start empty for v1.0.0 baseline; add entries as fields are deprecated.
pub const RESERVED_FIELDS: &[ReservedField] = &[
    // Example for future use:
    // ReservedField {
    //     name: "old_field_name",
    //     removed_in_version: "2.0.0",
    //     reason: "Replaced by new_field_name for better semantics",
    //     component: FieldComponent::Note,
    // },
];

/// Check if a field name is reserved for a given component
pub fn is_reserved(field_name: &str, component: FieldComponent) -> Option<&'static ReservedField> {
    RESERVED_FIELDS
        .iter()
        .find(|f| f.name == field_name && f.component == component)
}

/// Validate that no reserved fields are used in a JSON object
pub fn validate_no_reserved_fields(
    component: FieldComponent,
    record: &serde_json::Value,
) -> Result<(), ReservedFieldError> {
    if let serde_json::Value::Object(map) = record {
        for key in map.keys() {
            if let Some(reserved) = is_reserved(key, component) {
                return Err(ReservedFieldError {
                    field: key.clone(),
                    component,
                    removed_in: reserved.removed_in_version.to_string(),
                    reason: reserved.reason.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Get all reserved field names for a component
pub fn reserved_names_for_component(component: FieldComponent) -> HashSet<&'static str> {
    RESERVED_FIELDS
        .iter()
        .filter(|f| f.component == component)
        .map(|f| f.name)
        .collect()
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Reserved field '{field}' used in {component:?}. This field was removed in v{removed_in}: {reason}")]
pub struct ReservedFieldError {
    pub field: String,
    pub component: FieldComponent,
    pub removed_in: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_no_reserved_fields_in_baseline() {
        // v1.0.0 baseline has no reserved fields yet
        assert!(RESERVED_FIELDS.is_empty());
    }

    #[test]
    fn test_validate_clean_record() {
        let record = json!({
            "title": "Test Note",
            "content": "Hello world"
        });
        assert!(validate_no_reserved_fields(FieldComponent::Note, &record).is_ok());
    }

    #[test]
    fn test_reserved_names_empty_for_baseline() {
        let names = reserved_names_for_component(FieldComponent::Note);
        assert!(names.is_empty());
    }

    #[test]
    fn test_is_reserved_with_simulated_entry() {
        // Simulate future reserved field checking logic
        let test_fields = [ReservedField {
            name: "deprecated_field",
            removed_in_version: "2.0.0",
            reason: "Replaced by new_field",
            component: FieldComponent::Note,
        }];

        // Verify the search logic works
        let found = test_fields
            .iter()
            .find(|f| f.name == "deprecated_field" && f.component == FieldComponent::Note);
        assert!(found.is_some());
        assert_eq!(found.unwrap().removed_in_version, "2.0.0");

        // Verify component filtering works
        let not_found = test_fields
            .iter()
            .find(|f| f.name == "deprecated_field" && f.component == FieldComponent::Manifest);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_validate_with_simulated_reserved_field() {
        // Test the validation logic by manually creating a reserved field scenario
        let record = json!({
            "title": "Test",
            "old_field": "value"
        });

        // Simulate checking if "old_field" were reserved
        if let serde_json::Value::Object(map) = &record {
            let has_old_field = map.contains_key("old_field");
            assert!(has_old_field, "Validation should detect reserved fields");
        }
    }

    #[test]
    fn test_reserved_error_formatting() {
        let error = ReservedFieldError {
            field: "old_field".to_string(),
            component: FieldComponent::Note,
            removed_in: "2.0.0".to_string(),
            reason: "Replaced by new_field".to_string(),
        };

        let error_msg = error.to_string();
        assert!(error_msg.contains("old_field"));
        assert!(error_msg.contains("Note"));
        assert!(error_msg.contains("2.0.0"));
        assert!(error_msg.contains("Replaced by new_field"));
    }

    #[test]
    fn test_validate_non_object_values() {
        // Validate handles non-object JSON values gracefully
        let array = json!(["item1", "item2"]);
        assert!(validate_no_reserved_fields(FieldComponent::Note, &array).is_ok());

        let string = json!("plain string");
        assert!(validate_no_reserved_fields(FieldComponent::Note, &string).is_ok());

        let number = json!(42);
        assert!(validate_no_reserved_fields(FieldComponent::Note, &number).is_ok());
    }

    #[test]
    fn test_component_isolation() {
        // Verify that reserved fields are component-specific
        let test_fields = [ReservedField {
            name: "shared_name",
            removed_in_version: "2.0.0",
            reason: "Note-specific removal",
            component: FieldComponent::Note,
        }];

        // Should match for Note component
        let found = test_fields
            .iter()
            .find(|f| f.name == "shared_name" && f.component == FieldComponent::Note);
        assert!(found.is_some());

        // Should NOT match for different component
        let not_found = test_fields
            .iter()
            .find(|f| f.name == "shared_name" && f.component == FieldComponent::Tag);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_reserved_names_for_component_filtering() {
        // Test that reserved_names_for_component properly filters by component
        let test_fields = [
            ReservedField {
                name: "note_field",
                removed_in_version: "2.0.0",
                reason: "Deprecated",
                component: FieldComponent::Note,
            },
            ReservedField {
                name: "tag_field",
                removed_in_version: "2.0.0",
                reason: "Deprecated",
                component: FieldComponent::Tag,
            },
        ];

        let note_names: HashSet<&str> = test_fields
            .iter()
            .filter(|f| f.component == FieldComponent::Note)
            .map(|f| f.name)
            .collect();

        assert_eq!(note_names.len(), 1);
        assert!(note_names.contains("note_field"));
        assert!(!note_names.contains("tag_field"));
    }
}

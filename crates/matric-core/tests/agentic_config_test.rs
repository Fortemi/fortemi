//! Tests for AgenticConfig model (Issue #422).
//!
//! Verifies that AI-enhanced document generation metadata is properly
//! serialized/deserialized and provides the expected structure.

use matric_core::AgenticConfig;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_agentic_config_default() {
    let config = AgenticConfig::default();

    assert!(config.generation_prompt.is_none());
    assert!(config.required_sections.is_empty());
    assert!(config.optional_sections.is_empty());
    assert!(config.template_id.is_none());
    assert!(config.context_requirements.is_empty());
    assert!(config.validation_rules.is_empty());
    assert!(config.agent_hints.is_empty());
}

#[test]
fn test_agentic_config_serialization() {
    let mut context_requirements = HashMap::new();
    context_requirements.insert("needs_data_models".to_string(), true);
    context_requirements.insert("needs_endpoint_list".to_string(), true);

    let mut validation_rules = HashMap::new();
    validation_rules.insert("must_have_info".to_string(), json!(true));
    validation_rules.insert("must_have_paths".to_string(), json!(true));

    let config = AgenticConfig {
        generation_prompt: Some("Generate OpenAPI 3.1 specification".to_string()),
        required_sections: vec!["info".to_string(), "paths".to_string()],
        optional_sections: vec!["security".to_string(), "tags".to_string()],
        template_id: None,
        context_requirements,
        validation_rules,
        agent_hints: HashMap::new(),
    };

    let json = serde_json::to_value(&config).unwrap();

    assert_eq!(
        json["generation_prompt"],
        "Generate OpenAPI 3.1 specification"
    );
    assert_eq!(json["required_sections"], json!(["info", "paths"]));
    assert_eq!(json["optional_sections"], json!(["security", "tags"]));
    assert_eq!(json["context_requirements"]["needs_data_models"], true);
}

#[test]
fn test_agentic_config_deserialization() {
    let json = json!({
        "generation_prompt": "Create Rust source code",
        "required_sections": [],
        "context_requirements": {
            "needs_existing_code": true,
            "needs_crate_structure": true
        },
        "validation_rules": {
            "must_compile": true
        },
        "agent_hints": {
            "prefer_result_over_panic": true
        }
    });

    let config: AgenticConfig = serde_json::from_value(json).unwrap();

    assert_eq!(
        config.generation_prompt,
        Some("Create Rust source code".to_string())
    );
    assert!(config.required_sections.is_empty());
    assert_eq!(
        config.context_requirements.get("needs_existing_code"),
        Some(&true)
    );
    assert_eq!(
        config.validation_rules.get("must_compile"),
        Some(&json!(true))
    );
    assert_eq!(
        config.agent_hints.get("prefer_result_over_panic"),
        Some(&json!(true))
    );
}

#[test]
fn test_agentic_config_skip_empty_fields() {
    let config = AgenticConfig {
        generation_prompt: Some("Test prompt".to_string()),
        ..Default::default()
    };

    let json = serde_json::to_value(&config).unwrap();

    // Empty collections should be omitted from serialization
    assert!(!json.as_object().unwrap().contains_key("required_sections"));
    assert!(!json.as_object().unwrap().contains_key("optional_sections"));
    assert!(!json
        .as_object()
        .unwrap()
        .contains_key("context_requirements"));
    assert!(!json.as_object().unwrap().contains_key("validation_rules"));
    assert!(!json.as_object().unwrap().contains_key("agent_hints"));
}

#[test]
fn test_document_type_with_agentic_config() {
    // This test will verify that DocumentType properly includes agentic_config
    // once we update the model. For now, this documents the expected behavior.

    // Expected JSON structure after update:
    let expected_json = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "rust",
        "display_name": "Rust",
        "category": "code",
        "chunking_strategy": "syntactic",
        "chunk_size_default": 512,
        "chunk_overlap_default": 50,
        "preserve_boundaries": true,
        "is_system": true,
        "is_active": true,
        "agentic_config": {
            "generation_prompt": "Create Rust source code",
            "agent_hints": {
                "prefer_result_over_panic": true
            }
        }
    });

    // Verify expected structure
    assert!(expected_json.get("agentic_config").is_some());
    assert!(expected_json["agentic_config"]
        .get("generation_prompt")
        .is_some());
}

#[test]
fn test_agentic_config_roundtrip() {
    let mut agent_hints = HashMap::new();
    agent_hints.insert("include_code_examples".to_string(), json!(true));
    agent_hints.insert("use_consistent_headings".to_string(), json!(true));

    let original = AgenticConfig {
        generation_prompt: Some("Write markdown documentation".to_string()),
        required_sections: vec!["Overview".to_string()],
        optional_sections: vec!["Prerequisites".to_string(), "Examples".to_string()],
        template_id: None,
        context_requirements: HashMap::new(),
        validation_rules: HashMap::new(),
        agent_hints,
    };

    // Serialize to JSON
    let json = serde_json::to_value(&original).unwrap();

    // Deserialize back
    let deserialized: AgenticConfig = serde_json::from_value(json).unwrap();

    assert_eq!(original.generation_prompt, deserialized.generation_prompt);
    assert_eq!(original.required_sections, deserialized.required_sections);
    assert_eq!(original.optional_sections, deserialized.optional_sections);
    assert_eq!(original.agent_hints, deserialized.agent_hints);
}

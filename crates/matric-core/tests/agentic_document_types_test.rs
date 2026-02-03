//! Tests for agentic document types (Issue #429).
//!
//! Verifies that agentic category is properly supported and that
//! document type detection works for agentic primitives.

use matric_core::{AgenticConfig, DocumentCategory, DocumentType};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_agentic_category_exists() {
    // Verify that Agentic category can be constructed
    let category = DocumentCategory::Agentic;
    assert_eq!(category.to_string(), "agentic");
}

#[test]
fn test_agentic_category_parse() {
    // Verify that "agentic" string parses to Agentic category
    let category: DocumentCategory = "agentic".parse().unwrap();
    assert_eq!(category, DocumentCategory::Agentic);
}

#[test]
fn test_agentic_config_struct_exists() {
    // Verify AgenticConfig can be created with all fields
    let config = AgenticConfig {
        generation_prompt: Some("Test prompt".to_string()),
        required_sections: vec!["Role".to_string(), "Expertise".to_string()],
        optional_sections: vec!["Examples".to_string()],
        template_id: None,
        context_requirements: HashMap::new(),
        validation_rules: HashMap::new(),
        agent_hints: HashMap::new(),
    };

    assert_eq!(config.generation_prompt, Some("Test prompt".to_string()));
    assert_eq!(config.required_sections.len(), 2);
}

#[test]
fn test_document_type_has_agentic_config_field() {
    // Verify that DocumentType has agentic_config field
    // This test will fail until we add the field
    let doc_type_json = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "agent-definition",
        "display_name": "Agent Definition",
        "category": "agentic",
        "chunking_strategy": "whole",
        "chunk_size_default": 8000,
        "chunk_overlap_default": 50,
        "preserve_boundaries": true,
        "chunking_config": {},
        "is_system": true,
        "is_active": true,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "file_extensions": [".md", ".yaml"],
        "mime_types": [],
        "magic_patterns": ["## Role", "## Expertise"],
        "filename_patterns": ["*-agent.md"],
        "content_types": ["agentic", "config"],
        "agentic_config": {
            "generation_prompt": "Create an AI agent definition",
            "required_sections": ["Role", "Expertise"]
        }
    });

    // Try to deserialize - should work after we add the field
    let result: Result<DocumentType, _> = serde_json::from_value(doc_type_json);
    assert!(
        result.is_ok(),
        "DocumentType should deserialize with agentic_config"
    );

    let doc_type = result.unwrap();
    assert_eq!(doc_type.category, DocumentCategory::Agentic);

    let config = &doc_type.agentic_config;
    assert_eq!(
        config.generation_prompt,
        Some("Create an AI agent definition".to_string())
    );
    assert_eq!(config.required_sections, vec!["Role", "Expertise"]);
}

#[test]
fn test_agentic_config_optional_on_document_type() {
    // Verify that agentic_config is optional (can be None)
    let doc_type_json = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "markdown",
        "display_name": "Markdown",
        "category": "prose",
        "chunking_strategy": "semantic",
        "chunk_size_default": 512,
        "chunk_overlap_default": 50,
        "preserve_boundaries": true,
        "chunking_config": {},
        "is_system": true,
        "is_active": true,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "file_extensions": [".md"],
        "mime_types": [],
        "magic_patterns": [],
        "filename_patterns": [],
        "content_types": ["prose"]
    });

    let result: Result<DocumentType, _> = serde_json::from_value(doc_type_json);
    assert!(result.is_ok());

    let doc_type = result.unwrap();
    assert_eq!(doc_type.category, DocumentCategory::Prose);
    // agentic_config should be default for non-agentic types
    assert_eq!(doc_type.agentic_config, AgenticConfig::default());
}

#[test]
fn test_agentic_config_serialization_skips_empty() {
    // Verify empty AgenticConfig is skipped during serialization
    let config = AgenticConfig::default();
    let json = serde_json::to_value(&config).unwrap();

    // Default/empty config should serialize to empty object or be skipped
    assert!(json.as_object().map(|o| o.is_empty()).unwrap_or(false) || json.is_null());
}

#[test]
fn test_agent_definition_type_config() {
    // Test specific configuration for agent-definition type
    let config = AgenticConfig {
        generation_prompt: Some(
            "Create an AI agent definition following the Agent Design Bible principles".to_string(),
        ),
        required_sections: vec![
            "Role".to_string(),
            "Expertise".to_string(),
            "Tools".to_string(),
            "Success Criteria".to_string(),
        ],
        optional_sections: vec![],
        template_id: None,
        context_requirements: HashMap::new(),
        validation_rules: HashMap::new(),
        agent_hints: HashMap::new(),
    };

    assert_eq!(config.required_sections.len(), 4);
    assert!(config.generation_prompt.is_some());
}

#[test]
fn test_skill_definition_type_config() {
    // Test specific configuration for skill-definition type
    let config = AgenticConfig {
        generation_prompt: Some(
            "Create a skill definition with clear triggers and execution flow".to_string(),
        ),
        required_sections: vec![
            "Natural Language Triggers".to_string(),
            "Parameters".to_string(),
            "Execution".to_string(),
        ],
        optional_sections: vec![],
        template_id: None,
        context_requirements: HashMap::new(),
        validation_rules: HashMap::new(),
        agent_hints: HashMap::new(),
    };

    assert_eq!(config.required_sections.len(), 3);
    assert!(config
        .generation_prompt
        .unwrap()
        .contains("skill definition"));
}

#[test]
fn test_claude_md_type_config() {
    // Test configuration for CLAUDE.md detection
    let doc_type_json = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "claude-md",
        "display_name": "Claude Project Instructions",
        "category": "agentic",
        "chunking_strategy": "per_section",
        "chunk_size_default": 2000,
        "chunk_overlap_default": 50,
        "preserve_boundaries": true,
        "chunking_config": {},
        "is_system": true,
        "is_active": true,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "file_extensions": [".md"],
        "mime_types": [],
        "magic_patterns": ["## Architecture", "## Development", "## Key Features"],
        "filename_patterns": ["CLAUDE.md"],
        "content_types": ["agentic", "documentation", "config"],
        "agentic_config": {}
    });

    let result: Result<DocumentType, _> = serde_json::from_value(doc_type_json);
    assert!(result.is_ok());

    let doc_type = result.unwrap();
    assert_eq!(doc_type.name, "claude-md");
    assert_eq!(doc_type.category, DocumentCategory::Agentic);
    assert!(doc_type
        .filename_patterns
        .contains(&"CLAUDE.md".to_string()));
}

#[test]
fn test_mcp_server_config_type() {
    // Test configuration for .mcp.json detection
    let doc_type_json = json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "name": "mcp-server-config",
        "display_name": "MCP Server Configuration",
        "category": "agentic",
        "chunking_strategy": "whole",
        "chunk_size_default": 2000,
        "chunk_overlap_default": 50,
        "preserve_boundaries": true,
        "chunking_config": {},
        "is_system": true,
        "is_active": true,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "file_extensions": [".json"],
        "mime_types": [],
        "magic_patterns": ["mcpServers"],
        "filename_patterns": [".mcp.json", "mcp.json"],
        "content_types": ["agentic", "config"],
        "agentic_config": {}
    });

    let result: Result<DocumentType, _> = serde_json::from_value(doc_type_json);
    assert!(result.is_ok());

    let doc_type = result.unwrap();
    assert_eq!(doc_type.name, "mcp-server-config");
    assert!(doc_type
        .filename_patterns
        .contains(&".mcp.json".to_string()));
}

#[test]
fn test_all_agentic_document_types_expected() {
    // Document all expected agentic document types from the migration
    let expected_types = vec![
        "agent-definition",
        "skill-definition",
        "command-definition",
        "prompt-template",
        "system-prompt",
        "workflow-definition",
        "claude-md",
        "agents-md",
        "warp-md",
        "cursor-rules",
        "windsurf-rules",
        "copilot-instructions",
        "mcp-tool-definition",
        "mcp-server-config",
        "hook-definition",
        "aiwg-addon",
        "aiwg-framework",
        "aiwg-extension",
    ];

    assert_eq!(
        expected_types.len(),
        18,
        "Should have 18 agentic document types"
    );
}

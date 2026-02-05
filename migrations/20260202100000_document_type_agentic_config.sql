-- Add agentic_config for AI-enhanced document generation (Issue #422)
-- Provides AI agents with structured prompts, required sections, and context requirements

ALTER TABLE document_type ADD COLUMN agentic_config JSONB DEFAULT '{}';

-- Create index for querying documents with generation prompts
CREATE INDEX idx_document_type_agentic_config ON document_type USING GIN(agentic_config);

COMMENT ON COLUMN document_type.agentic_config IS 'AI generation metadata: generation_prompt, required_sections, optional_sections, context_requirements, validation_rules, agent_hints';

-- Seed agentic config for key document types
UPDATE document_type SET agentic_config = '{
  "generation_prompt": "Generate OpenAPI 3.1 specification with comprehensive endpoint documentation",
  "required_sections": ["info", "servers", "paths", "components"],
  "optional_sections": ["security", "tags", "externalDocs"],
  "context_requirements": {"needs_data_models": true, "needs_endpoint_list": true},
  "validation_rules": {"must_have_info": true, "must_have_paths": true}
}'::jsonb WHERE name = 'openapi';

UPDATE document_type SET agentic_config = '{
  "generation_prompt": "Create Rust source code following idiomatic patterns and best practices",
  "required_sections": [],
  "context_requirements": {"needs_existing_code": true, "needs_crate_structure": true},
  "validation_rules": {"must_compile": true},
  "agent_hints": {"prefer_result_over_panic": true, "use_clippy_recommendations": true}
}'::jsonb WHERE name = 'rust';

UPDATE document_type SET agentic_config = '{
  "generation_prompt": "Write clear, well-structured markdown documentation",
  "required_sections": ["Overview"],
  "optional_sections": ["Prerequisites", "Installation", "Usage", "Examples", "Troubleshooting"],
  "agent_hints": {"include_code_examples": true, "use_consistent_headings": true}
}'::jsonb WHERE name = 'markdown';

UPDATE document_type SET agentic_config = '{
  "generation_prompt": "Generate comprehensive API documentation following technical writing best practices",
  "required_sections": ["Introduction", "Authentication", "Endpoints", "Errors"],
  "optional_sections": ["Rate Limits", "Versioning", "SDKs", "Examples"]
}'::jsonb WHERE name = 'api-docs';

UPDATE document_type SET agentic_config = '{
  "generation_prompt": "Generate Python code following PEP 8 and type hints best practices",
  "required_sections": [],
  "context_requirements": {"needs_existing_code": true, "needs_imports": true},
  "validation_rules": {"must_pass_mypy": true},
  "agent_hints": {"use_type_hints": true, "prefer_dataclasses": true}
}'::jsonb WHERE name = 'python';

UPDATE document_type SET agentic_config = '{
  "generation_prompt": "Generate TypeScript code with strict type safety and modern patterns",
  "required_sections": [],
  "context_requirements": {"needs_existing_code": true, "needs_types": true},
  "validation_rules": {"must_compile": true},
  "agent_hints": {"use_strict_mode": true, "prefer_const": true}
}'::jsonb WHERE name = 'typescript';

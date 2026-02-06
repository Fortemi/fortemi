-- Add agentic_config for AI-enhanced document generation (Issue #422)
-- Provides AI agents with structured prompts, required sections, and context requirements

ALTER TABLE document_type ADD COLUMN agentic_config JSONB DEFAULT '{}';

-- Create index for querying documents with generation prompts
CREATE INDEX idx_document_type_agentic_config ON document_type USING GIN(agentic_config);

COMMENT ON COLUMN document_type.agentic_config IS 'AI generation metadata: generation_prompt, required_sections, optional_sections, context_requirements, validation_rules, agent_hints';

-- Seed data moved to: 20260202100000_seed_agentic_configs.sql

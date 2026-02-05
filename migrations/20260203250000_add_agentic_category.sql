-- Add 'agentic' category for AI/agent-related document types (Issue #429)

ALTER TYPE document_category ADD VALUE 'agentic';

COMMENT ON TYPE document_category IS 'Document categories including agentic for AI agent content';

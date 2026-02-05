-- Seed Agentic Document Types Migration (Issue #429)
-- Document types for AI agents, prompts, workflows, and training data

INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, agentic_config, is_system) VALUES

-- Agent Prompts
('agent-prompt', 'Agent Prompt', 'agentic', 'System prompts and instructions for AI agents',
 ARRAY['.prompt', '.txt', '.md'],
 ARRAY['%prompt%', '%system%', '%.prompt'],
 ARRAY['%You are%', '%Your role%', '%Act as%'],
 'whole',
 '{
   "generation_prompt": "Create a clear, specific system prompt that defines agent behavior, constraints, and output format",
   "required_sections": ["Role", "Instructions"],
   "optional_sections": ["Examples", "Constraints", "Output Format"],
   "context_requirements": {"needs_use_case": true, "needs_agent_capabilities": true},
   "agent_hints": {"be_specific": true, "include_examples": true, "define_boundaries": true}
 }'::jsonb,
 TRUE),

-- Claude Skills
('agent-skill', 'Claude Skill', 'agentic', 'Claude skill definitions in SKILL.md format',
 ARRAY['.skill.md', '.md'],
 ARRAY['%SKILL.md', '%skill%'],
 ARRAY['%# Skill:%', '%## Description%', '%## Usage%'],
 'per_section',
 '{
   "generation_prompt": "Generate a Claude skill following the SKILL.md format with clear description, usage, and examples",
   "required_sections": ["Skill", "Description", "Usage", "Examples"],
   "optional_sections": ["Parameters", "Return Value", "Notes"],
   "context_requirements": {"needs_skill_purpose": true, "needs_example_inputs": true},
   "agent_hints": {"follow_skill_md_format": true, "include_working_examples": true}
 }'::jsonb,
 TRUE),

-- Agent Workflows
('agent-workflow', 'Agent Workflow', 'agentic', 'Multi-step agent workflows and orchestration configs',
 ARRAY['.workflow.yaml', '.workflow.yml', '.workflow.json', '.yaml', '.yml'],
 ARRAY['%workflow%', '%.workflow.%'],
 ARRAY['%steps:%', '%agents:%', '%tasks:%'],
 'semantic',
 '{
   "generation_prompt": "Design a multi-step agent workflow with clear task definitions, dependencies, and error handling",
   "required_sections": ["steps", "agents"],
   "optional_sections": ["error_handling", "retry_policy", "timeout"],
   "context_requirements": {"needs_task_breakdown": true, "needs_agent_list": true},
   "validation_rules": {"must_have_steps": true, "must_define_agents": true},
   "agent_hints": {"define_dependencies": true, "include_error_paths": true}
 }'::jsonb,
 TRUE),

-- MCP Tools
('mcp-tool', 'MCP Tool', 'agentic', 'Model Context Protocol tool definitions and implementations',
 ARRAY['.mcp.json', '.mcp.yaml', '.json'],
 ARRAY['%mcp%', '%tool%'],
 ARRAY['%"name":%', '%"description":%', '%"inputSchema":%'],
 'whole',
 '{
   "generation_prompt": "Define an MCP tool with JSON Schema for parameters and clear usage documentation",
   "required_sections": ["name", "description", "inputSchema"],
   "optional_sections": ["examples", "outputSchema"],
   "context_requirements": {"needs_function_purpose": true, "needs_parameter_types": true},
   "validation_rules": {"must_have_json_schema": true},
   "agent_hints": {"use_json_schema": true, "include_examples": true}
 }'::jsonb,
 TRUE),

-- RAG Context
('rag-context', 'RAG Context', 'agentic', 'Retrieval-augmented generation context chunks',
 ARRAY['.rag.md', '.context.md', '.md'],
 ARRAY['%rag%', '%context%'],
 NULL,
 'semantic',
 '{
   "generation_prompt": "Extract and format relevant context for RAG retrieval with clear source attribution",
   "required_sections": ["Content", "Source"],
   "optional_sections": ["Metadata", "Tags", "Related"],
   "context_requirements": {"needs_source_document": true, "needs_chunk_boundaries": true},
   "agent_hints": {"preserve_meaning": true, "include_metadata": true, "maintain_context": true}
 }'::jsonb,
 TRUE),

-- AI Conversations
('ai-conversation', 'AI Conversation', 'agentic', 'AI chat transcripts and conversation logs',
 ARRAY['.conversation.json', '.chat.json', '.json', '.md'],
 ARRAY['%conversation%', '%chat%', '%transcript%'],
 ARRAY['%"role":%', '%"content":%', '%"assistant":%', '%"user":%'],
 'per_section',
 '{
   "generation_prompt": "Format AI conversation with clear role attribution and message boundaries",
   "required_sections": ["messages"],
   "optional_sections": ["metadata", "context", "annotations"],
   "context_requirements": {"needs_message_list": true},
   "agent_hints": {"preserve_turns": true, "maintain_context": true}
 }'::jsonb,
 TRUE),

-- Fine-tuning Data
('fine-tune-data', 'Fine-tuning Data', 'agentic', 'Training datasets for model fine-tuning',
 ARRAY['.jsonl', '.ndjson', '.csv'],
 ARRAY['%train%', '%finetune%', '%dataset%'],
 ARRAY['%{"prompt":%', '%{"messages":%'],
 'per_section',
 '{
   "generation_prompt": "Generate training examples with consistent format, diverse inputs, and quality outputs",
   "required_sections": ["input", "output"],
   "optional_sections": ["metadata", "weight", "category"],
   "context_requirements": {"needs_task_definition": true, "needs_example_quality": true},
   "validation_rules": {"must_have_pairs": true, "consistent_format": true},
   "agent_hints": {"ensure_diversity": true, "validate_quality": true, "balance_dataset": true}
 }'::jsonb,
 TRUE),

-- Evaluation Sets
('evaluation-set', 'Evaluation Set', 'agentic', 'Test cases and benchmarks for AI evaluation',
 ARRAY['.eval.json', '.eval.jsonl', '.test.json', '.json'],
 ARRAY['%eval%', '%test%', '%benchmark%'],
 ARRAY['%"input":%', '%"expected":%', '%"test_case":%'],
 'per_section',
 '{
   "generation_prompt": "Create evaluation test cases with clear inputs, expected outputs, and success criteria",
   "required_sections": ["test_cases", "expected_outputs"],
   "optional_sections": ["metrics", "scoring", "categories"],
   "context_requirements": {"needs_test_scenarios": true, "needs_success_criteria": true},
   "validation_rules": {"must_have_expected": true, "must_define_metrics": true},
   "agent_hints": {"cover_edge_cases": true, "define_metrics": true, "include_failure_cases": true}
 }'::jsonb,
 TRUE);

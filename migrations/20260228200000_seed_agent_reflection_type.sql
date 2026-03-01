-- Seed Agent Reflection Document Type (Issue #563)
-- Lightweight note type for agent-generated operational guidance.
-- Designed for high-volume, short (<100 words) structured reflections.
-- Callers should use revision_mode=none when creating these notes.

INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, agentic_config, is_system) VALUES

('agent-reflection', 'Agent Reflection', 'agentic', 'Agent-generated session reflection with operational insights',
 ARRAY['.reflection.json', '.reflection.md', '.json'],
 ARRAY['%reflection%', '%agent-reflection%'],
 ARRAY['%session_id%', '%insight%', '%tools_used%', '%successful_patterns%'],
 'whole',
 '{
   "generation_prompt": "Generate a concise agent reflection capturing session insights, tool usage patterns, and operational guidance in under 100 words",
   "required_sections": ["session_id", "insight", "tools_used"],
   "optional_sections": ["user_intent_summary", "successful_patterns", "failed_patterns", "timestamp"],
   "context_requirements": {"needs_session_context": true, "needs_tool_results": true},
   "agent_hints": {"keep_concise": true, "max_words": 100, "skip_revision": true, "skip_title_generation": true, "embed_immediately": true}
 }'::jsonb,
 TRUE);

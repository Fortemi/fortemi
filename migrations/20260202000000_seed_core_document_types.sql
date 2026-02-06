-- Seed: Core document types (14 foundational types)
INSERT INTO document_type (name, display_name, category, description, file_extensions, chunking_strategy, tree_sitter_language, is_system) VALUES
-- Prose types
('markdown', 'Markdown', 'prose', 'Markdown documentation and notes', ARRAY['.md', '.markdown', '.mdx'], 'semantic', NULL, TRUE),
('plaintext', 'Plain Text', 'prose', 'Plain text documents', ARRAY['.txt', '.text'], 'semantic', NULL, TRUE),

-- Code types
('rust', 'Rust', 'code', 'Rust programming language', ARRAY['.rs'], 'syntactic', 'rust', TRUE),
('python', 'Python', 'code', 'Python programming language', ARRAY['.py', '.pyi', '.pyw'], 'syntactic', 'python', TRUE),
('typescript', 'TypeScript', 'code', 'TypeScript programming language', ARRAY['.ts', '.tsx', '.mts', '.cts'], 'syntactic', 'typescript', TRUE),
('javascript', 'JavaScript', 'code', 'JavaScript programming language', ARRAY['.js', '.jsx', '.mjs', '.cjs'], 'syntactic', 'javascript', TRUE),
('go', 'Go', 'code', 'Go programming language', ARRAY['.go'], 'syntactic', 'go', TRUE),
('java', 'Java', 'code', 'Java programming language', ARRAY['.java'], 'syntactic', 'java', TRUE),

-- Config types
('json', 'JSON', 'config', 'JSON configuration files', ARRAY['.json'], 'fixed', 'json', TRUE),
('yaml', 'YAML', 'config', 'YAML configuration files', ARRAY['.yaml', '.yml'], 'fixed', 'yaml', TRUE),
('toml', 'TOML', 'config', 'TOML configuration files', ARRAY['.toml'], 'fixed', 'toml', TRUE),

-- Markup types
('html', 'HTML', 'markup', 'HTML documents', ARRAY['.html', '.htm'], 'syntactic', 'html', TRUE),
('xml', 'XML', 'markup', 'XML documents', ARRAY['.xml'], 'syntactic', NULL, TRUE),

-- SQL types
('sql', 'SQL', 'database', 'SQL scripts and migrations', ARRAY['.sql'], 'per_unit', NULL, TRUE);

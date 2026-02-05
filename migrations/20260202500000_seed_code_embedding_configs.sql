-- ============================================================================
-- Seed Code Embedding Configurations (#394)
-- ============================================================================
-- Adds code-specialized embedding configurations optimized for semantic
-- code search and technical documentation.
--
-- Reference: Issue #394
-- Related: ADR-027 (Code Embedding Models Integration)
-- ============================================================================

-- ============================================================================
-- Code-Specialized Embedding Configurations
-- ============================================================================

-- Code Search (Primary)
-- nomic-embed-text is well-suited for code due to its training on code+text
INSERT INTO embedding_config (
    name,
    description,
    provider,
    model,
    dimension,
    chunk_size,
    chunk_overlap,
    content_types,
    is_default
) VALUES (
    'code-search',
    'Code-optimized embedding for semantic code search using nomic-embed-text (768 dims)',
    'ollama',
    'nomic-embed-text',
    768,
    512,  -- Smaller chunks for code (function/class level)
    50,   -- Less overlap needed for syntactic boundaries
    ARRAY['code', 'technical'],
    FALSE
);

-- Code Search (Lightweight)
-- bge-small-en provides good performance with lower memory footprint
INSERT INTO embedding_config (
    name,
    description,
    provider,
    model,
    dimension,
    chunk_size,
    chunk_overlap,
    content_types,
    is_default
) VALUES (
    'code-bge-small',
    'Lightweight code embedding using bge-small-en (384 dims) for fast retrieval',
    'ollama',
    'bge-small-en',
    384,
    512,
    50,
    ARRAY['code'],
    FALSE
);

-- Mixed Code + Documentation
-- For repositories with interspersed code and markdown documentation
INSERT INTO embedding_config (
    name,
    description,
    provider,
    model,
    dimension,
    chunk_size,
    chunk_overlap,
    content_types,
    is_default
) VALUES (
    'code-docs',
    'Mixed code and documentation embedding using nomic-embed-text (768 dims)',
    'ollama',
    'nomic-embed-text',
    768,
    1000,  -- Larger chunks to capture context across code+comments
    100,
    ARRAY['code', 'prose', 'technical'],
    FALSE
);

-- API Schema & Specifications
-- For OpenAPI, GraphQL, Protobuf, and other API definitions
INSERT INTO embedding_config (
    name,
    description,
    provider,
    model,
    dimension,
    chunk_size,
    chunk_overlap,
    content_types,
    is_default
) VALUES (
    'api-schema',
    'API specifications and schema embedding using nomic-embed-text (768 dims)',
    'ollama',
    'nomic-embed-text',
    768,
    1000,  -- Keep full API definitions together
    100,
    ARRAY['api-spec', 'database', 'config'],
    FALSE
);

-- ============================================================================
-- Update Document Types with Recommended Configs
-- ============================================================================

-- Update code document types to use code-search config
UPDATE document_type
SET recommended_config_id = (SELECT id FROM embedding_config WHERE name = 'code-search')
WHERE category = 'code' AND is_system = TRUE;

-- Update API spec types to use api-schema config
UPDATE document_type
SET recommended_config_id = (SELECT id FROM embedding_config WHERE name = 'api-schema')
WHERE category = 'api-spec' AND is_system = TRUE;

-- Update database schema types to use api-schema config
UPDATE document_type
SET recommended_config_id = (SELECT id FROM embedding_config WHERE name = 'api-schema')
WHERE category = 'database' AND is_system = TRUE;

-- Update shell/build script types to use code-search config
UPDATE document_type
SET recommended_config_id = (SELECT id FROM embedding_config WHERE name = 'code-search')
WHERE category = 'shell' AND is_system = TRUE;

-- Update IaC types to use code-docs config (often mixed YAML + comments)
UPDATE document_type
SET recommended_config_id = (SELECT id FROM embedding_config WHERE name = 'code-docs')
WHERE category = 'iac' AND is_system = TRUE;

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE embedding_config IS 'Embedding configuration profiles with model, provider, and content type settings. Includes code-specialized configs for semantic code search.';

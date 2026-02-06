-- Document Type Registry Migration
-- Enables content-aware processing for code, configuration, and documentation

-- Document category enum
CREATE TYPE document_category AS ENUM (
    'prose',      -- Markdown, plaintext, documentation
    'code',       -- Programming languages
    'config',     -- JSON, YAML, TOML configuration
    'markup',     -- HTML, XML
    'data',       -- CSV, structured data
    'api-spec',   -- OpenAPI, GraphQL, Protobuf
    'iac',        -- Terraform, Kubernetes, Docker
    'database',   -- SQL, migrations, schemas
    'shell',      -- Bash, scripts, build files
    'docs',       -- RST, AsciiDoc, Jupyter
    'package',    -- Package manifests
    'observability', -- Logs, traces, metrics
    'legal',      -- Contracts, policies
    'communication', -- Email, chat
    'research',   -- Academic papers, citations
    'creative',   -- Blog posts, marketing
    'media',      -- Images, audio, video metadata
    'personal',   -- Journal, bookmarks
    'custom'      -- User-defined
);

-- Chunking strategy enum
CREATE TYPE chunking_strategy AS ENUM (
    'semantic',   -- Paragraph/section boundaries (prose)
    'syntactic',  -- Tree-sitter AST boundaries (code)
    'fixed',      -- Fixed token count with overlap
    'hybrid',     -- Combine semantic + syntactic
    'per_section',-- Section-based (docs)
    'per_unit',   -- Per logical unit (functions, classes)
    'whole'       -- Keep as single chunk
);

CREATE TABLE document_type (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    name TEXT NOT NULL UNIQUE,           -- e.g., 'rust', 'markdown', 'openapi'
    display_name TEXT NOT NULL,          -- e.g., 'Rust', 'Markdown', 'OpenAPI Spec'
    category document_category NOT NULL,
    description TEXT,

    -- Detection rules
    file_extensions TEXT[] DEFAULT '{}', -- e.g., ['.rs', '.rust']
    mime_types TEXT[] DEFAULT '{}',      -- e.g., ['text/x-rust']
    magic_patterns TEXT[] DEFAULT '{}',  -- Content patterns for detection
    filename_patterns TEXT[] DEFAULT '{}', -- e.g., ['Dockerfile', 'Makefile']

    -- Chunking configuration
    chunking_strategy chunking_strategy NOT NULL DEFAULT 'semantic',
    chunk_size_default INTEGER DEFAULT 512,
    chunk_overlap_default INTEGER DEFAULT 50,
    preserve_boundaries BOOLEAN DEFAULT TRUE,
    chunking_config JSONB DEFAULT '{}',  -- Strategy-specific options

    -- Embedding recommendation
    recommended_config_id UUID REFERENCES embedding_config(id),
    content_types TEXT[] DEFAULT '{}',   -- ['code'], ['prose', 'technical']

    -- Tree-sitter support
    tree_sitter_language TEXT,           -- e.g., 'rust', 'python' (NULL if not supported)

    -- System vs user-defined
    is_system BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT
);

-- Indexes
CREATE INDEX idx_document_type_category ON document_type(category);
CREATE INDEX idx_document_type_active ON document_type(is_active) WHERE is_active = TRUE;
CREATE INDEX idx_document_type_extensions ON document_type USING GIN(file_extensions);
CREATE INDEX idx_document_type_filename_patterns ON document_type USING GIN(filename_patterns);

-- Add foreign key to note table
ALTER TABLE note ADD COLUMN document_type_id UUID REFERENCES document_type(id);
CREATE INDEX idx_note_document_type ON note(document_type_id);

-- Seed data moved to 20260202000000_seed_core_document_types.sql

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_document_type_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER document_type_updated
    BEFORE UPDATE ON document_type
    FOR EACH ROW
    EXECUTE FUNCTION update_document_type_timestamp();

# Design Notes: Flexible Code & Document Type Support

## Philosophy: Maximum Flexibility

Rather than hardcoding specific models or document types, design for extensibility:

1. **Registry-based** - Document types and embedding configs are data, not code
2. **Strategy pattern** - Chunking and embedding strategies are pluggable
3. **Convention over configuration** - Sensible defaults, override when needed
4. **Self-describing** - Models declare their capabilities

## Proposed Document Type Registry

```sql
CREATE TABLE document_type (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,        -- e.g., "rust", "python", "markdown", "yaml"
    display_name TEXT NOT NULL,       -- e.g., "Rust Source Code"
    category TEXT NOT NULL,           -- "code", "markup", "config", "prose"

    -- File detection
    file_extensions TEXT[],           -- [".rs", ".rust"]
    mime_types TEXT[],                -- ["text/x-rust"]
    shebang_patterns TEXT[],          -- ["#!/usr/bin/env rustc"]

    -- Chunking strategy
    chunking_strategy TEXT NOT NULL DEFAULT 'semantic',  -- semantic, syntactic, fixed, hybrid
    chunk_size_default INT DEFAULT 512,
    chunk_overlap_default INT DEFAULT 50,
    preserve_boundaries BOOLEAN DEFAULT TRUE,  -- respect function/class boundaries

    -- Embedding recommendation
    recommended_config_id UUID REFERENCES embedding_config(id),

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Proposed Embedding Models for Code

### Local (Ollama)

| Model | Dimension | MRL | Best For | Notes |
|-------|-----------|-----|----------|-------|
| nomic-embed-text | 768 | ✓ | General + light code | Already have, works okay |
| jina-embeddings-v2-base-code | 768 | ✗ | Code-specific | Trained on code corpus |
| codestral-embed | 1024 | ? | Code + multilingual | Mistral's code model |

### Cloud (Optional)

| Provider | Model | Dimension | Best For |
|----------|-------|-----------|----------|
| Voyage AI | voyage-code-2 | 1536 | Production code search |
| Cohere | embed-english-v3.0 | 1024 | General with code support |
| OpenAI | text-embedding-3-large | 3072 | Highest quality, expensive |

### Hybrid Strategy

For maximum flexibility, support model routing:

```rust
pub enum EmbeddingStrategy {
    /// Use a single model for all content
    Single { config_id: Uuid },

    /// Route based on detected content type
    ContentAware {
        code_config_id: Uuid,
        prose_config_id: Uuid,
        default_config_id: Uuid,
    },

    /// Multi-vector: embed with multiple models, fuse at search time
    MultiVector {
        config_ids: Vec<Uuid>,
        fusion_weights: Vec<f32>,
    },
}
```

## Code-Aware Chunking

### Current: Naive Text Chunking
```
Split on token count, overlap by N tokens
```

### Proposed: Syntax-Aware Chunking

```rust
pub trait CodeChunker {
    /// Parse source into semantic units
    fn parse_units(&self, source: &str) -> Vec<CodeUnit>;

    /// Chunk respecting boundaries
    fn chunk(&self, source: &str, max_tokens: usize) -> Vec<Chunk>;
}

pub struct CodeUnit {
    kind: CodeUnitKind,  // Function, Class, Module, Comment, Import
    name: Option<String>,
    span: Range<usize>,
    context: String,     // Parent module/class path
}
```

### Tree-sitter Integration

Use tree-sitter for language-agnostic parsing:

```rust
// Supported languages via tree-sitter
const SUPPORTED_LANGUAGES: &[&str] = &[
    "rust", "python", "typescript", "javascript", "go",
    "java", "c", "cpp", "ruby", "php", "swift", "kotlin"
];
```

## API Extensions

### 1. Dynamic Embedding Config Registration

```
POST /api/v1/embedding-configs
{
    "name": "code-jina",
    "model": "jina-embeddings-v2-base-code",
    "dimension": 768,
    "chunk_size": 512,
    "chunk_overlap": 50,
    "provider": "ollama",  // ollama, openai, voyage, cohere
    "provider_config": {
        "base_url": "http://localhost:11434"
    },
    "supports_mrl": false,
    "content_types": ["code"]  // recommended content types
}
```

### 2. Document Type Registration

```
POST /api/v1/document-types
{
    "name": "rust",
    "display_name": "Rust Source Code",
    "category": "code",
    "file_extensions": [".rs"],
    "chunking_strategy": "syntactic",
    "preserve_boundaries": true,
    "recommended_config": "code-jina"
}
```

### 3. Content-Type Detection Endpoint

```
POST /api/v1/detect-content-type
{
    "content": "fn main() { println!(\"Hello\"); }",
    "filename": "main.rs"  // optional hint
}

Response:
{
    "detected_type": "rust",
    "confidence": 0.98,
    "recommended_config": "code-jina",
    "chunking_strategy": "syntactic"
}
```

## Implementation Phases

### Phase 1: Foundation (This Epic)
- [ ] Document type table and API
- [ ] Dynamic embedding config API
- [ ] Content-type detection (basic)
- [ ] Seed common document types

### Phase 2: Code Intelligence
- [ ] Tree-sitter integration
- [ ] Syntax-aware chunking
- [ ] Code-specific embedding models
- [ ] Language detection

### Phase 3: Self-Maintenance
- [ ] Ingest Matric Memory codebase
- [ ] Build code search embedding set
- [ ] Query interface for code questions
- [ ] Integration with development workflow

## Open Questions

1. **Model availability**: Which code models are available in Ollama?
2. **Chunking granularity**: Function-level vs. file-level vs. hybrid?
3. **Multi-language files**: How to handle mixed content (e.g., TSX with prose)?
4. **Cost management**: Rate limits for cloud embedding providers?
5. **Incremental updates**: Re-embed only changed functions, not whole files?

# ADR-025: Document Type Registry

**Status:** Implemented
**Date:** 2026-02-01
**Implementation Date:** 2026-02-01
**Deciders:** Architecture team

## Context

Matric Memory currently treats all content as generic text with a free-form `format` field (typically "markdown"). This works for prose but creates problems for code and technical content:

1. **Chunking**: Code requires syntax-aware chunking that respects function/class boundaries
2. **Embedding**: Code-optimized embedding models exist but we can't route content to them
3. **Detection**: No way to automatically detect content type from file extension or content
4. **Customization**: Users can't define new document types without code changes

For self-maintenance capabilities, the system must understand different content types and process them appropriately.

## Decision

Introduce a **Document Type Registry** as a database table with API endpoints for CRUD operations:

```sql
CREATE TABLE document_type (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    category TEXT NOT NULL,  -- 'code', 'markup', 'config', 'prose', 'data'

    -- Detection rules
    file_extensions TEXT[],
    mime_types TEXT[],
    magic_patterns TEXT[],   -- Content patterns for detection

    -- Processing configuration
    chunking_strategy TEXT NOT NULL DEFAULT 'semantic',
    chunk_size_default INT DEFAULT 512,
    chunk_overlap_default INT DEFAULT 50,
    preserve_boundaries BOOLEAN DEFAULT TRUE,

    -- Embedding recommendation
    recommended_config_id UUID REFERENCES embedding_config(id),

    -- Metadata
    is_system BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Chunking Strategies:**
- `semantic`: Split on paragraph/section boundaries (prose)
- `syntactic`: Split on syntax boundaries via tree-sitter (code)
- `fixed`: Fixed token count with overlap (fallback)
- `hybrid`: Combine semantic + syntactic for mixed content

**Seeded Document Types:**
- `markdown`, `plaintext` (prose)
- `rust`, `python`, `typescript`, `go`, `java` (code)
- `json`, `yaml`, `toml` (config)
- `html`, `xml` (markup)

## Consequences

### Positive
- (+) Extensible: Users add new types without code changes
- (+) Content-aware processing: Optimal chunking per type
- (+) Model routing: Use best embedding model for each type
- (+) Detection: Auto-detect from filename or content
- (+) Auditability: Document types are explicit, queryable

### Negative
- (-) Schema complexity: New table, relationships
- (-) Migration: Existing notes need `document_type_id` backfill
- (-) Maintenance: Must keep detection rules current
- (-) Potential conflicts: Multiple types could match same file

## Implementation

**Code Location:**
- Schema: `migrations/YYYYMMDD_document_type_registry.sql`
- Models: `crates/matric-core/src/models.rs`
- Repository: `crates/matric-db/src/document_types.rs`
- API: `crates/matric-api/src/handlers/document_types.rs`

**Key Changes:**
- Add `document_type` table with seeded types
- Add `document_type_id` to `note` table (nullable, backfill later)
- Create `DocumentTypeRepository` trait and implementation
- Add REST endpoints: `GET/POST/PUT/DELETE /api/v1/document-types`
- Add detection endpoint: `POST /api/v1/detect-document-type`

## Implementation Notes

### Migration Files

The document type registry was implemented across five migration files:

1. **`20260202000000_document_types.sql`** - Base schema and core types (14 types)
   - Created `document_category` enum (19 categories)
   - Created `chunking_strategy` enum (7 strategies)
   - Created `document_type` table with comprehensive fields
   - Added `document_type_id` foreign key to `note` table
   - Seeded core types: markdown, plaintext, rust, python, typescript, javascript, go, java, json, yaml, toml, html, xml, sql

2. **`20260201600000_document_type_agentic_config.sql`** - AI generation metadata
   - Added `agentic_config` JSONB column for AI-enhanced document generation
   - Includes generation prompts, required sections, context requirements, validation rules
   - Seeded configs for OpenAPI, Rust, Markdown, Python, TypeScript

3. **`20260202200000_seed_technical_document_types.sql`** - Technical types (45 types)
   - API specifications: OpenAPI, GraphQL, Protobuf, AsyncAPI, JSON Schema
   - Infrastructure as Code: Terraform, Kubernetes, Dockerfile, Docker Compose, Ansible, CloudFormation, Helm
   - Database: SQL migrations, Prisma, Drizzle, SQLAlchemy, ERD
   - Shell scripts: Bash, Zsh, PowerShell, Makefile, Justfile, CMake, Gradle
   - Documentation: RST, AsciiDoc, Org Mode, LaTeX, Man pages, Jupyter, MDX, Docstrings
   - Package manifests: Cargo.toml, package.json, pyproject.toml, go.mod, pom.xml, Gemfile, requirements.txt
   - Observability: Log files, stack traces, error reports, metrics, trace JSON

4. **`20260202300000_seed_general_document_types.sql`** - General-purpose types (56 types)
   - Additional programming languages, configuration formats, data formats, etc.

5. **`20260202400000_seed_research_document_types.sql`** - Research/academic types (8 types)
   - Specialized types for research workflows: reference cards, literature reviews, etc.

**Total Document Types Seeded:** 123 system types

### Schema Enhancements Beyond Original Design

The implemented schema includes several enhancements not in the original proposal:

1. **Additional Fields:**
   - `filename_patterns` - Pattern matching for files without extensions (e.g., `Dockerfile`, `Makefile`)
   - `tree_sitter_language` - Maps to tree-sitter grammar names for syntactic parsing
   - `content_types` - Array tagging content nature (e.g., `['prose', 'technical']`, `['code']`)
   - `chunking_config` - JSONB for strategy-specific configuration options
   - `agentic_config` - JSONB for AI generation metadata (Issue #422)
   - `is_active` - Soft deletion/deactivation support

2. **Expanded Enums:**
   - `document_category`: 19 categories (vs. 5 in original proposal)
     - Added: `api-spec`, `iac`, `database`, `shell`, `docs`, `package`, `observability`, `legal`, `communication`, `research`, `creative`, `media`, `personal`, `custom`
   - `chunking_strategy`: 7 strategies (vs. 4 in original proposal)
     - Added: `per_section`, `per_unit`, `whole`

3. **Additional Indexes:**
   - GIN index on `filename_patterns` for fast pattern matching
   - GIN index on `agentic_config` for AI generation queries
   - Partial index on `is_active` for active document type filtering

### Integration with Other Systems

**Chunking System (ADR-027):**
- Document types specify `chunking_strategy` field
- `syntactic` strategy triggers tree-sitter parsing for code
- `tree_sitter_language` field maps to language grammars
- Integration point: chunking service queries document type to determine processing strategy

**Embedding System (ADR-022, ADR-024):**
- `recommended_config_id` links to embedding configurations
- Enables content-aware embedding model selection
- Auto-embed rules can filter by document type category
- Full embedding sets can specialize by document type

**Detection System:**
- Multi-stage detection: file extension → filename pattern → magic pattern → content analysis
- Detection priority: most specific match wins
- Graceful fallback to generic types when detection fails

### Deviations from Original Design

1. **No Detection Endpoint:** The proposed `POST /api/v1/detect-document-type` endpoint was not implemented. Detection occurs implicitly during note creation/update based on filename and content.

2. **System Types Immutable:** System types (`is_system = TRUE`) have restricted modification to prevent breaking built-in processing pipelines.

3. **No Backfill Migration:** The `document_type_id` column on the `note` table remains nullable. Backfilling existing notes requires separate migration based on file extension heuristics.

4. **Agentic Config Addition:** The `agentic_config` column was added post-implementation to support AI-enhanced document generation workflows (Issue #422), extending beyond the original scope.

## References

- Related: ADR-022 (Embedding Set Types), ADR-024 (Auto-Embed Rules), ADR-027 (Code-Aware Chunking)
- Implementation Issues: #397-#403 (document type categories), #411 (research types), #422 (agentic config)
- Stakeholder Request: REQ-CODE-001

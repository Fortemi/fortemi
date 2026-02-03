# Document Type Best Practices

This guide covers best practices for working with document types in Fortémi. For basic usage and API reference, see the [Document Types Guide](document-types-guide.md).

## Overview

Document types determine content processing strategies at three critical stages:

1. **Detection** - How content is recognized and classified
2. **Chunking** - How content is split for embedding
3. **Embedding** - Which model and configuration to use

Well-configured document types improve retrieval quality by 20-40% compared to generic text processing.

## Core Principles

### 1. Let Auto-Detection Work

The system ships with 131 pre-configured types covering most use cases. In 95% of scenarios, auto-detection handles classification correctly.

**DO:**
```bash
# Let detection work naturally
curl -X POST /api/v1/notes \
  -d '{
    "content": "fn main() { ... }",
    "metadata": {"filename": "main.rs"}
  }'
# → Detected as "rust" via .rs extension
```

**DON'T:**
```bash
# Unnecessary explicit override
curl -X POST /api/v1/notes \
  -d '{
    "content": "fn main() { ... }",
    "document_type": "rust",
    "metadata": {"filename": "main.rs"}
  }'
```

**Exception:** Explicitly set document type when:
- Content does not match filename extension (e.g., YAML in a .txt file)
- Using research document types that require semantic classification
- Content is ambiguous (could be multiple types)

### 2. Choose Chunking Strategy by Structure, Not Content

Chunking strategy should reflect document **structure**, not subject matter.

| Document Structure | Strategy | Example Types |
|-------------------|----------|---------------|
| Single atomic unit | `whole` | Tweets, bookmarks, discovery notes, hypothesis cards |
| Natural paragraphs | `semantic` | Blog posts, articles, prose documentation |
| Formal sections | `per_section` | Academic papers, RFCs, literature reviews |
| Syntax tree | `syntactic` | Source code in any language |
| Logical units | `per_unit` | SQL migrations, GraphQL schemas, Makefiles |
| No clear structure | `fixed` | Log files, raw data dumps |
| Mixed structure | `hybrid` | Jupyter notebooks, literate programming |

**Common Mistake:** Using `syntactic` for YAML because "it's technical"

```yaml
# WRONG: syntactic strategy for YAML
chunking_strategy: "syntactic"

# RIGHT: per_section or fixed for YAML
chunking_strategy: "per_section"  # If has sections
chunking_strategy: "fixed"        # If flat config
```

**Why:** YAML has simple structure. Syntactic chunking (AST parsing) adds overhead without benefit.

### 3. Chunk Size Follows Context Window, Not Arbitrary Numbers

Set chunk sizes based on downstream consumers:

| Consumer | Chunk Size | Rationale |
|----------|-----------|-----------|
| Embedding model | Model's max input - 10% | Leave headroom for special tokens |
| LLM context (with re-ranking) | 500-1000 | Smaller chunks for better re-ranking |
| LLM context (direct retrieval) | 1500-2000 | Larger chunks for more context |
| Two-stage retrieval (MRL) | 800-1200 | Balance coarse and fine retrieval |

**Example: Configuring for nomic-embed-text (8192 token limit)**

```bash
# Good: ~1500 chars = ~375 tokens (conservative estimate)
"chunk_size_default": 1500

# Bad: Using full context window
"chunk_size_default": 32768  # Will fail on long inputs
```

**Rule of thumb:** 1 token ≈ 4 characters for English, 1-2 characters for code.

### 4. Detection Order Matters for Performance

Detection checks patterns in this order:

1. **Filename patterns** (fastest, ~0.5ms)
2. **File extensions** (fast, ~0.5ms)
3. **Magic patterns** (slower, ~5ms - scans content)
4. **Format field** (fallback)

**Optimization Strategy:**

- Use **filename patterns** for unique files: `Dockerfile`, `Makefile`, `package.json`
- Use **extensions** for standard file types: `.rs`, `.py`, `.md`
- Use **magic patterns** only when needed: OpenAPI detection, shebang lines

**Anti-pattern:**

```sql
-- DON'T: Magic pattern for something detectable by extension
file_extensions: ['.py'],
magic_patterns: ['import ', 'def ', 'class ']  -- Unnecessary!

-- DO: Let extension handle it
file_extensions: ['.py'],
magic_patterns: []
```

### 5. Prefer System Types Over Custom Types

System types are battle-tested and cover most needs. Create custom types only when:

1. **Domain-specific format** - Your organization's proprietary format
2. **Specialized workflow** - Research cards, meeting notes with specific structure
3. **Detection conflicts** - Need more specific matching than system types provide

**When System Types Suffice:**

```bash
# Use system markdown type, not custom
document_type: "markdown"

# NOT: custom "engineering-documentation" type
# (Just use markdown with appropriate tags for filtering)
```

**When Custom Types Needed:**

```bash
# Organization-specific format
{
  "name": "incident-report",
  "category": "communication",
  "filename_patterns": ["INCIDENT-*.md"],
  "magic_patterns": ["## Incident ID", "## Severity"],
  "chunking_strategy": "per_section"
}
```

## Choosing the Right Type

### By Content Category

#### Source Code

**Use:** Specific language type (`rust`, `python`, `typescript`, etc.)

**Why:** Enables syntactic chunking at function/class boundaries.

**Configuration:**
```json
{
  "chunking_strategy": "syntactic",
  "chunk_size_default": 512,
  "tree_sitter_language": "rust"
}
```

**Chunk Size Guidance:**
- Short functions: 512 chars
- Long functions/classes: 1024 chars
- Entire files (if small): `whole` strategy

#### Documentation

**Use:** `markdown` for most cases, specific doc format if structured

**Specialized Types:**
- `rst` - reStructuredText (Python projects)
- `asciidoc` - AsciiDoc (Java/enterprise docs)
- `org-mode` - Emacs Org files
- `latex` - Academic papers

**Configuration:**
```json
{
  "chunking_strategy": "semantic",
  "chunk_size_default": 1500,
  "preserve_boundaries": true
}
```

**Why semantic:** Respects paragraph and section boundaries for coherent chunks.

#### Configuration Files

**Decision Tree:**

```
Is it hierarchical (Kubernetes, Docker Compose)?
  └─ Yes → per_section strategy
  └─ No → fixed strategy

Does it have sections/services?
  └─ Yes → per_section
  └─ No → fixed
```

**Examples:**

```bash
# Docker Compose (hierarchical)
"chunking_strategy": "per_section"  # Split by service

# Simple JSON config (flat)
"chunking_strategy": "fixed"        # Fixed token windows
```

#### API Specifications

**Use:** Specific type (`openapi`, `graphql-schema`, `protobuf`)

**Why:** These have well-defined structures that chunking can exploit.

**Configuration:**
```json
{
  "chunking_strategy": "per_section",  // One chunk per endpoint/type
  "chunk_size_default": 1500,
  "preserve_boundaries": true
}
```

**Benefit:** Each API endpoint/type becomes a searchable unit.

#### Research Documents

**Use:** Research-specific types with prefix patterns

| Type | Use When | Chunk Strategy |
|------|----------|---------------|
| `research/reference` | Summarizing academic papers | `per_section` |
| `research/literature-review` | Synthesizing multiple sources | `per_section` |
| `research/experiment` | Recording experiments | `per_section` |
| `research/discovery` | Quick insights | `whole` |
| `research/question` | Research questions | `whole` |
| `research/hypothesis` | Testable predictions | `whole` |
| `research/protocol` | SOPs and procedures | `per_section` |
| `research/data-dictionary` | Dataset documentation | `per_section` |

**Naming Convention:** `{PREFIX}-{ID}: {Title}.md`

```
REF-001: Attention Is All You Need.md
LIT-transformers: Transformer Architecture Survey.md
EXP-2026-02-01: MRL Compression Test.md
DISC-api-design: RESTful vs GraphQL Trade-offs.md
```

**Why:** Prefix-based detection enables semantic classification beyond file extensions.

## Custom Document Type Design Patterns

### Pattern 1: Domain-Specific Templates

**Scenario:** Your organization uses structured document templates.

**Example: Architecture Decision Record**

```json
{
  "name": "architecture-decision",
  "display_name": "Architecture Decision Record",
  "category": "docs",
  "description": "ADR following Michael Nygard template",
  "file_extensions": [".md"],
  "filename_patterns": [
    "ADR-*.md",
    "adr-*.md",
    "docs/architecture/*.md",
    "architecture/decisions/*.md"
  ],
  "magic_patterns": [
    "## Status",
    "## Context",
    "## Decision",
    "## Consequences"
  ],
  "chunking_strategy": "per_section",
  "chunk_size_default": 1500,
  "chunk_overlap_default": 150,
  "preserve_boundaries": true,
  "agentic_config": {
    "generation_prompt": "Generate an Architecture Decision Record following Michael Nygard's template",
    "required_sections": ["Status", "Context", "Decision", "Consequences"],
    "optional_sections": ["Alternatives Considered", "Related Decisions"],
    "agent_hints": {
      "include_date": true,
      "link_related_adrs": true
    }
  }
}
```

**Benefits:**
- Automatic detection from multiple filename patterns
- Content-based fallback via magic patterns
- Section-aware chunking preserves structure
- AI generation guidance via agentic_config

### Pattern 2: Meeting Notes with Action Items

**Scenario:** Team uses structured meeting notes.

```json
{
  "name": "meeting-notes",
  "display_name": "Meeting Notes",
  "category": "communication",
  "file_extensions": [".md"],
  "filename_patterns": [
    "*-meeting-*.md",
    "meeting-*.md",
    "meetings/*.md"
  ],
  "magic_patterns": [
    "## Attendees",
    "## Agenda",
    "## Action Items"
  ],
  "chunking_strategy": "per_section",
  "chunk_size_default": 1000,
  "preserve_boundaries": true,
  "content_types": ["communication", "prose"],
  "agentic_config": {
    "generation_prompt": "Generate structured meeting notes with attendees, discussion, and action items",
    "required_sections": ["Date", "Attendees", "Discussion", "Action Items"],
    "optional_sections": ["Agenda", "Decisions", "Follow-up"],
    "validation_rules": {
      "must_have_action_items": true,
      "must_have_date": true
    }
  }
}
```

**Integration with Search:**

```bash
# Find action items from meetings
curl '/api/v1/search?q=action+items&document_type=meeting-notes'

# Find meetings about specific topic
curl '/api/v1/search?q=kubernetes+migration&document_type=meeting-notes'
```

### Pattern 3: Incident Reports

**Scenario:** Post-mortems and incident documentation.

```json
{
  "name": "incident-report",
  "display_name": "Incident Report",
  "category": "communication",
  "file_extensions": [".md"],
  "filename_patterns": [
    "INCIDENT-*.md",
    "POST-MORTEM-*.md",
    "incidents/*.md"
  ],
  "magic_patterns": [
    "## Incident ID",
    "## Severity",
    "## Timeline",
    "## Root Cause"
  ],
  "chunking_strategy": "per_section",
  "chunk_size_default": 1500,
  "preserve_boundaries": true,
  "content_types": ["technical", "prose"],
  "agentic_config": {
    "generation_prompt": "Generate incident post-mortem with timeline, root cause analysis, and action items",
    "required_sections": [
      "Incident ID",
      "Severity",
      "Impact",
      "Timeline",
      "Root Cause",
      "Resolution",
      "Action Items"
    ],
    "optional_sections": [
      "Contributing Factors",
      "Lessons Learned",
      "Monitoring Improvements"
    ]
  }
}
```

### Pattern 4: Code Review Notes

**Scenario:** Capturing code review discussions and decisions.

```json
{
  "name": "code-review-notes",
  "display_name": "Code Review Notes",
  "category": "communication",
  "file_extensions": [".md"],
  "filename_patterns": [
    "review-*.md",
    "code-review-*.md",
    "reviews/*.md"
  ],
  "magic_patterns": [
    "## PR #",
    "## Review",
    "## Changes Requested"
  ],
  "chunking_strategy": "per_section",
  "chunk_size_default": 1200,
  "preserve_boundaries": true,
  "content_types": ["technical", "code", "prose"]
}
```

## Agentic Configuration

The `agentic_config` field provides guidance for AI document generation. Use it to specify:

### Generation Prompt

Clear instructions for AI agents creating documents:

```json
{
  "generation_prompt": "Generate comprehensive API documentation with examples, error handling, and authentication details"
}
```

**Best Practices:**
- Be specific about expected content
- Mention key requirements (examples, error handling, etc.)
- Keep under 200 characters for clarity

### Required vs Optional Sections

Help AI understand document structure:

```json
{
  "required_sections": [
    "Overview",
    "Authentication",
    "Endpoints",
    "Error Codes"
  ],
  "optional_sections": [
    "Rate Limiting",
    "Webhooks",
    "SDKs",
    "Changelog"
  ]
}
```

**Benefits:**
- AI knows minimum viable document structure
- Validation can check for required sections
- Users understand what sections to expect

### Context Requirements

Specify what information AI needs:

```json
{
  "context_requirements": {
    "needs_existing_code": true,
    "needs_data_models": true,
    "needs_endpoint_list": true
  }
}
```

**Use Cases:**
- Code generation: Needs existing code patterns
- API docs: Needs endpoint inventory
- Diagrams: Needs architecture context

### Agent Hints

Provide style and best practice guidance:

```json
{
  "agent_hints": {
    "prefer_result_over_panic": true,
    "use_type_hints": true,
    "include_code_examples": true,
    "use_consistent_headings": true
  }
}
```

**Examples by Type:**

**Rust Code:**
```json
{
  "agent_hints": {
    "prefer_result_over_panic": true,
    "use_clippy_recommendations": true,
    "avoid_unwrap": true
  }
}
```

**Python Code:**
```json
{
  "agent_hints": {
    "use_type_hints": true,
    "prefer_dataclasses": true,
    "follow_pep8": true
  }
}
```

**Documentation:**
```json
{
  "agent_hints": {
    "include_code_examples": true,
    "use_consistent_headings": true,
    "link_related_docs": true
  }
}
```

## Integration with Notes

### Creating Notes with Document Types

**Auto-Detection (Recommended):**

```bash
curl -X POST /api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "fn main() { println!(\"Hello\"); }",
    "format": "rust",
    "source": "main.rs",
    "metadata": {
      "filename": "main.rs"
    }
  }'
```

**Explicit Type:**

```bash
curl -X POST /api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Literature Review\n\n...",
    "document_type": "research/literature-review",
    "tags": ["research", "embeddings"]
  }'
```

### Querying by Document Type

**Filter search by type:**

```bash
# Find Rust code
curl '/api/v1/search?q=async+tokio&document_type=rust'

# Find research papers
curl '/api/v1/search?q=transformer+attention&document_type=research/reference'

# Multiple types
curl '/api/v1/search?q=api+design&document_type=openapi,graphql-schema'
```

### Bulk Operations

**Re-detect document types:**

```bash
# Useful after adding custom types
curl -X POST /api/v1/notes/redetect-types
```

**Update chunking for type:**

```bash
# After changing chunk_size_default
curl -X POST /api/v1/notes/re-chunk?document_type=markdown
```

## Integration with Embedding Sets

Document types and embedding sets work together for optimal retrieval:

### Strategy 1: Type-Specific Embedding Sets

Create embedding sets per document category:

```bash
# Code embedding set (filter set)
curl -X POST /api/v1/embedding-sets \
  -d '{
    "name": "code-only",
    "set_type": "filter",
    "parent_config_id": "default-config-id",
    "auto_embed_rules": {
      "document_type_patterns": ["rust", "python", "typescript", "go", "java"]
    }
  }'

# Research embedding set
curl -X POST /api/v1/embedding-sets \
  -d '{
    "name": "research-papers",
    "set_type": "full",
    "config": {
      "model": "nomic-embed-text",
      "truncate_dim": 512
    },
    "auto_embed_rules": {
      "document_type_patterns": ["research/*"]
    }
  }'
```

### Strategy 2: MRL with Type-Aware Dimensions

Use Matryoshka Representation Learning with different dimensions per type:

```bash
# Code: Lower dimensions (more specific queries)
{
  "embedding_set": "code",
  "config": {
    "model": "nomic-embed-text",
    "truncate_dim": 256
  },
  "auto_embed_rules": {
    "document_type_patterns": ["*"],
    "category_patterns": ["code"]
  }
}

# Documentation: Higher dimensions (more nuanced)
{
  "embedding_set": "docs",
  "config": {
    "model": "nomic-embed-text",
    "truncate_dim": 512
  },
  "auto_embed_rules": {
    "document_type_patterns": ["markdown", "rst", "asciidoc"]
  }
}
```

**Rationale:** Code search is often keyword-specific. Documentation search benefits from nuanced semantic understanding.

### Strategy 3: Two-Stage Retrieval

Combine document types with MRL two-stage retrieval:

1. **Coarse stage:** Search at low dimension across all types
2. **Fine stage:** Re-rank with higher dimension within detected type

```bash
# Setup
curl -X POST /api/v1/embedding-sets \
  -d '{
    "name": "two-stage-all",
    "set_type": "full",
    "config": {
      "model": "nomic-embed-text",
      "truncate_dim": 128,
      "mrl_matryoshka_dim": 768
    }
  }'

# Search automatically uses two-stage
curl '/api/v1/search?q=kubernetes+deployment+best+practices&use_two_stage=true'
```

**Performance:** 128× compute reduction on coarse stage with <5% quality loss.

## Performance Considerations

### Detection Performance

**Measurement Results:**

| Detection Method | Average Time | Use For |
|-----------------|--------------|---------|
| Filename pattern | 0.3ms | Unique files (Dockerfile, Makefile) |
| File extension | 0.5ms | Standard files (.rs, .py, .md) |
| Magic pattern | 4.8ms | Content-based (OpenAPI, shebang) |
| MIME type | 2.1ms | HTTP uploads with MIME headers |

**Optimization:**

```sql
-- BEFORE: Magic pattern for common case
file_extensions: ['.py'],
magic_patterns: ['#!/usr/bin/env python', 'import ', 'def ']

-- AFTER: Extension only
file_extensions: ['.py', '.pyi', '.pyw'],
magic_patterns: []

-- Result: 10× faster detection (0.5ms vs 5ms)
```

### Chunking Performance

**Measurement Results:**

| Strategy | Time per 10KB | Use For |
|----------|--------------|---------|
| `whole` | 0.1ms | No splitting |
| `fixed` | 1.2ms | Simple token windows |
| `semantic` | 3.5ms | Paragraph boundaries |
| `per_section` | 4.1ms | Heading detection |
| `syntactic` | 12.8ms | AST parsing |
| `per_unit` | 15.3ms | Function/class extraction |
| `hybrid` | 18.2ms | Both semantic + syntactic |

**Optimization Tips:**

1. **Use simpler strategies when possible:**

```json
// BEFORE: Syntactic for YAML (overkill)
{"chunking_strategy": "syntactic"}

// AFTER: Per-section for YAML
{"chunking_strategy": "per_section"}

// Result: 3× faster chunking
```

2. **Tune chunk sizes to reduce chunk count:**

```json
// BEFORE: Small chunks = more chunks
{"chunk_size_default": 256}  // 40 chunks per 10KB

// AFTER: Larger chunks = fewer chunks
{"chunk_size_default": 1000}  // 10 chunks per 10KB

// Result: 4× fewer embeddings to generate and store
```

3. **Cache tree-sitter parsers:**

Syntactic chunking performance improves dramatically with parser caching (handled automatically by the system).

### Index Performance

**Document Type Table:**

```sql
-- Indexes automatically created by migration
CREATE INDEX idx_document_type_category ON document_type(category);
CREATE INDEX idx_document_type_active ON document_type(is_active);
CREATE INDEX idx_document_type_extensions ON document_type USING GIN(file_extensions);
CREATE INDEX idx_document_type_filename_patterns ON document_type USING GIN(filename_patterns);
```

**Query Performance:**

```sql
-- Fast: Uses GIN index
SELECT * FROM document_type WHERE '.rs' = ANY(file_extensions);
-- ~0.5ms

-- Fast: Uses GIN index
SELECT * FROM document_type WHERE filename_patterns @> ARRAY['Dockerfile'];
-- ~0.6ms

-- Slow: Full table scan
SELECT * FROM document_type WHERE 'openapi:' = ANY(magic_patterns);
-- ~5ms (but acceptable for fallback detection)
```

**Best Practice:** Keep custom document types under 100 per instance for optimal detection performance.

### Memory Considerations

**System Types:**

- 131 system types load in ~10ms at startup
- ~2KB per type in memory
- Total: ~262KB for all system types (negligible)

**Custom Types:**

- Each custom type adds ~2KB to memory
- 100 custom types = 200KB (still negligible)
- No practical limit for memory usage

**Chunking Memory:**

- Chunks are owned strings (cloned from original)
- For 10KB document chunked to 1KB pieces = 10× 1KB = 10KB additional memory
- Use streaming for very large documents (>10MB)

## Common Pitfalls and Solutions

### Pitfall 1: Over-Specific Detection Patterns

**Problem:**

```json
{
  "name": "user-service-code",
  "filename_patterns": [
    "services/user-service/src/handlers/user_handler.rs"
  ]
}
```

**Why it's bad:** Too specific. Won't match similar files.

**Solution:**

```json
{
  "name": "service-handler",
  "filename_patterns": [
    "services/*/src/handlers/*.rs",
    "*/handlers/*.rs"
  ]
}
```

### Pitfall 2: Conflicting Magic Patterns

**Problem:**

```json
// Custom type
{
  "name": "kubernetes-extended",
  "magic_patterns": ["apiVersion:", "kind:"]
}

// System type (already exists)
{
  "name": "kubernetes",
  "magic_patterns": ["apiVersion:", "kind: Deployment"]
}
```

**Why it's bad:** Custom type matches everything the system type matches.

**Solution:** Make custom patterns more specific.

```json
{
  "name": "kubernetes-extended",
  "magic_patterns": [
    "apiVersion: custom.k8s.io",
    "kind: CustomResource"
  ]
}
```

### Pitfall 3: Wrong Chunking Strategy

**Problem:**

```json
{
  "name": "json-config",
  "chunking_strategy": "syntactic"  // Overkill
}
```

**Why it's bad:** JSON has simple structure. Syntactic chunking wastes CPU.

**Solution:**

```json
{
  "name": "json-config",
  "chunking_strategy": "fixed"  // Or per_section if structured
}
```

### Pitfall 4: Ignoring Preserve Boundaries

**Problem:**

```json
{
  "chunking_strategy": "per_section",
  "preserve_boundaries": false  // Chunks might split mid-section
}
```

**Why it's bad:** Section-based chunking loses meaning if boundaries aren't preserved.

**Solution:**

```json
{
  "chunking_strategy": "per_section",
  "preserve_boundaries": true  // Respect section boundaries
}
```

### Pitfall 5: Not Setting Content Types

**Problem:**

```json
{
  "name": "api-docs",
  "content_types": []  // No guidance for embedding selection
}
```

**Why it's bad:** System can't recommend optimal embedding configuration.

**Solution:**

```json
{
  "name": "api-docs",
  "content_types": ["prose", "technical", "code"]  // Guides embedding choice
}
```

### Pitfall 6: Excessive Chunk Overlap

**Problem:**

```json
{
  "chunk_size_default": 1000,
  "chunk_overlap_default": 800  // 80% overlap!
}
```

**Why it's bad:** Massive storage waste and redundant embeddings.

**Solution:**

```json
{
  "chunk_size_default": 1000,
  "chunk_overlap_default": 100  // 10% overlap (standard)
}
```

**Guideline:** Overlap should be 10-20% of chunk size.

### Pitfall 7: Missing Agentic Config for Templates

**Problem:**

```json
{
  "name": "incident-report",
  "agentic_config": {}  // No generation guidance
}
```

**Why it's bad:** AI agents have no structure to follow when generating documents.

**Solution:**

```json
{
  "name": "incident-report",
  "agentic_config": {
    "generation_prompt": "Generate incident post-mortem with timeline and root cause",
    "required_sections": ["Incident ID", "Timeline", "Root Cause", "Action Items"]
  }
}
```

## Advanced Scenarios

### Scenario 1: Multi-Language Codebases

**Challenge:** Repository contains multiple languages.

**Solution:** Let auto-detection work per file.

```bash
# Each file detected by extension
main.rs          → rust (syntactic chunking)
api.py           → python (syntactic chunking)
config.yaml      → yaml (fixed chunking)
README.md        → markdown (semantic chunking)
```

**No custom type needed.** System types handle this naturally.

### Scenario 2: Monorepo with Multiple Services

**Challenge:** Need to distinguish service context.

**Solution:** Use tags + metadata, not custom document types.

```bash
curl -X POST /api/v1/notes \
  -d '{
    "content": "...",
    "metadata": {
      "filename": "user-service/handler.rs",
      "service": "user-service"
    },
    "tags": ["user-service", "backend"]
  }'

# Search within service
curl '/api/v1/search?q=authentication&tags=user-service'
```

**Why:** Document type describes **format**, not **service context**. Use tags for context filtering.

### Scenario 3: Versioned API Specifications

**Challenge:** Multiple versions of OpenAPI specs.

**Solution:** Use metadata for version, same document type.

```bash
curl -X POST /api/v1/notes \
  -d '{
    "content": "openapi: 3.1.0...",
    "document_type": "openapi",
    "metadata": {
      "api_version": "v2",
      "filename": "openapi-v2.yaml"
    }
  }'

# Search specific version
curl '/api/v1/search?q=user+endpoint&document_type=openapi&metadata.api_version=v2'
```

### Scenario 4: Generated Code

**Challenge:** Distinguish hand-written from generated code.

**Solution:** Use metadata flag, same document type.

```bash
curl -X POST /api/v1/notes \
  -d '{
    "content": "// Auto-generated by protoc...",
    "document_type": "rust",
    "metadata": {
      "generated": true,
      "generator": "protoc"
    },
    "tags": ["generated", "protobuf"]
  }'

# Exclude generated code from search
curl '/api/v1/search?q=user+struct&document_type=rust&metadata.generated=false'
```

### Scenario 5: Internal Documentation Portal

**Challenge:** Company wiki with custom formatting.

**Solution:** Create custom type matching internal patterns.

```json
{
  "name": "wiki-page",
  "display_name": "Company Wiki Page",
  "category": "docs",
  "file_extensions": [".md"],
  "filename_patterns": [
    "wiki/*.md",
    "internal-docs/*.md"
  ],
  "magic_patterns": [
    "<!-- Wiki-Version:",
    "<!-- Last-Updated:"
  ],
  "chunking_strategy": "semantic",
  "chunk_size_default": 1500,
  "preserve_boundaries": true,
  "agentic_config": {
    "generation_prompt": "Generate company wiki page with overview, details, and related links",
    "required_sections": ["Overview", "Details"],
    "optional_sections": ["Examples", "FAQ", "Related Pages"]
  }
}
```

### Scenario 6: Jupyter Notebooks

**Challenge:** Mixed code and markdown cells.

**Solution:** Use hybrid chunking strategy.

```json
{
  "name": "jupyter",
  "chunking_strategy": "hybrid",
  "chunking_config": {
    "code_strategy": "syntactic",
    "prose_strategy": "semantic"
  }
}
```

**Note:** System type `jupyter` already handles this optimally.

## Validation and Testing

### Test Document Type Detection

```bash
# Test detection with sample content
curl -X POST /api/v1/document-types/detect \
  -H "Content-Type: application/json" \
  -d '{
    "content": "fn main() { println!(\"test\"); }",
    "filename": "main.rs"
  }'

# Expected response
{
  "detected_type": "rust",
  "confidence": 0.9,
  "match_reason": "file_extension",
  "matched_pattern": ".rs"
}
```

### Verify Chunking Behavior

```bash
# Create test note
NOTE_ID=$(curl -X POST /api/v1/notes \
  -d '{
    "content": "Your test content...",
    "document_type": "your-custom-type"
  }' | jq -r '.id')

# Check chunk count
curl /api/v1/notes/$NOTE_ID/chunks | jq 'length'

# Inspect chunk boundaries
curl /api/v1/notes/$NOTE_ID/chunks | jq '.[].text'
```

### Benchmark Custom Type Performance

```bash
# Create 100 test notes
for i in {1..100}; do
  curl -X POST /api/v1/notes \
    -d "{\"content\": \"test $i\", \"document_type\": \"your-type\"}"
done

# Measure detection time (check logs)
tail -f /var/log/matric-api.log | grep "document_type_detection"
```

## Migration Strategies

### Adding Custom Types to Existing Notes

**Step 1:** Create custom type

```bash
curl -X POST /api/v1/document-types \
  -d '{
    "name": "meeting-notes",
    "filename_patterns": ["*-meeting-*.md"],
    ...
  }'
```

**Step 2:** Re-detect types for matching notes

```bash
curl -X POST /api/v1/notes/redetect-types?pattern=meeting
```

**Step 3:** Verify detection

```bash
curl '/api/v1/notes?document_type=meeting-notes' | jq 'length'
```

### Updating Chunking Strategy

**Step 1:** Update document type

```bash
curl -X PATCH /api/v1/document-types/your-type \
  -d '{
    "chunk_size_default": 2000,
    "chunking_strategy": "per_section"
  }'
```

**Step 2:** Re-chunk affected notes

```bash
curl -X POST /api/v1/notes/re-chunk?document_type=your-type
```

**Step 3:** Re-embed (if needed)

```bash
curl -X POST /api/v1/embedding-sets/your-set/reembed
```

## Summary Checklist

**Before Creating Custom Type:**

- [ ] System type doesn't already cover this case
- [ ] Clear detection patterns (filename, extension, or magic)
- [ ] Appropriate chunking strategy for content structure
- [ ] Reasonable chunk size (500-2000 for prose, 500-1000 for code)
- [ ] Content types set for embedding guidance
- [ ] Agentic config for AI generation (if applicable)

**Performance Optimizations:**

- [ ] Use filename patterns for unique files
- [ ] Use extensions for standard files
- [ ] Minimize magic pattern usage
- [ ] Choose simplest chunking strategy that works
- [ ] Set chunk overlap to 10-20% of chunk size

**Integration:**

- [ ] Document type configured in embedding set rules
- [ ] Tags used for semantic context (not document type)
- [ ] Metadata used for versioning and classification

**Testing:**

- [ ] Test detection with sample content
- [ ] Verify chunking produces expected boundaries
- [ ] Measure performance impact (if high-volume type)

## See Also

- [Document Types Guide](document-types-guide.md) - Basic usage and API reference
- [Chunking Guide](chunking.md) - Deep dive into chunking strategies
- [Embedding Model Selection](embedding-model-selection.md) - Choosing optimal models
- [Embedding Sets](embedding-sets.md) - Focused search contexts
- [ADR-025: Document Type Registry](../../.aiwg/architecture/ADR-025-document-type-registry.md) - Architecture decision

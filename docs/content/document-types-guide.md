# Document Type Best Practices Guide

This guide covers working with document types in Fortémi - from auto-detection to custom types to optimizing chunking strategies.

## Overview

Document types determine how content is:
1. **Detected** - Recognized from filename, extension, or content patterns
2. **Chunked** - Split into embeddable pieces using type-appropriate strategies
3. **Embedded** - Processed with optimal embedding models and dimensions

Fortémi includes 131 pre-configured types across 20 categories.

## Auto-Detection: Let It Work

For most use cases, auto-detection handles document types perfectly:

```bash
# Creating a note - type auto-detected from filename
curl -X POST /api/v1/notes \
  -d '{
    "content": "fn main() { ... }",
    "format": "rust",
    "source": "src/main.rs",
    "metadata": {"filename": "main.rs"}
  }'
# → document_type: "rust" (detected from .rs extension)
```

### Detection Priority

1. **Filename pattern** (confidence: 1.0)
   - `Dockerfile` → dockerfile
   - `docker-compose.yml` → docker-compose

2. **File extension** (confidence: 0.9)
   - `.rs` → rust
   - `.md` → markdown

3. **Content magic** (confidence: 0.7)
   - `openapi: 3.1` in content → openapi
   - `#!/bin/bash` → bash

4. **Format field** (confidence: 0.5)
   - format: "python" → python

5. **Default** (confidence: 0.1)
   - plaintext

## When to Set Document Type Explicitly

Explicitly set document type when:

1. **Content does not match filename** - A `.txt` file containing YAML
2. **Research documents** - Use specific types like `research/literature-review`, `research/question`
3. **Custom content** - Your organization's proprietary formats
4. **Ambiguous content** - Could be multiple types

```bash
# Explicitly set document type
curl -X POST /api/v1/notes \
  -d '{
    "content": "...",
    "document_type": "research/literature-review"
  }'
```

## Chunking Strategies

| Strategy | Best For | How It Works |
|----------|----------|--------------|
| `semantic` | Prose, documentation | Splits on paragraph/section boundaries |
| `syntactic` | Source code | Uses language-aware parsing (functions, classes) |
| `fixed` | Logs, raw data | Fixed token windows with overlap |
| `hybrid` | Mixed content | Combines semantic and syntactic |
| `per_section` | Structured docs | Splits on headings/sections |
| `per_unit` | Records | One chunk per logical unit |
| `whole` | Atomic content | No splitting (tweets, bookmarks) |

### Choosing the Right Strategy

```
Is it source code?
  └─ Yes → syntactic
  └─ No → Is it prose with sections?
            └─ Yes → semantic or per_section
            └─ No → Is it a single atomic unit?
                      └─ Yes → whole
                      └─ No → fixed
```

## Creating Custom Document Types

Create custom types for specialized content:

```bash
curl -X POST /api/v1/document-types \
  -d '{
    "name": "meeting-notes",
    "display_name": "Meeting Notes",
    "category": "communication",
    "description": "Team meeting notes with action items",
    "file_extensions": [".meeting.md"],
    "filename_patterns": ["*-meeting-*.md", "meeting-*.md"],
    "magic_patterns": ["## Attendees", "## Action Items"],
    "chunking_strategy": "per_section",
    "chunk_size_default": 1500
  }'
```

### Custom Type Guidelines

1. **Use specific patterns** - `meeting-*.md` better than `*.md`
2. **Add magic patterns** - Content markers improve detection
3. **Choose appropriate chunking** - Match your content structure
4. **Set reasonable chunk sizes** - 1000-2000 for prose, 500-1000 for code

## Categories Reference

| Category | Count | Examples |
|----------|-------|----------|
| code | 14 | rust, python, typescript, go, java, c, cpp |
| prose | 2 | markdown, plaintext |
| config | 3 | yaml, toml, json |
| api-spec | 5 | openapi, asyncapi, graphql-schema, protobuf, json-schema |
| iac | 7 | terraform, kubernetes, dockerfile, docker-compose, ansible, cloudformation, helm |
| database | 5 | sql, sql-migration, prisma, drizzle, sqlalchemy, erd |
| shell | 7 | bash, zsh, powershell, makefile, justfile, cmake, gradle |
| docs | 8 | rst, asciidoc, org-mode, latex, man-page, jupyter, mdx, docstring |
| package | 7 | cargo-toml, package-json, pyproject, gemfile, go-mod, pom-xml, composer-json |
| observability | 4 | log-file, prometheus-config, grafana-dashboard, opentelemetry-config |
| legal | 8 | contract, policy, proposal, invoice, report, sow, nda, terms-of-service |
| communication | 8 | email, email-thread, chat-log, slack-export, discord-log, meeting-notes, transcript, standup |
| research | 16 | academic-paper, arxiv, patent, thesis, citation, literature-review, research-note, whitepaper, research/reference, research/literature-review, research/experiment, research/discovery, research/question, research/hypothesis, research/protocol, research/data-dictionary |
| creative | 8 | blog-post, article, newsletter, press-release, social-post, ad-copy, script, book-chapter |
| media | 8 | image, image-with-text, screenshot, diagram, audio, video, podcast, presentation |
| personal | 8 | daily-note, journal, bookmark, highlight, annotation, todo-list, recipe, reading-list |
| data | 8 | csv, excel, parquet-schema, avro-schema, xml-data, ndjson, geojson, ical |
| markup | 2 | html, xml |
| agentic | 8 | agent-prompt, agent-skill, agent-workflow, mcp-tool, rag-context, ai-conversation, fine-tune-data, evaluation-set |
| custom | 0 | User-defined types |

## Research Document Types

Fortémi includes specialized document types for research workflows:

| Type | Prefix | Use Case | Chunking |
|------|--------|----------|----------|
| Reference Card | REF | Academic paper summaries with citations | per_section |
| Literature Review | LIT | Thematic analysis and synthesis | per_section |
| Experiment Log | EXP | Structured experimental records | per_section |
| Discovery Note | DISC | Quick-capture insights | whole |
| Research Question | RQ | Research questions with context | whole |
| Hypothesis Card | HYP | Testable predictions | whole |
| Protocol | PROT | Standard operating procedures | per_section |
| Data Dictionary | DATA | Dataset documentation | per_section |

**Naming Convention:** `{PREFIX}-{NUMBER|SLUG}: {TITLE}`

**Examples:**
- `REF-001: Attention Is All You Need.md`
- `LIT-transformers: Transformer Architecture Survey.md`
- `EXP-2024-02-01: Embedding Model Comparison.md`
- `DISC-mrl-compression: MRL enables 12x storage savings.md`

## Troubleshooting

### Wrong Type Detected

**Problem:** File detected as wrong type

**Solution:** Set explicit document_type or add more specific patterns

```bash
# Option 1: Set explicit type
curl -X POST /api/v1/notes -d '{"content": "...", "document_type": "yaml"}'

# Option 2: Create custom type with better patterns
curl -X POST /api/v1/document-types -d '{
  "name": "my-yaml-config",
  "file_extensions": [".yaml", ".yml"],
  "filename_patterns": ["config/*.yaml", "*.config.yaml"],
  "chunking_strategy": "per_section"
}'
```

### Chunks Too Large/Small

**Problem:** Content chunks do not fit context windows well

**Solution:** Create custom type with adjusted `chunk_size_default`

```bash
curl -X POST /api/v1/document-types -d '{
  "name": "large-documentation",
  "category": "prose",
  "chunking_strategy": "semantic",
  "chunk_size_default": 2000,
  "chunk_overlap_default": 200
}'
```

### Custom Type Not Matching

**Problem:** Custom type never detected

**Check:**
1. Patterns are specific enough
2. Magic patterns appear early in content (first 500 characters)
3. No higher-priority built-in type matches first

**Debug detection:**
```bash
# Test detection
curl -X POST /api/v1/document-types/detect \
  -d '{
    "content": "...",
    "filename": "test.txt"
  }'
```

## API Reference

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/document-types` | GET | List all types |
| `/api/v1/document-types` | POST | Create custom type |
| `/api/v1/document-types/:name` | GET | Get type details |
| `/api/v1/document-types/:name` | PATCH | Update type |
| `/api/v1/document-types/:name` | DELETE | Delete custom type |
| `/api/v1/document-types/detect` | POST | Auto-detect type |

### List Document Types

```bash
# List all types
curl /api/v1/document-types

# Filter by category
curl /api/v1/document-types?category=research

# System types only
curl /api/v1/document-types?is_system=true

# Custom types only
curl /api/v1/document-types?is_system=false
```

### Get Document Type Details

```bash
# Get by name
curl /api/v1/document-types/rust

# Response includes detection rules and chunking config
{
  "id": "...",
  "name": "rust",
  "display_name": "Rust",
  "category": "code",
  "description": "Rust programming language",
  "file_extensions": [".rs"],
  "chunking_strategy": "syntactic",
  "tree_sitter_language": "rust",
  "chunk_size_default": 512,
  "chunk_overlap_default": 50,
  "is_system": true
}
```

### Create Custom Document Type

```bash
curl -X POST /api/v1/document-types \
  -H "Content-Type: application/json" \
  -d '{
    "name": "architecture-decision",
    "display_name": "Architecture Decision Record",
    "category": "docs",
    "description": "ADR documents following Michael Nygard template",
    "file_extensions": [".md"],
    "filename_patterns": ["ADR-*.md", "adr-*.md", "*/architecture/*.md"],
    "magic_patterns": ["## Status", "## Context", "## Decision"],
    "chunking_strategy": "per_section",
    "chunk_size_default": 1500,
    "chunk_overlap_default": 150
  }'
```

### Update Document Type

```bash
# Only custom types can be updated
curl -X PATCH /api/v1/document-types/my-custom-type \
  -H "Content-Type: application/json" \
  -d '{
    "chunk_size_default": 2000
  }'
```

### Delete Document Type

```bash
# Only custom types can be deleted
curl -X DELETE /api/v1/document-types/my-custom-type
```

### Auto-Detect Document Type

```bash
curl -X POST /api/v1/document-types/detect \
  -H "Content-Type: application/json" \
  -d '{
    "content": "fn main() { println!(\"Hello\"); }",
    "filename": "main.rs"
  }'

# Response
{
  "detected_type": "rust",
  "confidence": 0.9,
  "match_reason": "file_extension",
  "matched_pattern": ".rs"
}
```

## MCP Integration

Document types integrate with Claude Code via MCP tools:

```javascript
// List document types
mcp.call("Fortémi", "document-type-list", {
  category: "research"
})

// Create custom type
mcp.call("Fortémi", "document-type-create", {
  name: "meeting-notes",
  display_name: "Meeting Notes",
  category: "communication",
  chunking_strategy: "per_section"
})

// Auto-detect type
mcp.call("Fortémi", "document-type-detect", {
  content: "...",
  filename: "test.py"
})
```

## Performance Considerations

### Detection Performance

- **Filename patterns:** <1ms (fastest, use for unique files like "Dockerfile")
- **File extensions:** <1ms (fast, use for standard extensions like ".rs")
- **Magic patterns:** <5ms (slower, scans first 500 chars of content)

**Optimization tip:** Use filename patterns for unique filenames, extensions for standard cases.

### Chunking Performance

| Strategy | Performance | Use Case |
|----------|-------------|----------|
| whole | Instant (no splitting) | Small atomic documents |
| fixed | Fast (simple token count) | Logs, raw data |
| semantic | Medium (paragraph boundaries) | Prose, documentation |
| per_section | Medium (heading detection) | Structured docs |
| syntactic | Slow (AST parsing) | Source code |
| per_unit | Slow (AST parsing) | Functions, classes |
| hybrid | Slowest (both passes) | Mixed content |

**Optimization tip:** Use simpler strategies when possible. Reserve syntactic/hybrid for code.

### Index Performance

- 123 types load in <10ms from database
- Detection uses GIN indexes for array matching
- Recommend limiting custom types to <100 per instance

## See Also

- [ADR-025: Document Type Registry](../../.aiwg/architecture/ADR-025-document-type-registry.md) - Architecture decision record
- [Embedding Model Selection](embedding-model-selection.md) - Choosing embedding models by content type
- [API Documentation](api.md) - Full REST API reference
- [MCP Server Guide](mcp.md) - Claude integration

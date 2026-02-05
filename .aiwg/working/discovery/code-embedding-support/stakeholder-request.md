# Stakeholder Request: Code & Technical Document Embedding Support

**Request ID**: REQ-CODE-001
**Date**: 2026-02-01
**Stakeholder**: Architecture Team
**Priority**: Must Have

## Business Value

Enable Matric Memory to serve as an AI-enhanced knowledge base for software development, supporting self-maintenance capabilities where the system can understand, index, and retrieve its own codebase and technical documentation.

## Desired Outcomes

1. **Code Search** - Semantic search across source code files (Rust, Python, TypeScript, etc.)
2. **Technical Doc Search** - API docs, ADRs, README files, configuration files
3. **Self-Maintenance** - System can query its own codebase for bug fixes and feature development
4. **Flexibility** - Support for any embedding model without code changes

## Current State

- 4 general-purpose text embedding models configured
- `format` field on notes is a free-form string (typically "markdown")
- No code-specific chunking or embedding strategies
- No API to dynamically add embedding configurations

## Gap Analysis

| Capability | Current | Needed |
|------------|---------|--------|
| Code-aware chunking | No | Yes - respect function/class boundaries |
| Code embedding models | No | Yes - code-optimized models |
| Language detection | No | Yes - route content appropriately |
| Dynamic config API | No | Yes - add models at runtime |
| Format/doctype registry | No | Yes - structured content types |

## Acceptance Criteria

1. [ ] Support at least 3 code embedding models (local Ollama + cloud options)
2. [ ] Code-aware chunking that respects syntax boundaries
3. [ ] API endpoint to register new embedding configurations
4. [ ] Content-type detection and routing
5. [ ] Document type registry with associated embedding strategies
6. [ ] Self-maintenance demo: system queries its own Rust codebase

## Related Issues

- Builds on #384-#389 (Full Embedding Sets)
- Enables future: AI-assisted code review, automated documentation

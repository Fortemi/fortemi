# ADR-027: Code-Aware Chunking with Tree-sitter

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team

## Context

Current text chunking uses a naive approach:
1. Split text into tokens
2. Create chunks of N tokens with M overlap
3. No awareness of content structure

This produces poor results for code:
- Functions split mid-implementation
- Class definitions separated from methods
- Import blocks merged with unrelated code
- Comments disconnected from the code they document

For effective code search, chunks must respect syntactic boundaries.

## Decision

Integrate **tree-sitter** for language-aware parsing and implement syntax-aware chunking:

**Architecture:**
```
┌─────────────────────────────────────────────────────────┐
│                    ChunkingService                       │
├─────────────────────────────────────────────────────────┤
│  detect_strategy(content, document_type) → Strategy      │
│  chunk(content, strategy, config) → Vec<Chunk>           │
└─────────────────────────────────────────────────────────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    ┌────────────┐  ┌────────────┐  ┌────────────┐
    │  Semantic  │  │  Syntactic │  │   Fixed    │
    │  Chunker   │  │  Chunker   │  │  Chunker   │
    │  (prose)   │  │(tree-sitter)│  │ (fallback) │
    └────────────┘  └────────────┘  └────────────┘
```

**Syntactic Chunking Algorithm:**
1. Parse source with tree-sitter for the detected language
2. Extract top-level units: functions, classes, modules, constants
3. For each unit:
   - If fits in chunk size → single chunk
   - If too large → split at nested boundaries (methods, blocks)
   - Preserve leading comments/docstrings with their target
4. Add context prefix: file path, parent scope

**Chunk Structure:**
```rust
pub struct CodeChunk {
    pub content: String,
    pub context: ChunkContext,
    pub span: Range<usize>,         // Byte range in original
    pub unit_kind: CodeUnitKind,    // Function, Class, Module, etc.
    pub unit_name: Option<String>,  // e.g., "parse_config"
    pub language: String,
}

pub struct ChunkContext {
    pub file_path: Option<String>,
    pub module_path: Vec<String>,   // e.g., ["matric_core", "models"]
    pub parent_name: Option<String>, // e.g., "impl EmbeddingSet"
}
```

**Supported Languages (tree-sitter grammars):**
- Rust, Python, TypeScript/JavaScript, Go, Java
- C/C++, Ruby, PHP, Swift, Kotlin
- JSON, YAML, TOML (config files)
- Markdown, HTML (markup)

## Consequences

### Positive
- (+) Semantic integrity: Chunks contain complete logical units
- (+) Better retrieval: Search finds whole functions, not fragments
- (+) Context preservation: Know where code came from
- (+) Language-agnostic: Tree-sitter handles 40+ languages
- (+) Accurate: AST-based, not regex heuristics

### Negative
- (-) Dependency: Tree-sitter crate and grammar binaries
- (-) Build complexity: Grammar compilation for each language
- (-) Performance: Parsing overhead vs. naive chunking
- (-) Edge cases: Malformed code, mixed content files

## Implementation

**Code Location:**
- Chunking service: `crates/matric-inference/src/chunking/`
- Tree-sitter integration: `crates/matric-inference/src/chunking/syntactic.rs`
- Language configs: `crates/matric-inference/src/chunking/languages/`

**Dependencies:**
```toml
[dependencies]
tree-sitter = "0.20"
tree-sitter-rust = "0.20"
tree-sitter-python = "0.20"
tree-sitter-typescript = "0.20"
# ... additional grammars
```

**Key Changes:**
- Add `ChunkingService` trait with strategy dispatch
- Implement `SyntacticChunker` using tree-sitter
- Update `EmbeddingService` to use chunking service
- Add language detection (file extension + content analysis)
- Expose chunking preview in API for debugging

**Performance Targets:**
- Parse + chunk 10K line file: < 100ms
- Memory: < 50MB for largest supported file
- Graceful fallback: If parse fails, use fixed chunking

## References

- [Tree-sitter documentation](https://tree-sitter.github.io/)
- Related: ADR-025 (Document Types), ADR-026 (Dynamic Configs)
- Stakeholder Request: REQ-CODE-001

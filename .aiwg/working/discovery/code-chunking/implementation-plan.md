# Code-Aware Chunking with Tree-sitter: Implementation Plan

**Issue:** #393
**Date:** 2026-02-01
**Status:** Research Complete

## Executive Summary

This plan outlines the implementation of syntax-aware code chunking using Tree-sitter for the matric-memory project. The feature will enable intelligent splitting of source code at semantic boundaries (functions, classes, modules) rather than arbitrary token counts, significantly improving embedding quality for code search.

## Current State Analysis

### Existing Chunking Infrastructure

**Location:** `crates/matric-db/src/chunking.rs`

The codebase has a well-designed chunking module with:

1. **`Chunker` trait** (line 109-115):
   ```rust
   pub trait Chunker: Send + Sync {
       fn chunk(&self, text: &str) -> Vec<Chunk>;
       fn config(&self) -> &ChunkerConfig;
   }
   ```

2. **`Chunk` struct** (line 59-69):
   ```rust
   pub struct Chunk {
       pub text: String,
       pub start_offset: usize,
       pub end_offset: usize,
       pub metadata: HashMap<String, String>,
   }
   ```

3. **Five existing chunkers:**
   - `SentenceChunker` - Sentence boundary detection
   - `ParagraphChunker` - Double-newline splitting
   - `SemanticChunker` - Markdown-aware (headings, lists, code blocks)
   - `SlidingWindowChunker` - Fixed-size with overlap
   - `RecursiveChunker` - Hierarchical fallback

4. **`ChunkerConfig`** (line 37-46):
   ```rust
   pub struct ChunkerConfig {
       pub max_chunk_size: usize,   // Default: 1000
       pub min_chunk_size: usize,   // Default: 100
       pub overlap: usize,          // Default: 100
   }
   ```

### Chunking Service Layer

**Location:** `crates/matric-api/src/services/chunking_service.rs`

The `ChunkingService` wraps chunkers with tokenization:
```rust
pub struct ChunkingService {
    chunker: SemanticChunker,
    tokenizer: Box<dyn Tokenizer>,
}
```

Key methods:
- `should_chunk(content, limit) -> bool` - Token-based threshold check
- `chunk_document(content) -> Vec<Chunk>` - Delegates to SemanticChunker

### Integration Points

1. **Embedding Repository** (`crates/matric-db/src/embeddings.rs`):
   - `store(note_id, chunks: Vec<(String, Vector)>, model)` - Stores chunk embeddings
   - `utils::chunk_text()` - Simple character-based chunking (legacy)

2. **Latency Module** (`crates/matric-inference/src/latency.rs`):
   - Defines `ChunkingStrategy` enum
   - `ContextOptimizer` uses chunking for context window management

3. **Job Handlers** (`crates/matric-jobs/src/handler.rs`):
   - Embedding jobs use chunking via handlers

### Existing ADRs

- **ADR-025** (Document Type Registry): Defines `chunking_strategy` field with values: `semantic`, `syntactic`, `fixed`, `hybrid`
- **ADR-027** (Code-Aware Chunking): Proposed architecture for Tree-sitter integration

---

## Proposed Architecture

### 1. New Data Structures

```rust
// File: crates/matric-db/src/chunking/code_types.rs

use std::ops::Range;

/// Kind of code unit extracted by Tree-sitter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeUnitKind {
    Function,
    Method,
    Class,
    Struct,
    Trait,
    Impl,
    Module,
    Constant,
    Variable,
    Import,
    Comment,
    Docstring,
    Unknown,
}

impl CodeUnitKind {
    /// Returns true if this kind represents a container for other code
    pub fn is_container(&self) -> bool {
        matches!(self, Self::Class | Self::Struct | Self::Trait | Self::Impl | Self::Module)
    }

    /// Returns true if this kind should be attached to the next unit
    pub fn is_leading(&self) -> bool {
        matches!(self, Self::Comment | Self::Docstring)
    }
}

/// A chunk of code with semantic context
#[derive(Debug, Clone)]
pub struct CodeChunk {
    /// The actual text content
    pub content: String,
    /// Contextual information about where this code came from
    pub context: ChunkContext,
    /// Byte range in original source
    pub span: Range<usize>,
    /// Type of code unit
    pub unit_kind: CodeUnitKind,
    /// Name of the unit if available (e.g., "parse_config", "UserService")
    pub unit_name: Option<String>,
    /// Programming language
    pub language: String,
    /// Nesting depth (0 = top-level)
    pub depth: u32,
}

impl CodeChunk {
    /// Convert to generic Chunk for storage compatibility
    pub fn to_chunk(&self) -> crate::chunking::Chunk {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("type".to_string(), "code".to_string());
        metadata.insert("unit_kind".to_string(), format!("{:?}", self.unit_kind));
        metadata.insert("language".to_string(), self.language.clone());
        if let Some(ref name) = self.unit_name {
            metadata.insert("unit_name".to_string(), name.clone());
        }
        if let Some(ref file) = self.context.file_path {
            metadata.insert("file_path".to_string(), file.clone());
        }
        if let Some(ref parent) = self.context.parent_name {
            metadata.insert("parent".to_string(), parent.clone());
        }

        crate::chunking::Chunk::with_metadata(
            self.content.clone(),
            self.span.start,
            self.span.end,
            metadata,
        )
    }
}

/// Context about where a code chunk originated
#[derive(Debug, Clone, Default)]
pub struct ChunkContext {
    /// Optional file path for provenance
    pub file_path: Option<String>,
    /// Module path components (e.g., ["matric_core", "models"])
    pub module_path: Vec<String>,
    /// Parent container name (e.g., "impl EmbeddingSet", "class UserService")
    pub parent_name: Option<String>,
    /// Line number in original file
    pub start_line: Option<u32>,
    /// End line number
    pub end_line: Option<u32>,
}

impl ChunkContext {
    /// Create a prefix string for embedding context
    pub fn to_prefix(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref file) = self.file_path {
            parts.push(format!("File: {}", file));
        }

        if !self.module_path.is_empty() {
            parts.push(format!("Module: {}", self.module_path.join("::")));
        }

        if let Some(ref parent) = self.parent_name {
            parts.push(format!("In: {}", parent));
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!("// {}\n", parts.join(" | "))
        }
    }
}
```

### 2. Chunking Service Trait

```rust
// File: crates/matric-db/src/chunking/service.rs

use super::{Chunk, ChunkerConfig, CodeChunk};

/// Strategy for chunking content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkingStrategy {
    /// Markdown-aware chunking for prose
    Semantic,
    /// Syntax-aware chunking using Tree-sitter
    Syntactic,
    /// Fixed-size sliding window
    Fixed,
    /// Combine semantic + syntactic for mixed content
    Hybrid,
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self::Semantic
    }
}

/// Configuration for the chunking service
#[derive(Debug, Clone)]
pub struct ChunkingServiceConfig {
    /// Base chunker configuration
    pub base: ChunkerConfig,
    /// Whether to preserve docstrings with their target
    pub preserve_docstrings: bool,
    /// Whether to include context prefix in chunks
    pub include_context: bool,
    /// Fallback strategy if primary fails
    pub fallback_strategy: ChunkingStrategy,
}

impl Default for ChunkingServiceConfig {
    fn default() -> Self {
        Self {
            base: ChunkerConfig::default(),
            preserve_docstrings: true,
            include_context: true,
            fallback_strategy: ChunkingStrategy::Fixed,
        }
    }
}

/// Unified chunking service with strategy dispatch
pub struct ChunkingService {
    config: ChunkingServiceConfig,
    syntactic: Option<SyntacticChunker>,
    semantic: SemanticChunker,
    fixed: SlidingWindowChunker,
}

impl ChunkingService {
    /// Create a new chunking service with default configuration
    pub fn new() -> Self {
        Self::with_config(ChunkingServiceConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: ChunkingServiceConfig) -> Self {
        Self {
            semantic: SemanticChunker::new(config.base.clone()),
            fixed: SlidingWindowChunker::new(config.base.clone()),
            syntactic: SyntacticChunker::new().ok(),
            config,
        }
    }

    /// Detect the best chunking strategy for content
    pub fn detect_strategy(&self, content: &str, document_type: Option<&str>) -> ChunkingStrategy {
        match document_type {
            Some("rust") | Some("python") | Some("typescript") | Some("javascript") |
            Some("go") | Some("java") | Some("c") | Some("cpp") => {
                if self.syntactic.is_some() {
                    ChunkingStrategy::Syntactic
                } else {
                    ChunkingStrategy::Fixed
                }
            }
            Some("markdown") | Some("plaintext") => ChunkingStrategy::Semantic,
            Some("json") | Some("yaml") | Some("toml") => ChunkingStrategy::Fixed,
            _ => {
                // Auto-detect based on content
                if Self::looks_like_code(content) {
                    if self.syntactic.is_some() {
                        ChunkingStrategy::Syntactic
                    } else {
                        ChunkingStrategy::Fixed
                    }
                } else {
                    ChunkingStrategy::Semantic
                }
            }
        }
    }

    /// Chunk content using the specified strategy
    pub fn chunk(&self, content: &str, strategy: ChunkingStrategy) -> Vec<Chunk> {
        match strategy {
            ChunkingStrategy::Semantic => self.semantic.chunk(content),
            ChunkingStrategy::Fixed => self.fixed.chunk(content),
            ChunkingStrategy::Syntactic => {
                match &self.syntactic {
                    Some(chunker) => {
                        match chunker.chunk_code(content, None) {
                            Ok(code_chunks) => {
                                code_chunks.into_iter().map(|c| c.to_chunk()).collect()
                            }
                            Err(_) => {
                                // Fallback on parse error
                                self.chunk(content, self.config.fallback_strategy)
                            }
                        }
                    }
                    None => self.chunk(content, self.config.fallback_strategy),
                }
            }
            ChunkingStrategy::Hybrid => {
                // Split by semantic boundaries first, then apply syntactic to code blocks
                let semantic_chunks = self.semantic.chunk(content);
                let mut result = Vec::new();

                for chunk in semantic_chunks {
                    if chunk.metadata.get("type").map(|s| s.as_str()) == Some("code") {
                        // Re-chunk code blocks syntactically
                        if let Some(ref chunker) = self.syntactic {
                            if let Ok(code_chunks) = chunker.chunk_code(&chunk.text, None) {
                                result.extend(code_chunks.into_iter().map(|c| c.to_chunk()));
                                continue;
                            }
                        }
                    }
                    result.push(chunk);
                }

                result
            }
        }
    }

    /// Chunk code with full context
    pub fn chunk_code(
        &self,
        content: &str,
        language: Option<&str>,
        file_path: Option<&str>,
    ) -> Result<Vec<CodeChunk>, ChunkingError> {
        let chunker = self.syntactic.as_ref()
            .ok_or(ChunkingError::SyntacticChunkerUnavailable)?;

        let mut chunks = chunker.chunk_code(content, language)?;

        // Add file path context
        if let Some(path) = file_path {
            for chunk in &mut chunks {
                chunk.context.file_path = Some(path.to_string());
            }
        }

        Ok(chunks)
    }

    /// Heuristic check if content looks like code
    fn looks_like_code(content: &str) -> bool {
        let code_indicators = [
            "fn ", "def ", "function ", "class ", "struct ", "impl ",
            "import ", "from ", "require(", "const ", "let ", "var ",
            "pub ", "private ", "public ", "static ", "async ",
        ];

        let lines: Vec<&str> = content.lines().take(20).collect();
        let indicator_count = lines.iter()
            .filter(|line| code_indicators.iter().any(|ind| line.contains(ind)))
            .count();

        // More than 20% of first 20 lines look like code
        indicator_count > lines.len() / 5
    }
}

/// Errors from chunking operations
#[derive(Debug, thiserror::Error)]
pub enum ChunkingError {
    #[error("Syntactic chunker not available (Tree-sitter not compiled)")]
    SyntacticChunkerUnavailable,

    #[error("Failed to parse source: {0}")]
    ParseError(String),

    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}
```

### 3. Syntactic Chunker Implementation

```rust
// File: crates/matric-db/src/chunking/syntactic.rs

use tree_sitter::{Language, Parser, Node, Tree};
use super::code_types::*;
use super::service::ChunkingError;
use super::ChunkerConfig;

/// Supported languages with their Tree-sitter configurations
pub struct LanguageConfig {
    /// Tree-sitter language parser
    pub language: Language,
    /// Node types that represent top-level units
    pub top_level_types: &'static [&'static str],
    /// Node types that represent documentation
    pub doc_types: &'static [&'static str],
    /// How to extract function/class names
    pub name_field: &'static str,
}

/// Registry of supported languages
pub struct LanguageRegistry {
    configs: std::collections::HashMap<String, LanguageConfig>,
}

impl LanguageRegistry {
    /// Create registry with all supported languages
    pub fn new() -> Self {
        let mut configs = std::collections::HashMap::new();

        // Rust
        configs.insert("rust".to_string(), LanguageConfig {
            language: tree_sitter_rust::language(),
            top_level_types: &[
                "function_item", "impl_item", "struct_item", "enum_item",
                "trait_item", "mod_item", "const_item", "static_item",
                "type_alias", "macro_definition"
            ],
            doc_types: &["line_comment", "block_comment"],
            name_field: "name",
        });

        // Python
        configs.insert("python".to_string(), LanguageConfig {
            language: tree_sitter_python::language(),
            top_level_types: &[
                "function_definition", "class_definition", "decorated_definition",
                "import_statement", "import_from_statement"
            ],
            doc_types: &["comment", "string"], // docstrings are strings
            name_field: "name",
        });

        // TypeScript/JavaScript
        configs.insert("typescript".to_string(), LanguageConfig {
            language: tree_sitter_typescript::language_typescript(),
            top_level_types: &[
                "function_declaration", "class_declaration", "interface_declaration",
                "type_alias_declaration", "enum_declaration", "export_statement",
                "import_statement", "lexical_declaration"
            ],
            doc_types: &["comment"],
            name_field: "name",
        });

        configs.insert("javascript".to_string(), LanguageConfig {
            language: tree_sitter_javascript::language(),
            top_level_types: &[
                "function_declaration", "class_declaration", "export_statement",
                "import_statement", "lexical_declaration", "variable_declaration"
            ],
            doc_types: &["comment"],
            name_field: "name",
        });

        // Go
        configs.insert("go".to_string(), LanguageConfig {
            language: tree_sitter_go::language(),
            top_level_types: &[
                "function_declaration", "method_declaration", "type_declaration",
                "const_declaration", "var_declaration"
            ],
            doc_types: &["comment"],
            name_field: "name",
        });

        Self { configs }
    }

    /// Get language config by name
    pub fn get(&self, name: &str) -> Option<&LanguageConfig> {
        self.configs.get(name)
    }

    /// Detect language from file extension
    pub fn detect_from_extension(ext: &str) -> Option<&'static str> {
        match ext {
            "rs" => Some("rust"),
            "py" => Some("python"),
            "ts" | "tsx" => Some("typescript"),
            "js" | "jsx" | "mjs" => Some("javascript"),
            "go" => Some("go"),
            "java" => Some("java"),
            "c" | "h" => Some("c"),
            "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
            "rb" => Some("ruby"),
            "php" => Some("php"),
            _ => None,
        }
    }
}

/// Syntactic chunker using Tree-sitter
pub struct SyntacticChunker {
    registry: LanguageRegistry,
    config: ChunkerConfig,
}

impl SyntacticChunker {
    /// Create a new syntactic chunker
    pub fn new() -> Result<Self, ChunkingError> {
        Ok(Self {
            registry: LanguageRegistry::new(),
            config: ChunkerConfig::default(),
        })
    }

    /// Create with custom config
    pub fn with_config(config: ChunkerConfig) -> Result<Self, ChunkingError> {
        Ok(Self {
            registry: LanguageRegistry::new(),
            config,
        })
    }

    /// Chunk source code into semantic units
    pub fn chunk_code(
        &self,
        source: &str,
        language: Option<&str>,
    ) -> Result<Vec<CodeChunk>, ChunkingError> {
        let lang_name = language.unwrap_or("rust"); // Default to Rust

        let lang_config = self.registry.get(lang_name)
            .ok_or_else(|| ChunkingError::UnsupportedLanguage(lang_name.to_string()))?;

        let mut parser = Parser::new();
        parser.set_language(lang_config.language)
            .map_err(|e| ChunkingError::ParseError(e.to_string()))?;

        let tree = parser.parse(source, None)
            .ok_or_else(|| ChunkingError::ParseError("Parse returned None".to_string()))?;

        let mut chunks = Vec::new();
        self.extract_units(
            &tree,
            source,
            lang_name,
            lang_config,
            &mut chunks,
            ChunkContext::default(),
            0,
        );

        // Handle oversized chunks by splitting
        let final_chunks = self.split_oversized(chunks, source);

        Ok(final_chunks)
    }

    /// Recursively extract code units from AST
    fn extract_units(
        &self,
        tree: &Tree,
        source: &str,
        lang_name: &str,
        config: &LanguageConfig,
        chunks: &mut Vec<CodeChunk>,
        parent_context: ChunkContext,
        depth: u32,
    ) {
        let root = tree.root_node();
        let mut cursor = root.walk();
        let mut pending_doc: Option<(usize, usize, String)> = None;

        for child in root.children(&mut cursor) {
            let node_type = child.kind();

            // Track documentation comments
            if config.doc_types.contains(&node_type) {
                let start = child.start_byte();
                let end = child.end_byte();
                let text = source[start..end].to_string();
                pending_doc = Some((start, end, text));
                continue;
            }

            // Extract top-level units
            if config.top_level_types.contains(&node_type) {
                let start = pending_doc.as_ref().map(|(s, _, _)| *s).unwrap_or(child.start_byte());
                let end = child.end_byte();
                let text = source[start..end].to_string();

                let unit_name = self.extract_name(&child, config.name_field, source);
                let unit_kind = self.node_type_to_kind(node_type);

                let mut context = parent_context.clone();
                context.start_line = Some(child.start_position().row as u32);
                context.end_line = Some(child.end_position().row as u32);

                if let Some(ref name) = unit_name {
                    if unit_kind.is_container() {
                        context.parent_name = Some(format!("{:?} {}", unit_kind, name));
                    }
                }

                chunks.push(CodeChunk {
                    content: text,
                    context,
                    span: start..end,
                    unit_kind,
                    unit_name,
                    language: lang_name.to_string(),
                    depth,
                });

                pending_doc = None;
            }
        }
    }

    /// Extract name from a node using the specified field
    fn extract_name(&self, node: &Node, field: &str, source: &str) -> Option<String> {
        node.child_by_field_name(field)
            .map(|name_node| source[name_node.start_byte()..name_node.end_byte()].to_string())
    }

    /// Map Tree-sitter node type to CodeUnitKind
    fn node_type_to_kind(&self, node_type: &str) -> CodeUnitKind {
        match node_type {
            s if s.contains("function") || s.contains("method") => CodeUnitKind::Function,
            s if s.contains("class") => CodeUnitKind::Class,
            s if s.contains("struct") => CodeUnitKind::Struct,
            s if s.contains("trait") || s.contains("interface") => CodeUnitKind::Trait,
            s if s.contains("impl") => CodeUnitKind::Impl,
            s if s.contains("mod") || s.contains("module") => CodeUnitKind::Module,
            s if s.contains("const") || s.contains("static") => CodeUnitKind::Constant,
            s if s.contains("import") || s.contains("use") => CodeUnitKind::Import,
            s if s.contains("comment") => CodeUnitKind::Comment,
            _ => CodeUnitKind::Unknown,
        }
    }

    /// Split chunks that exceed max size
    fn split_oversized(&self, chunks: Vec<CodeChunk>, source: &str) -> Vec<CodeChunk> {
        let mut result = Vec::new();

        for chunk in chunks {
            if chunk.content.len() <= self.config.max_chunk_size {
                result.push(chunk);
            } else {
                // Split large chunks at nested boundaries or lines
                let sub_chunks = self.split_large_chunk(&chunk, source);
                result.extend(sub_chunks);
            }
        }

        result
    }

    /// Split a large chunk into smaller pieces
    fn split_large_chunk(&self, chunk: &CodeChunk, _source: &str) -> Vec<CodeChunk> {
        let content = &chunk.content;
        let mut result = Vec::new();
        let mut start = 0;
        let part_num = std::cell::RefCell::new(0);

        while start < content.len() {
            let end = (start + self.config.max_chunk_size).min(content.len());

            // Try to break at a line boundary
            let break_at = if end < content.len() {
                content[start..end].rfind('\n')
                    .map(|pos| start + pos + 1)
                    .unwrap_or(end)
            } else {
                end
            };

            if break_at > start {
                *part_num.borrow_mut() += 1;
                let mut new_chunk = chunk.clone();
                new_chunk.content = content[start..break_at].to_string();
                new_chunk.span = (chunk.span.start + start)..(chunk.span.start + break_at);
                new_chunk.unit_name = chunk.unit_name.as_ref()
                    .map(|n| format!("{} (part {})", n, part_num.borrow()));
                result.push(new_chunk);
            }

            start = break_at;
        }

        result
    }
}
```

### 4. Module Organization

```
crates/matric-db/src/chunking/
  mod.rs           # Re-exports, backward compatibility
  code_types.rs    # CodeChunk, ChunkContext, CodeUnitKind
  service.rs       # ChunkingService trait and dispatch
  syntactic.rs     # SyntacticChunker with Tree-sitter
  semantic.rs      # Existing SemanticChunker (move from chunking.rs)
  sentence.rs      # Existing SentenceChunker
  paragraph.rs     # Existing ParagraphChunker
  sliding.rs       # Existing SlidingWindowChunker
  recursive.rs     # Existing RecursiveChunker
```

---

## Dependencies

### Tree-sitter Crates

Add to `crates/matric-db/Cargo.toml`:

```toml
[dependencies]
# Tree-sitter core
tree-sitter = "0.22"

# Language grammars - use matching versions
tree-sitter-rust = "0.21"
tree-sitter-python = "0.21"
tree-sitter-typescript = "0.21"
tree-sitter-javascript = "0.21"
tree-sitter-go = "0.21"
tree-sitter-java = "0.21"
tree-sitter-c = "0.21"
tree-sitter-cpp = "0.22"

[features]
default = []
tree-sitter = [
    "dep:tree-sitter",
    "dep:tree-sitter-rust",
    "dep:tree-sitter-python",
    "dep:tree-sitter-typescript",
    "dep:tree-sitter-javascript",
    "dep:tree-sitter-go",
]
# Full language support (larger binary)
tree-sitter-full = [
    "tree-sitter",
    "dep:tree-sitter-java",
    "dep:tree-sitter-c",
    "dep:tree-sitter-cpp",
]
```

**Note:** Tree-sitter grammars compile to native code. This adds:
- ~5MB to binary size per language
- ~10-30s to compile time per language
- C compiler requirement (usually already present)

### Version Compatibility

Current Cargo.lock shows no existing Tree-sitter usage, so we have flexibility in version selection. Recommend using 0.22.x for core and matching grammar versions.

---

## Integration Points

### 1. EmbeddingService Update

```rust
// In embedding job handler

async fn embed_note(note: &NoteFull, service: &ChunkingService) -> Result<Vec<Chunk>> {
    let doc_type = note.metadata.get("document_type")
        .map(|v| v.as_str().unwrap_or("markdown"));

    let strategy = service.detect_strategy(&note.content, doc_type);
    let chunks = service.chunk(&note.content, strategy);

    Ok(chunks)
}
```

### 2. Document Type Integration

Connects with ADR-025 Document Type Registry:

```sql
-- Query to get chunking strategy for a note
SELECT dt.chunking_strategy, dt.chunk_size_default
FROM note n
JOIN document_type dt ON dt.id = n.document_type_id
WHERE n.id = $1;
```

### 3. API Exposure

Add debug endpoint for chunk preview:

```
POST /api/v1/preview-chunks
{
    "content": "fn main() { ... }",
    "document_type": "rust",
    "strategy": "syntactic"
}

Response:
{
    "chunks": [
        {
            "text": "fn main() { ... }",
            "unit_kind": "Function",
            "unit_name": "main",
            "span": { "start": 0, "end": 25 },
            "context": { "file_path": null, "module_path": [] }
        }
    ],
    "strategy_used": "syntactic",
    "total_chunks": 1
}
```

---

## Performance Targets

| Metric | Target | Rationale |
|--------|--------|-----------|
| Parse + chunk 1K lines | < 10ms | Negligible latency impact |
| Parse + chunk 10K lines | < 100ms | Large file threshold |
| Memory per file | < 50MB | Reasonable for any file size |
| Fallback latency | < 5ms | Fixed chunking is fast |

### Benchmarking Strategy

```rust
#[bench]
fn bench_syntactic_chunker_rust_1k_lines(b: &mut Bencher) {
    let source = include_str!("testdata/sample_1k_lines.rs");
    let chunker = SyntacticChunker::new().unwrap();

    b.iter(|| {
        chunker.chunk_code(source, Some("rust")).unwrap()
    });
}
```

---

## Migration Strategy

### Phase 1: Add Infrastructure (Non-Breaking)

1. Create new `chunking/` module structure
2. Move existing chunkers to submodules
3. Add Tree-sitter dependencies (behind feature flag)
4. Implement `SyntacticChunker`
5. Add `ChunkingService` facade

### Phase 2: Integration

1. Update `ChunkingService` in matric-api to use new facade
2. Connect with Document Type Registry
3. Add API preview endpoint
4. Update embedding job handlers

### Phase 3: Rollout

1. Enable Tree-sitter feature by default
2. Monitor performance metrics
3. Add more language grammars as needed

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_syntactic_chunker_rust_function() {
    let source = r#"
/// Adds two numbers
fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let chunker = SyntacticChunker::new().unwrap();
    let chunks = chunker.chunk_code(source, Some("rust")).unwrap();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].unit_kind, CodeUnitKind::Function);
    assert_eq!(chunks[0].unit_name, Some("add".to_string()));
    assert!(chunks[0].content.contains("/// Adds two numbers"));
}

#[test]
fn test_syntactic_chunker_python_class() {
    let source = r#"
class UserService:
    """Service for user operations."""

    def __init__(self, db):
        self.db = db

    def get_user(self, id):
        return self.db.find(id)
"#;

    let chunker = SyntacticChunker::new().unwrap();
    let chunks = chunker.chunk_code(source, Some("python")).unwrap();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].unit_kind, CodeUnitKind::Class);
}

#[test]
fn test_chunking_service_strategy_detection() {
    let service = ChunkingService::new();

    assert_eq!(
        service.detect_strategy("fn main() {}", Some("rust")),
        ChunkingStrategy::Syntactic
    );

    assert_eq!(
        service.detect_strategy("# Hello World\n\nSome text", Some("markdown")),
        ChunkingStrategy::Semantic
    );
}

#[test]
fn test_oversized_chunk_splitting() {
    let config = ChunkerConfig {
        max_chunk_size: 100,
        ..Default::default()
    };

    let chunker = SyntacticChunker::with_config(config).unwrap();
    let source = "fn long_function() {\n".to_string() + &"    let x = 1;\n".repeat(50) + "}";

    let chunks = chunker.chunk_code(&source, Some("rust")).unwrap();

    assert!(chunks.len() > 1);
    for chunk in &chunks {
        assert!(chunk.content.len() <= 100);
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_embedding_with_syntactic_chunking() {
    let service = ChunkingService::new();
    let source = include_str!("testdata/sample_rust.rs");

    let chunks = service.chunk(source, ChunkingStrategy::Syntactic);

    // Verify chunks maintain semantic integrity
    for chunk in &chunks {
        // Each chunk should be valid Rust (parseable)
        if chunk.metadata.get("unit_kind").map(|s| s != "Import") != Some(true) {
            // Imports may be partial
            let re_parse = syn::parse_str::<syn::Item>(&chunk.text);
            assert!(re_parse.is_ok(), "Chunk is not valid Rust: {}", chunk.text);
        }
    }
}
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Tree-sitter compile failures | Feature flag, graceful fallback to fixed chunking |
| Parse errors on malformed code | Fallback strategy, log warnings |
| Memory issues on huge files | Streaming parser (future), size limits |
| Language detection errors | Explicit document_type, file extension hints |

---

## Open Questions

1. **Mixed-language files**: How to handle `.tsx` with prose in JSDoc comments?
   - Proposal: Use hybrid strategy, detect comment blocks

2. **Incremental re-embedding**: Should we only re-embed changed functions?
   - Proposal: Track chunk hashes, diff on update

3. **Context window optimization**: How much context prefix to include?
   - Proposal: Configurable, default to file path + parent name

4. **Grammar updates**: How to handle Tree-sitter grammar version updates?
   - Proposal: Pin versions, test suite for regressions

---

## Implementation Checklist

- [ ] Create `crates/matric-db/src/chunking/` module structure
- [ ] Define `CodeChunk`, `ChunkContext`, `CodeUnitKind` types
- [ ] Implement `ChunkingService` trait with strategy dispatch
- [ ] Implement `SyntacticChunker` with Tree-sitter
- [ ] Add Tree-sitter dependencies behind feature flag
- [ ] Create `LanguageRegistry` for supported languages
- [ ] Move existing chunkers to submodules
- [ ] Update `ChunkingService` in matric-api
- [ ] Connect with Document Type Registry (ADR-025)
- [ ] Add API chunk preview endpoint
- [ ] Write unit tests for all chunkers
- [ ] Write integration tests for embedding flow
- [ ] Add benchmarks
- [ ] Update documentation (`docs/content/chunking.md`)
- [ ] Update ADR-027 status to Accepted

---

## References

- **ADR-025**: Document Type Registry
- **ADR-027**: Code-Aware Chunking (proposed)
- **Tree-sitter Documentation**: https://tree-sitter.github.io/
- **Existing Chunking**: `crates/matric-db/src/chunking.rs`
- **Design Notes**: `.aiwg/working/discovery/code-embedding-support/design-notes.md`

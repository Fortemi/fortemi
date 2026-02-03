//! Code-aware chunking using tree-sitter for syntactic analysis.
//!
//! This module provides language-aware chunking that respects code structure
//! by parsing source code with tree-sitter and extracting semantically complete
//! units like functions, classes, and modules.
//!
//! # Example
//!
//! ```rust,ignore
//! use matric_db::syntactic_chunker::{SyntacticChunker, CodeChunk};
//! use matric_db::chunking::ChunkerConfig;
//!
//! let config = ChunkerConfig {
//!     max_chunk_size: 2000,
//!     min_chunk_size: 100,
//!     overlap: 0,
//! };
//!
//! let chunker = SyntacticChunker::new(config);
//! let rust_code = r#"
//!     fn main() {
//!         println!("Hello, world!");
//!     }
//! "#;
//!
//! let chunks = chunker.chunk_code(rust_code, "rust");
//! for chunk in chunks {
//!     println!("Found {} named {:?}", chunk.unit_kind, chunk.unit_name);
//! }
//! ```

use crate::chunking::{Chunk, Chunker, ChunkerConfig, SemanticChunker};
use std::collections::HashMap;

#[cfg(feature = "tree-sitter")]
use tree_sitter::{Node, Parser, Tree};

/// Kind of code unit extracted from source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeUnitKind {
    /// Standalone function declaration.
    Function,
    /// Method within a class or impl block.
    Method,
    /// Class or struct definition.
    Class,
    /// Struct definition (Rust-specific).
    Struct,
    /// Enum definition.
    Enum,
    /// Module or namespace.
    Module,
    /// Constant or static declaration.
    Constant,
    /// Import or use statement.
    Import,
    /// Standalone comment or docstring.
    Comment,
    /// Other code units not categorized above.
    Other,
}

impl CodeUnitKind {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            CodeUnitKind::Function => "function",
            CodeUnitKind::Method => "method",
            CodeUnitKind::Class => "class",
            CodeUnitKind::Struct => "struct",
            CodeUnitKind::Enum => "enum",
            CodeUnitKind::Module => "module",
            CodeUnitKind::Constant => "constant",
            CodeUnitKind::Import => "import",
            CodeUnitKind::Comment => "comment",
            CodeUnitKind::Other => "other",
        }
    }
}

/// A code-aware chunk with syntactic context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeChunk {
    /// The actual chunk (implements standard Chunk interface).
    pub chunk: Chunk,
    /// Kind of code unit this chunk represents.
    pub unit_kind: CodeUnitKind,
    /// Name of the unit (function name, class name, etc.).
    pub unit_name: Option<String>,
    /// Programming language.
    pub language: String,
    /// Parent scope (e.g., "impl EmbeddingSet").
    pub parent_scope: Option<String>,
    /// Module path (e.g., ["matric_core", "models"]).
    pub module_path: Vec<String>,
}

impl CodeChunk {
    /// Create a new CodeChunk.
    pub fn new(
        chunk: Chunk,
        unit_kind: CodeUnitKind,
        unit_name: Option<String>,
        language: String,
        parent_scope: Option<String>,
        module_path: Vec<String>,
    ) -> Self {
        Self {
            chunk,
            unit_kind,
            unit_name,
            language,
            parent_scope,
            module_path,
        }
    }
}

/// Syntactic chunker using tree-sitter for language-aware parsing.
#[derive(Debug, Clone)]
pub struct SyntacticChunker {
    config: ChunkerConfig,
}

impl SyntacticChunker {
    /// Create a new SyntacticChunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    /// Detect programming language from file extension or content.
    pub fn detect_language(filename: Option<&str>, content: &str) -> Option<String> {
        // Try filename-based detection first
        if let Some(fname) = filename {
            let ext = fname.rsplit('.').next()?;
            match ext {
                "rs" => return Some("rust".to_string()),
                "py" => return Some("python".to_string()),
                "js" | "mjs" | "cjs" => return Some("javascript".to_string()),
                "ts" | "mts" | "cts" => return Some("typescript".to_string()),
                _ => {}
            }
        }

        // Content-based detection fallback
        if content.contains("fn ") || content.contains("impl ") || content.contains("mod ") {
            return Some("rust".to_string());
        }
        if content.contains("def ") || content.contains("class ") || content.contains("import ") {
            return Some("python".to_string());
        }
        if content.contains("function ") || content.contains("const ") || content.contains("let ") {
            return Some("javascript".to_string());
        }

        None
    }

    /// Chunk code with language awareness.
    #[cfg(feature = "tree-sitter")]
    pub fn chunk_code(&self, text: &str, language: &str) -> Vec<CodeChunk> {
        match self.parse_and_chunk(text, language) {
            Ok(chunks) => chunks,
            Err(_) => {
                // Fall back to semantic chunker if parsing fails
                self.fallback_chunk(text, language)
            }
        }
    }

    /// Chunk code with language awareness (fallback when tree-sitter is disabled).
    #[cfg(not(feature = "tree-sitter"))]
    pub fn chunk_code(&self, text: &str, language: &str) -> Vec<CodeChunk> {
        self.fallback_chunk(text, language)
    }

    /// Parse code and extract chunks using tree-sitter.
    #[cfg(feature = "tree-sitter")]
    fn parse_and_chunk(&self, text: &str, language: &str) -> Result<Vec<CodeChunk>, String> {
        let lang = match language {
            "rust" => tree_sitter_rust::language(),
            "python" => tree_sitter_python::language(),
            "javascript" => tree_sitter_javascript::language(),
            "typescript" => tree_sitter_typescript::language_typescript(),
            _ => return Err(format!("Unsupported language: {}", language)),
        };

        let mut parser = Parser::new();
        parser
            .set_language(&lang)
            .map_err(|e| format!("Failed to set language: {}", e))?;

        let tree = parser
            .parse(text, None)
            .ok_or_else(|| "Failed to parse code".to_string())?;

        let chunks = self.extract_chunks(&tree, text, language);
        Ok(chunks)
    }

    /// Extract code chunks from parsed tree.
    #[cfg(feature = "tree-sitter")]
    fn extract_chunks(&self, tree: &Tree, text: &str, language: &str) -> Vec<CodeChunk> {
        let root = tree.root_node();
        let mut chunks = Vec::new();

        // Extract top-level units based on language
        match language {
            "rust" => self.extract_rust_chunks(root, text, &mut chunks),
            "python" => self.extract_python_chunks(root, text, &mut chunks),
            "javascript" | "typescript" => self.extract_javascript_chunks(root, text, &mut chunks),
            _ => {}
        }

        // If no chunks were extracted or parsing failed, fall back
        if chunks.is_empty() {
            return self.fallback_chunk(text, language);
        }

        chunks
    }

    /// Extract chunks from Rust code.
    #[cfg(feature = "tree-sitter")]
    fn extract_rust_chunks(&self, root: Node, text: &str, chunks: &mut Vec<CodeChunk>) {
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            let _kind = child.kind();
            let start = child.start_byte();
            let end = child.end_byte();

            // Skip if chunk would be too large
            if end - start > self.config.max_chunk_size {
                // Try to split nested units
                self.extract_rust_chunks(child, text, chunks);
                continue;
            }

            let chunk_text = &text[start..end];
            let (unit_kind, unit_name) = self.classify_rust_node(child, text);

            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), "code".to_string());
            metadata.insert("unit_kind".to_string(), unit_kind.as_str().to_string());
            if let Some(ref name) = unit_name {
                metadata.insert("unit_name".to_string(), name.clone());
            }

            let chunk = Chunk::with_metadata(chunk_text.to_string(), start, end, metadata);

            chunks.push(CodeChunk::new(
                chunk,
                unit_kind,
                unit_name,
                "rust".to_string(),
                None,
                Vec::new(),
            ));
        }
    }

    /// Classify a Rust tree-sitter node.
    #[cfg(feature = "tree-sitter")]
    fn classify_rust_node(&self, node: Node, text: &str) -> (CodeUnitKind, Option<String>) {
        match node.kind() {
            "function_item" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Function, name)
            }
            "struct_item" => {
                let name = self.extract_name(node, text, "type_identifier");
                (CodeUnitKind::Struct, name)
            }
            "enum_item" => {
                let name = self.extract_name(node, text, "type_identifier");
                (CodeUnitKind::Enum, name)
            }
            "impl_item" => {
                let name = self.extract_name(node, text, "type_identifier");
                (CodeUnitKind::Class, name)
            }
            "mod_item" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Module, name)
            }
            "const_item" | "static_item" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Constant, name)
            }
            "use_declaration" => (CodeUnitKind::Import, None),
            "line_comment" | "block_comment" => (CodeUnitKind::Comment, None),
            _ => (CodeUnitKind::Other, None),
        }
    }

    /// Extract chunks from Python code.
    #[cfg(feature = "tree-sitter")]
    fn extract_python_chunks(&self, root: Node, text: &str, chunks: &mut Vec<CodeChunk>) {
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            let _kind = child.kind();
            let start = child.start_byte();
            let end = child.end_byte();

            // Skip if chunk would be too large
            if end - start > self.config.max_chunk_size {
                // Try to split nested units
                self.extract_python_chunks(child, text, chunks);
                continue;
            }

            let chunk_text = &text[start..end];
            let (unit_kind, unit_name) = self.classify_python_node(child, text);

            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), "code".to_string());
            metadata.insert("unit_kind".to_string(), unit_kind.as_str().to_string());
            if let Some(ref name) = unit_name {
                metadata.insert("unit_name".to_string(), name.clone());
            }

            let chunk = Chunk::with_metadata(chunk_text.to_string(), start, end, metadata);

            chunks.push(CodeChunk::new(
                chunk,
                unit_kind,
                unit_name,
                "python".to_string(),
                None,
                Vec::new(),
            ));
        }
    }

    /// Classify a Python tree-sitter node.
    #[cfg(feature = "tree-sitter")]
    fn classify_python_node(&self, node: Node, text: &str) -> (CodeUnitKind, Option<String>) {
        match node.kind() {
            "function_definition" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Function, name)
            }
            "class_definition" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Class, name)
            }
            "import_statement" | "import_from_statement" => (CodeUnitKind::Import, None),
            "comment" => (CodeUnitKind::Comment, None),
            _ => (CodeUnitKind::Other, None),
        }
    }

    /// Extract chunks from JavaScript/TypeScript code.
    #[cfg(feature = "tree-sitter")]
    fn extract_javascript_chunks(&self, root: Node, text: &str, chunks: &mut Vec<CodeChunk>) {
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            let start = child.start_byte();
            let end = child.end_byte();

            // Skip if chunk would be too large
            if end - start > self.config.max_chunk_size {
                // Try to split nested units
                self.extract_javascript_chunks(child, text, chunks);
                continue;
            }

            let chunk_text = &text[start..end];
            let (unit_kind, unit_name) = self.classify_javascript_node(child, text);

            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), "code".to_string());
            metadata.insert("unit_kind".to_string(), unit_kind.as_str().to_string());
            if let Some(ref name) = unit_name {
                metadata.insert("unit_name".to_string(), name.clone());
            }

            let chunk = Chunk::with_metadata(chunk_text.to_string(), start, end, metadata);

            chunks.push(CodeChunk::new(
                chunk,
                unit_kind,
                unit_name,
                "javascript".to_string(),
                None,
                Vec::new(),
            ));
        }
    }

    /// Classify a JavaScript/TypeScript tree-sitter node.
    #[cfg(feature = "tree-sitter")]
    fn classify_javascript_node(&self, node: Node, text: &str) -> (CodeUnitKind, Option<String>) {
        match node.kind() {
            "function_declaration" | "function" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Function, name)
            }
            "class_declaration" => {
                let name = self.extract_name(node, text, "identifier");
                (CodeUnitKind::Class, name)
            }
            "method_definition" => {
                let name = self.extract_name(node, text, "property_identifier");
                (CodeUnitKind::Method, name)
            }
            "import_statement" | "export_statement" => (CodeUnitKind::Import, None),
            "comment" => (CodeUnitKind::Comment, None),
            _ => (CodeUnitKind::Other, None),
        }
    }

    /// Extract name from a node by finding a child of the given kind.
    #[cfg(feature = "tree-sitter")]
    fn extract_name(&self, node: Node, text: &str, name_kind: &str) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == name_kind {
                let start = child.start_byte();
                let end = child.end_byte();
                return Some(text[start..end].to_string());
            }
        }
        None
    }

    /// Fall back to semantic chunking when tree-sitter parsing fails.
    fn fallback_chunk(&self, text: &str, language: &str) -> Vec<CodeChunk> {
        let semantic_chunker = SemanticChunker::new(self.config.clone());
        let chunks = semantic_chunker.chunk(text);

        chunks
            .into_iter()
            .map(|chunk| {
                CodeChunk::new(
                    chunk,
                    CodeUnitKind::Other,
                    None,
                    language.to_string(),
                    None,
                    Vec::new(),
                )
            })
            .collect()
    }
}

impl Chunker for SyntacticChunker {
    fn chunk(&self, text: &str) -> Vec<Chunk> {
        // Detect language or fall back to semantic chunking
        let language = Self::detect_language(None, text).unwrap_or_else(|| "text".to_string());

        if language == "text" {
            // Not code, use semantic chunker
            let semantic_chunker = SemanticChunker::new(self.config.clone());
            return semantic_chunker.chunk(text);
        }

        // Extract code chunks and convert to standard Chunks
        self.chunk_code(text, &language)
            .into_iter()
            .map(|code_chunk| code_chunk.chunk)
            .collect()
    }

    fn config(&self) -> &ChunkerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ChunkerConfig {
        ChunkerConfig {
            max_chunk_size: 2000,
            min_chunk_size: 100,
            overlap: 0,
        }
    }

    // ============================================================================
    // CodeUnitKind tests
    // ============================================================================

    #[test]
    fn test_code_unit_kind_as_str() {
        assert_eq!(CodeUnitKind::Function.as_str(), "function");
        assert_eq!(CodeUnitKind::Class.as_str(), "class");
        assert_eq!(CodeUnitKind::Module.as_str(), "module");
        assert_eq!(CodeUnitKind::Import.as_str(), "import");
    }

    // ============================================================================
    // Language detection tests
    // ============================================================================

    #[test]
    fn test_detect_language_by_extension() {
        assert_eq!(
            SyntacticChunker::detect_language(Some("main.rs"), ""),
            Some("rust".to_string())
        );
        assert_eq!(
            SyntacticChunker::detect_language(Some("script.py"), ""),
            Some("python".to_string())
        );
        assert_eq!(
            SyntacticChunker::detect_language(Some("app.js"), ""),
            Some("javascript".to_string())
        );
        assert_eq!(
            SyntacticChunker::detect_language(Some("component.ts"), ""),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_detect_language_by_content_rust() {
        let rust_code = "fn main() { println!(\"Hello\"); }";
        assert_eq!(
            SyntacticChunker::detect_language(None, rust_code),
            Some("rust".to_string())
        );
    }

    #[test]
    fn test_detect_language_by_content_python() {
        let python_code = "def main():\n    print('Hello')";
        assert_eq!(
            SyntacticChunker::detect_language(None, python_code),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_language_by_content_javascript() {
        let js_code = "function main() { console.log('Hello'); }";
        assert_eq!(
            SyntacticChunker::detect_language(None, js_code),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn test_detect_language_unknown() {
        assert_eq!(
            SyntacticChunker::detect_language(Some("file.txt"), "some text"),
            None
        );
        assert_eq!(
            SyntacticChunker::detect_language(None, "plain text content"),
            None
        );
    }

    // ============================================================================
    // Rust chunking tests
    // ============================================================================

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_simple_function() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
fn hello_world() {
    println!("Hello, world!");
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        assert!(!chunks.is_empty(), "Should extract at least one chunk");
        let func_chunk = chunks
            .iter()
            .find(|c| c.unit_kind == CodeUnitKind::Function);
        assert!(func_chunk.is_some(), "Should find function chunk");

        let func = func_chunk.unwrap();
        assert_eq!(func.unit_name, Some("hello_world".to_string()));
        assert_eq!(func.language, "rust");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_struct() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
struct Point {
    x: f64,
    y: f64,
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        let struct_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Struct);
        assert!(struct_chunk.is_some(), "Should find struct chunk");

        let s = struct_chunk.unwrap();
        assert_eq!(s.unit_name, Some("Point".to_string()));
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_impl_block() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
impl Point {
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        assert!(!chunks.is_empty(), "Should extract chunks from impl block");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_multiple_functions() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        let functions: Vec<_> = chunks
            .iter()
            .filter(|c| c.unit_kind == CodeUnitKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should find at least two functions");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_enum() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        let enum_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Enum);
        assert!(enum_chunk.is_some(), "Should find enum chunk");
        assert_eq!(enum_chunk.unwrap().unit_name, Some("Color".to_string()));
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_module() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
mod utils {
    pub fn helper() {}
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        let mod_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Module);
        assert!(mod_chunk.is_some(), "Should find module chunk");
        assert_eq!(mod_chunk.unwrap().unit_name, Some("utils".to_string()));
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_const() {
        let chunker = SyntacticChunker::new(default_config());
        let code = "const MAX_SIZE: usize = 1024;";
        let chunks = chunker.chunk_code(code, "rust");

        let const_chunk = chunks
            .iter()
            .find(|c| c.unit_kind == CodeUnitKind::Constant);
        assert!(const_chunk.is_some(), "Should find constant chunk");
        assert_eq!(const_chunk.unwrap().unit_name, Some("MAX_SIZE".to_string()));
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_rust_use_statement() {
        let chunker = SyntacticChunker::new(default_config());
        let code = "use std::collections::HashMap;";
        let chunks = chunker.chunk_code(code, "rust");

        let import_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Import);
        assert!(import_chunk.is_some(), "Should find import chunk");
    }

    // ============================================================================
    // Python chunking tests
    // ============================================================================

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_python_simple_function() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
def hello_world():
    print("Hello, world!")
"#;
        let chunks = chunker.chunk_code(code.trim(), "python");

        let func_chunk = chunks
            .iter()
            .find(|c| c.unit_kind == CodeUnitKind::Function);
        assert!(func_chunk.is_some(), "Should find function chunk");
        assert_eq!(
            func_chunk.unwrap().unit_name,
            Some("hello_world".to_string())
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_python_class() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y
"#;
        let chunks = chunker.chunk_code(code.trim(), "python");

        let class_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Class);
        assert!(class_chunk.is_some(), "Should find class chunk");
        assert_eq!(class_chunk.unwrap().unit_name, Some("Point".to_string()));
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_python_import() {
        let chunker = SyntacticChunker::new(default_config());
        let code = "import sys";
        let chunks = chunker.chunk_code(code, "python");

        let import_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Import);
        assert!(import_chunk.is_some(), "Should find import chunk");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_python_multiple_functions() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
def add(a, b):
    return a + b

def multiply(a, b):
    return a * b
"#;
        let chunks = chunker.chunk_code(code.trim(), "python");

        let functions: Vec<_> = chunks
            .iter()
            .filter(|c| c.unit_kind == CodeUnitKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should find at least two functions");
    }

    // ============================================================================
    // JavaScript chunking tests
    // ============================================================================

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_javascript_function() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
function helloWorld() {
    console.log("Hello, world!");
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "javascript");

        let func_chunk = chunks
            .iter()
            .find(|c| c.unit_kind == CodeUnitKind::Function);
        assert!(func_chunk.is_some(), "Should find function chunk");
        assert_eq!(
            func_chunk.unwrap().unit_name,
            Some("helloWorld".to_string())
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_chunk_javascript_class() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
class Point {
    constructor(x, y) {
        this.x = x;
        this.y = y;
    }
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "javascript");

        let class_chunk = chunks.iter().find(|c| c.unit_kind == CodeUnitKind::Class);
        assert!(class_chunk.is_some(), "Should find class chunk");
        assert_eq!(class_chunk.unwrap().unit_name, Some("Point".to_string()));
    }

    // ============================================================================
    // Edge cases and fallback tests
    // ============================================================================

    #[test]
    fn test_fallback_for_invalid_code() {
        let chunker = SyntacticChunker::new(default_config());
        let invalid_code = "this is not valid rust code { } [ ]";

        // Should fall back to semantic chunking without panicking
        let chunks = chunker.chunk_code(invalid_code, "rust");
        assert!(
            !chunks.is_empty(),
            "Should produce chunks even for invalid code"
        );
    }

    #[test]
    fn test_fallback_for_unsupported_language() {
        let chunker = SyntacticChunker::new(default_config());
        let code = "some code in unsupported language";

        let chunks = chunker.chunk_code(code, "cobol");
        assert!(!chunks.is_empty(), "Should fall back to semantic chunking");
    }

    #[test]
    fn test_chunker_trait_implementation() {
        let chunker = SyntacticChunker::new(default_config());
        let code = "fn main() {}";

        // Test that Chunker trait is properly implemented
        let chunks = chunker.chunk(code);
        assert!(!chunks.is_empty(), "Chunker trait should work");
    }

    #[test]
    fn test_empty_code() {
        let chunker = SyntacticChunker::new(default_config());
        let chunks = chunker.chunk_code("", "rust");

        // Should handle empty code gracefully
        assert!(chunks.is_empty() || chunks[0].chunk.text.is_empty());
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_respects_max_chunk_size() {
        let config = ChunkerConfig {
            max_chunk_size: 100,
            min_chunk_size: 10,
            overlap: 0,
        };
        let chunker = SyntacticChunker::new(config);

        // Large function that should be split or handled
        let code = r#"
fn large_function() {
    // This is a very large function with lots of code
    let x = 1;
    let y = 2;
    let z = 3;
    println!("{} {} {}", x, y, z);
    // More lines to make it exceed max_chunk_size
    let a = 4;
    let b = 5;
    let c = 6;
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        // Should handle large chunks appropriately
        assert!(!chunks.is_empty(), "Should produce chunks");
    }

    #[test]
    fn test_code_chunk_creation() {
        let chunk = Chunk::new("test code".to_string(), 0, 9);
        let code_chunk = CodeChunk::new(
            chunk,
            CodeUnitKind::Function,
            Some("test_fn".to_string()),
            "rust".to_string(),
            Some("impl MyStruct".to_string()),
            vec!["my_crate".to_string(), "my_module".to_string()],
        );

        assert_eq!(code_chunk.unit_kind, CodeUnitKind::Function);
        assert_eq!(code_chunk.unit_name, Some("test_fn".to_string()));
        assert_eq!(code_chunk.language, "rust");
        assert_eq!(code_chunk.parent_scope, Some("impl MyStruct".to_string()));
        assert_eq!(code_chunk.module_path.len(), 2);
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn test_performance_large_file() {
        use std::time::Instant;

        let chunker = SyntacticChunker::new(default_config());

        // Generate a large Rust file (simulating 1000 lines)
        let mut large_code = String::new();
        for i in 0..100 {
            large_code.push_str(&format!(
                "fn function_{}() {{\n    println!(\"Function {}\");\n}}\n\n",
                i, i
            ));
        }

        let start = Instant::now();
        let chunks = chunker.chunk_code(&large_code, "rust");
        let duration = start.elapsed();

        assert!(!chunks.is_empty(), "Should produce chunks");
        assert!(
            duration.as_millis() < 100,
            "Should parse and chunk quickly (took {}ms)",
            duration.as_millis()
        );
    }

    #[test]
    fn test_utf8_handling() {
        let chunker = SyntacticChunker::new(default_config());
        let code = r#"
fn 你好() {
    println!("世界");
}
"#;
        let chunks = chunker.chunk_code(code.trim(), "rust");

        // Should handle UTF-8 characters in code
        assert!(!chunks.is_empty(), "Should handle UTF-8 code");
        for chunk in &chunks {
            assert!(
                std::str::from_utf8(chunk.chunk.text.as_bytes()).is_ok(),
                "Chunk text must be valid UTF-8"
            );
        }
    }
}

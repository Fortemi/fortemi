//! CodeAstAdapter — extracts code structure using regex-based declaration detection.
//!
//! Detects function, class, struct, enum, trait, interface, and module declarations
//! across common programming languages. Falls back to TextNativeAdapter behavior
//! for unsupported languages (returns raw text with basic metadata).

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

/// Adapter for extracting code structure from source files.
///
/// Uses regex-based pattern matching to detect declarations in common languages.
/// Falls back to plain text extraction for unknown file types.
pub struct CodeAstAdapter;

/// A detected code declaration.
#[derive(Debug, Clone, serde::Serialize)]
struct Declaration {
    kind: String, // "function", "class", "struct", "enum", "trait", "interface", "method", "impl"
    name: String,
    line_start: usize,
    line_end: usize, // estimated end (next declaration or EOF)
}

/// Detect programming language from filename extension.
fn detect_language(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" | "jsx" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" => Some("cpp"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "swift" => Some("swift"),
        "kt" | "kts" => Some("kotlin"),
        "cs" => Some("csharp"),
        "scala" => Some("scala"),
        "lua" => Some("lua"),
        "sh" | "bash" | "zsh" => Some("shell"),
        _ => None,
    }
}

/// Extract declarations from source code using language-specific regex patterns.
fn extract_declarations(text: &str, language: &str) -> Vec<Declaration> {
    let lines: Vec<&str> = text.lines().collect();
    let mut declarations = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1; // 1-indexed

        // For Python, preserve indentation to detect methods vs functions
        // For other languages, trim whitespace
        let line_to_check = if language == "python" {
            *line
        } else {
            line.trim()
        };

        if let Some(decl) = match_declaration(line_to_check, language) {
            declarations.push(Declaration {
                kind: decl.0.to_string(),
                name: decl.1,
                line_start: line_num,
                line_end: line_num, // will be updated below
            });
        }
    }

    // Estimate end lines: each declaration ends where the next one begins (or EOF)
    let total_lines = lines.len();
    for i in 0..declarations.len() {
        declarations[i].line_end = if i + 1 < declarations.len() {
            declarations[i + 1].line_start.saturating_sub(1)
        } else {
            total_lines
        };
    }

    declarations
}

/// Match a single line against language-specific declaration patterns.
/// Returns (kind, name) if a declaration is found.
fn match_declaration(line: &str, language: &str) -> Option<(&'static str, String)> {
    // Skip comments (use trimmed for comment detection)
    let trimmed = line.trim();
    if trimmed.starts_with("//")
        || trimmed.starts_with('#') && language != "python"
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
    {
        return None;
    }

    match language {
        "rust" => match_rust_declaration(line),
        "python" => match_python_declaration(line),
        "javascript" | "typescript" => match_js_ts_declaration(line),
        "go" => match_go_declaration(line),
        "java" | "kotlin" | "csharp" | "scala" => match_java_like_declaration(line),
        "c" | "cpp" => match_c_declaration(line),
        "ruby" => match_ruby_declaration(line),
        _ => None,
    }
}

fn extract_name_after(line: &str, keyword: &str) -> Option<String> {
    let after = line.split(keyword).nth(1)?.trim();
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn match_rust_declaration(line: &str) -> Option<(&'static str, String)> {
    // pub fn name, fn name, pub async fn name, async fn name
    if line.contains(" fn ") || line.starts_with("fn ") {
        return extract_name_after(line, "fn ").map(|n| ("function", n));
    }
    if line.contains("struct ") && !line.contains("use ") {
        return extract_name_after(line, "struct ").map(|n| ("struct", n));
    }
    if line.contains("enum ") && !line.contains("use ") {
        return extract_name_after(line, "enum ").map(|n| ("enum", n));
    }
    if line.contains("trait ") && !line.contains("use ") {
        return extract_name_after(line, "trait ").map(|n| ("trait", n));
    }
    if line.starts_with("impl ") || line.starts_with("pub impl ") {
        return extract_name_after(line, "impl ").map(|n| ("impl", n));
    }
    if line.contains("mod ") && !line.contains("use ") {
        return extract_name_after(line, "mod ").map(|n| ("module", n));
    }
    None
}

fn match_python_declaration(line: &str) -> Option<(&'static str, String)> {
    // Check indented methods first (before trimming)
    if line.starts_with("    def ") || line.starts_with("    async def ") {
        let keyword = "def ";
        return extract_name_after(line, keyword).map(|n| ("method", n));
    }
    // Top-level functions and classes
    if line.starts_with("def ") || line.starts_with("async def ") {
        return extract_name_after(line, "def ").map(|n| ("function", n));
    }
    if line.starts_with("class ") {
        return extract_name_after(line, "class ").map(|n| ("class", n));
    }
    None
}

fn match_js_ts_declaration(line: &str) -> Option<(&'static str, String)> {
    if line.contains("function ") {
        return extract_name_after(line, "function ").map(|n| ("function", n));
    }
    if line.contains("class ") {
        return extract_name_after(line, "class ").map(|n| ("class", n));
    }
    if line.contains("interface ") {
        return extract_name_after(line, "interface ").map(|n| ("interface", n));
    }
    if line.contains("type ") && line.contains('=') {
        return extract_name_after(line, "type ").map(|n| ("type", n));
    }
    if line.contains("enum ") {
        return extract_name_after(line, "enum ").map(|n| ("enum", n));
    }
    // Arrow functions: const name = ... =>
    if (line.starts_with("const ") || line.starts_with("export const ") || line.starts_with("let "))
        && line.contains("=>")
    {
        let keyword = if line.starts_with("let ") {
            "let "
        } else {
            "const "
        };
        return extract_name_after(line, keyword).map(|n| ("function", n));
    }
    None
}

fn match_go_declaration(line: &str) -> Option<(&'static str, String)> {
    if line.starts_with("func ") {
        // func (r *Receiver) Method or func Name
        if line.contains(") ") && line.starts_with("func (") {
            // Method: func (r *Type) Name(
            let after_paren = line.split(") ").nth(1)?;
            return extract_name_after(&format!("X {}", after_paren), "X ").map(|n| ("method", n));
        }
        return extract_name_after(line, "func ").map(|n| ("function", n));
    }
    if line.starts_with("type ") && line.contains("struct") {
        return extract_name_after(line, "type ").map(|n| ("struct", n));
    }
    if line.starts_with("type ") && line.contains("interface") {
        return extract_name_after(line, "type ").map(|n| ("interface", n));
    }
    None
}

fn match_java_like_declaration(line: &str) -> Option<(&'static str, String)> {
    // class/interface/enum
    if line.contains("class ") {
        return extract_name_after(line, "class ").map(|n| ("class", n));
    }
    if line.contains("interface ") {
        return extract_name_after(line, "interface ").map(|n| ("interface", n));
    }
    if line.contains("enum ") {
        return extract_name_after(line, "enum ").map(|n| ("enum", n));
    }
    // Methods: public/private/protected ... name(
    if line.contains('(')
        && !line.contains("new ")
        && !line.starts_with("import")
        && !line.starts_with("package")
    {
        let before_paren = line.split('(').next()?;
        let parts: Vec<&str> = before_paren.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts.last()?;
            if !["if", "while", "for", "switch", "catch", "return", "throw"].contains(name) {
                let n = name
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>();
                if !n.is_empty() {
                    return Some(("method", n));
                }
            }
        }
    }
    None
}

fn match_c_declaration(line: &str) -> Option<(&'static str, String)> {
    if line.starts_with("struct ") || line.contains(" struct ") {
        return extract_name_after(line, "struct ").map(|n| ("struct", n));
    }
    if line.starts_with("enum ") || line.contains(" enum ") {
        return extract_name_after(line, "enum ").map(|n| ("enum", n));
    }
    if line.starts_with("typedef ") {
        // Get last word before semicolon as name
        let name = line.trim_end_matches(';').split_whitespace().last()?;
        return Some(("type", name.to_string()));
    }
    // Function: return_type name(params) {
    if line.contains('(') && !line.starts_with('#') && !line.starts_with("//") {
        let before_paren = line.split('(').next()?;
        let parts: Vec<&str> = before_paren.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts.last()?;
            if !["if", "while", "for", "switch", "return", "sizeof"].contains(name) {
                let n: String = name
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !n.is_empty()
                    && n.chars()
                        .next()
                        .map(|c| c.is_alphabetic() || c == '_')
                        .unwrap_or(false)
                {
                    return Some(("function", n));
                }
            }
        }
    }
    None
}

fn match_ruby_declaration(line: &str) -> Option<(&'static str, String)> {
    if line.starts_with("def ") || line.starts_with("  def ") {
        return extract_name_after(line, "def ").map(|n| ("method", n));
    }
    if line.starts_with("class ") {
        return extract_name_after(line, "class ").map(|n| ("class", n));
    }
    if line.starts_with("module ") {
        return extract_name_after(line, "module ").map(|n| ("module", n));
    }
    None
}

#[async_trait]
impl ExtractionAdapter for CodeAstAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::CodeAst
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        let text = String::from_utf8_lossy(data).into_owned();
        let total_lines = text.lines().count();

        let language = detect_language(filename);

        let declarations = if let Some(lang) = language {
            extract_declarations(&text, lang)
        } else {
            Vec::new()
        };

        let metadata = json!({
            "language": language.unwrap_or("unknown"),
            "total_lines": total_lines,
            "total_declarations": declarations.len(),
            "declarations": declarations,
            "char_count": text.len(),
        });

        Ok(ExtractionResult {
            extracted_text: Some(text),
            metadata,
            ai_description: None,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true) // No external dependencies
    }

    fn name(&self) -> &str {
        "code_ast"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_code_ast_rust_declarations() {
        let adapter = CodeAstAdapter;
        let rust_code = r#"
use std::collections::HashMap;

pub struct MyStruct {
    field: i32,
}

pub enum MyEnum {
    Variant1,
    Variant2,
}

pub trait MyTrait {
    fn method(&self);
}

impl MyStruct {
    pub fn new() -> Self {
        Self { field: 0 }
    }
}

pub mod my_module {
    pub fn helper() {}
}

pub async fn async_function() {
    // implementation
}

fn private_function() {
    // implementation
}
"#;

        let result = adapter
            .extract(rust_code.as_bytes(), "test.rs", "text/plain", &json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["language"], "rust");
        // Total: struct, enum, trait, trait::method, impl, impl::new, module, module::helper, async_function, private_function = 10
        assert_eq!(result.metadata["total_declarations"], 10);

        let declarations = result.metadata["declarations"].as_array().unwrap();
        assert_eq!(declarations.len(), 10);

        // Check struct
        assert_eq!(declarations[0]["kind"], "struct");
        assert_eq!(declarations[0]["name"], "MyStruct");
        assert_eq!(declarations[0]["line_start"], 4);

        // Check enum
        assert_eq!(declarations[1]["kind"], "enum");
        assert_eq!(declarations[1]["name"], "MyEnum");

        // Check trait
        assert_eq!(declarations[2]["kind"], "trait");
        assert_eq!(declarations[2]["name"], "MyTrait");

        // Check trait method
        assert_eq!(declarations[3]["kind"], "function");
        assert_eq!(declarations[3]["name"], "method");

        // Check impl
        assert_eq!(declarations[4]["kind"], "impl");
        assert_eq!(declarations[4]["name"], "MyStruct");

        // Check impl method
        assert_eq!(declarations[5]["kind"], "function");
        assert_eq!(declarations[5]["name"], "new");

        // Check module
        assert_eq!(declarations[6]["kind"], "module");
        assert_eq!(declarations[6]["name"], "my_module");

        // Check module function
        assert_eq!(declarations[7]["kind"], "function");
        assert_eq!(declarations[7]["name"], "helper");

        // Check async function
        assert_eq!(declarations[8]["kind"], "function");
        assert_eq!(declarations[8]["name"], "async_function");

        // Check private function
        assert_eq!(declarations[9]["kind"], "function");
        assert_eq!(declarations[9]["name"], "private_function");
    }

    #[tokio::test]
    async fn test_code_ast_python_declarations() {
        let adapter = CodeAstAdapter;
        let python_code = r#"
import sys

class MyClass:
    def __init__(self):
        self.value = 0

    def method(self):
        return self.value

def top_level_function():
    return 42

async def async_function():
    await some_task()
"#;

        let result = adapter
            .extract(
                python_code.as_bytes(),
                "test.py",
                "text/x-python",
                &json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["language"], "python");

        let declarations = result.metadata["declarations"].as_array().unwrap();
        assert_eq!(declarations.len(), 5); // class, __init__, method, top_level_function, async_function

        // Check class
        assert_eq!(declarations[0]["kind"], "class");
        assert_eq!(declarations[0]["name"], "MyClass");

        // Check methods (indented, should be "method" kind)
        assert_eq!(declarations[1]["kind"], "method");
        assert_eq!(declarations[1]["name"], "__init__");

        assert_eq!(declarations[2]["kind"], "method");
        assert_eq!(declarations[2]["name"], "method");

        // Check function
        assert_eq!(declarations[3]["kind"], "function");
        assert_eq!(declarations[3]["name"], "top_level_function");

        // Check async function
        assert_eq!(declarations[4]["kind"], "function");
        assert_eq!(declarations[4]["name"], "async_function");
    }

    #[tokio::test]
    async fn test_code_ast_javascript_declarations() {
        let adapter = CodeAstAdapter;
        let js_code = r#"
class MyClass {
    constructor() {}
}

function regularFunction() {
    return 42;
}

const arrowFunc = () => {
    return 1;
};

export const exportedArrow = (x) => x * 2;

let anotherArrow = () => true;
"#;

        let result = adapter
            .extract(js_code.as_bytes(), "test.js", "text/javascript", &json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["language"], "javascript");

        let declarations = result.metadata["declarations"].as_array().unwrap();
        assert!(declarations.len() >= 4);

        // Check class
        assert_eq!(declarations[0]["kind"], "class");
        assert_eq!(declarations[0]["name"], "MyClass");

        // Check function
        assert_eq!(declarations[1]["kind"], "function");
        assert_eq!(declarations[1]["name"], "regularFunction");

        // Check arrow functions
        assert_eq!(declarations[2]["kind"], "function");
        assert_eq!(declarations[2]["name"], "arrowFunc");
    }

    #[tokio::test]
    async fn test_code_ast_go_declarations() {
        let adapter = CodeAstAdapter;
        let go_code = r#"
package main

type MyStruct struct {
    Field int
}

type MyInterface interface {
    Method() error
}

func regularFunction() {
    // implementation
}

func (m *MyStruct) Method() error {
    return nil
}
"#;

        let result = adapter
            .extract(go_code.as_bytes(), "test.go", "text/x-go", &json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["language"], "go");

        let declarations = result.metadata["declarations"].as_array().unwrap();
        assert_eq!(declarations.len(), 4);

        // Check struct
        assert_eq!(declarations[0]["kind"], "struct");
        assert_eq!(declarations[0]["name"], "MyStruct");

        // Check interface
        assert_eq!(declarations[1]["kind"], "interface");
        assert_eq!(declarations[1]["name"], "MyInterface");

        // Check function
        assert_eq!(declarations[2]["kind"], "function");
        assert_eq!(declarations[2]["name"], "regularFunction");

        // Check method
        assert_eq!(declarations[3]["kind"], "method");
        assert_eq!(declarations[3]["name"], "Method");
    }

    #[tokio::test]
    async fn test_code_ast_unknown_language() {
        let adapter = CodeAstAdapter;
        let result = adapter
            .extract(b"Some random text", "unknown.xyz", "text/plain", &json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["language"], "unknown");
        assert_eq!(result.metadata["total_declarations"], 0);
        assert_eq!(result.extracted_text.as_deref(), Some("Some random text"));
        assert_eq!(result.metadata["char_count"], 16);
    }

    #[tokio::test]
    async fn test_code_ast_empty_file() {
        let adapter = CodeAstAdapter;
        let result = adapter
            .extract(b"", "empty.rs", "text/plain", &json!({}))
            .await
            .unwrap();

        assert_eq!(result.metadata["language"], "rust");
        assert_eq!(result.metadata["total_lines"], 0);
        assert_eq!(result.metadata["total_declarations"], 0);
        assert_eq!(result.extracted_text.as_deref(), Some(""));
    }

    #[tokio::test]
    async fn test_code_ast_strategy() {
        let adapter = CodeAstAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::CodeAst);
    }

    #[tokio::test]
    async fn test_code_ast_name() {
        let adapter = CodeAstAdapter;
        assert_eq!(adapter.name(), "code_ast");
    }

    #[tokio::test]
    async fn test_code_ast_health_check() {
        let adapter = CodeAstAdapter;
        assert!(adapter.health_check().await.unwrap());
    }
}

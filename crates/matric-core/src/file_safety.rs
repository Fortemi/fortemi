//! File safety validation for blocking executables and dangerous file types.
//!
//! Multi-layer protection:
//! 1. Magic byte detection for executables
//! 2. Extension blocklist
//! 3. Permission enforcement (0644, no execute)

use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Magic byte signatures for executable files
pub const MAGIC_SIGNATURES: &[(&str, &[u8])] = &[
    ("Windows PE/MZ", &[0x4D, 0x5A]),           // MZ header
    ("ELF", &[0x7F, 0x45, 0x4C, 0x46]),         // Linux ELF
    ("Mach-O 32", &[0xFE, 0xED, 0xFA, 0xCE]),   // macOS 32-bit
    ("Mach-O 64", &[0xFE, 0xED, 0xFA, 0xCF]),   // macOS 64-bit
    ("Mach-O Fat", &[0xCA, 0xFE, 0xBA, 0xBE]),  // Universal binary (also Java)
    ("Java Class", &[0xCA, 0xFE, 0xBA, 0xBE]),  // Java class file
    ("WebAssembly", &[0x00, 0x61, 0x73, 0x6D]), // WASM
    ("Shell script", &[0x23, 0x21]),            // #! shebang
    ("PowerShell BOM", &[0xEF, 0xBB, 0xBF]),    // UTF-8 BOM (check content)
];

/// Blocked file extensions (case-insensitive)
static BLOCKED_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // Windows executables
        "exe", "dll", "scr", "pif", "com", "msi", "msp", "mst", // Unix executables
        "so", "dylib", "out", // Scripts
        "sh", "bash", "zsh", "csh", "ksh", "fish", "bat", "cmd", "ps1", "psm1", "psd1", "vbs",
        "vbe", "js", "jse", "wsf", "wsh", // Java/JVM
        "jar", "war", "ear", "class", // Packages
        "deb", "rpm", "apk", "app", "dmg", "pkg", // Office macros
        "xlsm", "xlsb", "xltm", "docm", "dotm", "pptm", "potm", "ppam",
        // Other dangerous
        "reg", "inf", "scf", "lnk", "url", "hta",
    ]
    .into_iter()
    .collect()
});

/// Result of file safety validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub allowed: bool,
    pub block_reason: Option<String>,
    pub detected_type: Option<String>,
}

impl ValidationResult {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            block_reason: None,
            detected_type: None,
        }
    }

    pub fn blocked(reason: impl Into<String>, detected: impl Into<String>) -> Self {
        Self {
            allowed: false,
            block_reason: Some(reason.into()),
            detected_type: Some(detected.into()),
        }
    }
}

/// Validate file safety
pub fn validate_file(filename: &str, data: &[u8], max_size_bytes: u64) -> ValidationResult {
    // Check size limit
    if data.len() as u64 > max_size_bytes {
        return ValidationResult::blocked(
            format!("File exceeds maximum size of {} bytes", max_size_bytes),
            "oversized",
        );
    }

    // Check extension blocklist
    if let Some(ext) = filename.rsplit('.').next() {
        if BLOCKED_EXTENSIONS.contains(ext.to_lowercase().as_str()) {
            return ValidationResult::blocked(
                format!("File extension .{} is not allowed", ext),
                format!("blocked_extension:{}", ext),
            );
        }
    }

    // Check magic bytes
    for (name, magic) in MAGIC_SIGNATURES {
        if data.len() >= magic.len() && &data[..magic.len()] == *magic {
            // Special case: shebang needs more validation
            if *name == "Shell script" {
                // Only block if it's actually a script shebang
                if data.len() > 2 {
                    let start = String::from_utf8_lossy(&data[..std::cmp::min(64, data.len())]);
                    if start.starts_with("#!/") || start.starts_with("#! /") {
                        return ValidationResult::blocked(
                            "Executable scripts are not allowed",
                            "shell_script",
                        );
                    }
                }
                continue;
            }

            // Special case: CA FE BA BE could be Java or Mach-O Fat
            if magic == &[0xCA, 0xFE, 0xBA, 0xBE] {
                return ValidationResult::blocked(
                    "Java class files and Mach-O binaries are not allowed",
                    "java_or_macho",
                );
            }

            return ValidationResult::blocked(
                format!("Executable file detected: {}", name),
                format!("executable:{}", name.to_lowercase().replace(' ', "_")),
            );
        }
    }

    ValidationResult::allowed()
}

/// Detect actual content type from file magic bytes.
///
/// Returns the detected MIME type if magic bytes match a known format,
/// falling back to extension-based detection, then to the claimed type.
pub fn detect_content_type(filename: &str, data: &[u8], claimed: &str) -> String {
    // 1. Try magic byte detection via infer
    if let Some(kind) = infer::get(data) {
        return kind.mime_type().to_string();
    }

    // 2. Fallback: extension-based detection for text formats (no magic bytes)
    if let Some(ext) = filename.rsplit('.').next() {
        if let Some(mime) = mime_from_extension(ext) {
            return mime.to_string();
        }
    }

    // 3. Final fallback: use the claimed type
    claimed.to_string()
}

/// Map common text/code extensions to MIME types (formats without magic bytes).
fn mime_from_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        // Text
        "txt" => Some("text/plain"),
        "csv" => Some("text/csv"),
        "tsv" => Some("text/tab-separated-values"),
        "log" => Some("text/plain"),
        // Markup
        "html" | "htm" => Some("text/html"),
        "xml" | "xsl" | "xslt" => Some("application/xml"),
        "json" => Some("application/json"),
        "yaml" | "yml" => Some("application/yaml"),
        "toml" => Some("application/toml"),
        // Markdown/docs
        "md" | "markdown" => Some("text/markdown"),
        "rst" => Some("text/x-rst"),
        "tex" | "latex" => Some("application/x-tex"),
        // Code
        "rs" => Some("text/x-rust"),
        "py" => Some("text/x-python"),
        "js" | "mjs" | "cjs" => Some("text/javascript"),
        "ts" | "tsx" => Some("text/typescript"),
        "jsx" => Some("text/jsx"),
        "go" => Some("text/x-go"),
        "java" => Some("text/x-java"),
        "c" | "h" => Some("text/x-c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("text/x-c++"),
        "cs" => Some("text/x-csharp"),
        "rb" => Some("text/x-ruby"),
        "php" => Some("text/x-php"),
        "swift" => Some("text/x-swift"),
        "kt" | "kts" => Some("text/x-kotlin"),
        "scala" => Some("text/x-scala"),
        "sql" => Some("application/sql"),
        "r" => Some("text/x-r"),
        "lua" => Some("text/x-lua"),
        "pl" | "pm" => Some("text/x-perl"),
        // Config
        "ini" | "cfg" | "conf" => Some("text/plain"),
        "env" => Some("text/plain"),
        "gitignore" | "dockerignore" => Some("text/plain"),
        "dockerfile" => Some("text/plain"),
        "makefile" => Some("text/plain"),
        _ => None,
    }
}

/// Sanitize filename for safe storage
pub fn sanitize_filename(filename: &str) -> String {
    // Remove path components
    let name = filename.rsplit(['/', '\\']).next().unwrap_or(filename);

    // Replace dangerous characters
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // Ensure not empty and not too long
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        return "unnamed_file".to_string();
    }

    // Truncate if too long (preserve extension)
    if sanitized.len() > 255 {
        if let Some(dot_pos) = sanitized.rfind('.') {
            let ext = &sanitized[dot_pos..];
            let name = &sanitized[..255 - ext.len()];
            return format!("{}{}", name, ext);
        }
        return sanitized[..255].to_string();
    }

    sanitized.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_png_magic_bytes() {
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = detect_content_type("fake.txt", &png, "text/plain");
        assert_eq!(result, "image/png");
    }

    #[test]
    fn test_detect_jpeg_magic_bytes() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        let result = detect_content_type("photo.jpg", &jpeg, "application/octet-stream");
        assert_eq!(result, "image/jpeg");
    }

    #[test]
    fn test_detect_pdf_magic_bytes() {
        let pdf = b"%PDF-1.4 fake content";
        let result = detect_content_type("doc.pdf", pdf, "application/octet-stream");
        assert_eq!(result, "application/pdf");
    }

    #[test]
    fn test_detect_overrides_wrong_claim() {
        // Client claims text/plain but file is actually PNG
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = detect_content_type("image.png", &png, "text/plain");
        assert_eq!(result, "image/png");
    }

    #[test]
    fn test_detect_falls_back_to_extension_for_text() {
        // Plain text has no magic bytes — falls back to extension
        let result = detect_content_type("notes.md", b"# Hello world", "application/octet-stream");
        assert_eq!(result, "text/markdown");
    }

    #[test]
    fn test_detect_falls_back_to_claimed_for_unknown() {
        let result = detect_content_type("data.xyz", b"random bytes", "application/custom");
        assert_eq!(result, "application/custom");
    }

    #[test]
    fn test_detect_csv_by_extension() {
        let result = detect_content_type(
            "data.csv",
            b"name,age\nAlice,30",
            "application/octet-stream",
        );
        assert_eq!(result, "text/csv");
    }

    #[test]
    fn test_blocks_exe() {
        // Extension is checked first, so .exe files get blocked by extension
        let result = validate_file("malware.exe", b"MZ\x90\x00", 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains(".exe"));
    }

    #[test]
    fn test_blocks_pe_magic() {
        // Test PE magic detection without exe extension
        let result = validate_file("malware.bin", b"MZ\x90\x00", 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains("Windows PE"));
    }

    #[test]
    fn test_blocks_elf() {
        let result = validate_file("binary", b"\x7FELF\x02\x01\x01", 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains("ELF"));
    }

    #[test]
    fn test_blocks_extension() {
        let result = validate_file("script.sh", b"echo hello", 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains(".sh"));
    }

    #[test]
    fn test_allows_image() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let result = validate_file("image.png", &png_header, 100_000_000);
        assert!(result.allowed);
    }

    #[test]
    fn test_sanitize_removes_path() {
        assert_eq!(sanitize_filename("/etc/passwd"), "passwd");
        assert_eq!(
            sanitize_filename("C:\\Windows\\system32.dll"),
            "system32.dll"
        );
    }

    #[test]
    fn test_blocks_shebang() {
        let result = validate_file("script.txt", b"#!/bin/bash\necho hello", 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains("script"));
    }

    #[test]
    fn test_blocks_macho() {
        let result = validate_file("binary", &[0xFE, 0xED, 0xFA, 0xCE, 0x00], 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains("Mach-O"));
    }

    #[test]
    fn test_blocks_wasm() {
        let result = validate_file("module.wasm", &[0x00, 0x61, 0x73, 0x6D], 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains("WebAssembly"));
    }

    #[test]
    fn test_blocks_oversized() {
        let large_data = vec![0u8; 101];
        let result = validate_file("large.txt", &large_data, 100);
        assert!(!result.allowed);
        assert!(result
            .block_reason
            .unwrap()
            .contains("exceeds maximum size"));
    }

    #[test]
    fn test_sanitize_removes_dangerous_chars() {
        assert_eq!(sanitize_filename("file<>:test.txt"), "file___test.txt");
        assert_eq!(sanitize_filename("file|name?.txt"), "file_name_.txt");
    }

    #[test]
    fn test_sanitize_truncates_long_names() {
        let long_name = format!("{}.txt", "a".repeat(300));
        let sanitized = sanitize_filename(&long_name);
        assert!(sanitized.len() <= 255);
        assert!(sanitized.ends_with(".txt"));
    }

    #[test]
    fn test_sanitize_handles_empty() {
        assert_eq!(sanitize_filename(""), "unnamed_file");
        assert_eq!(sanitize_filename("   "), "unnamed_file");
    }

    #[test]
    fn test_allows_text_with_hash() {
        // Markdown heading should not be confused with shebang
        let result = validate_file("notes.md", b"# Heading\nSome text", 100_000_000);
        assert!(result.allowed);
    }

    #[test]
    fn test_blocks_java_class() {
        let result = validate_file("Evil.class", &[0xCA, 0xFE, 0xBA, 0xBE, 0x00], 100_000_000);
        assert!(!result.allowed);
        // Extension check happens first
        assert!(result.block_reason.unwrap().contains(".class"));
    }

    #[test]
    fn test_blocks_jar_extension() {
        let result = validate_file("malware.jar", b"PK\x03\x04", 100_000_000);
        assert!(!result.allowed);
        assert!(result.block_reason.unwrap().contains(".jar"));
    }

    #[test]
    fn test_allows_pdf() {
        let pdf_header = b"%PDF-1.4";
        let result = validate_file("document.pdf", pdf_header, 100_000_000);
        assert!(result.allowed);
    }

    #[test]
    fn test_allows_zip() {
        let zip_header = b"PK\x03\x04";
        let result = validate_file("archive.zip", zip_header, 100_000_000);
        assert!(result.allowed);
    }

    #[test]
    fn test_upload_size_boundary_at_default_limit() {
        use crate::defaults::MAX_UPLOAD_SIZE_BYTES;
        let limit = MAX_UPLOAD_SIZE_BYTES as u64;

        // Exactly at limit — should be allowed
        let data_at_limit = vec![b'A'; MAX_UPLOAD_SIZE_BYTES];
        let result = validate_file("big.txt", &data_at_limit, limit);
        assert!(
            result.allowed,
            "File exactly at MAX_UPLOAD_SIZE_BYTES should be allowed"
        );

        // One byte over — should be blocked
        let data_over_limit = vec![b'A'; MAX_UPLOAD_SIZE_BYTES + 1];
        let result = validate_file("toobig.txt", &data_over_limit, limit);
        assert!(
            !result.allowed,
            "File one byte over MAX_UPLOAD_SIZE_BYTES should be blocked"
        );
        assert!(result
            .block_reason
            .unwrap()
            .contains("exceeds maximum size"));
    }

    #[test]
    fn test_upload_size_boundary_custom_limit() {
        // Simulate a custom limit (e.g. operator sets MATRIC_MAX_UPLOAD_SIZE_BYTES=10MB)
        let custom_limit: u64 = 10 * 1024 * 1024;

        let data_at = vec![b'A'; custom_limit as usize];
        let result = validate_file("file.txt", &data_at, custom_limit);
        assert!(
            result.allowed,
            "File exactly at custom limit should be allowed"
        );

        let data_over = vec![b'A'; custom_limit as usize + 1];
        let result = validate_file("file.txt", &data_over, custom_limit);
        assert!(!result.allowed, "File over custom limit should be blocked");
    }
}

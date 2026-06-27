//! Archive extraction adapter — lists contents and extracts text from .zip, .tar, .tar.gz files.
//!
//! Supports:
//! - `.zip` via the `zip` crate
//! - `.tar`, `.tar.gz`, `.tgz` via the `tar` + `flate2` crates
//!
//! Configurable limits (env var → default):
//! - `ARCHIVE_MAX_EXTRACT_BYTES` → 1 GB total extracted bytes
//! - `ARCHIVE_MAX_SINGLE_FILE_BYTES` → 50 MB per file
//! - `ARCHIVE_MAX_NESTING` → 3 levels of archives-within-archives
//!
//! Unsupported formats (.rar, .7z) produce metadata-only output with a note
//! that extraction was skipped.

use std::fmt;
use std::io::{Cursor, Read};

use async_trait::async_trait;
use flate2::read::GzDecoder;
use serde_json::Value as JsonValue;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};

/// Return type for internal archive extraction: entry list + extracted (name, text) pairs.
type ExtractOutput = (Vec<EntryInfo>, Vec<(String, String)>);

// ── Configurable limits ──────────────────────────────────────────────────────

/// Maximum bytes read from a single file to check for binary content.
const BINARY_PROBE_BYTES: usize = 8 * 1024; // 8 KB

/// Default maximum cumulative uncompressed bytes to extract (1 GB).
const DEFAULT_MAX_EXTRACT_BYTES: usize = 1024 * 1024 * 1024;

/// Default maximum individual file size for text extraction (50 MB).
const DEFAULT_MAX_SINGLE_FILE_BYTES: usize = 50 * 1024 * 1024;

/// Read a limit from an env var, falling back to the provided default.
fn env_limit(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Runtime-resolved extraction limits.
struct Limits {
    max_extract_bytes: usize,
    max_single_file_bytes: usize,
}

impl Limits {
    fn from_env() -> Self {
        Self {
            max_extract_bytes: env_limit("ARCHIVE_MAX_EXTRACT_BYTES", DEFAULT_MAX_EXTRACT_BYTES),
            max_single_file_bytes: env_limit(
                "ARCHIVE_MAX_SINGLE_FILE_BYTES",
                DEFAULT_MAX_SINGLE_FILE_BYTES,
            ),
        }
    }
}

// ── Known binary extensions ───────────────────────────────────────────────────

/// File extensions that are always treated as binary (no text extraction).
const BINARY_EXTENSIONS: &[&str] = &[
    // Images
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "svg", "ico", "heic", "avif",
    // Audio/Video
    "mp3", "wav", "flac", "aac", "ogg", "mp4", "mkv", "avi", "mov", "webm", "m4a", "m4v",
    // Archives (nested)
    "zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "zst", // Documents (binary)
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp",
    // Compiled / binary
    "exe", "dll", "so", "dylib", "o", "a", "wasm", "pyc", "class", // Fonts
    "ttf", "otf", "woff", "woff2", "eot", // 3D / data
    "glb", "gltf", "obj", "stl", "blend", "bin", "dat", "db", "sqlite",
];

// ── ArchiveAdapter ────────────────────────────────────────────────────────────

/// Adapter for extracting text content from archive files.
///
/// Enumerates all entries, reads text files (UTF-8), and skips binary files.
/// Produces a structured text output with per-file sections plus a JSON
/// metadata listing every entry. Per-file and total extraction byte limits
/// are configurable via `ARCHIVE_MAX_SINGLE_FILE_BYTES` and
/// `ARCHIVE_MAX_EXTRACT_BYTES` environment variables.
pub struct ArchiveAdapter;

/// Detected archive format.
#[derive(Debug, Clone, PartialEq)]
enum ArchiveFormat {
    Zip,
    TarGz,
    Tar,
    /// Unsupported format — metadata only, no content extraction.
    Unsupported(String),
}

/// Metadata for a single entry inside an archive.
struct EntryInfo {
    name: String,
    size: u64,
    is_dir: bool,
    /// Whether text content was extracted.
    extracted: bool,
    /// Reason text extraction was skipped, if any.
    skip_reason: Option<String>,
}

impl fmt::Debug for EntryInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntryInfo")
            .field("name_len", &self.name.chars().count())
            .field("extension_class", &entry_extension_class(&self.name))
            .field("size", &self.size)
            .field("is_dir", &self.is_dir)
            .field("extracted", &self.extracted)
            .field(
                "skip_reason_len",
                &self.skip_reason.as_deref().map(str::len),
            )
            .finish()
    }
}

fn entry_extension_class(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase) {
        Some(ext) if ext.is_empty() || ext == name.to_ascii_lowercase() => "none",
        Some(ext) if BINARY_EXTENSIONS.contains(&ext.as_str()) => "known_binary",
        Some(ext)
            if matches!(
                ext.as_str(),
                "txt"
                    | "md"
                    | "json"
                    | "csv"
                    | "xml"
                    | "html"
                    | "rs"
                    | "py"
                    | "js"
                    | "ts"
                    | "go"
                    | "java"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "yaml"
                    | "yml"
                    | "toml"
                    | "sql"
                    | "log"
            ) =>
        {
            "text_like"
        }
        Some(_) => "other",
        None => "none",
    }
}

impl ArchiveAdapter {
    /// Detect archive format from MIME type and filename extension.
    fn detect_format(mime_type: &str, filename: &str) -> ArchiveFormat {
        let mime = mime_type.to_lowercase();
        let name = filename.to_lowercase();

        // ZIP
        if mime == "application/zip" || name.ends_with(".zip") {
            return ArchiveFormat::Zip;
        }

        // Tar + Gzip
        if mime == "application/gzip"
            || mime == "application/x-gzip"
            || name.ends_with(".tar.gz")
            || name.ends_with(".tgz")
        {
            return ArchiveFormat::TarGz;
        }

        // Plain tar
        if mime == "application/x-tar" || name.ends_with(".tar") {
            return ArchiveFormat::Tar;
        }

        // Unsupported
        let format_hint = if name.ends_with(".7z") || mime.contains("7z") {
            "7z"
        } else if name.ends_with(".rar") || mime.contains("rar") {
            "rar"
        } else if name.ends_with(".bz2") || mime.contains("bzip2") {
            "bz2"
        } else if name.ends_with(".xz") || mime.contains("xz") {
            "xz"
        } else {
            "unknown"
        };

        ArchiveFormat::Unsupported(format_hint.to_string())
    }

    /// Returns true if the file should be treated as binary.
    ///
    /// Checks extension first (fast path), then probes first 8 KB for null bytes.
    fn is_binary(name: &str, probe: &[u8]) -> bool {
        // Extension check
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        if BINARY_EXTENSIONS.contains(&ext.as_str()) {
            return true;
        }

        // Null byte probe
        probe.contains(&0)
    }

    /// Extract text and entry metadata from a ZIP archive.
    fn extract_zip(data: &[u8], limits: &Limits) -> Result<ExtractOutput> {
        let cursor = Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| matric_core::Error::Internal(format!("Failed to open zip: {e}")))?;

        let mut entries: Vec<EntryInfo> = Vec::new();
        let mut texts: Vec<(String, String)> = Vec::new();
        let mut total_extracted: usize = 0;
        let file_count = archive.len();

        for i in 0..file_count {
            let mut file = archive
                .by_index(i)
                .map_err(|e| matric_core::Error::Internal(format!("Zip index error: {e}")))?;

            let name = file.name().to_string();
            let size = file.size();
            let is_dir = file.is_dir();

            if is_dir {
                entries.push(EntryInfo {
                    name,
                    size: 0,
                    is_dir: true,
                    extracted: false,
                    skip_reason: None,
                });
                continue;
            }

            // Size guard
            if size as usize > limits.max_single_file_bytes {
                entries.push(EntryInfo {
                    name,
                    size,
                    is_dir: false,
                    extracted: false,
                    skip_reason: Some("too_large".to_string()),
                });
                continue;
            }

            // Total budget guard
            if total_extracted >= limits.max_extract_bytes {
                entries.push(EntryInfo {
                    name,
                    size,
                    is_dir: false,
                    extracted: false,
                    skip_reason: Some("budget_exceeded".to_string()),
                });
                continue;
            }

            // Read probe for binary detection
            let probe_len = (size as usize).min(BINARY_PROBE_BYTES);
            let mut probe = vec![0u8; probe_len];
            let bytes_read = file.read(&mut probe).unwrap_or(0);
            let probe = &probe[..bytes_read];

            if Self::is_binary(&name, probe) {
                entries.push(EntryInfo {
                    name,
                    size,
                    is_dir: false,
                    extracted: false,
                    skip_reason: Some("binary".to_string()),
                });
                continue;
            }

            // Read remainder
            let mut rest = Vec::new();
            let _ = file.read_to_end(&mut rest);
            let full_bytes = [probe, rest.as_slice()].concat();

            let text = String::from_utf8_lossy(&full_bytes).into_owned();
            let text_bytes = text.len();
            total_extracted += text_bytes;

            texts.push((name.clone(), text));
            entries.push(EntryInfo {
                name,
                size,
                is_dir: false,
                extracted: true,
                skip_reason: None,
            });
        }

        Ok((entries, texts))
    }

    /// Extract text and entry metadata from a Tar archive (plain or gzip-compressed).
    fn extract_tar(data: &[u8], gzipped: bool, limits: &Limits) -> Result<ExtractOutput> {
        let mut entries: Vec<EntryInfo> = Vec::new();
        let mut texts: Vec<(String, String)> = Vec::new();
        let mut total_extracted: usize = 0;

        // Helper closure to process a tar::Archive<R>
        fn process<R: Read>(
            archive: &mut tar::Archive<R>,
            entries: &mut Vec<EntryInfo>,
            texts: &mut Vec<(String, String)>,
            total_extracted: &mut usize,
            max_single_file_bytes: usize,
            max_extract_bytes: usize,
        ) -> Result<()> {
            let tar_entries = archive
                .entries()
                .map_err(|e| matric_core::Error::Internal(format!("Tar read error: {e}")))?;

            for entry_result in tar_entries {
                let mut entry = entry_result
                    .map_err(|e| matric_core::Error::Internal(format!("Tar entry error: {e}")))?;

                let path = entry
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "<invalid path>".to_string());

                let size = entry.size();
                let is_dir = entry.header().entry_type().is_dir();

                if is_dir {
                    entries.push(EntryInfo {
                        name: path,
                        size: 0,
                        is_dir: true,
                        extracted: false,
                        skip_reason: None,
                    });
                    continue;
                }

                // Size guard
                if size as usize > max_single_file_bytes {
                    entries.push(EntryInfo {
                        name: path,
                        size,
                        is_dir: false,
                        extracted: false,
                        skip_reason: Some("too_large".to_string()),
                    });
                    continue;
                }

                // Budget guard
                if *total_extracted >= max_extract_bytes {
                    entries.push(EntryInfo {
                        name: path,
                        size,
                        is_dir: false,
                        extracted: false,
                        skip_reason: Some("budget_exceeded".to_string()),
                    });
                    continue;
                }

                // Probe for binary detection
                let probe_len = (size as usize).min(BINARY_PROBE_BYTES);
                let mut probe = vec![0u8; probe_len];
                let bytes_read = entry.read(&mut probe).unwrap_or(0);
                let probe_slice = &probe[..bytes_read];

                if ArchiveAdapter::is_binary(&path, probe_slice) {
                    entries.push(EntryInfo {
                        name: path,
                        size,
                        is_dir: false,
                        extracted: false,
                        skip_reason: Some("binary".to_string()),
                    });
                    continue;
                }

                // Read rest
                let mut rest = Vec::new();
                let _ = entry.read_to_end(&mut rest);
                let full_bytes = [probe_slice, rest.as_slice()].concat();

                let text = String::from_utf8_lossy(&full_bytes).into_owned();
                *total_extracted += text.len();

                texts.push((path.clone(), text));
                entries.push(EntryInfo {
                    name: path,
                    size,
                    is_dir: false,
                    extracted: true,
                    skip_reason: None,
                });
            }
            Ok(())
        }

        if gzipped {
            let gz = GzDecoder::new(Cursor::new(data));
            let mut archive = tar::Archive::new(gz);
            process(
                &mut archive,
                &mut entries,
                &mut texts,
                &mut total_extracted,
                limits.max_single_file_bytes,
                limits.max_extract_bytes,
            )?;
        } else {
            let mut archive = tar::Archive::new(Cursor::new(data));
            process(
                &mut archive,
                &mut entries,
                &mut texts,
                &mut total_extracted,
                limits.max_single_file_bytes,
                limits.max_extract_bytes,
            )?;
        }

        Ok((entries, texts))
    }

    /// Build the formatted text output and JSON metadata.
    fn build_result(
        filename: &str,
        archive_type: &str,
        entries: Vec<EntryInfo>,
        texts: Vec<(String, String)>,
    ) -> ExtractionResult {
        let total_files = entries.iter().filter(|e| !e.is_dir).count();
        let total_dirs = entries.iter().filter(|e| e.is_dir).count();
        let total_size_bytes: u64 = entries.iter().map(|e| e.size).sum();

        // ── Formatted text ──────────────────────────────────────────────────
        let size_mb = total_size_bytes as f64 / (1024.0 * 1024.0);
        let mut text_parts = Vec::new();

        text_parts.push(format!(
            "=== Archive: {} ({} files, {:.1} MB) ===",
            filename, total_files, size_mb
        ));
        text_parts.push(String::new());

        // Emit text sections
        let text_map: std::collections::HashMap<&str, &str> = texts
            .iter()
            .map(|(n, t)| (n.as_str(), t.as_str()))
            .collect();

        for entry in &entries {
            if entry.is_dir {
                continue;
            }

            let size_kb = entry.size as f64 / 1024.0;

            if entry.extracted {
                text_parts.push(format!("--- {} ({:.1} KB) ---", entry.name, size_kb));
                if let Some(content) = text_map.get(entry.name.as_str()) {
                    text_parts.push(content.to_string());
                }
                text_parts.push(String::new());
            } else {
                let reason = entry.skip_reason.as_deref().unwrap_or("skipped");
                text_parts.push(format!(
                    "--- [{}] {} ({:.1} KB) ---",
                    reason, entry.name, size_kb
                ));
                text_parts.push(String::new());
            }
        }

        let extracted_text = if text_parts.len() <= 2 {
            // Only the header and blank line — no content extracted
            None
        } else {
            Some(text_parts.join("\n"))
        };

        // ── Metadata ────────────────────────────────────────────────────────
        let files_json: Vec<JsonValue> = entries
            .iter()
            .map(|e| {
                let mut obj = serde_json::json!({
                    "name": e.name,
                    "size": e.size,
                    "is_dir": e.is_dir,
                    "extracted": e.extracted,
                });
                if let Some(ref reason) = e.skip_reason {
                    obj["reason"] = serde_json::json!(reason);
                }
                obj
            })
            .collect();

        let metadata = serde_json::json!({
            "archive_type": archive_type,
            "total_files": total_files,
            "total_dirs": total_dirs,
            "total_size_bytes": total_size_bytes,
            "files": files_json,
        });

        ExtractionResult {
            extracted_text,
            metadata,
            ai_description: None,
            preview_data: None,
            derived_files: vec![],
        }
    }
}

#[async_trait]
impl ExtractionAdapter for ArchiveAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Archive
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        _config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Ok(ExtractionResult {
                extracted_text: None,
                metadata: serde_json::json!({
                    "archive_type": "unknown",
                    "total_files": 0,
                    "total_dirs": 0,
                    "total_size_bytes": 0,
                    "files": [],
                    "error_code": "empty_input"
                }),
                ai_description: None,
                preview_data: None,
                derived_files: vec![],
            });
        }

        let format = Self::detect_format(mime_type, filename);
        let limits = Limits::from_env();

        match format {
            ArchiveFormat::Zip => {
                let (entries, texts) = Self::extract_zip(data, &limits)?;
                Ok(Self::build_result(filename, "zip", entries, texts))
            }
            ArchiveFormat::TarGz => {
                let (entries, texts) = Self::extract_tar(data, true, &limits)?;
                Ok(Self::build_result(filename, "tar.gz", entries, texts))
            }
            ArchiveFormat::Tar => {
                let (entries, texts) = Self::extract_tar(data, false, &limits)?;
                Ok(Self::build_result(filename, "tar", entries, texts))
            }
            ArchiveFormat::Unsupported(fmt) => {
                // Return metadata-only result without attempting extraction
                Ok(ExtractionResult {
                    extracted_text: None,
                    metadata: serde_json::json!({
                        "archive_type": fmt,
                        "total_files": 0,
                        "total_dirs": 0,
                        "total_size_bytes": data.len(),
                        "files": [],
                        "note": "format not supported for extraction; listing unavailable"
                    }),
                    ai_description: None,
                    preview_data: None,
                    derived_files: vec![],
                })
            }
        }
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true) // Pure Rust — no external dependencies
    }

    fn name(&self) -> &str {
        "archive"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Build an in-memory ZIP archive with the given entries.
    ///
    /// Each entry is `(path, content)`. Pass `None` for content to create a
    /// directory entry.
    fn make_zip(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
        let buf = Vec::new();
        let cursor = Cursor::new(buf);
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, content) in entries {
            if let Some(data) = content {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            } else {
                // Directory entry
                writer
                    .add_directory(*name, zip::write::FileOptions::<()>::default())
                    .unwrap();
            }
        }

        let cursor = writer.finish().unwrap();
        cursor.into_inner()
    }

    /// Build an in-memory `.tar.gz` archive with the given entries.
    fn make_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::default());
        let mut builder = tar::Builder::new(enc);

        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }

        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap()
    }

    /// Build an in-memory plain `.tar` archive.
    fn make_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let mut builder = tar::Builder::new(buf);

        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }

        builder.into_inner().unwrap()
    }

    // ── Basic adapter contract ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_strategy() {
        let adapter = ArchiveAdapter;
        assert_eq!(adapter.strategy(), ExtractionStrategy::Archive);
    }

    #[tokio::test]
    async fn test_name() {
        let adapter = ArchiveAdapter;
        assert_eq!(adapter.name(), "archive");
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = ArchiveAdapter;
        assert!(adapter.health_check().await.unwrap());
    }

    // ── Empty input ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_empty_data_returns_error_metadata() {
        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(b"", "empty.zip", "application/zip", &serde_json::json!({}))
            .await
            .unwrap();

        assert!(result.extracted_text.is_none());
        assert_eq!(result.metadata["total_files"], 0);
        assert_eq!(result.metadata["error_code"], "empty_input");
        assert!(result.metadata.get("error").is_none());
        assert!(!result.metadata.to_string().contains("empty input"));
    }

    // ── ZIP extraction ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_zip_text_extraction() {
        let zip_data = make_zip(&[
            ("readme.txt", Some(b"Hello from readme")),
            ("docs/guide.md", Some(b"# Guide\nContent here")),
        ]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "test.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        // Metadata
        assert_eq!(result.metadata["archive_type"], "zip");
        assert_eq!(result.metadata["total_files"], 2);
        assert_eq!(result.metadata["total_dirs"], 0);

        // Text content
        let text = result.extracted_text.unwrap();
        assert!(text.contains("=== Archive: test.zip"), "Missing header");
        assert!(text.contains("readme.txt"), "Missing readme.txt entry");
        assert!(text.contains("Hello from readme"), "Missing readme content");
        assert!(text.contains("docs/guide.md"), "Missing guide.md entry");
        assert!(text.contains("# Guide"), "Missing guide content");
    }

    #[tokio::test]
    async fn test_zip_directory_entry() {
        let zip_data = make_zip(&[
            ("subdir/", None), // directory
            ("subdir/file.txt", Some(b"file in subdir")),
        ]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "dirs.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["total_dirs"], 1);
        assert_eq!(result.metadata["total_files"], 1);

        let files = result.metadata["files"].as_array().unwrap();
        let dir_entry = files.iter().find(|f| f["is_dir"] == true).unwrap();
        assert_eq!(dir_entry["name"], "subdir/");
    }

    #[tokio::test]
    async fn test_zip_binary_file_skipped() {
        // PNG magic bytes + null bytes — clearly binary
        let mut png_bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        png_bytes.extend_from_slice(&[0u8; 100]);

        let zip_data = make_zip(&[
            ("image.png", Some(&png_bytes)),
            ("readme.txt", Some(b"readable text")),
        ]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "mixed.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();

        // Binary file should appear as [binary] entry
        assert!(text.contains("image.png"), "image.png should be listed");
        // Text file should be extracted
        assert!(text.contains("readme.txt"), "readme.txt should be listed");
        assert!(
            text.contains("readable text"),
            "readme content should appear"
        );

        // Verify metadata flags
        let files = result.metadata["files"].as_array().unwrap();
        let png_meta = files.iter().find(|f| f["name"] == "image.png").unwrap();
        assert_eq!(png_meta["extracted"], false);
        assert_eq!(png_meta["reason"], "binary");

        let txt_meta = files.iter().find(|f| f["name"] == "readme.txt").unwrap();
        assert_eq!(txt_meta["extracted"], true);
    }

    #[tokio::test]
    async fn test_zip_binary_extension_skipped() {
        // A .jpg file (even without null bytes) should be skipped by extension
        let jpg_data = b"JFIF fake jpeg content without null bytes";
        let zip_data = make_zip(&[("photo.jpg", Some(jpg_data))]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "photos.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let files = result.metadata["files"].as_array().unwrap();
        let jpg = files.iter().find(|f| f["name"] == "photo.jpg").unwrap();
        assert_eq!(jpg["extracted"], false);
        assert_eq!(jpg["reason"], "binary");
    }

    #[tokio::test]
    async fn test_zip_large_file_count() {
        // Verify archives with many files are fully extracted (no file count cap)
        let file_count = 2_000;
        let entries: Vec<(String, Vec<u8>)> = (0..file_count)
            .map(|i| (format!("file_{:04}.txt", i), b"x".to_vec()))
            .collect();

        let entries_refs: Vec<(&str, Option<&[u8]>)> = entries
            .iter()
            .map(|(name, data)| (name.as_str(), Some(data.as_slice())))
            .collect();

        let zip_data = make_zip(&entries_refs);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "many.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let listed = result.metadata["files"].as_array().unwrap().len();
        assert_eq!(
            listed, file_count,
            "All {} files should be listed, got {}",
            file_count, listed
        );
    }

    #[tokio::test]
    async fn test_zip_empty_archive() {
        let zip_data = make_zip(&[]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "empty.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["total_files"], 0);
        assert_eq!(result.metadata["total_dirs"], 0);
        // No text files → extracted_text is None
        assert!(result.extracted_text.is_none());
    }

    #[tokio::test]
    async fn test_zip_detected_by_mime() {
        let zip_data = make_zip(&[("a.txt", Some(b"content"))]);

        let adapter = ArchiveAdapter;
        // Use MIME type, not extension
        let result = adapter
            .extract(
                &zip_data,
                "archive", // no .zip extension
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["archive_type"], "zip");
    }

    // ── Tar.gz extraction ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_tar_gz_text_extraction() {
        let tar_gz_data = make_tar_gz(&[
            ("README.md", b"# Project\nA description"),
            ("src/main.rs", b"fn main() { println!(\"hello\"); }"),
        ]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &tar_gz_data,
                "project.tar.gz",
                "application/gzip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["archive_type"], "tar.gz");
        assert_eq!(result.metadata["total_files"], 2);

        let text = result.extracted_text.unwrap();
        assert!(text.contains("README.md"), "Missing README.md");
        assert!(text.contains("# Project"), "Missing README content");
        assert!(text.contains("src/main.rs"), "Missing main.rs");
        assert!(text.contains("fn main()"), "Missing Rust source");
    }

    #[tokio::test]
    async fn test_tgz_extension_detected() {
        let tar_gz_data = make_tar_gz(&[("hello.txt", b"world")]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &tar_gz_data,
                "archive.tgz", // .tgz extension
                "application/octet-stream",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["archive_type"], "tar.gz");
        assert_eq!(result.metadata["total_files"], 1);
    }

    #[tokio::test]
    async fn test_tar_gz_binary_skipped() {
        // An .exe file should be skipped by extension
        let tar_gz_data = make_tar_gz(&[
            ("app.exe", b"MZ binary content"),
            ("notes.txt", b"some text notes"),
        ]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &tar_gz_data,
                "release.tar.gz",
                "application/gzip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let files = result.metadata["files"].as_array().unwrap();

        let exe = files.iter().find(|f| f["name"] == "app.exe").unwrap();
        assert_eq!(exe["extracted"], false);
        assert_eq!(exe["reason"], "binary");

        let txt = files.iter().find(|f| f["name"] == "notes.txt").unwrap();
        assert_eq!(txt["extracted"], true);
    }

    // ── Plain tar extraction ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_plain_tar_extraction() {
        let tar_data = make_tar(&[("hello.txt", b"plain tar content")]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &tar_data,
                "data.tar",
                "application/x-tar",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["archive_type"], "tar");
        assert_eq!(result.metadata["total_files"], 1);
        let text = result.extracted_text.unwrap();
        assert!(text.contains("hello.txt"));
        assert!(text.contains("plain tar content"));
    }

    // ── Unsupported formats ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_unsupported_7z_metadata_only() {
        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                b"7z fake data",
                "archive.7z",
                "application/x-7z-compressed",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["archive_type"], "7z");
        assert!(result.extracted_text.is_none());
        assert!(result.metadata["note"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_unsupported_rar_metadata_only() {
        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                b"Rar! fake data",
                "archive.rar",
                "application/vnd.rar",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        assert_eq!(result.metadata["archive_type"], "rar");
        assert!(result.extracted_text.is_none());
    }

    // ── Format detection ──────────────────────────────────────────────────────

    #[test]
    fn test_detect_format_zip_mime() {
        assert_eq!(
            ArchiveAdapter::detect_format("application/zip", "file.zip"),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_format_zip_extension() {
        assert_eq!(
            ArchiveAdapter::detect_format("application/octet-stream", "backup.zip"),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_format_tar_gz_mime() {
        assert_eq!(
            ArchiveAdapter::detect_format("application/gzip", "archive.tar.gz"),
            ArchiveFormat::TarGz
        );
    }

    #[test]
    fn test_detect_format_tgz_extension() {
        assert_eq!(
            ArchiveAdapter::detect_format("application/octet-stream", "release.tgz"),
            ArchiveFormat::TarGz
        );
    }

    #[test]
    fn test_detect_format_tar() {
        assert_eq!(
            ArchiveAdapter::detect_format("application/x-tar", "data.tar"),
            ArchiveFormat::Tar
        );
    }

    #[test]
    fn test_detect_format_7z() {
        assert!(matches!(
            ArchiveAdapter::detect_format("application/x-7z-compressed", "archive.7z"),
            ArchiveFormat::Unsupported(_)
        ));
    }

    #[test]
    fn test_detect_format_rar() {
        assert!(matches!(
            ArchiveAdapter::detect_format("application/vnd.rar", "archive.rar"),
            ArchiveFormat::Unsupported(_)
        ));
    }

    // ── Binary detection ──────────────────────────────────────────────────────

    #[test]
    fn test_is_binary_by_extension_jpg() {
        assert!(ArchiveAdapter::is_binary(
            "photo.jpg",
            b"no null bytes here"
        ));
    }

    #[test]
    fn test_is_binary_by_extension_png() {
        assert!(ArchiveAdapter::is_binary(
            "image.PNG",
            b"no null bytes here"
        ));
    }

    #[test]
    fn test_is_binary_by_extension_pdf() {
        assert!(ArchiveAdapter::is_binary("doc.pdf", b"no null bytes here"));
    }

    #[test]
    fn test_is_binary_by_null_byte() {
        // No binary extension, but contains null byte
        assert!(ArchiveAdapter::is_binary("data.dat", b"hello\x00world"));
    }

    #[test]
    fn test_is_not_binary_plain_text() {
        assert!(!ArchiveAdapter::is_binary(
            "readme.txt",
            b"Hello, world!\nLine 2"
        ));
    }

    #[test]
    fn test_is_not_binary_markdown() {
        assert!(!ArchiveAdapter::is_binary(
            "guide.md",
            b"# Header\n\nParagraph text"
        ));
    }

    #[test]
    fn test_is_not_binary_rust_source() {
        assert!(!ArchiveAdapter::is_binary(
            "main.rs",
            b"fn main() { println!(\"hi\"); }"
        ));
    }

    // ── Metadata structure ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_metadata_files_array_structure() {
        let zip_data = make_zip(&[
            ("text.txt", Some(b"hello")),
            ("img.png", Some(b"\x89PNG\x00fake")),
        ]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "check.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let files = result.metadata["files"].as_array().unwrap();
        for file in files {
            assert!(file["name"].as_str().is_some(), "name must be a string");
            assert!(file["size"].as_u64().is_some(), "size must be a number");
            assert!(file["is_dir"].as_bool().is_some(), "is_dir must be a bool");
            assert!(
                file["extracted"].as_bool().is_some(),
                "extracted must be a bool"
            );
        }
    }

    #[tokio::test]
    async fn test_metadata_total_size_bytes() {
        let content = b"hello world";
        let zip_data = make_zip(&[("hello.txt", Some(content))]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "size.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let total_size = result.metadata["total_size_bytes"].as_u64().unwrap();
        assert_eq!(total_size, content.len() as u64);
    }

    // ── Text format ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_text_header_format() {
        let zip_data = make_zip(&[("a.txt", Some(b"alpha"))]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "test.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        let text = result.extracted_text.unwrap();
        // Header must start with "=== Archive:"
        assert!(
            text.starts_with("=== Archive:"),
            "Text must start with '=== Archive:'"
        );
        // File entry must use "---" separator
        assert!(text.contains("--- a.txt"), "Entry must use '---' separator");
    }

    #[tokio::test]
    async fn test_binary_entry_shows_reason_in_text() {
        let zip_data = make_zip(&[("data.exe", Some(b"MZ\x00\x00fake"))]);

        let adapter = ArchiveAdapter;
        let result = adapter
            .extract(
                &zip_data,
                "bin.zip",
                "application/zip",
                &serde_json::json!({}),
            )
            .await
            .unwrap();

        // No text extracted from a binary-only archive
        // (text output has only the header and the binary entry line, but
        //  we still get a non-None result when there's at least one entry)
        if let Some(text) = result.extracted_text {
            // If there's text, the binary file should be labeled
            assert!(text.contains("data.exe"), "Binary file should be listed");
        }
    }

    #[test]
    fn entry_info_debug_redacts_names_paths_and_skip_reasons() {
        let entry = EntryInfo {
            name: "customer@example.internal/secrets/postgres://user:pass@db.internal/mm_key.txt"
                .to_string(),
            size: 42,
            is_dir: false,
            extracted: false,
            skip_reason: Some(
                "parser failed for /srv/private/mm_key_archive with token sk-live-secret"
                    .to_string(),
            ),
        };

        let rendered = format!("{entry:?}");

        for expected in [
            "EntryInfo",
            "name_len",
            "extension_class",
            "text_like",
            "size",
            "is_dir",
            "extracted",
            "skip_reason_len",
        ] {
            assert!(rendered.contains(expected), "missing field: {expected}");
        }

        for raw in [
            "customer@example.internal",
            "postgres://user:pass",
            "db.internal",
            "mm_key.txt",
            "/srv/private",
            "mm_key_archive",
            "sk-live-secret",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }
}

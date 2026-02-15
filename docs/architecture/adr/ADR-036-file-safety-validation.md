# ADR-036: File Safety Validation and Executable Blocking

**Status:** Implemented
**Date:** 2026-02-02
**Deciders:** Architecture team
**Related:** ADR-031 (Intelligent Attachment Processing), ADR-033 (File Storage), Epic #430

## Context

Matric-memory accepts a wide variety of file types for intelligent processing - images, videos, documents, 3D models, music files, and more. However, we must **never accept binary executables** or other potentially dangerous files that could:

1. Be used to store/distribute malware
2. Execute unintended code if served incorrectly
3. Exploit vulnerabilities in processing pipelines
4. Create legal liability for the platform

### Risk Categories

| Risk Level | File Types | Action |
|------------|-----------|--------|
| **BLOCK** | Executables, scripts, archives with executables | Reject immediately |
| **WARN** | Office macros, PDFs with JavaScript | Accept with flag |
| **SCAN** | All accepted files | Virus scan (optional) |
| **ALLOW** | Safe content types | Normal processing |

## Decision

Implement **multi-layer file safety validation** using magic byte detection, extension checking, and configurable blocklists.

### 1. Blocked File Types (Hard Block)

These files are **never accepted**, regardless of extension or claimed MIME type:

#### Executable Binaries
```
# Windows
.exe, .dll, .sys, .drv, .ocx, .scr, .pif, .com, .bat, .cmd, .ps1, .psm1, .vbs, .vbe, .js, .jse, .ws, .wsf, .wsc, .wsh, .msc

# Linux/Unix
.elf, .so, .out, .run, .bin (with ELF header), .sh, .bash, .zsh, .csh, .fish

# macOS
.app, .dmg, .pkg, .dylib, .kext, .command

# Java/JVM
.jar, .war, .ear, .class

# .NET
.msi, .msix, .appx

# Python (bytecode)
.pyc, .pyo, .pyd

# Other
.deb, .rpm, .apk (Android), .ipa (iOS)
```

#### Archives (may contain executables)
```
# Blocked by default (configurable)
.zip, .rar, .7z, .tar, .gz, .bz2, .xz, .cab, .iso, .img

# Note: Can be enabled with scan_archives=true config
```

#### Active Content
```
# Office macros
.xlsm, .xlsb, .xltm, .docm, .dotm, .pptm, .potm, .ppam, .sldm

# HTA (HTML Application)
.hta, .htc

# Compiled help
.chm, .hlp

# Browser extensions
.crx, .xpi

# Registry
.reg
```

### 2. Magic Byte Detection

Extensions can be spoofed. We validate using magic bytes (file signatures):

```rust
// crates/matric-core/src/file_safety.rs

use std::io::Read;

/// Known dangerous magic bytes
pub static BLOCKED_MAGIC: &[(&[u8], &str)] = &[
    // Windows PE executable
    (b"MZ", "Windows executable (PE/MZ)"),

    // ELF (Linux/Unix executable)
    (b"\x7fELF", "Linux executable (ELF)"),

    // Mach-O (macOS executable)
    (b"\xfe\xed\xfa\xce", "macOS executable (Mach-O 32-bit)"),
    (b"\xfe\xed\xfa\xcf", "macOS executable (Mach-O 64-bit)"),
    (b"\xca\xfe\xba\xbe", "macOS universal binary"),
    (b"\xcf\xfa\xed\xfe", "macOS executable (Mach-O 64-bit LE)"),

    // Java class file
    (b"\xca\xfe\xba\xbe", "Java class file"),

    // DEX (Android)
    (b"dex\n", "Android DEX"),

    // WebAssembly
    (b"\x00asm", "WebAssembly binary"),

    // Shell scripts (with shebang)
    (b"#!/", "Script with shebang"),
    (b"#! /", "Script with shebang (space)"),

    // Windows shortcuts
    (b"\x4c\x00\x00\x00\x01\x14\x02\x00", "Windows shortcut (LNK)"),

    // Compiled Python
    // Note: Magic varies by Python version, check multiple
];

/// Archive formats (configurable block)
pub static ARCHIVE_MAGIC: &[(&[u8], &str)] = &[
    (b"PK\x03\x04", "ZIP archive"),
    (b"PK\x05\x06", "ZIP archive (empty)"),
    (b"Rar!\x1a\x07", "RAR archive"),
    (b"\x1f\x8b", "GZIP"),
    (b"BZh", "BZIP2"),
    (b"\xfd7zXZ\x00", "XZ"),
    (b"7z\xbc\xaf\x27\x1c", "7-Zip"),
];

pub struct FileSafetyCheck {
    pub is_safe: bool,
    pub blocked_reason: Option<String>,
    pub warnings: Vec<String>,
    pub detected_type: Option<String>,
    pub magic_match: Option<String>,
}

pub fn check_file_safety(data: &[u8], filename: &str, config: &SafetyConfig) -> FileSafetyCheck {
    let mut result = FileSafetyCheck {
        is_safe: true,
        blocked_reason: None,
        warnings: vec![],
        detected_type: None,
        magic_match: None,
    };

    // 1. Check magic bytes against blocklist
    for (magic, description) in BLOCKED_MAGIC {
        if data.starts_with(magic) {
            result.is_safe = false;
            result.blocked_reason = Some(format!(
                "Blocked file type detected: {} (magic bytes match)",
                description
            ));
            result.magic_match = Some(description.to_string());
            return result;
        }
    }

    // 2. Check archives (configurable)
    if !config.allow_archives {
        for (magic, description) in ARCHIVE_MAGIC {
            if data.starts_with(magic) {
                result.is_safe = false;
                result.blocked_reason = Some(format!(
                    "Archive files not allowed: {}",
                    description
                ));
                return result;
            }
        }
    }

    // 3. Check extension blocklist
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    if let Some(ext) = &ext {
        if config.blocked_extensions.contains(ext) {
            result.is_safe = false;
            result.blocked_reason = Some(format!(
                "File extension '{}' is not allowed",
                ext
            ));
            return result;
        }
    }

    // 4. Check for extension/content mismatch (warning)
    if let Some(detected) = detect_content_type(data) {
        result.detected_type = Some(detected.clone());

        if let Some(ext) = &ext {
            if !content_type_matches_extension(&detected, ext) {
                result.warnings.push(format!(
                    "Extension '{}' doesn't match detected content type '{}'",
                    ext, detected
                ));
            }
        }
    }

    // 5. Check for suspicious patterns in text files
    if is_text_like(data) {
        if contains_script_injection(data) {
            result.warnings.push(
                "File contains potential script injection patterns".to_string()
            );
        }
    }

    result
}
```

### 3. Configuration

```rust
/// File safety configuration
#[derive(Debug, Clone, Deserialize)]
pub struct SafetyConfig {
    /// Allow archive uploads (default: false)
    pub allow_archives: bool,

    /// Scan archives for executables if allowed (default: true)
    pub scan_archives: bool,

    /// Allow Office documents with macros (default: false)
    pub allow_macros: bool,

    /// Maximum file size in bytes (default: 100MB)
    pub max_file_size: u64,

    /// Additional blocked extensions
    pub blocked_extensions: HashSet<String>,

    /// Additional allowed extensions (overrides defaults)
    pub allowed_extensions: HashSet<String>,

    /// Enable ClamAV scanning (default: false)
    pub enable_virus_scan: bool,

    /// Quarantine suspicious files instead of rejecting
    pub quarantine_mode: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allow_archives: false,
            scan_archives: true,
            allow_macros: false,
            max_file_size: 100 * 1024 * 1024,  // 100MB
            blocked_extensions: default_blocked_extensions(),
            allowed_extensions: HashSet::new(),
            enable_virus_scan: false,
            quarantine_mode: false,
        }
    }
}
```

### 4. Allowed File Types (Whitelist)

Rather than just blocking, we maintain an explicit allowlist:

```rust
/// Explicitly allowed content types
pub static ALLOWED_CONTENT_TYPES: &[&str] = &[
    // Images
    "image/jpeg", "image/png", "image/gif", "image/webp",
    "image/svg+xml", "image/bmp", "image/tiff", "image/heic", "image/heif",
    "image/avif", "image/x-icon",

    // Video
    "video/mp4", "video/webm", "video/ogg", "video/quicktime",
    "video/x-msvideo", "video/x-matroska", "video/3gpp",

    // Audio
    "audio/mpeg", "audio/ogg", "audio/wav", "audio/webm",
    "audio/aac", "audio/flac", "audio/x-m4a",
    "audio/midi", "audio/x-midi",
    // Tracker formats
    "audio/x-mod", "audio/x-s3m", "audio/x-xm", "audio/x-it",

    // Documents
    "application/pdf",
    "text/plain", "text/markdown", "text/csv", "text/html",
    "application/json", "application/xml",
    "application/rtf",

    // Office (non-macro versions only)
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",  // .docx
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",        // .xlsx
    "application/vnd.openxmlformats-officedocument.presentationml.presentation", // .pptx
    "application/vnd.oasis.opendocument.text",        // .odt
    "application/vnd.oasis.opendocument.spreadsheet", // .ods

    // 3D Models
    "model/gltf-binary", "model/gltf+json",
    "model/stl", "model/obj",
    "application/x-ply",

    // Code (text-based, no bytecode)
    "text/x-python", "text/x-rust", "text/x-c", "text/x-java",
    "application/javascript", "text/typescript",

    // Diagrams
    "text/x-mermaid", "text/vnd.graphviz",

    // Geospatial
    "application/geo+json", "application/gpx+xml",
    "application/vnd.google-earth.kml+xml",

    // Data
    "application/x-yaml", "application/toml",
];
```

### 5. Processing Pipeline Integration

```rust
// In upload handler

pub async fn upload_attachment(
    file_data: Vec<u8>,
    filename: String,
    config: &SafetyConfig,
) -> Result<AttachmentId, UploadError> {
    // 1. Size check
    if file_data.len() as u64 > config.max_file_size {
        return Err(UploadError::FileTooLarge {
            size: file_data.len(),
            max: config.max_file_size,
        });
    }

    // 2. Safety check
    let safety = check_file_safety(&file_data, &filename, config);

    if !safety.is_safe {
        return Err(UploadError::BlockedFileType {
            reason: safety.blocked_reason.unwrap_or_default(),
            detected: safety.magic_match,
        });
    }

    // 3. Log warnings
    for warning in &safety.warnings {
        tracing::warn!(filename = %filename, warning = %warning, "File safety warning");
    }

    // 4. Optional virus scan
    if config.enable_virus_scan {
        let scan_result = virus_scan(&file_data).await?;
        if scan_result.is_infected {
            if config.quarantine_mode {
                return quarantine_file(file_data, filename, scan_result).await;
            } else {
                return Err(UploadError::VirusDetected {
                    threat: scan_result.threat_name,
                });
            }
        }
    }

    // 5. Proceed with normal upload
    store_file(file_data, filename).await
}
```

### 6. Error Messages

Clear, user-friendly error messages:

```rust
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("File type not allowed: {reason}")]
    BlockedFileType {
        reason: String,
        detected: Option<String>,
    },

    #[error("File too large: {size} bytes exceeds maximum of {max} bytes")]
    FileTooLarge { size: usize, max: u64 },

    #[error("Potential security threat detected: {threat}")]
    VirusDetected { threat: String },

    #[error("File extension '{ext}' does not match content type '{detected}'")]
    ExtensionMismatch { ext: String, detected: String },
}

// API response
{
    "error": "blocked_file_type",
    "message": "Executable files are not allowed. Detected: Windows executable (PE/MZ)",
    "code": "FILE_BLOCKED_EXECUTABLE",
    "help": "Only documents, images, videos, and other content files are accepted."
}
```

### 7. MCP Tool Integration

```javascript
// MCP error response
{
  "tool": "attach_file",
  "status": "error",
  "error": {
    "code": "FILE_BLOCKED_EXECUTABLE",
    "message": "Executable files are not allowed",
    "detected_type": "Windows executable (PE/MZ)",
    "allowed_types": ["images", "videos", "documents", "audio", "3d-models"]
  }
}
```

### 8. Audit Logging

All blocked uploads are logged for security audit:

```sql
CREATE TABLE file_upload_audit (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),

    -- Request context
    uploaded_by UUID,
    ip_address INET,
    user_agent TEXT,

    -- File info
    filename TEXT NOT NULL,
    file_size BIGINT,
    content_hash TEXT,  -- BLAKE3 even for blocked files

    -- Detection
    blocked BOOLEAN NOT NULL,
    block_reason TEXT,
    detected_magic TEXT,
    claimed_mime_type TEXT,
    detected_mime_type TEXT,

    -- Warnings
    warnings JSONB DEFAULT '[]',

    -- Virus scan
    virus_scan_performed BOOLEAN DEFAULT FALSE,
    virus_detected BOOLEAN,
    virus_threat_name TEXT,

    -- Outcome
    outcome TEXT NOT NULL,  -- 'accepted', 'blocked', 'quarantined'
    attachment_id UUID REFERENCES file_attachment(id),

    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_upload_audit_blocked ON file_upload_audit(blocked, created_at);
CREATE INDEX idx_upload_audit_user ON file_upload_audit(uploaded_by, created_at);
```

### 9. ClamAV Integration (Optional)

```rust
// Optional virus scanning with ClamAV

pub async fn virus_scan(data: &[u8]) -> Result<ScanResult, ScanError> {
    // Connect to clamd socket
    let mut stream = TcpStream::connect("127.0.0.1:3310").await?;

    // Send INSTREAM command
    stream.write_all(b"zINSTREAM\0").await?;

    // Send data in chunks
    for chunk in data.chunks(2048) {
        let len = (chunk.len() as u32).to_be_bytes();
        stream.write_all(&len).await?;
        stream.write_all(chunk).await?;
    }

    // End stream
    stream.write_all(&[0, 0, 0, 0]).await?;

    // Read response
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    // Parse: "stream: OK" or "stream: <threat> FOUND"
    if response.contains("OK") {
        Ok(ScanResult { is_infected: false, threat_name: None })
    } else if let Some(threat) = parse_threat(&response) {
        Ok(ScanResult { is_infected: true, threat_name: Some(threat) })
    } else {
        Err(ScanError::ParseError(response))
    }
}
```

### 10. Filesystem Permission Enforcement

**Critical**: All stored files must have **read/write only** permissions. Execute permission is **expressly forbidden** at every layer.

#### Storage Layer (Rust)

```rust
use std::os::unix::fs::PermissionsExt;

/// Write file with secure permissions - NO EXECUTE
pub async fn write_secure_file(path: &Path, data: &[u8]) -> Result<()> {
    // 1. Write to temp file first
    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path).await?;
    file.write_all(data).await?;
    file.sync_all().await?;

    // 2. Set permissions: rw-r--r-- (0644) - NO EXECUTE
    let permissions = Permissions::from_mode(0o644);
    std::fs::set_permissions(&temp_path, permissions)?;

    // 3. Atomic rename
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Verify file permissions are secure
pub fn verify_no_execute(path: &Path) -> Result<bool> {
    let metadata = std::fs::metadata(path)?;
    let mode = metadata.permissions().mode();

    // Check no execute bits set (owner, group, other)
    let has_execute = (mode & 0o111) != 0;

    if has_execute {
        tracing::error!(
            path = %path.display(),
            mode = format!("{:o}", mode),
            "SECURITY: File has execute permission!"
        );
        return Ok(false);
    }

    Ok(true)
}
```

#### Directory Structure

```bash
# Storage directory permissions
/var/lib/matric/storage/          # drwxr-xr-x (755)
├── blobs/                        # drwxr-xr-x (755)
│   └── aa/bb/*.bin              # -rw-r--r-- (644) NEVER EXECUTABLE
├── previews/                     # drwxr-xr-x (755)
│   └── *.png, *.jpg             # -rw-r--r-- (644)
└── temp/                         # drwxr-xr-x (755)
    └── upload-*.tmp             # -rw------- (600)
```

#### Mount Options (Defense in Depth)

```bash
# /etc/fstab - mount storage volume with noexec
/dev/sdb1  /var/lib/matric/storage  ext4  defaults,noexec,nosuid,nodev  0 2
```

#### Docker Container

```dockerfile
# Dockerfile - storage volume is noexec
VOLUME ["/var/lib/matric/storage"]

# In docker-compose.yml
volumes:
  matric-storage:
    driver: local
    driver_opts:
      type: none
      o: bind,noexec,nosuid,nodev
      device: /data/matric-storage
```

#### Nginx Serving (No Execution)

```nginx
# Serve attachments with restricted headers
location /attachments/ {
    alias /var/lib/matric/storage/blobs/;

    # Force download, never execute in browser
    add_header Content-Disposition "attachment";
    add_header X-Content-Type-Options "nosniff";
    add_header X-Download-Options "noopen";
    add_header Content-Security-Policy "default-src 'none'";

    # No script execution
    add_header X-XSS-Protection "1; mode=block";

    # Deny range requests for executables (prevents partial download attacks)
    if ($request_method = HEAD) {
        return 200;
    }
}
```

#### Periodic Audit

```rust
/// Background job to audit file permissions
pub async fn audit_file_permissions(storage_path: &Path) -> AuditResult {
    let mut violations = Vec::new();

    for entry in WalkDir::new(storage_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let metadata = entry.metadata()?;
            let mode = metadata.permissions().mode();

            // Check for any execute bit
            if (mode & 0o111) != 0 {
                violations.push(PermissionViolation {
                    path: entry.path().to_path_buf(),
                    current_mode: mode,
                    expected_mode: 0o644,
                });

                // Auto-fix: remove execute permission
                let fixed = Permissions::from_mode(mode & !0o111);
                std::fs::set_permissions(entry.path(), fixed)?;

                tracing::warn!(
                    path = %entry.path().display(),
                    "Fixed execute permission on file"
                );
            }
        }
    }

    AuditResult {
        files_checked: count,
        violations_found: violations.len(),
        violations_fixed: violations.len(),
        violations,
    }
}
```

#### Summary: Defense in Depth

| Layer | Protection |
|-------|------------|
| **Application** | Write files with 0644, verify on read |
| **Filesystem** | Mount with `noexec` option |
| **Container** | Volume mounted noexec, nosuid, nodev |
| **Web Server** | `Content-Disposition: attachment`, `nosniff` |
| **Audit** | Periodic job to detect and fix violations |

**Principle**: Even if an attacker uploads malicious content disguised as allowed type, it can **never be executed** at any layer.

## Consequences

### Positive

- (+) **No executable storage**: Platform cannot be used for malware distribution
- (+) **Magic byte detection**: Can't bypass by renaming `.exe` to `.jpg`
- (+) **Configurable**: Admins can adjust allowed types
- (+) **Audit trail**: All blocked uploads logged for security review
- (+) **Clear errors**: Users know exactly why upload failed
- (+) **Defense in depth**: Multiple validation layers

### Negative

- (-) **Legitimate use blocked**: Some users may want to store code/scripts
- (-) **Processing overhead**: Magic byte checking adds latency
- (-) **False positives**: Some legitimate files may match patterns

### Mitigations

- Code files (`.py`, `.rs`, `.js`) allowed as TEXT, not binary execution
- Magic byte check is fast (~microseconds)
- Configurable allowlist for specific use cases
- Quarantine mode for uncertain files

## Implementation

### Phase 1: Core Blocking
- [ ] Magic byte detection for executables
- [ ] Extension blocklist enforcement
- [ ] Size limit enforcement
- [ ] Clear error messages
- [ ] Audit logging

### Phase 2: Advanced Detection
- [ ] Content type inference (infer crate)
- [ ] Extension/content mismatch warnings
- [ ] Archive scanning (if enabled)
- [ ] Script pattern detection in text files

### Phase 3: Virus Scanning (Optional)
- [ ] ClamAV integration
- [ ] Quarantine system
- [ ] Scheduled scan of existing files

## References

- File signatures database: https://www.garykessler.net/library/file_sigs.html
- ClamAV: https://www.clamav.net/
- OWASP File Upload: https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html
- infer crate (Rust): https://docs.rs/infer/

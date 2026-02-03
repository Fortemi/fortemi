# File Attachments Guide

Fortémi provides a secure, content-addressable file attachment system for storing files alongside notes. Files are automatically deduplicated, safely validated, and can have their content extracted for search indexing.

## Overview

The file attachment system supports:

- **Content-addressable storage** with BLAKE3 hashing for automatic deduplication
- **Multiple storage backends** (database inline, filesystem, S3-compatible)
- **UUIDv7-based filesystem paths** for time-ordered, scalable storage
- **Automatic content extraction** for indexing and search
- **EXIF metadata extraction** from images (GPS, timestamps, camera info)
- **Multi-layer security validation** (magic bytes, extension blocklist, size limits)
- **Reference counting** for safe garbage collection

## Supported File Types

### Images

**Formats:**
- JPEG (.jpg, .jpeg)
- PNG (.png)
- GIF (.gif)
- WebP (.webp)
- HEIF/HEIC (.heic, .heif)
- TIFF (.tiff, .tif)
- RAW formats (Canon .cr2, Nikon .nef, Sony .arw, etc.)

**Automatic Processing:**
- EXIF metadata extraction (GPS coordinates, capture time, camera device)
- Image orientation detection
- Dimensions extraction
- Vision model analysis (optional, using LLaVA or similar models)

**Example Use Cases:**
- Photo attachments with GPS location tracking
- Screenshots for documentation
- Diagrams and charts

### Documents

**Formats:**
- PDF (.pdf)
- Markdown (.md)
- Plain text (.txt)
- Microsoft Office (.docx, .xlsx, .pptx)
- OpenDocument (.odt, .ods, .odp)
- HTML (.html, .htm)

**Automatic Processing:**
- PDF text extraction using `pdftotext`
- OCR for scanned PDFs using Tesseract (when `pdf_ocr` strategy is enabled)
- Office document conversion using Pandoc
- Direct text extraction for plaintext/markdown

**Example Use Cases:**
- Research papers and publications
- Meeting notes and reports
- Project documentation

### Audio

**Formats:**
- MP3 (.mp3)
- WAV (.wav)
- FLAC (.flac)
- M4A (.m4a)
- OGG (.ogg)
- Opus (.opus)

**Automatic Processing:**
- Audio transcription using Whisper models
- Speech-to-text for voice notes
- Metadata extraction (duration, bitrate, codec)

**Example Use Cases:**
- Voice memos
- Meeting recordings
- Podcast clips

### Video

**Formats:**
- MP4 (.mp4)
- WebM (.webm)
- MOV (.mov)
- AVI (.avi)
- MKV (.mkv)

**Automatic Processing:**
- Video multimodal processing (frame extraction + audio transcription)
- Keyframe analysis for visual content
- Audio track transcription

**Example Use Cases:**
- Tutorial videos
- Screencasts
- Presentation recordings

### 3D Models

**Formats:**
- GLB/GLTF (.glb, .gltf)
- STL (.stl)
- OBJ (.obj)
- FBX (.fbx)

**Automatic Processing:**
- Structured metadata extraction
- Model bounds and statistics

**Example Use Cases:**
- CAD designs
- 3D printing models
- Game assets

### Source Code

**Formats:**
- All programming language extensions (detected by extension)
- Common examples: .rs, .py, .js, .ts, .java, .cpp, .go, etc.

**Automatic Processing:**
- Code AST parsing using tree-sitter
- Syntax highlighting metadata
- Function/class extraction for semantic search

**Example Use Cases:**
- Code snippets
- Configuration files
- Scripts and utilities

### Archives

**Formats:**
- ZIP (.zip)
- TAR (.tar, .tar.gz, .tgz)
- 7z (.7z)

**Automatic Processing:**
- Archive listing
- Metadata extraction (compressed size, file count)

**Example Use Cases:**
- Project backups
- File collections
- Data exports

## Automatic Content Extraction

Fortémi automatically extracts searchable content from attachments using document type-specific extraction strategies.

### Extraction Strategies

#### TextNative

**For:** Plaintext, Markdown, CSV, JSON, YAML, XML

**How it works:**
- Direct UTF-8 text extraction
- No conversion needed
- Fastest strategy

**Output:**
- Raw text content indexed for full-text search
- Structured data preserved for display

#### PdfText

**For:** Text-based PDF documents

**How it works:**
- Uses `pdftotext` for text extraction
- Preserves text layout when possible
- Handles embedded fonts

**Output:**
- Searchable text content
- Page metadata (page count, dimensions)

**Example extracted metadata:**
```json
{
  "pages": 10,
  "title": "Research Paper Title",
  "author": "Author Name",
  "creation_date": "2026-01-15"
}
```

#### PdfOcr

**For:** Scanned PDFs, image-based PDFs

**How it works:**
- Uses Tesseract OCR for text recognition
- Automatically triggered when `pdftotext` yields low confidence
- Supports multiple languages

**Output:**
- OCR-extracted text with confidence scores
- Page-by-page processing metadata

**Configuration:**
```json
{
  "extraction_strategy": "pdf_ocr",
  "extraction_config": {
    "languages": ["eng", "fra"],
    "dpi": 300,
    "page_segmentation_mode": 3
  }
}
```

#### Vision

**For:** Images (JPEG, PNG, WebP, HEIF)

**How it works:**
- Analyzes image content using vision models (LLaVA, GPT-4V)
- Generates descriptive captions
- Identifies objects, scenes, text in images

**Output:**
- Natural language image description
- Detected objects and concepts
- OCR text from images

**Example output:**
```json
{
  "caption": "A diagram showing system architecture with three main components",
  "detected_objects": ["server", "database", "client"],
  "extracted_text": "Load Balancer → App Server → PostgreSQL"
}
```

#### AudioTranscribe

**For:** Audio files (MP3, WAV, M4A, FLAC)

**How it works:**
- Uses Whisper models for speech recognition
- Generates timestamped transcriptions
- Supports multiple languages

**Output:**
- Full transcription text
- Timestamps for each segment
- Speaker diarization (optional)

**Example output:**
```json
{
  "transcript": "Welcome to the meeting. Today we'll discuss...",
  "segments": [
    {
      "start": 0.0,
      "end": 5.2,
      "text": "Welcome to the meeting."
    }
  ],
  "language": "en",
  "confidence": 0.94
}
```

#### VideoMultimodal

**For:** Video files (MP4, WebM, MOV)

**How it works:**
- Extracts keyframes for visual analysis
- Transcribes audio track
- Combines visual and audio insights

**Output:**
- Video transcript
- Keyframe descriptions
- Scene changes and timestamps

#### CodeAst

**For:** Source code files

**How it works:**
- Parses code using tree-sitter
- Extracts functions, classes, imports
- Generates semantic code summaries

**Output:**
- Function signatures and documentation
- Code structure metadata
- Import dependencies

**Example output:**
```json
{
  "functions": [
    {
      "name": "compute_hash",
      "signature": "fn compute_hash(data: &[u8]) -> String",
      "docstring": "Compute BLAKE3 hash of data"
    }
  ],
  "imports": ["blake3", "uuid"],
  "language": "rust"
}
```

#### OfficeConvert

**For:** Microsoft Office, OpenDocument formats

**How it works:**
- Uses Pandoc to convert to Markdown
- Preserves document structure
- Extracts images and media

**Output:**
- Markdown representation
- Embedded images extracted separately
- Document metadata

#### StructuredExtract

**For:** JSON, YAML, XML, TOML

**How it works:**
- Parses structured data
- Generates human-readable summaries
- Indexes key-value pairs

**Output:**
- Flattened key-value text
- Schema detection
- Nested structure metadata

## EXIF Metadata Extraction

For image files, Fortémi automatically extracts EXIF metadata for enhanced provenance tracking and spatial search.

### Extracted Fields

**Temporal Metadata:**
- `datetime` - Original capture time (from `DateTimeOriginal`, `DateTimeDigitized`, or `DateTime`)
- Timezone offsets (from `OffsetTimeOriginal`, `OffsetTimeDigitized`)

**Spatial Metadata (GPS):**
- `latitude` - Decimal degrees (positive = North, negative = South)
- `longitude` - Decimal degrees (positive = East, negative = West)
- `altitude` - Meters above sea level

**Device Information:**
- `make` - Camera/device manufacturer (e.g., "Apple")
- `model` - Device model (e.g., "iPhone 15 Pro")
- `software` - Processing software (e.g., "Adobe Lightroom 2026.1")

**Image Properties:**
- `orientation` - EXIF orientation tag (1-8)
- `dimensions` - Width × height in pixels

### GPS Integration

GPS coordinates are automatically converted to PostGIS-compatible formats for spatial queries.

**WKT Format:**
```sql
-- Point in Well-Known Text format
POINT(2.2945 48.8584)
```

**PostGIS Geography:**
```sql
-- Geographic point with WGS84 (SRID 4326)
ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography
```

**Use Cases:**
- Find all photos taken within 5km of a location
- Group photos by geographic region
- Create travel timeline maps

### Privacy Considerations

EXIF data may contain sensitive information:

- **GPS coordinates** reveal photo locations
- **Timestamps** reveal when photos were taken
- **Device info** may reveal camera model

Always review EXIF data before sharing notes publicly. Use EXIF stripping tools when privacy is required.

## File Safety and Validation

Fortémi implements multi-layer security validation to prevent malicious file uploads.

### Security Layers

#### 1. Magic Byte Detection

Files are validated by their actual content (magic bytes), not just extension.

**Blocked Signatures:**
```
Windows PE/MZ      - 0x4D 0x5A
ELF (Linux)        - 0x7F 0x45 0x4C 0x46
Mach-O (macOS)     - 0xFE 0xED 0xFA 0xCE/CF
Java Class         - 0xCA 0xFE 0xBA 0xBE
WebAssembly        - 0x00 0x61 0x73 0x6D
Shell Scripts      - #! (shebang)
```

**Why:** Prevents execution of disguised binaries (e.g., `malware.jpg` that's actually an `.exe`)

#### 2. Extension Blocklist

Dangerous extensions are explicitly blocked:

**Executables:**
`.exe`, `.dll`, `.scr`, `.com`, `.msi`, `.so`, `.dylib`

**Scripts:**
`.sh`, `.bat`, `.cmd`, `.ps1`, `.vbs`, `.js`, `.jse`

**JVM Bytecode:**
`.jar`, `.war`, `.ear`, `.class`

**Packages:**
`.deb`, `.rpm`, `.apk`, `.dmg`, `.pkg`

**Office Macros:**
`.xlsm`, `.xlsb`, `.docm`, `.pptm` (macro-enabled formats)

**Other Dangerous:**
`.reg`, `.inf`, `.scf`, `.lnk`, `.hta`

**Why:** Defense-in-depth against extension-based attacks

#### 3. Size Limits

Files larger than the configured maximum are rejected.

**Default Limits:**
- API upload: 100 MB (configurable via `MAX_UPLOAD_SIZE`)
- Database inline storage threshold: 10 MB

**Why:** Prevents denial-of-service and storage exhaustion

#### 4. Filename Sanitization

Filenames are automatically sanitized to prevent path traversal and filesystem attacks.

**Removed/Replaced:**
- Path separators (`/`, `\`)
- Control characters (`\0`, `\x01`, etc.)
- Dangerous characters (`<`, `>`, `:`, `"`, `|`, `?`, `*`)
- Overly long names (truncated to 255 characters)

**Example:**
```rust
// Input:  "../../../etc/passwd"
// Output: "........etc.passwd"

// Input:  "file<dangerous>.txt"
// Output: "file_dangerous_.txt"

// Input:  ""  (empty)
// Output: "unnamed_file"
```

**Why:** Prevents directory traversal and filesystem injection

### Validation Flow

```
Upload Request
     │
     ▼
┌─────────────────┐
│ Size Check      │ → Reject if > MAX_UPLOAD_SIZE
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Extension Check │ → Reject if in blocklist
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Magic Byte Check│ → Reject if executable signature
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Sanitize Name   │ → Clean filename
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Compute Hash    │ → BLAKE3 for deduplication
└────────┬────────┘
         │
         ▼
     Store ✓
```

### Quarantine Status

Files that fail validation are marked with `status: quarantined` and:

- Not indexed for search
- Not processed for content extraction
- Visible only to administrators
- Can be manually reviewed and deleted

## Content Deduplication

Files are deduplicated using BLAKE3 hashing to save storage space.

### How It Works

1. **Hash Computation:**
   ```rust
   // Compute content hash
   let hash = blake3::hash(file_data);
   let content_hash = format!("blake3:{}", hash.to_hex());
   ```

2. **Lookup Existing Blob:**
   ```sql
   SELECT id FROM attachment_blob
   WHERE content_hash = $1;
   ```

3. **Reuse or Create:**
   - If blob exists → reuse existing blob ID
   - If new → create new blob and store data

4. **Reference Counting:**
   ```sql
   -- Incremented on attach
   UPDATE attachment_blob
   SET reference_count = reference_count + 1
   WHERE id = $1;

   -- Decremented on delete
   UPDATE attachment_blob
   SET reference_count = reference_count - 1
   WHERE id = $1;
   ```

5. **Garbage Collection:**
   - Blobs with `reference_count = 0` are candidates for cleanup
   - Configurable grace period (default: 24 hours)
   - Manual cleanup via `cleanup_orphaned_blobs(min_age_hours)`

### Storage Savings

**Example Scenario:**
- Upload `document.pdf` (5 MB) to 10 different notes
- Without deduplication: 50 MB storage
- With deduplication: 5 MB storage + 10 small metadata records
- **Savings:** 90%

**Real-World Benefits:**
- Repeated screenshots (e.g., same error message)
- Common reference documents
- Duplicate uploads by different users

## Storage Architecture

Fortémi uses a flexible storage backend system supporting multiple storage types.

### Storage Backends

#### 1. Database (Inline)

**For:** Small files (<10 MB by default)

**Storage Location:** PostgreSQL `BYTEA` column

**Advantages:**
- Atomic transactions with metadata
- Automatic backup with database
- No filesystem dependencies

**Disadvantages:**
- Increases database size
- TOAST limit: 1 GB per row
- Slower for large files

**Configuration:**
```rust
let repo = PgFileStorageRepository::new(
    pool,
    backend,
    10_485_760  // 10 MB threshold
);
```

#### 2. Filesystem

**For:** Large files (≥10 MB by default)

**Storage Location:** UUIDv7-based directory structure

**Path Format:**
```
blobs/{first-2-hex}/{next-2-hex}/{uuid}.bin

Example:
blobs/01/94/01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f.bin
```

**Advantages:**
- No database size increase
- Fast filesystem operations
- Easy to snapshot/backup separately
- Scales to billions of files (65,536 subdirectories)

**Disadvantages:**
- Requires filesystem access
- Backup complexity (database + filesystem)

**Directory Structure:**
```
/var/matric/blobs/
├── 01/
│   ├── 94/
│   │   ├── 01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f.bin
│   │   └── 01949a1b-2c3d-4e5f-6a7b-8c9d0e1f2a3b.bin
│   └── 95/
│       └── 01959c2d-3e4f-5a6b-7c8d-9e0f1a2b3c4d.bin
└── 02/
    └── ...
```

**UUIDv7 Benefits:**
- Time-ordered: newer files in newer directories
- Natural sharding: first 4 hex chars distribute across 65K dirs
- Predictable paths: deterministic generation from UUID

#### 3. S3-Compatible (Future)

**For:** Cloud storage integration

**Storage Location:** S3, MinIO, Backblaze B2, etc.

**Advantages:**
- Unlimited scalability
- Geographic replication
- CDN integration

**Status:** Planned, not yet implemented

### Storage Backend Selection

**Decision Flow:**
```
File Size Check
     │
     ▼
< 10 MB? → YES → Database (inline BYTEA)
     │
    NO
     │
     ▼
≥ 10 MB → Filesystem (UUIDv7 paths)
```

**Override Threshold:**
```bash
# Set inline threshold via environment variable
export INLINE_STORAGE_THRESHOLD_MB=5

# Or in Rust configuration
let repo = PgFileStorageRepository::new(
    pool,
    backend,
    5 * 1024 * 1024  // 5 MB threshold
);
```

### Database Schema

```sql
-- Content-addressable blob storage
CREATE TABLE attachment_blob (
    id UUID PRIMARY KEY,                    -- UUIDv7 blob ID
    content_hash TEXT NOT NULL UNIQUE,      -- blake3:{64-hex}
    content_type TEXT NOT NULL,             -- MIME type
    size_bytes BIGINT NOT NULL,             -- File size
    storage_backend TEXT NOT NULL,          -- 'database', 'filesystem', 's3'
    storage_path TEXT,                      -- Path for filesystem/s3
    data BYTEA,                             -- Inline data for 'database' backend
    reference_count INT NOT NULL DEFAULT 1, -- Number of attachments referencing this
    verified_at TIMESTAMPTZ,                -- Last integrity check
    verification_status TEXT,               -- 'verified', 'failed', 'pending'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Attachment metadata (links notes to blobs)
CREATE TABLE attachment (
    id UUID PRIMARY KEY,                    -- UUIDv7 attachment ID
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID NOT NULL REFERENCES attachment_blob(id),
    filename TEXT NOT NULL,                 -- Display name
    original_filename TEXT,                 -- Original upload name
    document_type_id UUID REFERENCES document_type(id),
    status TEXT NOT NULL DEFAULT 'uploaded', -- Processing status
    extraction_strategy TEXT,               -- Content extraction strategy
    extracted_text TEXT,                    -- Searchable extracted content
    extracted_metadata JSONB,               -- Structured metadata (EXIF, etc.)
    has_preview BOOLEAN NOT NULL DEFAULT FALSE,
    is_canonical_content BOOLEAN NOT NULL DEFAULT FALSE,
    display_order INT,                      -- Ordering for multiple attachments
    processing_error TEXT,                  -- Error message if failed
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_attachment_note ON attachment(note_id);
CREATE INDEX idx_attachment_blob ON attachment(blob_id);
CREATE INDEX idx_attachment_status ON attachment(status);
CREATE INDEX idx_blob_hash ON attachment_blob(content_hash);
CREATE INDEX idx_blob_backend ON attachment_blob(storage_backend);
CREATE INDEX idx_blob_orphans ON attachment_blob(reference_count) WHERE reference_count = 0;
```

## API Examples

### Upload File

```bash
curl -X POST http://localhost:3000/api/v1/notes/{note_id}/attachments \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@document.pdf" \
  -F "filename=research-paper.pdf"
```

**Response (201 Created):**
```json
{
  "id": "01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f",
  "note_id": "01948f7e-1234-5678-9abc-def012345678",
  "blob_id": "01948f7e-4567-89ab-cdef-0123456789ab",
  "filename": "research-paper.pdf",
  "content_type": "application/pdf",
  "size_bytes": 5242880,
  "status": "uploaded",
  "document_type_name": "pdf",
  "has_preview": false,
  "is_canonical_content": false,
  "created_at": "2026-02-02T12:00:00Z"
}
```

### Download File

```bash
curl -X GET http://localhost:3000/api/v1/attachments/{attachment_id}/download \
  -H "Authorization: Bearer $TOKEN" \
  -O research-paper.pdf
```

**Response:**
- `Content-Type`: `application/pdf` (original MIME type)
- `Content-Disposition`: `attachment; filename="research-paper.pdf"`
- Binary file data

### List Attachments for Note

```bash
curl -X GET http://localhost:3000/api/v1/notes/{note_id}/attachments \
  -H "Authorization: Bearer $TOKEN"
```

**Response (200 OK):**
```json
{
  "attachments": [
    {
      "id": "01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f",
      "note_id": "01948f7e-1234-5678-9abc-def012345678",
      "filename": "research-paper.pdf",
      "content_type": "application/pdf",
      "size_bytes": 5242880,
      "status": "completed",
      "document_type_name": "pdf",
      "has_preview": true,
      "is_canonical_content": false,
      "created_at": "2026-02-02T12:00:00Z"
    },
    {
      "id": "01948f7e-9abc-def0-1234-56789abcdef0",
      "note_id": "01948f7e-1234-5678-9abc-def012345678",
      "filename": "diagram.png",
      "content_type": "image/png",
      "size_bytes": 245760,
      "status": "completed",
      "document_type_name": "image",
      "has_preview": true,
      "is_canonical_content": false,
      "created_at": "2026-02-02T12:05:00Z"
    }
  ]
}
```

### Get Attachment Metadata

```bash
curl -X GET http://localhost:3000/api/v1/attachments/{attachment_id} \
  -H "Authorization: Bearer $TOKEN"
```

**Response (200 OK):**
```json
{
  "id": "01948f7e-9abc-def0-1234-56789abcdef0",
  "note_id": "01948f7e-1234-5678-9abc-def012345678",
  "blob_id": "01948f7e-4567-89ab-cdef-0123456789ab",
  "filename": "vacation-photo.jpg",
  "original_filename": "IMG_20260125_143022.jpg",
  "content_type": "image/jpeg",
  "status": "completed",
  "extraction_strategy": "vision",
  "extracted_text": "A scenic mountain view with snow-capped peaks and a lake in the foreground",
  "extracted_metadata": {
    "exif": {
      "datetime": "2026-01-25T14:30:22Z",
      "gps": {
        "latitude": 46.5197,
        "longitude": 6.6323,
        "altitude": 1850.0
      },
      "device": {
        "make": "Apple",
        "model": "iPhone 15 Pro"
      },
      "dimensions": {
        "width": 4032,
        "height": 3024
      }
    },
    "vision": {
      "description": "Scenic mountain landscape",
      "objects": ["mountain", "lake", "snow", "sky"],
      "confidence": 0.92
    }
  },
  "has_preview": true,
  "is_canonical_content": false,
  "created_at": "2026-02-02T12:05:00Z",
  "updated_at": "2026-02-02T12:05:15Z"
}
```

### Delete Attachment

```bash
curl -X DELETE http://localhost:3000/api/v1/attachments/{attachment_id} \
  -H "Authorization: Bearer $TOKEN"
```

**Response (204 No Content)**

**Note:** The underlying blob is not immediately deleted. It remains in storage until the reference count reaches zero and the garbage collection grace period expires.

### Update Attachment Metadata

```bash
curl -X PATCH http://localhost:3000/api/v1/attachments/{attachment_id} \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "filename": "updated-name.pdf",
    "is_canonical_content": true
  }'
```

**Response (200 OK):**
```json
{
  "id": "01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f",
  "filename": "updated-name.pdf",
  "is_canonical_content": true,
  "updated_at": "2026-02-02T12:30:00Z"
}
```

## Best Practices

### File Naming

**Do:**
- Use descriptive names: `project-architecture-diagram.png`
- Include dates for versioning: `meeting-notes-2026-02-02.pdf`
- Use lowercase with hyphens: `research-paper-draft.docx`

**Don't:**
- Use generic names: `document.pdf`, `image.jpg`
- Use special characters: `file<1>.txt`, `doc:final.pdf`
- Use very long names (>100 characters)

### Organization

**Strategy 1: One Note Per Document**
```
Note: "Research Paper - Machine Learning Applications"
└── Attachment: research-paper.pdf
```

**Strategy 2: Collection Note with Multiple Attachments**
```
Note: "Project Assets"
├── Attachment: architecture-diagram.png
├── Attachment: database-schema.pdf
├── Attachment: api-documentation.md
└── Attachment: deployment-guide.pdf
```

**Strategy 3: Image Gallery**
```
Note: "Conference Photos - AI Summit 2026"
├── Attachment: keynote-speaker.jpg
├── Attachment: panel-discussion.jpg
├── Attachment: networking-event.jpg
└── Attachment: closing-ceremony.jpg
```

### Search Tips

**1. Search by Extracted Content:**
```bash
# Full-text search includes extracted attachment content
curl -X GET "http://localhost:3000/api/v1/search?q=machine+learning"
# Returns notes with "machine learning" in:
# - Note content
# - Attachment extracted text (PDF, OCR, transcripts)
```

**2. Filter by Document Type:**
```bash
# Find all PDF attachments
GET /api/v1/notes?document_type=pdf

# Find all images
GET /api/v1/notes?document_type=image
```

**3. Search by EXIF Metadata:**
```sql
-- Find photos taken in Paris (within 10km radius)
SELECT n.id, n.title, a.filename
FROM note n
JOIN attachment a ON a.note_id = n.id
WHERE a.extracted_metadata->'exif'->'gps' IS NOT NULL
  AND ST_DWithin(
    ST_SetSRID(ST_MakePoint(
      (a.extracted_metadata->'exif'->'gps'->>'longitude')::float,
      (a.extracted_metadata->'exif'->'gps'->>'latitude')::float
    ), 4326)::geography,
    ST_SetSRID(ST_MakePoint(2.3522, 48.8566), 4326)::geography,
    10000  -- 10km
  );
```

**4. Search by Capture Date:**
```sql
-- Find photos taken in January 2026
SELECT n.id, n.title, a.filename,
       a.extracted_metadata->'exif'->>'datetime' as capture_time
FROM note n
JOIN attachment a ON a.note_id = n.id
WHERE a.extracted_metadata->'exif'->>'datetime' BETWEEN '2026-01-01' AND '2026-01-31';
```

### Performance Optimization

**1. Use Filesystem Backend for Large Files**

Configure inline threshold based on your usage:
```bash
# For mostly small files (text, configs)
export INLINE_STORAGE_THRESHOLD_MB=10

# For large documents
export INLINE_STORAGE_THRESHOLD_MB=5
```

**2. Regular Garbage Collection**

Run periodic cleanup to remove orphaned blobs:
```sql
-- Delete blobs with zero references older than 24 hours
SELECT cleanup_orphaned_blobs(24);
```

**3. Optimize Extraction Strategy**

Choose appropriate strategies per document type:
```json
// Fast: skip extraction for temporary files
{
  "document_type": "temp-file",
  "extraction_strategy": "text_native",
  "requires_attachment": false
}

// Thorough: full OCR for scanned documents
{
  "document_type": "scanned-paper",
  "extraction_strategy": "pdf_ocr",
  "extraction_config": {
    "languages": ["eng"],
    "dpi": 300
  }
}
```

**4. Batch Uploads**

For multiple files, upload in parallel:
```bash
# Upload multiple files concurrently
for file in *.pdf; do
  curl -X POST /api/v1/notes/$NOTE_ID/attachments \
    -F "file=@$file" &
done
wait
```

### Security Best Practices

**1. Validate File Sources**

Only upload files from trusted sources. The security validation catches most threats, but defense-in-depth is important.

**2. Review Quarantined Files**

Check quarantined attachments regularly:
```sql
SELECT a.filename, a.processing_error, a.created_at
FROM attachment a
WHERE a.status = 'quarantined'
ORDER BY a.created_at DESC;
```

**3. Strip Sensitive EXIF Data**

Before sharing notes publicly:
```bash
# Remove EXIF data from images
exiftool -all= image.jpg

# Or use ImageMagick
convert image.jpg -strip image-clean.jpg
```

**4. Limit Upload Sizes**

Configure appropriate limits:
```bash
# Environment variable
export MAX_UPLOAD_SIZE=104857600  # 100 MB

# Nginx proxy
client_max_body_size 100M;
```

**5. Regular Integrity Checks**

Verify blob integrity periodically:
```sql
-- Check unverified blobs
SELECT id, storage_path, size_bytes
FROM attachment_blob
WHERE verification_status IS NULL
   OR verified_at < NOW() - INTERVAL '30 days';
```

## Troubleshooting

### Issue: Upload Fails with "File type not allowed"

**Cause:** File extension is in the blocklist or magic bytes indicate executable.

**Solution:**
1. Check file type: `file --mime-type yourfile.ext`
2. If legitimate, convert to allowed format
3. For code files, use plaintext upload with syntax highlighting

### Issue: Extracted Text is Empty

**Cause:** Extraction strategy mismatch or processing failure.

**Solution:**
1. Check attachment status: `GET /api/v1/attachments/{id}`
2. Review `processing_error` field
3. Manually trigger re-extraction with correct strategy

### Issue: Duplicate Files Taking Up Space

**Cause:** Deduplication not working due to different content hashes.

**Solution:**
1. Files are identical only if byte-for-byte same
2. Even minor differences (metadata, compression) create different hashes
3. This is intentional for data integrity

### Issue: GPS Coordinates Not Extracted

**Cause:** Image doesn't contain GPS EXIF data.

**Solution:**
1. Verify EXIF data: `exiftool image.jpg | grep GPS`
2. Many images have GPS stripped for privacy
3. Use manual geolocation tagging if needed

### Issue: Slow Download for Large Files

**Cause:** Database inline storage for large files.

**Solution:**
1. Migrate large blobs to filesystem:
   ```sql
   SELECT mark_blob_migrated_to_filesystem(id)
   FROM attachment_blob
   WHERE storage_backend = 'database'
     AND size_bytes > 10485760;  -- 10 MB
   ```
2. Manually copy data to filesystem path
3. Update storage backend

### Issue: Orphaned Blobs Not Cleaned Up

**Cause:** Grace period not expired or cleanup not run.

**Solution:**
```sql
-- Check orphaned blobs
SELECT COUNT(*), SUM(size_bytes) as total_bytes
FROM attachment_blob
WHERE reference_count = 0;

-- Run cleanup (age in hours)
SELECT cleanup_orphaned_blobs(24);  -- 24 hour grace period
```

## Further Reading

- [Document Type Guide](document-types-guide.md) - Learn about document types and extraction strategies
- [Search Guide](search-guide.md) - Advanced search techniques for attachment content
- [API Reference](api.md) - Complete API documentation
- [Security Best Practices](operations.md) - Operational security guidelines

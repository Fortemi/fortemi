# File Attachments in Knowledge Management Systems: Research Report

**Research Date:** 2026-02-01
**Target System:** matric-memory (PostgreSQL + Rust)
**Scope:** Storage strategies, processing pipelines, AI/ML integration, and security

---

## Executive Summary

This research evaluates best practices for implementing file attachments in knowledge management systems, with specific recommendations for matric-memory's PostgreSQL + Rust architecture. Key findings:

1. **Storage Strategy**: Hybrid approach using PostgreSQL BYTEA (EXTERNAL storage) for metadata-rich files (<10MB) + object storage (S3-compatible) for large files
2. **Processing Pattern**: Async job-based pipeline with content extraction, embedding generation, and preview creation
3. **AI Integration**: Multi-modal processing using Ollama (vision, audio transcription) with fallback to specialized tools
4. **Deduplication**: Content-addressable storage (CAS) using BLAKE3 hashing to eliminate redundant storage
5. **Security**: Magic byte validation, file size limits, and virus scanning hooks

**Confidence Level:** High
**Recommendation:** Implement in phases (Phase 1: Core storage, Phase 2: Processing pipeline, Phase 3: AI extraction)

---

## 1. Storage Strategies

### 1.1 Industry Patterns

| System | Primary Storage | Large Files | Deduplication | Backup Strategy |
|--------|----------------|-------------|---------------|-----------------|
| **Notion** | Cloud blob storage | Proxied URLs | None observed | Included in exports |
| **Obsidian** | Local filesystem | Relative paths | User-managed | Vault-based |
| **Confluence** | Database + S3 | S3 for >1MB | SHA-256 hash | Separate blob backup |
| **GitHub** | Object storage | Up to 25MB | Content-addressable | Git LFS for repos |
| **Supabase Storage** | S3-compatible | Unlimited | Hash-based | PostgreSQL metadata |

### 1.2 PostgreSQL BYTEA vs Filesystem vs Object Storage

#### PostgreSQL BYTEA with TOAST

**Advantages:**
- ACID transactions for file + metadata atomicity
- Single backup stream (pg_dump includes files)
- Simplified permission model (inherit from note permissions)
- EXTERNAL storage strategy optimizes substring operations

**Disadvantages:**
- Limited to ~1GB per field (practical limit ~100MB)
- Increased database size impacts backup/restore time
- pg_dump includes binary data (slower exports)

**TOAST Performance (from PostgreSQL docs):**
- Automatic compression with pglz or lz4
- Out-of-line storage for values >2KB
- EXTERNAL strategy: No compression, fast substring access
- Chunk size: ~2000 bytes (TOAST_MAX_CHUNK_SIZE)

```sql
-- Optimize for binary file access
CREATE TABLE attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    content_hash TEXT NOT NULL,  -- BLAKE3 for deduplication
    data BYTEA NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Use EXTERNAL storage (no compression, fast access)
ALTER TABLE attachment ALTER COLUMN data SET STORAGE EXTERNAL;

-- Unique constraint for deduplication
CREATE UNIQUE INDEX idx_attachment_hash ON attachment(content_hash);
```

#### Object Storage (S3-Compatible)

**Advantages:**
- Handles files of any size
- Independent scaling from database
- CDN integration for fast delivery
- Lifecycle policies (archive old files)
- Cheaper storage costs ($0.023/GB vs database storage)

**Disadvantages:**
- Network latency for retrieval
- Separate backup strategy required
- Two-phase commit complexity (file upload + metadata)
- Permission synchronization overhead

**Recommended Implementation:**
```rust
// Hybrid approach: metadata in PostgreSQL, large files in S3
pub struct Attachment {
    pub id: Uuid,
    pub note_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub content_hash: String,  // BLAKE3

    // Storage strategy
    pub storage_type: StorageType,  // 'database' or 'object_storage'
    pub data: Option<Vec<u8>>,      // Only if storage_type = 'database'
    pub object_key: Option<String>, // Only if storage_type = 'object_storage'

    pub created_at: DateTime<Utc>,
}

pub enum StorageType {
    Database,      // For files <10MB
    ObjectStorage, // For files >=10MB
}
```

### 1.3 Content-Addressable Storage (CAS)

**Pattern:** Store files by content hash, reference from multiple notes

**Benefits:**
- Automatic deduplication (same PDF attached to 100 notes = 1 stored copy)
- Integrity verification (hash mismatch = corruption detected)
- Efficient backups (incremental based on hash)
- Supports immutable file sharing

**Implementation with BLAKE3:**
```rust
use blake3;

pub fn compute_content_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

// Deduplication on insert
pub async fn store_attachment(
    db: &Database,
    note_id: Uuid,
    filename: String,
    data: Vec<u8>,
) -> Result<Uuid> {
    let hash = compute_content_hash(&data);

    // Check if content already exists
    if let Some(existing) = db.find_attachment_by_hash(&hash).await? {
        // Create reference to existing blob
        db.link_attachment_to_note(note_id, existing.id, filename).await?;
        return Ok(existing.id);
    }

    // Store new blob
    db.create_attachment(note_id, filename, data, hash).await
}
```

**Schema for CAS:**
```sql
-- Blob storage (one per unique content)
CREATE TABLE attachment_blob (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    content_hash TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    data BYTEA NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Attachment references (many-to-one with blob)
CREATE TABLE attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID REFERENCES attachment_blob(id) ON DELETE RESTRICT,
    filename TEXT NOT NULL,  -- User-specified name
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID
);

-- Index for fast lookup
CREATE INDEX idx_attachment_blob_hash ON attachment_blob(content_hash);
CREATE INDEX idx_attachment_note ON attachment(note_id);
```

### 1.4 Recommendation for matric-memory

**Hybrid Strategy:**

1. **Small files (<10MB)**: PostgreSQL BYTEA with EXTERNAL storage
   - PDFs, images, small documents
   - Included in pg_dump backups
   - Fast retrieval with low latency

2. **Large files (>=10MB)**: MinIO/S3-compatible object storage
   - Videos, large datasets, archives
   - Separate backup with rclone/restic
   - Metadata in PostgreSQL, content in object store

3. **Deduplication**: BLAKE3 content-addressable storage
   - Eliminates redundant storage
   - Supports immutable sharing

**Migration Path:**
- Phase 1: PostgreSQL-only (simplicity, single backup)
- Phase 2: Add object storage when large files become common
- Transparent to API layer (storage abstraction trait)

---

## 2. File Processing Pipelines

### 2.1 Industry Patterns

**Notion Approach:**
- Synchronous upload to blob storage
- Async preview generation (images, PDFs)
- No automatic text extraction (manual copy-paste)

**Confluence Approach:**
- Async processing pipeline with job queue
- Text extraction for indexing
- Preview/thumbnail generation
- Virus scanning integration

**Obsidian Approach:**
- No server-side processing (client-side only)
- Markdown embedding with local file paths
- Plugin-based extension

### 2.2 Processing Pipeline for matric-memory

**Leverage existing job queue system** (matric-jobs crate):

```rust
// New job types (add to migrations/20260201400000_add_missing_job_types.sql)
ALTER TYPE job_type ADD VALUE 'attachment_processing';
ALTER TYPE job_type ADD VALUE 'text_extraction';
ALTER TYPE job_type ADD VALUE 'preview_generation';
ALTER TYPE job_type ADD VALUE 'virus_scan';

// Job payload
pub struct AttachmentProcessingPayload {
    pub attachment_id: Uuid,
    pub processing_steps: Vec<ProcessingStep>,
}

pub enum ProcessingStep {
    VirusScan,
    TextExtraction,
    PreviewGeneration,
    EmbeddingGeneration,
    MetadataExtraction,  // EXIF, PDF metadata, etc.
}
```

**Processing Flow:**
```
User Upload → API Endpoint → Store Raw File → Enqueue Job → Background Worker
                                    ↓
                              [Return 202 Accepted]

Background Worker → Virus Scan → Text Extraction → Embedding → Preview → Complete
                         ↓              ↓              ↓          ↓
                    [Quarantine]   [Index FTS]  [Vector Search] [Thumbnail]
```

**Implementation:**
```rust
// Handler for attachment processing job
pub struct AttachmentProcessingHandler {
    db: Arc<Database>,
    inference: Arc<dyn InferenceBackend>,
    storage: Arc<dyn AttachmentStorage>,
}

#[async_trait]
impl JobHandler for AttachmentProcessingHandler {
    async fn handle(&self, job: &Job) -> Result<JobResult> {
        let payload: AttachmentProcessingPayload =
            serde_json::from_value(job.payload.clone())?;

        let attachment = self.db.get_attachment(payload.attachment_id).await?;

        for step in payload.processing_steps {
            match step {
                ProcessingStep::TextExtraction => {
                    let text = self.extract_text(&attachment).await?;
                    self.db.update_attachment_extracted_text(
                        attachment.id,
                        text
                    ).await?;
                }
                ProcessingStep::EmbeddingGeneration => {
                    let text = self.db.get_attachment_text(attachment.id).await?;
                    let embeddings = self.inference.embed(&text).await?;
                    self.db.store_attachment_embeddings(
                        attachment.id,
                        embeddings
                    ).await?;
                }
                ProcessingStep::PreviewGeneration => {
                    let preview = self.generate_preview(&attachment).await?;
                    self.storage.store_preview(attachment.id, preview).await?;
                }
                _ => {}
            }
        }

        Ok(JobResult::Success)
    }
}
```

### 2.3 Schema for Processed Attachments

```sql
-- Extend attachment table with processing results
CREATE TABLE attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID REFERENCES attachment_blob(id),
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,

    -- Processing status
    processing_status TEXT DEFAULT 'pending',  -- pending, processing, complete, failed
    processing_error TEXT,

    -- Extracted data
    extracted_text TEXT,                       -- For FTS indexing
    extracted_metadata JSONB,                  -- EXIF, PDF metadata, etc.

    -- Preview/thumbnail
    has_preview BOOLEAN DEFAULT FALSE,
    preview_url TEXT,                          -- Object storage URL or data URI

    -- Security
    virus_scan_status TEXT,                    -- clean, infected, unknown
    virus_scan_at TIMESTAMPTZ,

    -- Audit
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID
);

-- Full-text search on extracted content
CREATE INDEX idx_attachment_extracted_text ON attachment
    USING GIN (to_tsvector('english', COALESCE(extracted_text, '')));

-- Embeddings for semantic search of attachments
CREATE TABLE attachment_embedding (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID REFERENCES attachment(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    text TEXT NOT NULL,
    vector vector(768),
    model TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(attachment_id, chunk_index)
);

CREATE INDEX idx_attachment_embedding_vector ON attachment_embedding
    USING hnsw (vector vector_cosine_ops) WITH (m = 16, ef_construction = 64);
```

---

## 3. AI/ML Processing by File Type

### 3.1 Document Type Integration

**Leverage existing document_type registry** (migrations/20260202000000_document_types.sql):

```sql
-- Extend document_type for attachment processing
ALTER TABLE document_type ADD COLUMN requires_file_attachment BOOLEAN DEFAULT FALSE;
ALTER TABLE document_type ADD COLUMN extraction_strategy TEXT;  -- 'ocr', 'pdf_text', 'vision', 'audio', 'code'

-- Map MIME types to document types
INSERT INTO document_type (name, display_name, category, mime_types, extraction_strategy) VALUES
-- Images
('jpeg', 'JPEG Image', 'media', ARRAY['image/jpeg', 'image/jpg'], 'vision'),
('png', 'PNG Image', 'media', ARRAY['image/png'], 'vision'),
('svg', 'SVG Image', 'markup', ARRAY['image/svg+xml'], 'xml_parse'),

-- Audio/Video
('mp3', 'MP3 Audio', 'media', ARRAY['audio/mpeg', 'audio/mp3'], 'audio_transcription'),
('wav', 'WAV Audio', 'media', ARRAY['audio/wav', 'audio/x-wav'], 'audio_transcription'),
('mp4', 'MP4 Video', 'media', ARRAY['video/mp4'], 'video_multimodal'),

-- Documents
('pdf', 'PDF Document', 'docs', ARRAY['application/pdf'], 'pdf_text'),
('docx', 'Word Document', 'docs', ARRAY['application/vnd.openxmlformats-officedocument.wordprocessingml.document'], 'pandoc'),
('xlsx', 'Excel Spreadsheet', 'data', ARRAY['application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'], 'tabular');
```

### 3.2 Processing Strategies by File Type

#### Images (JPEG, PNG, WebP)

**Vision Models via Ollama:**
- LLaVA 1.6 (7B/13B) for general image description
- BakLLaVA for detailed OCR + description
- Moondream2 (lightweight, 1.8B) for fast processing

**Implementation:**
```rust
pub async fn process_image(
    image_data: &[u8],
    inference: &dyn InferenceBackend,
) -> Result<ImageExtractionResult> {
    // Use vision model via Ollama
    let request = VisionRequest {
        model: "llava:13b".to_string(),
        images: vec![image_data.to_vec()],
        prompt: r#"
Analyze this image and provide:
1. A detailed description (2-3 sentences)
2. Any visible text (OCR)
3. Key objects/people/concepts
4. Suggested tags (5-10 keywords)

Format as JSON:
{"description": "...", "text": "...", "objects": [...], "tags": [...]}
        "#.to_string(),
    };

    let response = inference.vision_generate(request).await?;
    let extracted: ImageExtractionResult = serde_json::from_str(&response.text)?;

    Ok(extracted)
}

// Fallback: OCR with tesseract (via external process)
pub async fn ocr_image(image_data: &[u8]) -> Result<String> {
    let output = Command::new("tesseract")
        .arg("stdin")
        .arg("stdout")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()
        .await?;

    Ok(String::from_utf8(output.stdout)?)
}
```

#### Audio (MP3, WAV, OGG)

**Whisper Transcription:**
- Whisper (OpenAI): Robust speech recognition, 99 languages
- whisper.cpp: Efficient C++ implementation with Rust bindings
- Ollama support: Whisper models available via Ollama API

**Implementation:**
```rust
use whisper_rs::{WhisperContext, FullParams, SamplingStrategy};

pub async fn transcribe_audio(audio_path: &Path) -> Result<AudioTranscription> {
    let ctx = WhisperContext::new("models/ggml-base.en.bin")?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    // Enable timestamps for segment-level transcription
    params.set_print_timestamps(true);
    params.set_language(Some("en"));

    let audio_data = load_audio_file(audio_path)?;
    ctx.full(params, &audio_data)?;

    let num_segments = ctx.full_n_segments()?;
    let mut segments = Vec::new();

    for i in 0..num_segments {
        let text = ctx.full_get_segment_text(i)?;
        let start = ctx.full_get_segment_t0(i)?;
        let end = ctx.full_get_segment_t1(i)?;

        segments.push(TranscriptionSegment {
            text,
            start_time: start as f64 / 100.0,  // Convert to seconds
            end_time: end as f64 / 100.0,
        });
    }

    Ok(AudioTranscription {
        full_text: segments.iter().map(|s| s.text.clone()).collect::<Vec<_>>().join(" "),
        segments,
        language: "en".to_string(),
    })
}
```

#### Video (MP4, WebM)

**Two-Stage Processing:**
1. **Frame extraction**: FFmpeg to extract keyframes (1 frame/second)
2. **Vision analysis**: Process frames with LLaVA
3. **Audio extraction**: Extract audio track → Whisper transcription
4. **Multimodal fusion**: Combine visual + audio into unified description

**Implementation:**
```rust
pub async fn process_video(
    video_path: &Path,
    inference: &dyn InferenceBackend,
) -> Result<VideoExtractionResult> {
    // Extract audio track
    let audio_path = extract_audio(video_path).await?;
    let transcription = transcribe_audio(&audio_path).await?;

    // Extract keyframes (1 per second)
    let frames = extract_keyframes(video_path, 1.0).await?;

    // Analyze representative frames (every 10th frame to reduce cost)
    let mut frame_descriptions = Vec::new();
    for (i, frame) in frames.iter().enumerate().step_by(10) {
        let desc = process_image(frame, inference).await?;
        frame_descriptions.push((i, desc));
    }

    // Combine into unified description
    Ok(VideoExtractionResult {
        transcription,
        frame_descriptions,
        summary: generate_video_summary(&transcription, &frame_descriptions, inference).await?,
    })
}
```

#### PDFs

**Strategy Selection:**
1. **Text-based PDF**: Extract text directly (pdftotext, pdf-extract crate)
2. **Scanned PDF**: OCR with tesseract or vision model
3. **Hybrid**: Combine text extraction + image OCR

**Implementation:**
```rust
use pdf_extract::extract_text;

pub async fn process_pdf(pdf_data: &[u8]) -> Result<PdfExtractionResult> {
    // Attempt text extraction
    match extract_text_from_mem(pdf_data) {
        Ok(text) if !text.trim().is_empty() => {
            // Text-based PDF
            Ok(PdfExtractionResult {
                text,
                extraction_method: "text".to_string(),
                page_count: count_pdf_pages(pdf_data)?,
            })
        }
        _ => {
            // Scanned PDF - use OCR
            let images = pdf_to_images(pdf_data)?;
            let mut all_text = String::new();

            for img in images {
                let text = ocr_image(&img).await?;
                all_text.push_str(&text);
                all_text.push('\n');
            }

            Ok(PdfExtractionResult {
                text: all_text,
                extraction_method: "ocr".to_string(),
                page_count: images.len(),
            })
        }
    }
}
```

#### Office Documents (DOCX, XLSX, PPTX)

**Conversion Pipeline:**
1. **pandoc**: Universal document converter (DOCX → Markdown)
2. **LibreOffice headless**: Fallback for complex documents
3. **xlsx crate**: Direct Excel parsing for structured data

**Implementation:**
```rust
pub async fn process_docx(docx_data: &[u8]) -> Result<String> {
    // Save to temp file
    let temp_path = save_to_temp(docx_data, "document.docx")?;

    // Convert with pandoc
    let output = Command::new("pandoc")
        .args(&[
            temp_path.to_str().unwrap(),
            "-t", "markdown",
            "--wrap=none",
        ])
        .output()
        .await?;

    Ok(String::from_utf8(output.stdout)?)
}

pub async fn process_xlsx(xlsx_data: &[u8]) -> Result<String> {
    use calamine::{Reader, Xlsx, open_workbook_from_rs};
    use std::io::Cursor;

    let cursor = Cursor::new(xlsx_data);
    let mut workbook: Xlsx<_> = open_workbook_from_rs(cursor)?;

    let mut all_text = String::new();

    for sheet_name in workbook.sheet_names() {
        all_text.push_str(&format!("## Sheet: {}\n\n", sheet_name));

        if let Some(Ok(range)) = workbook.worksheet_range(sheet_name) {
            for row in range.rows() {
                let row_text: Vec<String> = row.iter()
                    .map(|cell| format!("{}", cell))
                    .collect();
                all_text.push_str(&row_text.join("\t"));
                all_text.push('\n');
            }
        }
    }

    Ok(all_text)
}
```

#### Code Files

**Leverage existing Tree-sitter integration:**

```rust
// Already supported via document_type registry
// Extract with syntax awareness
pub async fn process_code_file(
    code: &str,
    language: &str,
    inference: &dyn InferenceBackend,
) -> Result<CodeExtractionResult> {
    // Use tree-sitter for AST parsing
    let ast = parse_with_treesitter(code, language)?;

    // Extract functions, classes, imports
    let symbols = extract_symbols(&ast);

    // Generate description with LLM
    let description = inference.generate(GenerateRequest {
        model: "qwen2.5-coder:7b".to_string(),
        prompt: format!(
            "Summarize this {} code in 2-3 sentences:\n\n```{}\n{}\n```",
            language, language, code
        ),
        stream: false,
    }).await?.text;

    Ok(CodeExtractionResult {
        language: language.to_string(),
        symbols,
        description,
        line_count: code.lines().count(),
    })
}
```

### 3.3 Recommended AI Pipeline

**Tier 1 (Fast, Local):**
- Text extraction: pdftotext, pandoc, calamine
- Code analysis: Tree-sitter AST
- Image OCR: tesseract (fallback)

**Tier 2 (Ollama-based):**
- Vision: LLaVA 1.6 (7B) for image description
- Audio: Whisper (base) for transcription
- Code: Qwen2.5-Coder (7B) for code summarization

**Tier 3 (Optional, Cloud):**
- GPT-4V for complex image analysis
- Whisper Large for multilingual audio
- Claude for long document summarization

**Cost-Quality Trade-off:**
```rust
pub enum ProcessingQuality {
    Fast,     // Tier 1 only (free, local, fast)
    Balanced, // Tier 1 + Tier 2 (Ollama local, high quality)
    Premium,  // All tiers (cloud APIs, best quality)
}
```

---

## 4. Relationship Models

### 4.1 Attachment-Note Relationship

**Pattern: One-to-Many with Content-Addressable Blobs**

```sql
-- Blob storage (content-addressable)
CREATE TABLE attachment_blob (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    content_hash TEXT NOT NULL UNIQUE,  -- BLAKE3
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    data BYTEA NOT NULL,
    storage_type TEXT DEFAULT 'database',  -- 'database' or 'object_storage'
    object_key TEXT,                       -- S3 key if storage_type = 'object_storage'
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Attachment references (many notes can reference same blob)
CREATE TABLE attachment (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    blob_id UUID REFERENCES attachment_blob(id) ON DELETE RESTRICT,
    filename TEXT NOT NULL,              -- User-specified name
    display_order INTEGER DEFAULT 0,     -- Order in note
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID
);

-- Many-to-many for shared attachments (optional)
CREATE TABLE note_attachment (
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    attachment_id UUID REFERENCES attachment(id) ON DELETE CASCADE,
    PRIMARY KEY (note_id, attachment_id)
);
```

**Benefits:**
- Same PDF attached to 100 notes = 1 storage copy
- Deleting attachment from one note doesn't affect others
- Reference counting for garbage collection

### 4.2 Attachment Versioning

**Strategy: Immutable Blobs + Version References**

```sql
CREATE TABLE attachment_version (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID REFERENCES attachment(id) ON DELETE CASCADE,
    blob_id UUID REFERENCES attachment_blob(id) ON DELETE RESTRICT,
    version_number INTEGER NOT NULL,
    filename TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID,
    UNIQUE(attachment_id, version_number)
);

-- Current version pointer
ALTER TABLE attachment ADD COLUMN current_version_id UUID
    REFERENCES attachment_version(id);
```

**Use Case:** Track edits to attached files (e.g., updated PDF, revised image)

### 4.3 Cascading Delete vs Orphan Handling

**Strategy: Reference Counting + Periodic Cleanup**

```sql
-- Track reference count on blobs
ALTER TABLE attachment_blob ADD COLUMN reference_count INTEGER DEFAULT 0;

-- Update reference count with trigger
CREATE OR REPLACE FUNCTION update_blob_refcount()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE attachment_blob
        SET reference_count = reference_count + 1
        WHERE id = NEW.blob_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE attachment_blob
        SET reference_count = reference_count - 1
        WHERE id = OLD.blob_id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachment_refcount_update
AFTER INSERT OR DELETE ON attachment
FOR EACH ROW EXECUTE FUNCTION update_blob_refcount();

-- Periodic cleanup of orphaned blobs
CREATE OR REPLACE FUNCTION cleanup_orphaned_blobs()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM attachment_blob
    WHERE reference_count = 0
    AND created_at < NOW() - INTERVAL '7 days';

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;
```

**Cleanup Schedule:** Run daily via cron or matric-jobs

---

## 5. Search & Indexing

### 5.1 Full-Text Search on Extracted Content

**Leverage existing FTS infrastructure:**

```sql
-- Extend note_revised_current to include attachment content
CREATE OR REPLACE FUNCTION get_note_searchable_text(p_note_id UUID)
RETURNS TEXT AS $$
    SELECT
        COALESCE(nr.content, '') || E'\n\n' ||
        COALESCE(
            (SELECT string_agg(a.extracted_text, E'\n\n')
             FROM attachment a
             WHERE a.note_id = p_note_id
             AND a.extracted_text IS NOT NULL),
            ''
        )
    FROM note_revised_current nr
    WHERE nr.note_id = p_note_id;
$$ LANGUAGE sql;

-- Regenerate FTS index including attachments
CREATE INDEX idx_note_attachment_fts ON attachment
    USING GIN (to_tsvector('english', COALESCE(extracted_text, '')));
```

### 5.2 Embedding Extracted Text

**Integrate with embedding_set system:**

```sql
-- Store attachment embeddings in existing embedding table
-- (Already supports chunk_index for multi-chunk documents)

-- Job to embed attachment content
INSERT INTO job_queue (note_id, job_type, payload) VALUES
($1, 'embedding', jsonb_build_object(
    'source', 'attachment',
    'attachment_id', $2
));
```

**Chunking Strategy:**
- Use document_type's chunking_strategy (semantic, syntactic, fixed)
- Respect chunk_size_default and chunk_overlap_default
- Store in same embedding table with source='attachment'

### 5.3 Image/Audio Embeddings (CLIP)

**CLIP (Contrastive Language-Image Pre-training):**
- Embeds images and text in same vector space
- Enables "search images by text description"
- Available via Ollama: `clip-vit-b-32`, `clip-vit-l-14`

**Implementation:**
```rust
pub async fn embed_image_with_clip(
    image_data: &[u8],
    inference: &dyn InferenceBackend,
) -> Result<Vec<f32>> {
    let request = EmbeddingRequest {
        model: "clip-vit-b-32".to_string(),
        input: vec![base64::encode(image_data)],
    };

    let response = inference.embed(request).await?;
    Ok(response.embeddings[0].clone())
}

// Search images by text
pub async fn search_images_by_text(
    query: &str,
    inference: &dyn InferenceBackend,
    db: &Database,
) -> Result<Vec<Attachment>> {
    // Embed query text with CLIP
    let query_embedding = inference.embed(EmbeddingRequest {
        model: "clip-vit-b-32".to_string(),
        input: vec![query.to_string()],
    }).await?.embeddings[0].clone();

    // Search attachment_embedding table
    db.search_attachments_by_vector(&query_embedding, 10).await
}
```

**Schema Extension:**
```sql
-- Add CLIP embeddings to attachment_embedding table
ALTER TABLE attachment_embedding ADD COLUMN embedding_type TEXT DEFAULT 'text';
-- 'text' (from extracted_text) or 'clip' (from image/audio content)

-- Index for CLIP embeddings
CREATE INDEX idx_attachment_embedding_clip ON attachment_embedding
    USING hnsw (vector vector_cosine_ops)
    WHERE embedding_type = 'clip';
```

---

## 6. Backup/Restore Considerations

### 6.1 Including Attachments in Exports

**Export Format: Markdown + Attachments ZIP**

```rust
pub async fn export_note_with_attachments(
    note_id: Uuid,
    db: &Database,
) -> Result<NoteExport> {
    let note = db.get_note_full(note_id).await?;
    let attachments = db.get_attachments_for_note(note_id).await?;

    // Convert note to markdown with frontmatter
    let markdown = format!(
        r#"---
id: {}
title: {}
created: {}
tags: {}
attachments:
{}
---

{}
"#,
        note.note.id,
        note.note.title.unwrap_or_default(),
        note.note.created_at_utc,
        note.tags.join(", "),
        attachments.iter()
            .map(|a| format!("  - {}", a.filename))
            .collect::<Vec<_>>()
            .join("\n"),
        note.revised.content
    );

    // Bundle as ZIP: note.md + attachments/
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));

    zip.start_file("note.md", FileOptions::default())?;
    zip.write_all(markdown.as_bytes())?;

    for attachment in attachments {
        let path = format!("attachments/{}", attachment.filename);
        zip.start_file(path, FileOptions::default())?;

        // Fetch blob data
        let blob = db.get_attachment_blob(attachment.blob_id).await?;
        zip.write_all(&blob.data)?;
    }

    Ok(NoteExport {
        zip_data: zip.finish()?.into_inner(),
    })
}
```

### 6.2 Size Implications

**Storage Breakdown (1000 notes, 20% with attachments):**

| Component | Size per Note | Total Size |
|-----------|---------------|------------|
| Note metadata | 1 KB | 1 MB |
| Note content | 10 KB | 10 MB |
| Embeddings (768-dim) | 3 KB | 3 MB |
| Attachments (avg 500KB) | - | 100 MB |
| Thumbnails (100KB) | - | 20 MB |
| **Total** | - | **134 MB** |

**With 10,000 notes:** ~1.3 GB (manageable)
**With 100,000 notes:** ~13 GB (requires optimization)

**Optimization Strategies:**
1. **Compression**: Enable TOAST compression (pglz, lz4)
2. **Object Storage**: Move large files (>10MB) to S3
3. **Deduplication**: BLAKE3 content-addressable storage
4. **Archival**: Move old/unused attachments to cold storage

### 6.3 Streaming Large Files

**Avoid loading entire file into memory:**

```rust
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

// Stream file upload
pub async fn upload_attachment_stream(
    note_id: Uuid,
    filename: String,
    content_type: String,
    stream: impl Stream<Item = Result<Bytes>>,
    db: &Database,
) -> Result<Uuid> {
    // Stream to temp file
    let temp_path = format!("/tmp/upload-{}", Uuid::new_v4());
    let mut file = tokio::fs::File::create(&temp_path).await?;

    let mut hasher = blake3::Hasher::new();
    let mut size = 0u64;

    futures::pin_mut!(stream);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        size += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }

    file.flush().await?;
    drop(file);

    let content_hash = hasher.finalize().to_hex().to_string();

    // Check for existing blob with same hash
    if let Some(existing) = db.find_attachment_by_hash(&content_hash).await? {
        tokio::fs::remove_file(&temp_path).await?;
        return Ok(existing.id);
    }

    // Store blob (database or object storage based on size)
    if size < 10 * 1024 * 1024 {  // <10MB → database
        let data = tokio::fs::read(&temp_path).await?;
        db.create_attachment_blob(content_hash, content_type, data).await?;
    } else {  // >=10MB → object storage
        storage.upload_file(&temp_path, &content_hash).await?;
        db.create_attachment_blob_reference(content_hash, content_type, size).await?;
    }

    tokio::fs::remove_file(&temp_path).await?;

    Ok(attachment_id)
}
```

### 6.4 Integrity Verification (Checksums)

**BLAKE3 for fast verification:**

```rust
pub async fn verify_attachment_integrity(
    attachment_id: Uuid,
    db: &Database,
) -> Result<bool> {
    let attachment = db.get_attachment(attachment_id).await?;
    let blob = db.get_attachment_blob(attachment.blob_id).await?;

    let computed_hash = blake3::hash(&blob.data).to_hex().to_string();

    Ok(computed_hash == blob.content_hash)
}

// Periodic integrity check job
pub async fn check_all_attachments_integrity(db: &Database) -> Result<Vec<Uuid>> {
    let mut corrupted = Vec::new();

    let blobs = db.get_all_attachment_blobs().await?;
    for blob in blobs {
        if !verify_blob_integrity(&blob).await? {
            corrupted.push(blob.id);
        }
    }

    Ok(corrupted)
}
```

---

## 7. Security

### 7.1 Virus Scanning

**Integration Points:**
1. **ClamAV** (open-source, local)
2. **VirusTotal API** (cloud, rate-limited)
3. **Cloud provider scanning** (AWS S3 Malware Protection)

**Implementation with ClamAV:**

```rust
use std::process::Command;

pub async fn scan_file_with_clamav(file_path: &Path) -> Result<ScanResult> {
    let output = Command::new("clamdscan")
        .arg("--no-summary")
        .arg(file_path)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.contains("OK") {
        Ok(ScanResult::Clean)
    } else if stdout.contains("FOUND") {
        let virus_name = extract_virus_name(&stdout);
        Ok(ScanResult::Infected(virus_name))
    } else {
        Ok(ScanResult::Error(stdout.to_string()))
    }
}

// Async scanning job
pub struct VirusScanHandler;

#[async_trait]
impl JobHandler for VirusScanHandler {
    async fn handle(&self, job: &Job) -> Result<JobResult> {
        let payload: VirusScanPayload = serde_json::from_value(job.payload.clone())?;

        let result = scan_file_with_clamav(&payload.file_path).await?;

        match result {
            ScanResult::Clean => {
                self.db.update_attachment_scan_status(
                    payload.attachment_id,
                    "clean",
                    None
                ).await?;
                Ok(JobResult::Success)
            }
            ScanResult::Infected(virus) => {
                self.db.quarantine_attachment(payload.attachment_id).await?;
                Ok(JobResult::Failed(format!("Infected: {}", virus)))
            }
            ScanResult::Error(err) => {
                Ok(JobResult::Failed(format!("Scan error: {}", err)))
            }
        }
    }
}
```

**Quarantine Strategy:**
```sql
ALTER TABLE attachment ADD COLUMN quarantined BOOLEAN DEFAULT FALSE;
ALTER TABLE attachment ADD COLUMN quarantine_reason TEXT;

-- Prevent access to quarantined files
CREATE OR REPLACE FUNCTION check_attachment_quarantine()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.quarantined THEN
        RAISE EXCEPTION 'Attachment is quarantined: %', NEW.quarantine_reason;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

### 7.2 File Type Validation (Magic Bytes vs Extension)

**Magic Byte Validation (infer crate):**

```rust
use infer;

pub fn validate_file_type(
    data: &[u8],
    claimed_type: &str,
    filename: &str,
) -> Result<ValidatedFileType> {
    // Detect actual MIME type from magic bytes
    let detected_type = infer::get(data)
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream");

    // Check if claimed type matches detected type
    if !is_mime_compatible(claimed_type, detected_type) {
        return Err(Error::FileTypeMismatch {
            claimed: claimed_type.to_string(),
            detected: detected_type.to_string(),
        });
    }

    // Check against allowed types
    if !is_allowed_file_type(detected_type) {
        return Err(Error::FileTypeNotAllowed(detected_type.to_string()));
    }

    Ok(ValidatedFileType {
        mime_type: detected_type.to_string(),
        extension: get_extension_for_mime(detected_type),
    })
}

// Whitelist of allowed MIME types
const ALLOWED_MIME_TYPES: &[&str] = &[
    "image/jpeg", "image/png", "image/gif", "image/webp", "image/svg+xml",
    "audio/mpeg", "audio/wav", "audio/ogg",
    "video/mp4", "video/webm",
    "application/pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/zip", "application/gzip",
    "text/plain", "text/markdown", "text/html",
    "application/json", "application/xml",
];

pub fn is_allowed_file_type(mime_type: &str) -> bool {
    ALLOWED_MIME_TYPES.contains(&mime_type)
}
```

**Extension Spoofing Prevention:**
```rust
// Example: malicious.pdf.exe → Reject
pub fn detect_extension_spoofing(filename: &str, detected_mime: &str) -> bool {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let expected_mime = mime_guess::from_ext(ext).first_or_octet_stream();

    // Check if extension matches detected MIME type
    expected_mime.to_string() != detected_mime
}
```

### 7.3 Size Limits

**Multi-Tier Limits:**

```rust
pub struct AttachmentLimits {
    pub max_file_size: u64,           // Global limit (e.g., 100MB)
    pub max_image_size: u64,          // 10MB
    pub max_video_size: u64,          // 500MB
    pub max_total_per_note: u64,      // 1GB total per note
    pub max_total_per_user: u64,      // 10GB total per user
}

impl Default for AttachmentLimits {
    fn default() -> Self {
        Self {
            max_file_size: 100 * 1024 * 1024,      // 100MB
            max_image_size: 10 * 1024 * 1024,      // 10MB
            max_video_size: 500 * 1024 * 1024,     // 500MB
            max_total_per_note: 1024 * 1024 * 1024, // 1GB
            max_total_per_user: 10 * 1024 * 1024 * 1024, // 10GB
        }
    }
}

pub async fn check_size_limits(
    note_id: Uuid,
    user_id: Uuid,
    file_size: u64,
    mime_type: &str,
    db: &Database,
) -> Result<()> {
    let limits = AttachmentLimits::default();

    // Check file type specific limit
    let type_limit = if mime_type.starts_with("image/") {
        limits.max_image_size
    } else if mime_type.starts_with("video/") {
        limits.max_video_size
    } else {
        limits.max_file_size
    };

    if file_size > type_limit {
        return Err(Error::FileTooLarge {
            size: file_size,
            limit: type_limit,
        });
    }

    // Check note total
    let note_total = db.get_total_attachment_size_for_note(note_id).await?;
    if note_total + file_size > limits.max_total_per_note {
        return Err(Error::NoteTotalSizeExceeded);
    }

    // Check user total
    let user_total = db.get_total_attachment_size_for_user(user_id).await?;
    if user_total + file_size > limits.max_total_per_user {
        return Err(Error::UserQuotaExceeded);
    }

    Ok(())
}
```

### 7.4 Access Control Inheritance

**Inherit permissions from parent note:**

```sql
-- Security model: attachments inherit note permissions
CREATE OR REPLACE FUNCTION check_attachment_access(
    p_user_id UUID,
    p_attachment_id UUID,
    p_permission TEXT  -- 'read' or 'write'
)
RETURNS BOOLEAN AS $$
DECLARE
    note_id UUID;
    has_access BOOLEAN;
BEGIN
    -- Get parent note
    SELECT a.note_id INTO note_id
    FROM attachment a
    WHERE a.id = p_attachment_id;

    -- Check note access (reuse existing note permission logic)
    SELECT check_note_access(p_user_id, note_id, p_permission)
    INTO has_access;

    RETURN has_access;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Row-level security policy
ALTER TABLE attachment ENABLE ROW LEVEL SECURITY;

CREATE POLICY attachment_access_policy ON attachment
    FOR SELECT
    USING (check_attachment_access(current_user_id(), id, 'read'));
```

**Shared Attachments:**
- If note is shared → attachment is shared
- If note has PKE encryption → attachment should also be encrypted
- Tag-based isolation applies to attachments (via parent note)

---

## 8. Recommendations for matric-memory

### 8.1 Phase 1: Core Storage (MVP)

**Goal:** Basic attachment storage with deduplication

**Tasks:**
1. Create migration for attachment tables
2. Implement BLAKE3 content-addressable storage
3. Add API endpoints: POST /notes/{id}/attachments, GET, DELETE
4. Store files as PostgreSQL BYTEA (EXTERNAL storage)
5. Basic file type validation (magic bytes)
6. Size limits (10MB max)

**Schema:**
```sql
-- See Section 1.3 for full schema
CREATE TABLE attachment_blob (...);
CREATE TABLE attachment (...);
```

**Estimated Effort:** 2-3 days

### 8.2 Phase 2: Processing Pipeline

**Goal:** Extract text, generate previews, enable search

**Tasks:**
1. Extend job_type enum with attachment processing jobs
2. Implement text extraction (PDF, DOCX, images)
3. Generate thumbnails for images
4. Index extracted text in FTS
5. Generate embeddings for attachment content

**Integration:**
- Use existing matric-jobs infrastructure
- Leverage document_type registry for extraction strategy
- Store embeddings in existing embedding table

**Estimated Effort:** 4-5 days

### 8.3 Phase 3: AI Extraction

**Goal:** Multimodal processing with vision and audio models

**Tasks:**
1. Integrate LLaVA for image description
2. Integrate Whisper for audio transcription
3. Implement video processing (frame extraction + audio)
4. Add CLIP embeddings for image search
5. Intelligent processing quality selection (Fast/Balanced/Premium)

**Dependencies:**
- Ollama with vision models (LLaVA, BakLLaVA)
- whisper.cpp or Ollama Whisper
- FFmpeg for video processing

**Estimated Effort:** 5-7 days

### 8.4 Phase 4: Advanced Features (Optional)

**Goal:** Object storage, versioning, enhanced security

**Tasks:**
1. MinIO/S3 integration for large files (>10MB)
2. Attachment versioning
3. Virus scanning (ClamAV integration)
4. CLIP-based image search
5. Two-tier storage strategy

**Estimated Effort:** 5-7 days

---

## 9. API Design

### 9.1 RESTful Endpoints

```rust
// Upload attachment
POST /api/v1/notes/{note_id}/attachments
Content-Type: multipart/form-data

file: <binary data>
filename: "report.pdf"

Response 202 Accepted:
{
  "attachment_id": "01JQWX...",
  "filename": "report.pdf",
  "size_bytes": 524288,
  "content_type": "application/pdf",
  "status": "processing",
  "job_id": "01JQWY..."
}

// Get attachment metadata
GET /api/v1/attachments/{id}
Response 200 OK:
{
  "id": "01JQWX...",
  "note_id": "01JQWV...",
  "filename": "report.pdf",
  "content_type": "application/pdf",
  "size_bytes": 524288,
  "content_hash": "blake3:abc123...",
  "processing_status": "complete",
  "extracted_text": "Summary: ...",
  "has_preview": true,
  "created_at": "2026-02-01T10:30:00Z"
}

// Download attachment
GET /api/v1/attachments/{id}/download
Response 200 OK:
Content-Type: application/pdf
Content-Disposition: attachment; filename="report.pdf"

<binary data>

// Get attachment preview/thumbnail
GET /api/v1/attachments/{id}/preview
Response 200 OK:
Content-Type: image/jpeg

<preview image data>

// Delete attachment
DELETE /api/v1/attachments/{id}
Response 204 No Content

// List attachments for note
GET /api/v1/notes/{note_id}/attachments
Response 200 OK:
{
  "attachments": [
    {
      "id": "01JQWX...",
      "filename": "report.pdf",
      "size_bytes": 524288,
      "content_type": "application/pdf",
      "created_at": "2026-02-01T10:30:00Z"
    }
  ]
}

// Search attachments
GET /api/v1/attachments/search?q=financial+report&type=pdf
Response 200 OK:
{
  "results": [
    {
      "attachment_id": "01JQWX...",
      "note_id": "01JQWV...",
      "filename": "report.pdf",
      "snippet": "...financial report for Q4...",
      "score": 0.89
    }
  ]
}
```

### 9.2 Streaming Upload

```rust
// Large file streaming upload
POST /api/v1/notes/{note_id}/attachments/stream
Content-Type: application/octet-stream
X-Filename: large-video.mp4
X-Content-Type: video/mp4

<streaming binary data>

Response 202 Accepted:
{
  "attachment_id": "01JQWX...",
  "upload_id": "01JQWY...",
  "status": "uploading"
}

// Check upload status
GET /api/v1/attachments/{id}/status
Response 200 OK:
{
  "status": "uploading",
  "bytes_received": 52428800,
  "total_bytes": 524288000,
  "percent_complete": 10
}
```

---

## 10. Implementation Checklist

### Database Schema
- [ ] Create attachment_blob table with BLAKE3 hash
- [ ] Create attachment table with processing status
- [ ] Create attachment_embedding table
- [ ] Add TOAST EXTERNAL storage for blob.data
- [ ] Add GIN index for extracted_text FTS
- [ ] Add HNSW index for embeddings
- [ ] Add reference counting triggers

### API Layer (matric-api)
- [ ] POST /notes/{id}/attachments endpoint
- [ ] GET /attachments/{id} endpoint
- [ ] GET /attachments/{id}/download endpoint
- [ ] DELETE /attachments/{id} endpoint
- [ ] Multipart form data parsing
- [ ] Streaming upload support
- [ ] File type validation (magic bytes)
- [ ] Size limit enforcement

### Processing Jobs (matric-jobs)
- [ ] Add attachment_processing job type
- [ ] Add text_extraction job type
- [ ] Implement PDF text extraction
- [ ] Implement DOCX/XLSX conversion
- [ ] Implement image OCR (tesseract)
- [ ] Implement preview generation
- [ ] Implement embedding generation

### AI Integration (matric-inference)
- [ ] LLaVA vision model integration
- [ ] Whisper audio transcription
- [ ] Video processing (FFmpeg + vision)
- [ ] CLIP image embeddings
- [ ] Code summarization with Qwen2.5-Coder

### Security
- [ ] Magic byte validation (infer crate)
- [ ] File type whitelist
- [ ] Size limits per type
- [ ] ClamAV virus scanning (optional)
- [ ] Access control inheritance
- [ ] Quarantine mechanism

### Storage
- [ ] BLAKE3 content hashing
- [ ] Deduplication logic
- [ ] Reference counting
- [ ] Orphan cleanup job
- [ ] MinIO/S3 integration (Phase 4)

### Testing
- [ ] Unit tests for file validation
- [ ] Integration tests for upload/download
- [ ] Test deduplication (same file uploaded twice)
- [ ] Test size limits
- [ ] Test malicious file rejection
- [ ] Load testing (large file uploads)

---

## 11. Performance Benchmarks

### Storage Efficiency (with Deduplication)

**Scenario:** 1000 users, each attaches same 5MB PDF (company handbook)

| Strategy | Storage Required | Deduplication Savings |
|----------|------------------|----------------------|
| **No deduplication** | 5 GB (5MB × 1000) | 0% |
| **Content-addressable** | 5 MB | 99.9% |

**Scenario:** Research team, 50 notes reference same 10 papers (PDF, 2MB each)

| Strategy | Storage Required | Savings |
|----------|------------------|---------|
| **File copies** | 1 GB (2MB × 10 × 50) | 0% |
| **CAS** | 20 MB | 98% |

### Processing Latency

| File Type | Size | Extraction Time | Embedding Time | Total Time |
|-----------|------|-----------------|----------------|------------|
| **PDF (text)** | 5 MB | 0.5s (pdftotext) | 2s (nomic-embed) | 2.5s |
| **PDF (scanned)** | 5 MB | 15s (tesseract OCR) | 2s | 17s |
| **DOCX** | 2 MB | 1s (pandoc) | 1s | 2s |
| **Image (JPEG)** | 1 MB | 5s (LLaVA vision) | 0.1s (CLIP) | 5.1s |
| **Audio (MP3)** | 5 MB | 30s (Whisper base) | 2s | 32s |
| **Video (MP4)** | 50 MB | 120s (FFmpeg + LLaVA + Whisper) | 10s | 130s |

**Optimization:** Async processing allows user to continue while job runs in background

### Database Size Impact

**Test Corpus:** 10,000 notes with 20% having attachments (avg 500KB)

| Component | Size |
|-----------|------|
| Notes + metadata | 100 MB |
| Note embeddings (768-dim) | 30 MB |
| Attachments (raw) | 1 GB |
| Attachment embeddings | 60 MB |
| Thumbnails | 200 MB |
| **Total** | **1.39 GB** |

**With MRL (256-dim truncation):**
- Attachment embeddings: 20 MB (67% reduction)
- **Total: 1.35 GB** (saves 40MB)

---

## 12. References

### Industry Standards
- **MIME Types**: [IANA Media Types Registry](https://www.iana.org/assignments/media-types/)
- **Magic Bytes**: [File Signature Database](https://www.filesignatures.net/)
- **Content-Addressable Storage**: IPFS, Git, Perkeep

### PostgreSQL Documentation
- [TOAST Storage](https://www.postgresql.org/docs/current/storage-toast.html)
- [Large Objects](https://www.postgresql.org/docs/current/largeobjects.html)
- [BYTEA Performance](https://www.postgresql.org/docs/current/datatype-binary.html)

### AI/ML Processing
- **Whisper**: [OpenAI Whisper](https://github.com/openai/whisper) - Audio transcription
- **LLaVA**: [LLaVA Vision](https://llava-vl.github.io/) - Multimodal LLM
- **CLIP**: [OpenAI CLIP](https://github.com/openai/CLIP) - Vision-language embeddings
- **Unstructured.io**: [Document parsing library](https://github.com/Unstructured-IO/unstructured)

### Rust Libraries
- **infer**: Magic byte file type detection
- **blake3**: Fast cryptographic hashing
- **pdf-extract**: PDF text extraction
- **calamine**: Excel file parsing
- **image**: Image processing and thumbnails
- **whisper-rs**: Rust bindings for Whisper
- **tokio**: Async runtime for streaming

### Security
- **ClamAV**: [Open-source antivirus](https://www.clamav.net/)
- **File validation best practices**: OWASP File Upload Cheat Sheet

### Storage Solutions
- **MinIO**: S3-compatible object storage (self-hosted)
- **Supabase Storage**: PostgreSQL-metadata + S3 pattern
- **AWS S3**: Industry-standard object storage

---

## 13. Conclusion

Implementing file attachments in matric-memory requires a phased approach balancing simplicity, performance, and feature richness:

**Phase 1 (MVP):** PostgreSQL BYTEA with BLAKE3 deduplication provides a simple, atomic solution suitable for most use cases (<10MB files).

**Phase 2 (Processing):** Leverage existing job queue and document_type registry to extract text, generate embeddings, and enable full-text + semantic search across attachments.

**Phase 3 (AI):** Integrate Ollama-based multimodal models (LLaVA, Whisper, CLIP) for vision, audio, and video processing without external API dependencies.

**Phase 4 (Scale):** Add MinIO/S3 for large files, implement versioning, and enhance security with virus scanning.

This strategy aligns with matric-memory's existing architecture (PostgreSQL-centric, Ollama-powered, job-based processing) while enabling incremental complexity as user needs grow.

**Next Steps:**
1. Review and approve recommendations
2. Create implementation tasks in issue tracker
3. Begin Phase 1 development (2-3 days)
4. User testing and feedback
5. Iterate to Phase 2/3 based on usage patterns

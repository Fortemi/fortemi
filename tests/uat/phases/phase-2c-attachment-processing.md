# UAT Phase 2C: Attachment Processing Pipeline

**Purpose**: Verify document type auto-detection on upload, extraction strategy assignment, user-supplied overrides, multi-file notes, content extraction, and job queue integration for the attachment processing pipeline
**Duration**: ~20 minutes
**Prerequisites**: Phase 2B file attachments completed, test data generated
**Critical**: Yes (100% pass required)

> **Test Data**: This phase uses files from `tests/uat/data/`. Generate with:
> ```bash
> cd tests/uat/data/scripts && ./generate-test-data.sh
> ```
> Key files: `documents/code-python.py`, `documents/pdf-single-page.pdf`, `documents/markdown-formatted.md`,
> `documents/json-config.json`, `documents/csv-data.csv`, `documents/code-rust.rs`,
> `edge-cases/empty.txt`, `edge-cases/binary-wrong-ext.jpg`, `images/jpeg-with-exif.jpg`,
> `audio/english-speech-5s.mp3`

> **Relationship to Other Phases**:
> - **Phase 2B** tests file upload/download/dedup/EXIF/safety but does NOT cover document type detection or extraction pipelines
> - **Phase 8** tests document type CRUD/detection via API but does NOT test integration with actual file uploads
> - **Phase 2C** bridges the gap: upload → detect → extract → embed → link

---

## Section 1: Document Type Auto-Detection on Upload

### PROC-001: Auto-detect Python Code

**Description**: Upload a Python source file as an attachment and verify document type is auto-detected as "python" with `syntactic` chunking strategy

**Prerequisites**:
- Test data file: `tests/uat/data/documents/code-python.py`

**Steps**:
1. Create a test note:
   ```javascript
   create_note({ content: "# Python Code Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
2. Store the returned note ID as `proc_note_id`
3. Upload Python file:
   ```javascript
   upload_attachment({ note_id: proc_note_id, filename: "code-python.py", content_type: "text/x-python", data: <base64-file-bytes> })
   ```
4. Get the attachment details:
   ```javascript
   get_attachment({ id: <attachment_id> })
   ```

**Expected Results**:
- Attachment returned with `extraction_strategy: "code_ast"`
- Document type detected as "python" (or `document_type_name: "python"`)
- Chunking strategy for the detected type is `syntactic`

**Verification**:
- `detect_document_type({ filename: "code-python.py" })` returns `{ detected_type: "python", confidence: 0.9 }`
- Attachment record reflects detected type

**Store**: `proc_note_id`, `proc_python_attachment_id`

---

### PROC-002: Auto-detect PDF

**Description**: Upload a PDF file and verify document type detected as "pdf" with `fixed` chunking

**Prerequisites**:
- Test data file: `tests/uat/data/documents/pdf-single-page.pdf`
- `proc_note_id` from PROC-001

**Steps**:
1. Upload PDF:
   ```javascript
   upload_attachment({ note_id: proc_note_id, filename: "pdf-single-page.pdf", content_type: "application/pdf", data: <base64-pdf-bytes> })
   ```
2. Get attachment:
   ```javascript
   get_attachment({ id: <attachment_id> })
   ```

**Expected Results**:
- `extraction_strategy: "pdf_text"`
- Document type detected as "pdf"

**Store**: `proc_pdf_attachment_id`

---

### PROC-003: Auto-detect Markdown

**Description**: Upload a Markdown file and verify document type detected as "markdown" with `semantic` chunking

**Prerequisites**:
- Test data file: `tests/uat/data/documents/markdown-formatted.md`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Markdown Detection Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
2. Upload Markdown:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "markdown-formatted.md", content_type: "text/markdown", data: <base64-bytes> })
   ```
3. Get attachment details

**Expected Results**:
- `extraction_strategy: "text_native"` (text/markdown maps to TextNative)
- Document type detected as "markdown"

**Store**: `proc_md_note_id`, `proc_md_attachment_id`

---

### PROC-004: Auto-detect JSON Config

**Description**: Upload a JSON configuration file and verify document type detected as "json" with `whole` chunking

**Prerequisites**:
- Test data file: `tests/uat/data/documents/json-config.json`
- `proc_note_id` from PROC-001

**Steps**:
1. Upload JSON:
   ```javascript
   upload_attachment({ note_id: proc_note_id, filename: "json-config.json", content_type: "application/json", data: <base64-bytes> })
   ```
2. Get attachment details

**Expected Results**:
- `extraction_strategy: "structured_extract"` (application/json maps to StructuredExtract)
- Document type detected as "json"

**Store**: `proc_json_attachment_id`

---

### PROC-005: Auto-detect from MIME Only

**Description**: Upload a file with a generic name but specific MIME type to verify detection falls back to MIME-based classification

**Prerequisites**:
- JPEG image file (e.g., `tests/uat/data/images/jpeg-with-exif.jpg`)

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# MIME Detection Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
2. Upload with generic filename but specific MIME:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "data.bin", content_type: "image/jpeg", data: <base64-jpeg-bytes> })
   ```
3. Get attachment details

**Expected Results**:
- `extraction_strategy: "vision"` (image/jpeg maps to Vision)
- MIME type stored as "image/jpeg"
- Detection method is "mime_type" (since `.bin` extension doesn't match any known type)

**Store**: `proc_mime_note_id`, `proc_mime_attachment_id`

---

## Section 2: User-Supplied Document Type Override

### PROC-006: Override with Valid Type

**Description**: Upload a `.txt` file but supply `document_type_id` for "markdown", verify the override takes precedence over auto-detection

**Prerequisites**:
- Know the UUID for document type "markdown": `get_document_type({ name: "markdown" })`

**Steps**:
1. Get markdown type ID:
   ```javascript
   get_document_type({ name: "markdown" })
   ```
2. Store `markdown_type_id`
3. Create note:
   ```javascript
   create_note({ content: "# Override Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
4. Upload `.txt` file with override:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "readme.txt", content_type: "text/plain", data: <base64-bytes>, document_type_id: markdown_type_id })
   ```
5. Get attachment details

**Expected Results**:
- Document type is "markdown" (not "plaintext" or "text")
- The override `document_type_id` takes precedence over extension-based detection

**Store**: `proc_override_note_id`

---

### PROC-007: Override with Invalid Type

**Description**: Upload a file with an invalid (non-existent) `document_type_id` and verify graceful error handling

**Prerequisites**: None

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Invalid Override Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
2. Upload with fake type ID:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "test.txt", content_type: "text/plain", data: <base64-bytes>, document_type_id: "00000000-0000-0000-0000-000000000000" })
   ```

**Expected Results**:
- Either: Returns error with clear message about invalid document type
- Or: Falls back to auto-detection (accepts upload, ignores invalid override)
- No crash or panic

---

### PROC-008: No Override Uses Detection

**Description**: Upload a `.rs` Rust file without specifying document type and verify auto-detection returns "rust"

**Prerequisites**:
- Test data file: `tests/uat/data/documents/code-rust.rs`
- `proc_note_id` from PROC-001

**Steps**:
1. Upload Rust file without `document_type_id`:
   ```javascript
   upload_attachment({ note_id: proc_note_id, filename: "code-rust.rs", content_type: "text/x-rust", data: <base64-bytes> })
   ```
2. Get attachment details

**Expected Results**:
- Document type detected as "rust"
- `extraction_strategy: "code_ast"` (code files use CodeAst when detected via extension)
- No `document_type_id` was supplied, so detection ran automatically

**Store**: `proc_rust_attachment_id`

---

### PROC-009: Override MIME-based Detection

**Description**: Upload a file detected as "plaintext" by extension, override to "yaml", and verify chunking strategy changes

**Prerequisites**:
- Know the UUID for document type "yaml": `get_document_type({ name: "yaml" })`

**Steps**:
1. Get YAML type ID:
   ```javascript
   get_document_type({ name: "yaml" })
   ```
2. Create note:
   ```javascript
   create_note({ content: "# YAML Override Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
3. Upload `.txt` file with YAML override:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "config.txt", content_type: "text/plain", data: <base64-yaml-content>, document_type_id: yaml_type_id })
   ```
4. Get attachment details

**Expected Results**:
- Document type is "yaml" (overridden from what would have been "plaintext" or "text")
- Chunking strategy matches YAML type configuration (`whole`)

---

## Section 3: Extraction Strategy Assignment

### PROC-010: Text File -> TextNative

**Description**: Upload a plain text file and verify extraction strategy is `text_native`

**Prerequisites**:
- `proc_note_id` from PROC-001

**Steps**:
1. Upload `.txt`:
   ```javascript
   upload_attachment({ note_id: proc_note_id, filename: "readme.txt", content_type: "text/plain", data: <base64-bytes> })
   ```
2. Get attachment details

**Expected Results**:
- `extraction_strategy: "text_native"`

**Verification**:
- Matches `ExtractionStrategy::from_mime_type("text/plain")` -> `TextNative`

---

### PROC-011: PDF -> PdfText

**Description**: Upload a `.pdf` file and verify extraction strategy is `pdf_text`

**Prerequisites**:
- `proc_pdf_attachment_id` from PROC-002

**Steps**:
1. Use attachment from PROC-002 or upload new PDF:
   ```javascript
   get_attachment({ id: proc_pdf_attachment_id })
   ```

**Expected Results**:
- `extraction_strategy: "pdf_text"`

**Verification**:
- Matches `ExtractionStrategy::from_mime_type("application/pdf")` -> `PdfText`

---

### PROC-012: Image -> Vision

**Description**: Upload a `.jpg` image and verify extraction strategy is `vision`

**Prerequisites**:
- Test data file: `tests/uat/data/images/jpeg-with-exif.jpg`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Image Strategy Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
2. Upload JPEG:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "test-photo.jpg", content_type: "image/jpeg", data: <base64-bytes> })
   ```
3. Get attachment details

**Expected Results**:
- `extraction_strategy: "vision"`

**Verification**:
- Matches `ExtractionStrategy::from_mime_type("image/jpeg")` -> `Vision`

**Store**: `proc_image_note_id`, `proc_image_attachment_id`

---

### PROC-013: Audio -> AudioTranscribe

**Description**: Upload an `.mp3` audio file and verify extraction strategy is `audio_transcribe`

**Prerequisites**:
- Test data file: `tests/uat/data/audio/english-speech-5s.mp3`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Audio Strategy Test", tags: ["uat/proc-pipeline"], revision_mode: "none" })
   ```
2. Upload MP3:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "english-speech.mp3", content_type: "audio/mpeg", data: <base64-bytes> })
   ```
3. Get attachment details

**Expected Results**:
- `extraction_strategy: "audio_transcribe"`

**Verification**:
- Matches `ExtractionStrategy::from_mime_type("audio/mpeg")` -> `AudioTranscribe`

**Store**: `proc_audio_note_id`, `proc_audio_attachment_id`

---

### PROC-014: Code -> CodeAst

**Description**: Upload `.py` and `.rs` source files and verify extraction strategy is `code_ast` (when extension is used for disambiguation)

**Prerequisites**:
- `proc_python_attachment_id` from PROC-001
- `proc_rust_attachment_id` from PROC-008

**Steps**:
1. Verify Python attachment:
   ```javascript
   get_attachment({ id: proc_python_attachment_id })
   ```
2. Verify Rust attachment:
   ```javascript
   get_attachment({ id: proc_rust_attachment_id })
   ```

**Expected Results**:
- Both attachments have extraction strategy reflecting code handling
- `ExtractionStrategy::from_mime_and_extension("text/x-python", Some("py"))` -> `CodeAst`
- `ExtractionStrategy::from_mime_and_extension("text/x-rust", Some("rs"))` -> `CodeAst`

---

## Section 4: Multi-File Notes

### PROC-015: Multiple Files on One Note

**Description**: Upload 3 files of different types (`.py`, `.md`, `.jpg`) to the same note and verify all 3 attachments listed with correct types

**Prerequisites**:
- Test data files: `documents/code-python.py`, `documents/markdown-formatted.md`, `images/jpeg-with-exif.jpg`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Multi-File Note", tags: ["uat/proc-multifile"], revision_mode: "none" })
   ```
2. Store as `multifile_note_id`
3. Upload Python file:
   ```javascript
   upload_attachment({ note_id: multifile_note_id, filename: "code-python.py", content_type: "text/x-python", data: <base64-bytes> })
   ```
4. Upload Markdown file:
   ```javascript
   upload_attachment({ note_id: multifile_note_id, filename: "markdown-formatted.md", content_type: "text/markdown", data: <base64-bytes> })
   ```
5. Upload JPEG:
   ```javascript
   upload_attachment({ note_id: multifile_note_id, filename: "photo.jpg", content_type: "image/jpeg", data: <base64-bytes> })
   ```
6. List attachments:
   ```javascript
   list_attachments({ note_id: multifile_note_id })
   ```

**Expected Results**:
- Returns array of 3 attachments
- Each attachment has correct `filename`, `content_type`, and `extraction_strategy`
- Python: `code_ast`, Markdown: `text_native`, JPEG: `vision`

**Store**: `multifile_note_id`

---

### PROC-016: Mixed Types Same Note

**Description**: Upload code + PDF + image to same note, verify each gets correct extraction strategy independently

**Prerequisites**:
- `multifile_note_id` from PROC-015

**Steps**:
1. List all attachments on the multi-file note:
   ```javascript
   list_attachments({ note_id: multifile_note_id })
   ```
2. Get details for each attachment individually

**Expected Results**:
- Python file: extraction strategy related to code processing
- If a PDF were added: `pdf_text`
- JPEG: `vision`
- Each attachment's strategy is determined independently, not influenced by siblings

---

### PROC-017: Max Attachments

**Description**: Upload 10 files to a single note and verify all are stored and listed correctly

**Prerequisites**:
- Various test data files

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Max Attachments Test", tags: ["uat/proc-multifile"], revision_mode: "none" })
   ```
2. Upload 10 different files (mix of types: .txt, .py, .rs, .js, .ts, .md, .json, .yaml, .csv, .jpg)
3. List attachments:
   ```javascript
   list_attachments({ note_id: <note_id> })
   ```

**Expected Results**:
- Returns array of 10 attachments
- All filenames, content types, and sizes are correct
- No errors or truncation

**Store**: `proc_max_note_id`

---

### PROC-018: Multiple Notes Each with Files

**Description**: Create 3 separate notes, each with 2 different file types, and verify attachment isolation (note A's files don't appear on note B)

**Prerequisites**:
- Various test data files

**Steps**:
1. Create 3 notes:
   ```javascript
   create_note({ content: "# Note A", tags: ["uat/proc-isolation"], revision_mode: "none" })
   create_note({ content: "# Note B", tags: ["uat/proc-isolation"], revision_mode: "none" })
   create_note({ content: "# Note C", tags: ["uat/proc-isolation"], revision_mode: "none" })
   ```
2. Upload 2 files to each note (different types per note):
   - Note A: `.py` + `.jpg`
   - Note B: `.md` + `.json`
   - Note C: `.rs` + `.csv`
3. List attachments for each note separately

**Expected Results**:
- Note A: exactly 2 attachments (Python + JPEG)
- Note B: exactly 2 attachments (Markdown + JSON)
- Note C: exactly 2 attachments (Rust + CSV)
- No cross-contamination between notes

---

### PROC-019: Same File Different Notes

**Description**: Upload identical file to 2 different notes, verify content deduplication (same `blob_id`) but separate attachment records with independent type detection

**Prerequisites**:
- Test data file: `documents/code-python.py`

**Steps**:
1. Create 2 notes:
   ```javascript
   create_note({ content: "# Dedup Note 1", tags: ["uat/proc-dedup"], revision_mode: "none" })
   create_note({ content: "# Dedup Note 2", tags: ["uat/proc-dedup"], revision_mode: "none" })
   ```
2. Upload same Python file to both notes:
   ```javascript
   upload_attachment({ note_id: note_1_id, filename: "code-python.py", content_type: "text/x-python", data: <base64-bytes> })
   upload_attachment({ note_id: note_2_id, filename: "code-python.py", content_type: "text/x-python", data: <base64-bytes> })
   ```
3. Get both attachment details

**Expected Results**:
- Two distinct attachment records (different `id` values)
- Same `blob_id` (content deduplication)
- Both have same document type detected ("python")
- Both have same extraction strategy
- Each attachment's `note_id` points to the correct note

---

## Section 5: Content Extraction Verification

### PROC-020: Text Extraction from Plain Text

**Description**: Upload a plain English text file and verify `extracted_text` field contains the source text

**Prerequisites**:
- Test data file: `tests/uat/data/multilingual/english.txt`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Text Extraction Test", tags: ["uat/proc-extraction"], revision_mode: "none" })
   ```
2. Upload text file:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "english.txt", content_type: "text/plain", data: <base64-bytes> })
   ```
3. Wait 3 seconds for extraction job to process
4. Get attachment details:
   ```javascript
   get_attachment({ id: <attachment_id> })
   ```

**Expected Results**:
- `extracted_text` field is present and non-empty
- Contains recognizable phrases from the source text (e.g., "quick brown fox", "natural language processing")
- `extraction_strategy: "text_native"`

**Store**: `proc_text_extraction_id`

---

### PROC-021: Structured Data Extraction (JSON)

**Description**: Upload a JSON config file and verify extracted metadata includes top-level keys and structure

**Prerequisites**:
- Test data file: `tests/uat/data/documents/json-config.json`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# JSON Extraction Test", tags: ["uat/proc-extraction"], revision_mode: "none" })
   ```
2. Upload JSON:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "json-config.json", content_type: "application/json", data: <base64-bytes> })
   ```
3. Wait 3 seconds for extraction
4. Get attachment details

**Expected Results**:
- `extraction_strategy: "structured_extract"`
- `extracted_metadata` includes information about JSON structure
- Top-level keys identifiable (e.g., "name", "version", "database", "embedding", "search")

---

### PROC-022: Structured Data Extraction (CSV)

**Description**: Upload a CSV data file and verify extracted metadata includes column names and row count

**Prerequisites**:
- Test data file: `tests/uat/data/documents/csv-data.csv`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# CSV Extraction Test", tags: ["uat/proc-extraction"], revision_mode: "none" })
   ```
2. Upload CSV:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "csv-data.csv", content_type: "text/csv", data: <base64-bytes> })
   ```
3. Wait 3 seconds for extraction
4. Get attachment details

**Expected Results**:
- `extraction_strategy: "structured_extract"`
- `extracted_metadata` includes CSV structure information
- Column names identifiable (e.g., "id", "name", "email", "created_at", "status")

---

### PROC-023: Code Extraction Preserves Structure

**Description**: Upload Python source code and verify extraction captures class/function names

**Prerequisites**:
- Test data file: `tests/uat/data/documents/code-python.py`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Code Extraction Test", tags: ["uat/proc-extraction"], revision_mode: "none" })
   ```
2. Upload Python code:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "code-python.py", content_type: "text/x-python", data: <base64-bytes> })
   ```
3. Wait 3 seconds for extraction
4. Get attachment details

**Expected Results**:
- Extraction captures code structure information
- Class name "DataProcessor" identifiable in extracted text or metadata
- Function name "main" identifiable
- Method names "process", "_transform" identifiable

---

### PROC-024: Empty File Extraction

**Description**: Upload an empty text file and verify graceful handling with no crash

**Prerequisites**:
- Test data file: `tests/uat/data/edge-cases/empty.txt`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Empty File Test", tags: ["uat/proc-extraction"], revision_mode: "none" })
   ```
2. Upload empty file:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "empty.txt", content_type: "text/plain", data: "" })
   ```
3. Wait 2 seconds
4. Get attachment details

**Expected Results**:
- Upload succeeds (no error)
- Attachment record created
- `extracted_text` is null or empty
- Metadata may note empty content (e.g., `"empty_content": true`)
- No crash, no panic, no unhandled error

---

## Section 6: Job Queue Integration

### PROC-025: Upload Creates Extraction Job

**Description**: Upload a file and verify a job of appropriate type is created in the job queue

**Prerequisites**:
- Working job system (Phase 15 validated)

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Job Queue Test", tags: ["uat/proc-jobs"], revision_mode: "none" })
   ```
2. Upload a text file:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "test-for-job.txt", content_type: "text/plain", data: <base64-text-bytes> })
   ```
3. Immediately query jobs:
   ```javascript
   list_jobs({ status: "pending" })
   ```
   or
   ```javascript
   list_jobs({ limit: 10 })
   ```

**Expected Results**:
- A job exists that references the uploaded attachment
- Job type is related to content extraction/processing
- Job was created within seconds of upload

**Store**: `proc_job_note_id`, `proc_job_attachment_id`

---

### PROC-026: Job References Correct Attachment

**Description**: Verify the queued job's payload contains the correct attachment ID

**Prerequisites**:
- `proc_job_attachment_id` from PROC-025

**Steps**:
1. List recent jobs:
   ```javascript
   list_jobs({ limit: 5 })
   ```
2. Find the job related to the attachment from PROC-025
3. Get job details:
   ```javascript
   get_job({ id: <job_id> })
   ```

**Expected Results**:
- Job payload or metadata references `proc_job_attachment_id`
- Job type corresponds to the extraction strategy for the uploaded file

---

### PROC-027: Job Status Lifecycle

**Description**: Upload a file and poll job status through `pending` -> `processing` -> `completed`

**Prerequisites**:
- Working job worker (processing jobs)

**Steps**:
1. Create note and upload file:
   ```javascript
   create_note({ content: "# Job Lifecycle Test", tags: ["uat/proc-jobs"], revision_mode: "none" })
   upload_attachment({ note_id: <note_id>, filename: "lifecycle-test.txt", content_type: "text/plain", data: <base64-bytes> })
   ```
2. Immediately check jobs for `pending` status
3. Wait 2-5 seconds and check again
4. Wait up to 30 seconds total, polling every 3 seconds

**Expected Results**:
- Job initially in `pending` or `processing` state
- Job eventually reaches `completed` state
- Job `completed_at` timestamp is present when done
- No stuck jobs (unless worker is not running, in which case job stays `pending`)

---

### PROC-028: Failed Extraction Doesn't Crash

**Description**: Upload a file with wrong extension (random bytes with `.jpg` extension) and verify the extraction job completes with an error status but doesn't crash the system

**Prerequisites**:
- Test data file: `tests/uat/data/edge-cases/binary-wrong-ext.jpg`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# Failed Extraction Test", tags: ["uat/proc-jobs"], revision_mode: "none" })
   ```
2. Upload binary-wrong-ext.jpg:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "binary-wrong-ext.jpg", content_type: "image/jpeg", data: <base64-random-bytes> })
   ```
3. Wait 5-10 seconds for processing attempt
4. Check job status:
   ```javascript
   list_jobs({ limit: 5 })
   ```
5. Verify system health:
   ```javascript
   health_check()
   ```

**Expected Results**:
- Upload itself may succeed or fail depending on validation
- If upload succeeds: extraction job created but completes with `failed` or `error` status
- System remains healthy (health check passes)
- No crash, no panic in logs
- Error message is descriptive (e.g., "Invalid image format" or "Extraction failed")

---

## Section 7: End-to-End Pipeline

### PROC-029: Full Pipeline - Text File

**Description**: Upload a text file and verify the complete pipeline: upload -> detect type -> extract text -> searchable content exists

**Prerequisites**:
- Test data file: `tests/uat/data/multilingual/english.txt`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# E2E Text Pipeline", tags: ["uat/proc-e2e"], revision_mode: "none" })
   ```
2. Upload English text file:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "english.txt", content_type: "text/plain", data: <base64-bytes> })
   ```
3. Wait 5 seconds for processing
4. Get attachment to verify extraction:
   ```javascript
   get_attachment({ id: <attachment_id> })
   ```
5. Search for content from the uploaded file:
   ```javascript
   search_notes({ query: "transformer architecture attention mechanisms", tags: ["uat/proc-e2e"] })
   ```

**Expected Results**:
- Attachment has `extraction_strategy: "text_native"`
- `extracted_text` contains the source text
- Search returns the note (content from attachment is searchable)
- Full pipeline completed without errors

---

### PROC-030: Full Pipeline - Code File

**Description**: Upload a Rust source file and verify the complete pipeline: upload -> detect "rust" -> extract with CodeAst -> function/struct names in metadata

**Prerequisites**:
- Test data file: `tests/uat/data/documents/code-rust.rs`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# E2E Code Pipeline", tags: ["uat/proc-e2e"], revision_mode: "none" })
   ```
2. Upload Rust code:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "code-rust.rs", content_type: "text/x-rust", data: <base64-bytes> })
   ```
3. Wait 5 seconds for processing
4. Get attachment:
   ```javascript
   get_attachment({ id: <attachment_id> })
   ```

**Expected Results**:
- Document type detected as "rust"
- Extraction captures code structure
- Struct names ("ProcessorConfig", "DataProcessor") identifiable in extracted content or metadata
- Function names ("new", "process", "transform") identifiable

---

### PROC-031: Full Pipeline - Multi-File Note

**Description**: Upload PDF + code + image to one note, verify each processed with different strategies, all results associated to the same note

**Prerequisites**:
- Test data files: `documents/pdf-single-page.pdf`, `documents/code-python.py`, `images/jpeg-with-exif.jpg`

**Steps**:
1. Create note:
   ```javascript
   create_note({ content: "# E2E Multi-File Pipeline", tags: ["uat/proc-e2e"], revision_mode: "none" })
   ```
2. Upload PDF:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "document.pdf", content_type: "application/pdf", data: <base64-bytes> })
   ```
3. Upload Python code:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "processor.py", content_type: "text/x-python", data: <base64-bytes> })
   ```
4. Upload JPEG:
   ```javascript
   upload_attachment({ note_id: <note_id>, filename: "photo.jpg", content_type: "image/jpeg", data: <base64-bytes> })
   ```
5. Wait 10 seconds for all extractions
6. List attachments:
   ```javascript
   list_attachments({ note_id: <note_id> })
   ```
7. Get each attachment individually

**Expected Results**:
- 3 attachments listed for the note
- PDF: `extraction_strategy: "pdf_text"`
- Python: extraction strategy for code handling
- JPEG: `extraction_strategy: "vision"`
- Each attachment processed independently with correct strategy
- All associated to the same `note_id`
- No interference between extraction pipelines

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| PROC-001 | Auto-detect Python Code | |
| PROC-002 | Auto-detect PDF | |
| PROC-003 | Auto-detect Markdown | |
| PROC-004 | Auto-detect JSON Config | |
| PROC-005 | Auto-detect from MIME Only | |
| PROC-006 | Override with Valid Type | |
| PROC-007 | Override with Invalid Type | |
| PROC-008 | No Override Uses Detection | |
| PROC-009 | Override MIME-based Detection | |
| PROC-010 | Text File -> TextNative | |
| PROC-011 | PDF -> PdfText | |
| PROC-012 | Image -> Vision | |
| PROC-013 | Audio -> AudioTranscribe | |
| PROC-014 | Code -> CodeAst | |
| PROC-015 | Multiple Files One Note | |
| PROC-016 | Mixed Types Same Note | |
| PROC-017 | Max Attachments (10) | |
| PROC-018 | Multiple Notes with Files | |
| PROC-019 | Same File Different Notes | |
| PROC-020 | Text Extraction Plain Text | |
| PROC-021 | JSON Structure Extraction | |
| PROC-022 | CSV Structure Extraction | |
| PROC-023 | Code Structure Extraction | |
| PROC-024 | Empty File Extraction | |
| PROC-025 | Upload Creates Extraction Job | |
| PROC-026 | Job References Attachment | |
| PROC-027 | Job Status Lifecycle | |
| PROC-028 | Failed Extraction No Crash | |
| PROC-029 | E2E Text File Pipeline | |
| PROC-030 | E2E Code File Pipeline | |
| PROC-031 | E2E Multi-File Pipeline | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:

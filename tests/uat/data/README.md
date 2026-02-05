# UAT Test Data Package

Comprehensive test data for exercising all Fortémi capabilities including:
- Document type detection and chunking
- EXIF metadata extraction
- Multilingual full-text search
- W3C PROV provenance tracking
- Vision extraction (image description)
- Audio transcription
- Edge case handling

## Directory Structure

```
tests/uat/data/
├── README.md                          # This file
├── MANIFEST.md                        # Detailed file inventory
├── images/                            # Image files with metadata
│   ├── jpeg-with-exif.jpg            # JPEG with full EXIF (GPS, camera, date)
│   ├── jpeg-no-metadata.jpg          # JPEG stripped of metadata
│   ├── png-transparent.png           # PNG with transparency
│   ├── webp-modern.webp              # WebP for modern format support
│   ├── faces-group-photo.jpg         # Image with human faces
│   ├── object-scene.jpg              # Scene with recognizable objects
│   └── emoji-unicode-名前.jpg         # Unicode filename edge case
├── documents/                         # Document files
│   ├── pdf-single-page.pdf           # Simple 1-page PDF
│   ├── pdf-multi-page.pdf            # Multi-page PDF with TOC
│   ├── code-python.py                # Python code sample
│   ├── code-rust.rs                  # Rust code sample
│   ├── code-javascript.js            # JavaScript code sample
│   ├── code-typescript.ts            # TypeScript code sample
│   ├── markdown-formatted.md         # Markdown with various elements
│   ├── json-config.json              # JSON configuration
│   ├── yaml-config.yaml              # YAML configuration
│   └── csv-data.csv                  # CSV data file
├── audio/                             # Audio samples
│   ├── english-speech-5s.mp3         # 5-second English speech
│   ├── spanish-greeting.mp3          # Spanish language sample
│   └── chinese-phrase.mp3            # Chinese (Mandarin) sample
├── multilingual/                      # Multilingual text samples
│   ├── english.txt                   # English text
│   ├── german.txt                    # German text
│   ├── french.txt                    # French text
│   ├── spanish.txt                   # Spanish text
│   ├── portuguese.txt                # Portuguese text
│   ├── russian.txt                   # Russian text (Cyrillic)
│   ├── chinese-simplified.txt        # Simplified Chinese (CJK)
│   ├── japanese.txt                  # Japanese (CJK)
│   ├── korean.txt                    # Korean (CJK)
│   ├── arabic.txt                    # Arabic text
│   ├── greek.txt                     # Greek text
│   ├── hebrew.txt                    # Hebrew text
│   └── emoji-heavy.txt               # Text with many emoji
├── edge-cases/                        # Edge case test files
│   ├── empty.txt                     # Empty file (0 bytes)
│   ├── large-text-100kb.txt          # Large text file (>100KB)
│   ├── binary-wrong-ext.jpg          # Binary file misnamed as image
│   ├── unicode-filename-测试.txt      # Unicode in filename
│   ├── whitespace-only.txt           # File with only whitespace
│   └── malformed-json.json           # JSON with syntax errors
├── provenance/                        # Files for provenance testing
│   ├── paris-eiffel-tower.jpg        # GPS: Paris, France
│   ├── newyork-statue-liberty.jpg    # GPS: New York, USA
│   ├── tokyo-shibuya.jpg             # GPS: Tokyo, Japan
│   ├── dated-2020-01-01.jpg          # Known timestamp: 2020-01-01
│   ├── dated-2025-12-31.jpg          # Known timestamp: 2025-12-31
│   ├── duplicate-content-1.txt       # Duplicate content test
│   └── duplicate-content-2.txt       # Same content, different file
└── scripts/                           # Generation scripts
    ├── generate-test-data.sh         # Main generation script
    ├── create-exif-images.py         # Python script for EXIF injection
    ├── generate-multilingual.py      # Generate multilingual samples
    └── create-audio-samples.sh       # Generate audio samples
```

## File Specifications

See `MANIFEST.md` for detailed specifications of each test file including:
- Expected metadata
- File size
- Content description
- Expected extraction results
- Test scenarios

## Usage

### Quick Setup

Generate all synthetic test data:

```bash
cd tests/uat/data/scripts
./generate-test-data.sh
```

This will create all necessary test files in their respective directories.

### Individual Generation

Generate specific categories:

```bash
# Images with EXIF
python3 scripts/create-exif-images.py

# Multilingual text samples
python3 scripts/generate-multilingual.py

# Audio samples (requires ffmpeg)
./scripts/create-audio-samples.sh
```

### Download Pre-built Test Data

For convenience, pre-built test data is available:

```bash
# Download from release artifacts
wget https://github.com/fortemi/fortemi/releases/download/v2026.2.0/uat-test-data.tar.gz
tar -xzf uat-test-data.tar.gz -C tests/uat/data/
```

## Test Scenarios

### 1. Image Metadata Extraction

**Files**: `images/jpeg-with-exif.jpg`, `provenance/paris-eiffel-tower.jpg`

**Expected behavior**:
- Extract GPS coordinates and convert to PostGIS geography
- Extract camera make/model (e.g., "Apple iPhone 15 Pro")
- Extract capture datetime and convert to UTC
- Store as W3C PROV provenance data

**Verification**:
```bash
# Upload image
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@images/jpeg-with-exif.jpg" \
  -F "tags=test,image"

# Check extracted metadata
curl http://localhost:3000/api/v1/notes/{note_id} | jq '.note.metadata'
```

### 2. Document Type Auto-Detection

**Files**: `documents/code-python.py`, `documents/markdown-formatted.md`

**Expected behavior**:
- Auto-detect document type from file extension and magic patterns
- Apply appropriate chunking strategy (syntactic for code, semantic for prose)
- Associate with document_type_id

**Verification**:
```sql
SELECT n.id, n.title, dt.name as document_type, dt.chunking_strategy
FROM note n
JOIN document_type dt ON n.document_type_id = dt.id
WHERE n.title LIKE '%python%';
```

### 3. Multilingual Full-Text Search

**Files**: `multilingual/*.txt`

**Expected behavior**:
- English/German/French/Spanish/Portuguese: Use stemming via `websearch_to_tsquery`
- CJK (Chinese/Japanese/Korean): Use bigram matching
- Arabic/Russian/Greek/Hebrew: Basic tokenization
- Emoji: Trigram substring matching

**Verification**:
```bash
# English stemming
curl "http://localhost:3000/api/v1/search?q=running&tags=test" # matches "run", "runs", "running"

# CJK bigram
curl "http://localhost:3000/api/v1/search?q=東京&tags=test" # matches Chinese/Japanese text

# Emoji search
curl "http://localhost:3000/api/v1/search?q=🎉&tags=test" # matches emoji content
```

### 4. Provenance Tracking

**Files**: `provenance/paris-eiffel-tower.jpg`, `provenance/duplicate-content-*.txt`

**Expected behavior**:
- Track GPS coordinates as spatial provenance
- Track timestamps as temporal provenance
- Detect duplicate content via content hash
- Link related notes through provenance chains

**Verification**:
```sql
-- Check spatial provenance
SELECT n.id, n.title,
       ST_AsText(p.location_geography::geometry) as location,
       p.created_at_utc
FROM note n
JOIN provenance_edge p ON n.id = p.revision_id
WHERE p.location_geography IS NOT NULL;

-- Check duplicate content
SELECT hash, COUNT(*) as count
FROM note_original
GROUP BY hash
HAVING COUNT(*) > 1;
```

### 5. Edge Case Handling

**Files**: `edge-cases/*`

**Expected behavior**:
- Empty file: Accept but warn (no content to index)
- Large file (>100KB): Chunk appropriately based on document type
- Binary with wrong extension: Reject with clear error message
- Unicode filename: Store correctly without mojibake
- Whitespace-only: Accept but mark as empty content
- Malformed JSON: Store as plain text if JSON parsing fails

**Verification**:
```bash
# Empty file
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/empty.txt" \
  -F "tags=test,edge-case"
# Expected: HTTP 200, warning in metadata

# Large file
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/large-text-100kb.txt" \
  -F "tags=test,edge-case"
# Expected: HTTP 200, multiple chunks created

# Unicode filename
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@edge-cases/unicode-filename-测试.txt" \
  -F "tags=test,unicode"
# Expected: HTTP 200, filename stored correctly
```

## Coverage Matrix

| Capability | Test Files | Expected Result |
|------------|------------|-----------------|
| **EXIF GPS extraction** | `images/jpeg-with-exif.jpg`, `provenance/paris-*.jpg` | PostGIS geography with coordinates |
| **EXIF datetime extraction** | `provenance/dated-*.jpg` | UTC timestamp in metadata |
| **EXIF camera info** | `images/jpeg-with-exif.jpg` | Device make/model in metadata |
| **Image without metadata** | `images/jpeg-no-metadata.jpg` | No EXIF metadata, file accepted |
| **Modern image formats** | `images/webp-modern.webp` | WebP support verified |
| **Vision extraction** | `images/faces-group-photo.jpg` | AI-generated description of scene |
| **PDF single page** | `documents/pdf-single-page.pdf` | Text extraction, whole chunking |
| **PDF multi-page** | `documents/pdf-multi-page.pdf` | Text extraction, per-section chunking |
| **Code syntactic chunking** | `documents/code-*.{py,rs,js,ts}` | Tree-sitter syntactic chunks |
| **Markdown semantic chunking** | `documents/markdown-formatted.md` | Semantic paragraph chunks |
| **JSON/YAML parsing** | `documents/*.{json,yaml}` | Structured data extraction |
| **Audio transcription (EN)** | `audio/english-speech-5s.mp3` | Speech-to-text transcription |
| **Audio transcription (ES)** | `audio/spanish-greeting.mp3` | Spanish transcription |
| **Audio transcription (ZH)** | `audio/chinese-phrase.mp3` | Chinese transcription |
| **FTS stemming (EN/DE/FR/ES/PT)** | `multilingual/{english,german,french,spanish,portuguese}.txt` | Stemmed search matches |
| **FTS bigram (CJK)** | `multilingual/{chinese,japanese,korean}.txt` | Character bigram matches |
| **FTS basic (AR/RU/EL/HE)** | `multilingual/{arabic,russian,greek,hebrew}.txt` | Basic tokenization |
| **Emoji/trigram search** | `multilingual/emoji-heavy.txt` | Trigram substring matches |
| **Empty file handling** | `edge-cases/empty.txt` | Graceful handling, warning |
| **Large file chunking** | `edge-cases/large-text-100kb.txt` | Appropriate chunking strategy |
| **Binary detection** | `edge-cases/binary-wrong-ext.jpg` | Error with clear message |
| **Unicode filenames** | `edge-cases/unicode-filename-测试.txt`, `images/emoji-unicode-名前.jpg` | Correct storage |
| **Whitespace-only** | `edge-cases/whitespace-only.txt` | Empty content flag |
| **Malformed data** | `edge-cases/malformed-json.json` | Fallback to plain text |
| **GPS provenance** | `provenance/paris-*.jpg`, `provenance/newyork-*.jpg`, `provenance/tokyo-*.jpg` | Spatial provenance tracking |
| **Temporal provenance** | `provenance/dated-*.jpg` | Timestamp provenance tracking |
| **Content deduplication** | `provenance/duplicate-content-*.txt` | Same hash detected |

## Size Guidelines

To keep the repository lean:
- Images: Maximum 500KB each (compressed)
- Audio: Maximum 100KB each (5-10 seconds, compressed)
- Documents: Maximum 200KB each
- Total package: <10MB

For larger test files, use the download mechanism or generate synthetically.

## Dependencies

### Required Tools

- **Python 3.8+** (for generation scripts)
- **ffmpeg** (for audio generation)
- **ImageMagick** (for image manipulation)
- **exiftool** (for EXIF injection)

Install on Ubuntu/Debian:
```bash
sudo apt-get install python3 ffmpeg imagemagick exiftool python3-pip
pip3 install Pillow piexif faker gtts pydub
```

Install on macOS:
```bash
brew install python ffmpeg imagemagick exiftool
pip3 install Pillow piexif faker gtts pydub
```

## Maintenance

### Adding New Test Files

1. Add file to appropriate directory
2. Update `MANIFEST.md` with file specification
3. Update this README's coverage matrix
4. Add verification steps to test scenarios
5. Update generation scripts if synthetic

### Updating Existing Files

When updating test files:
1. Document changes in `MANIFEST.md`
2. Update expected results in test scenarios
3. Regenerate using scripts for consistency
4. Verify with integration tests

## References

- EXIF metadata extraction: `crates/matric-core/src/exif.rs`
- Document type registry: `migrations/20260202*_seed_*_document_types.sql`
- W3C PROV provenance: `crates/matric-db/src/provenance.rs`
- Multilingual FTS: `docs/content/search-capabilities.md`
- Document type detection: `crates/matric-core/src/models.rs`

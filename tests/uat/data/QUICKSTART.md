# UAT Test Data - Quick Start Guide

This guide gets you up and running with the comprehensive UAT test data package in under 5 minutes.

## Prerequisites

Install required tools:

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y python3 python3-pip imagemagick exiftool ffmpeg

# macOS
brew install python imagemagick exiftool ffmpeg

# Python packages
pip3 install Pillow piexif faker gtts
```

## Quick Generation

Generate all test data with a single command:

```bash
cd tests/uat/data/scripts
./generate-test-data.sh
```

This will create ~46 test files (~6MB total) across all categories:
- Images with EXIF metadata (7 files)
- Documents (10 files: Python, Rust, JS, TS, Markdown, JSON, YAML, CSV)
- Audio samples (3 files: English, Spanish, Chinese)
- Multilingual text (13 files: 13 languages)
- Edge cases (6 files: empty, large, malformed, etc.)
- Provenance test files (7 files: GPS, timestamps, duplicates)

## Verify Generation

```bash
# Check what was created
find ../images ../documents ../audio ../multilingual ../edge-cases ../provenance -type f | wc -l

# Should output: 46 (or close to it)

# Check total size
du -sh ..
# Should be under 10MB
```

## Quick Test

Upload a test image to verify EXIF extraction:

```bash
# Start matric-memory API (if not running)
docker compose -f docker-compose.bundle.yml up -d

# Upload image with EXIF
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../images/jpeg-with-exif.jpg" \
  -F "tags=test,exif" \
  | jq '.note.metadata'

# Expected: GPS coordinates, camera info, timestamp
```

## Test Scenarios

### 1. EXIF Metadata Extraction

```bash
# Upload Paris image
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../provenance/paris-eiffel-tower.jpg" \
  -F "tags=test,provenance"

# Check extracted metadata
curl http://localhost:3000/api/v1/notes | jq '.notes[] | select(.tags[] == "provenance")'
```

### 2. Document Type Detection

```bash
# Upload Python code
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../documents/code-python.py" \
  -F "tags=test,code"

# Check document type
curl http://localhost:3000/api/v1/notes | jq '.notes[] | select(.tags[] == "code") | .document_type_name'
# Expected: "python"
```

### 3. Multilingual Search

```bash
# Upload multilingual samples
for lang in english german french spanish; do
  curl -X POST http://localhost:3000/api/v1/notes \
    -F "content=@../multilingual/${lang}.txt" \
    -F "tags=test,multilingual,${lang}"
done

# Test English stemming
curl "http://localhost:3000/api/v1/search?q=running&tags=test"
# Should match "run", "runs", "running"

# Test German stemming
curl "http://localhost:3000/api/v1/search?q=laufen&tags=test"
# Should match "läuft", "lief", "gelaufen"
```

### 4. CJK Search

```bash
# Upload CJK samples
for lang in chinese-simplified japanese korean; do
  curl -X POST http://localhost:3000/api/v1/notes \
    -F "content=@../multilingual/${lang}.txt" \
    -F "tags=test,cjk,${lang}"
done

# Test Chinese bigram search
curl "http://localhost:3000/api/v1/search?q=北京&tags=test"
# Should match "北京市", "北京大学"

# Test Japanese bigram search
curl "http://localhost:3000/api/v1/search?q=東京&tags=test"
# Should match "東京都", "東京大学"
```

### 5. Emoji Search

```bash
# Upload emoji-heavy content
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../multilingual/emoji-heavy.txt" \
  -F "tags=test,emoji"

# Search for specific emoji
curl "http://localhost:3000/api/v1/search?q=🎉&tags=test"
# Should find documents with that emoji
```

### 6. Edge Cases

```bash
# Empty file
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../edge-cases/empty.txt" \
  -F "tags=test,edge-case"
# Expected: HTTP 200, warning about empty content

# Large file
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../edge-cases/large-text-100kb.txt" \
  -F "tags=test,edge-case,large"
# Expected: HTTP 200, multiple chunks created

# Binary with wrong extension
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../edge-cases/binary-wrong-ext.jpg" \
  -F "tags=test,edge-case,invalid"
# Expected: HTTP 400 or error message

# Unicode filename
curl -X POST http://localhost:3000/api/v1/notes \
  -F "content=@../edge-cases/unicode-filename-测试.txt" \
  -F "tags=test,edge-case,unicode"
# Expected: HTTP 200, filename stored correctly
```

## Coverage Checklist

Use this checklist to verify all capabilities are tested:

- [ ] **EXIF extraction**: GPS, datetime, camera info
- [ ] **Document types**: Auto-detection from extension and magic
- [ ] **Code chunking**: Syntactic chunks for Python, Rust, JS, TS
- [ ] **Semantic chunking**: Markdown, plain text
- [ ] **Multilingual FTS**: Stemming for EN/DE/FR/ES/PT/RU
- [ ] **CJK search**: Bigram matching for ZH/JA/KO
- [ ] **Emoji search**: Trigram matching
- [ ] **Provenance**: GPS coordinates, timestamps
- [ ] **Edge cases**: Empty, large, malformed, unicode
- [ ] **Audio transcription**: EN/ES/ZH speech-to-text

## Troubleshooting

### Images not generated

```bash
# Check dependencies
python3 -c "import PIL, piexif" || pip3 install Pillow piexif
```

### Audio generation fails

```bash
# gTTS required
python3 -c "from gtts import gTTS" || pip3 install gtts

# ffmpeg required
ffmpeg -version || brew install ffmpeg  # macOS
ffmpeg -version || sudo apt-get install ffmpeg  # Ubuntu
```

### EXIF not extracted

```bash
# Verify EXIF is present
exiftool tests/uat/data/images/jpeg-with-exif.jpg | grep GPS
# Should show latitude/longitude

# Check matric-memory logs
docker compose -f docker-compose.bundle.yml logs -f api
```

### Search not working

```bash
# Verify FTS environment variables
docker compose -f docker-compose.bundle.yml exec api env | grep FTS

# Should see:
# FTS_SCRIPT_DETECTION=true
# FTS_TRIGRAM_FALLBACK=true
# FTS_BIGRAM_CJK=true
```

## Next Steps

1. **Review generated files**: Check `MANIFEST.md` for detailed specifications
2. **Run full UAT suite**: `cd tests/uat && ./run-uat.sh` (if available)
3. **Integration tests**: Incorporate into CI/CD pipeline
4. **Performance testing**: Use large file variants for load testing

## Customization

### Generate Specific Categories Only

```bash
# Images only
python3 scripts/create-exif-images.py

# Multilingual only
python3 scripts/generate-multilingual.py

# Code samples only
python3 scripts/generate-code-samples.py

# Documents only
python3 scripts/generate-documents.py

# Edge cases only
python3 scripts/generate-edge-cases.py

# Audio only
bash scripts/create-audio-samples.sh
```

### Modify Test Data

Edit the generation scripts to customize:
- GPS coordinates in `create-exif-images.py`
- Language samples in `generate-multilingual.py`
- Code complexity in `generate-code-samples.py`

## References

- Full documentation: `README.md`
- File specifications: `MANIFEST.md`
- Generation scripts: `scripts/`
- System capabilities: `/docs/content/`

## Support

If you encounter issues:
1. Check `MANIFEST.md` for expected results
2. Verify dependencies are installed
3. Review generation script output for errors
4. Check matric-memory API logs
5. Open an issue on Gitea with error details

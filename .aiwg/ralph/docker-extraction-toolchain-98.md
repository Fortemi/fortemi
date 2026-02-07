# Docker Extraction Toolchain Implementation (Issue #98)

## Summary
Added extraction toolchain dependencies to Docker images to support the document extraction pipeline.

## Changes Made

### 1. `/home/roctinam/dev/fortemi/Dockerfile` (Standalone API Image)
**Lines 58-67**: Added extraction tools to runtime stage

```dockerfile
# Install runtime dependencies including extraction toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    poppler-utils \
    tesseract-ocr \
    tesseract-ocr-eng \
    pandoc \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*
```

### 2. `/home/roctinam/dev/fortemi/Dockerfile.bundle` (All-in-One Bundle)
**Lines 48-67**: Added extraction tools to runtime stage

```dockerfile
# Install runtime dependencies for matric-api, Node.js for MCP server, PostGIS, and extraction toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    gnupg \
    # Extraction toolchain
    poppler-utils \
    tesseract-ocr \
    tesseract-ocr-eng \
    pandoc \
    ffmpeg \
    # PostGIS for spatial/geographic queries (W3C PROV prov:atLocation)
    postgresql-16-postgis-3 \
    postgresql-16-postgis-3-scripts \
    && mkdir -p /etc/apt/keyrings \
    ...
```

## Extraction Tools Installed

| Tool | Package | Purpose | Used By |
|------|---------|---------|---------|
| pdftotext, pdftoppm, pdfinfo | poppler-utils | PDF text extraction & rendering | PdfText, PdfOcr adapters |
| tesseract | tesseract-ocr + tesseract-ocr-eng | OCR engine with English language data | PdfOcr adapter |
| pandoc | pandoc | Universal document converter | OfficeConvert adapter |
| ffmpeg | ffmpeg | Media processing (video/audio) | VideoMultimodal adapter |

## Implementation Details

- All tools installed in **runtime stage only** (not build stage)
- Used `--no-install-recommends` to minimize image size
- Cleaned up apt lists after installation with `rm -rf /var/lib/apt/lists/*`
- No changes to build stage or Dockerfile.testdb (PostgreSQL test image)
- Multi-stage build pattern preserved

## Files Modified

1. `/home/roctinam/dev/fortemi/Dockerfile` - Added 5 extraction packages
2. `/home/roctinam/dev/fortemi/Dockerfile.bundle` - Added 4 extraction packages (poppler-utils was already present)

## Next Steps

1. Rebuild Docker images to include new dependencies
2. Test extraction adapters in Docker environment
3. Update CI/CD pipeline if needed to use updated images

## Testing Verification

The extraction tools can be verified after building with:

```bash
# Build and test
docker build -f Dockerfile -t fortemi:api .
docker run --rm fortemi:api sh -c "which pdftotext && which tesseract && which pandoc && which ffmpeg"

# Bundle
docker build -f Dockerfile.bundle -t fortemi:bundle .
docker run --rm fortemi:bundle sh -c "which pdftotext && which tesseract && which pandoc && which ffmpeg"
```

## Related Issues

- Issue #98: Docker extraction toolchain support
- Related to extraction pipeline implementation issues #103, #111, #112, #114, #115, #105

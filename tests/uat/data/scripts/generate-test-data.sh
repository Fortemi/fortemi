#!/usr/bin/env bash
# Generate all UAT test data for matric-memory
#
# This script creates comprehensive test files for exercising all system
# capabilities including EXIF extraction, document type detection, multilingual
# search, and provenance tracking.
#
# Requirements:
#   - Python 3.8+ (for generation scripts)
#   - ffmpeg (for audio generation)
#   - ImageMagick (for image manipulation)
#   - exiftool (for EXIF injection)
#
# Usage:
#   ./generate-test-data.sh [--skip-downloads]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$(dirname "$SCRIPT_DIR")"

# Use venv Python if available
VENV_DIR="$DATA_DIR/.venv"
if [ -d "$VENV_DIR" ] && [ -f "$VENV_DIR/bin/python3" ]; then
    PYTHON="$VENV_DIR/bin/python3"
    PIP="$VENV_DIR/bin/pip"
    echo "Using venv Python: $PYTHON"
else
    PYTHON="python3"
    PIP="pip3"
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Flags
SKIP_DOWNLOADS=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-downloads)
            SKIP_DOWNLOADS=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--skip-downloads]"
            echo ""
            echo "Options:"
            echo "  --skip-downloads    Skip downloading external images (use generated only)"
            echo "  -h, --help          Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

# Check dependencies
check_dependency() {
    if ! command -v "$1" &> /dev/null; then
        echo -e "${RED}Error: $1 is not installed${NC}"
        echo "Install it with: $2"
        return 1
    fi
    echo -e "${GREEN}✓${NC} $1 found"
    return 0
}

echo "Checking dependencies..."
check_dependency python3 "apt-get install python3 (Ubuntu) or brew install python3 (macOS)" || exit 1
check_dependency convert "apt-get install imagemagick (Ubuntu) or brew install imagemagick (macOS)" || exit 1
check_dependency exiftool "apt-get install exiftool (Ubuntu) or brew install exiftool (macOS)" || exit 1

# Optional: ffmpeg for audio
if command -v ffmpeg &> /dev/null; then
    echo -e "${GREEN}✓${NC} ffmpeg found"
    HAS_FFMPEG=true
else
    echo -e "${YELLOW}⚠${NC} ffmpeg not found - audio generation will be skipped"
    echo "  Install with: apt-get install ffmpeg (Ubuntu) or brew install ffmpeg (macOS)"
    HAS_FFMPEG=false
fi

# Check Python packages
echo ""
echo "Checking Python packages..."
$PYTHON -c "import PIL" 2>/dev/null && echo -e "${GREEN}✓${NC} Pillow" || {
    echo -e "${YELLOW}⚠${NC} Pillow not installed - installing..."
    $PIP install Pillow
}

$PYTHON -c "import piexif" 2>/dev/null && echo -e "${GREEN}✓${NC} piexif" || {
    echo -e "${YELLOW}⚠${NC} piexif not installed - installing..."
    $PIP install piexif
}

$PYTHON -c "from faker import Faker" 2>/dev/null && echo -e "${GREEN}✓${NC} faker" || {
    echo -e "${YELLOW}⚠${NC} faker not installed - installing..."
    $PIP install faker
}

# Check for gTTS (optional for audio)
if $HAS_FFMPEG; then
    $PYTHON -c "from gtts import gTTS" 2>/dev/null && echo -e "${GREEN}✓${NC} gTTS" || {
        echo -e "${YELLOW}⚠${NC} gTTS not installed - installing..."
        $PIP install gtts
    }
fi

echo ""
echo "=== Starting test data generation ==="
echo ""

# Create directory structure
echo "Creating directory structure..."
mkdir -p "$DATA_DIR"/{images,documents,audio,multilingual,edge-cases,provenance}
echo -e "${GREEN}✓${NC} Directories created"

# Generate images with EXIF
echo ""
echo "Generating images..."
$PYTHON "$SCRIPT_DIR/create-exif-images.py" || {
    echo -e "${RED}✗${NC} Image generation failed"
    exit 1
}
echo -e "${GREEN}✓${NC} Images generated"

# Generate multilingual text samples
echo ""
echo "Generating multilingual text samples..."
$PYTHON "$SCRIPT_DIR/generate-multilingual.py" || {
    echo -e "${RED}✗${NC} Multilingual text generation failed"
    exit 1
}
echo -e "${GREEN}✓${NC} Multilingual samples generated"

# Generate code samples
echo ""
echo "Generating code samples..."
$PYTHON "$SCRIPT_DIR/generate-code-samples.py" || {
    echo -e "${RED}✗${NC} Code sample generation failed"
    exit 1
}
echo -e "${GREEN}✓${NC} Code samples generated"

# Generate edge cases
echo ""
echo "Generating edge case files..."
$PYTHON "$SCRIPT_DIR/generate-edge-cases.py" || {
    echo -e "${RED}✗${NC} Edge case generation failed"
    exit 1
}
echo -e "${GREEN}✓${NC} Edge cases generated"

# Generate audio samples (if ffmpeg available)
if $HAS_FFMPEG; then
    echo ""
    echo "Generating audio samples..."
    bash "$SCRIPT_DIR/create-audio-samples.sh" || {
        echo -e "${YELLOW}⚠${NC} Audio generation failed (non-critical)"
    }
    echo -e "${GREEN}✓${NC} Audio samples generated"
else
    echo ""
    echo -e "${YELLOW}⚠${NC} Skipping audio generation (ffmpeg not available)"
fi

# Generate documents
echo ""
echo "Generating document samples..."
$PYTHON "$SCRIPT_DIR/generate-documents.py" || {
    echo -e "${RED}✗${NC} Document generation failed"
    exit 1
}
echo -e "${GREEN}✓${NC} Documents generated"

# Summary
echo ""
echo "=== Test data generation complete ==="
echo ""
echo "Summary:"
find "$DATA_DIR/images" -type f 2>/dev/null | wc -l | xargs echo "  Images:"
find "$DATA_DIR/documents" -type f 2>/dev/null | wc -l | xargs echo "  Documents:"
find "$DATA_DIR/audio" -type f 2>/dev/null | wc -l | xargs echo "  Audio:"
find "$DATA_DIR/multilingual" -type f 2>/dev/null | wc -l | xargs echo "  Multilingual:"
find "$DATA_DIR/edge-cases" -type f 2>/dev/null | wc -l | xargs echo "  Edge cases:"
find "$DATA_DIR/provenance" -type f 2>/dev/null | wc -l | xargs echo "  Provenance:"

TOTAL_SIZE=$(du -sh "$DATA_DIR" | cut -f1)
echo "  Total size: $TOTAL_SIZE"

echo ""
echo -e "${GREEN}✓${NC} All test data generated successfully"
echo ""
echo "Next steps:"
echo "  1. Review generated files in: $DATA_DIR"
echo "  2. Run UAT tests: cd tests/uat && ./run-uat.sh"
echo "  3. Check MANIFEST.md for expected results"

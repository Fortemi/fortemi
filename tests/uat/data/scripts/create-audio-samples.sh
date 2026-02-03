#!/usr/bin/env bash
# Generate audio samples for transcription testing
#
# Requires: ffmpeg, gTTS (Google Text-to-Speech)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$(dirname "$SCRIPT_DIR")"
AUDIO_DIR="$DATA_DIR/audio"

mkdir -p "$AUDIO_DIR"

echo "Generating audio samples..."

# Check if gTTS is available
VENV_DIR="$DATA_DIR/.venv"
if [ -d "$VENV_DIR" ] && [ -f "$VENV_DIR/bin/python3" ]; then
    PYTHON="$VENV_DIR/bin/python3"
else
    PYTHON="python3"
fi

if ! $PYTHON -c "from gtts import gTTS" 2>/dev/null; then
    echo "Error: gTTS not installed"
    echo "Install with: pip3 install gtts"
    exit 1
fi

# Function to generate audio with gTTS
generate_audio() {
    local text="$1"
    local lang="$2"
    local output="$3"

    $PYTHON << EOF
from gtts import gTTS
import os

text = """$text"""
tts = gTTS(text=text, lang='$lang', slow=False)
tts.save('$output')
print(f"  ✓ Created {os.path.basename('$output')}")
EOF
}

# English sample
echo "  Creating english-speech-5s.mp3..."
generate_audio \
    "Welcome to Matric Memory. This is a test of the audio transcription system." \
    "en" \
    "$AUDIO_DIR/english-speech-5s.mp3"

# Spanish sample
echo "  Creating spanish-greeting.mp3..."
generate_audio \
    "Hola, bienvenido a Matric Memory." \
    "es" \
    "$AUDIO_DIR/spanish-greeting.mp3"

# Chinese sample
echo "  Creating chinese-phrase.mp3..."
generate_audio \
    "欢迎使用 Matric Memory" \
    "zh-CN" \
    "$AUDIO_DIR/chinese-phrase.mp3"

echo ""
echo "✓ Generated 3 audio files"
echo "  Languages: English, Spanish, Chinese"
echo ""
echo "Note: Audio files generated with gTTS (synthetic speech)"
echo "For production UAT, consider using real human speech samples"

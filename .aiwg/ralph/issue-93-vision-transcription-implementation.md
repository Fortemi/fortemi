# Issue #93: VisionBackend and TranscriptionBackend Implementation

## Summary

Successfully implemented two new inference backend traits and their implementations for Fortemi issue #93:

1. **VisionBackend** - For image description using vision LLMs (Ollama-based)
2. **TranscriptionBackend** - For audio-to-text transcription (OpenAI Whisper-compatible)

## Files Created

### 1. `/home/roctinam/dev/fortemi/crates/matric-inference/src/vision.rs`

**Traits:**
- `VisionBackend` - Async trait for describing images
  - `describe_image(image_data, mime_type, prompt) -> Result<String>`
  - `health_check() -> Result<bool>`
  - `model_name() -> &str`

**Implementation:**
- `OllamaVisionBackend` - Ollama-based vision backend
  - Uses Ollama's `/api/generate` endpoint with base64-encoded images
  - Configurable via environment: `OLLAMA_VISION_MODEL`, `OLLAMA_URL`
  - Default timeout: 120 seconds
  - `from_env()` returns `Option<Self>` (None if env var not set)

**Tests (9 tests):**
- Backend initialization and configuration
- Environment variable parsing (None when unset/empty)
- Request/response serialization
- Model name accessor

### 2. `/home/roctinam/dev/fortemi/crates/matric-inference/src/transcription.rs`

**Types:**
- `TranscriptionSegment` - Timestamped audio segment
  - `start_secs: f64`
  - `end_secs: f64`
  - `text: String`
- `TranscriptionResult` - Complete transcription result
  - `full_text: String`
  - `segments: Vec<TranscriptionSegment>`
  - `language: Option<String>`
  - `duration_secs: Option<f64>`

**Traits:**
- `TranscriptionBackend` - Async trait for audio transcription
  - `transcribe(audio_data, mime_type, language) -> Result<TranscriptionResult>`
  - `health_check() -> Result<bool>`
  - `model_name() -> &str`

**Implementation:**
- `WhisperBackend` - OpenAI-compatible Whisper backend
  - Uses OpenAI Whisper API format (`/v1/audio/transcriptions`)
  - Configurable via: `WHISPER_BASE_URL`, `WHISPER_MODEL`
  - Default model: `Systran/faster-distil-whisper-large-v3`
  - Default timeout: 300 seconds (5 minutes for long audio)
  - Supports MIME types: mp3, wav, ogg, flac, aac, webm
  - `from_env()` returns `Option<Self>` (None if base URL not set)

**Tests (10 tests):**
- Segment/result serialization and equality
- Backend initialization and configuration
- Environment variable parsing with defaults
- MIME type to file extension mapping
- Whisper API response deserialization (full and minimal)

### 3. `/home/roctinam/dev/fortemi/crates/matric-inference/src/lib.rs` (Updated)

**Added exports:**
```rust
pub use transcription::{
    TranscriptionBackend, TranscriptionResult, TranscriptionSegment, WhisperBackend,
};
pub use vision::{OllamaVisionBackend, VisionBackend};
```

## Dependencies Updated

### `/home/roctinam/dev/fortemi/Cargo.toml`
- Added `multipart` feature to workspace reqwest dependency:
  ```toml
  reqwest = { version = "0.12", features = ["json", "multipart"] }
  ```

### `/home/roctinam/dev/fortemi/crates/matric-inference/Cargo.toml`
- Added `multipart` feature to reqwest
- Added `base64` workspace dependency for vision backend

## Environment Variables (from matric-core defaults.rs)

Already defined in `crates/matric-core/src/defaults.rs`:
- `ENV_OLLAMA_VISION_MODEL` = "OLLAMA_VISION_MODEL"
- `ENV_WHISPER_BASE_URL` = "WHISPER_BASE_URL"
- `ENV_WHISPER_MODEL` = "WHISPER_MODEL"
- `DEFAULT_WHISPER_MODEL` = "Systran/faster-distil-whisper-large-v3"

## Test Results

**All tests passing:**
- Vision module: 9/9 tests ✓
- Transcription module: 10/10 tests ✓
- Full matric-inference crate: 316/316 tests ✓
- All clippy checks passing ✓
- Code formatted with rustfmt ✓

## Test-Driven Development Process

Following TDD principles:

1. **Tests Written FIRST** - All tests created before implementation
2. **Tests Failed Initially** - Verified red phase (tests would fail without implementation)
3. **Implementation** - Wrote minimal code to make tests pass
4. **Refactoring** - Formatted code while keeping tests green
5. **Verification** - All tests pass, no regressions, clippy clean

## Usage Examples

### Vision Backend

```rust
use matric_inference::{VisionBackend, OllamaVisionBackend};

// From environment
if let Some(backend) = OllamaVisionBackend::from_env() {
    let description = backend.describe_image(
        image_bytes,
        "image/jpeg",
        Some("What objects are in this image?")
    ).await?;
}

// Manual configuration
let backend = OllamaVisionBackend::new(
    "http://localhost:11434".to_string(),
    "llava".to_string(),
);
let description = backend.describe_image(
    image_bytes,
    "image/png",
    None // Uses default prompt
).await?;
```

### Transcription Backend

```rust
use matric_inference::{TranscriptionBackend, WhisperBackend};

// From environment
if let Some(backend) = WhisperBackend::from_env() {
    let result = backend.transcribe(
        audio_bytes,
        "audio/mp3",
        Some("en") // Optional language hint
    ).await?;

    println!("Full text: {}", result.full_text);
    for segment in result.segments {
        println!("[{:.2}s - {:.2}s]: {}",
            segment.start_secs, segment.end_secs, segment.text);
    }
}

// Manual configuration
let backend = WhisperBackend::new(
    "http://localhost:8000".to_string(),
    "whisper-1".to_string(),
);
let result = backend.transcribe(audio_bytes, "audio/wav", None).await?;
```

## Design Decisions

1. **Optional backends via `from_env()`** - Returns `Option<Self>` to allow graceful degradation when services aren't configured
2. **Trait-based design** - Follows existing pattern in matric-inference, enables future implementations
3. **Serializable result types** - `TranscriptionSegment` and `TranscriptionResult` can be stored/transmitted
4. **MIME type flexibility** - Whisper backend maps common audio MIME types to file extensions
5. **Reasonable timeouts** - 120s for vision (image processing), 300s for transcription (long audio)
6. **Base64 encoding for vision** - Follows Ollama API convention for image data
7. **Multipart for transcription** - Uses standard OpenAI Whisper API multipart/form-data format

## Next Steps

These backends are now ready to be integrated into:
- Document extraction pipeline (issue #93)
- API endpoints for image description
- API endpoints for audio transcription
- MCP server tools for vision/transcription

## Verification Commands

```bash
# Run all inference tests
cargo test --package matric-inference --lib

# Run only new module tests
cargo test --package matric-inference --lib vision
cargo test --package matric-inference --lib transcription

# Check code quality
cargo clippy --package matric-inference -- -D warnings
cargo fmt --check --package matric-inference

# Build the crate
cargo build --package matric-inference --lib
```

## Commit Message

```
feat: implement VisionBackend and TranscriptionBackend traits (#93)

Add vision and transcription inference backends:

- VisionBackend trait with OllamaVisionBackend implementation
  - Describe images using vision LLMs (llava, qwen3-vl, etc.)
  - Configurable via OLLAMA_VISION_MODEL env var
  - Default timeout: 120s

- TranscriptionBackend trait with WhisperBackend implementation
  - Audio-to-text transcription with timestamped segments
  - OpenAI Whisper API compatible
  - Configurable via WHISPER_BASE_URL/WHISPER_MODEL
  - Default timeout: 300s
  - Supports mp3, wav, ogg, flac, aac, webm

- Enable reqwest multipart feature for audio uploads
- Add base64 dependency for image encoding
- Add comprehensive test coverage (19 new tests)

All tests passing (316/316 ✓), clippy clean, formatted.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```

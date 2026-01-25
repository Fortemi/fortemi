# ADR-005: Optional Streaming Support

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-010-openai-backend.md

## Context

Streaming is important for UX during long text generations - users see output incrementally rather than waiting for the complete response. However, not all use cases need streaming:
- Embedding operations return fixed vectors (no streaming possible)
- Batch processing can use non-streaming for simplicity
- Background jobs don't need streaming

## Decision

Implement streaming as a separate `StreamingGeneration` trait that backends can optionally implement. The non-streaming `generate()` method remains the primary API.

Streaming uses Server-Sent Events (SSE) format, which is standard for OpenAI-compatible APIs.

## Consequences

### Positive
- (+) Non-streaming code path remains simple
- (+) Streaming is opt-in for callers who need it
- (+) Backends without streaming support still work
- (+) SSE parsing handles OpenAI, Ollama, and compatible APIs

### Negative
- (-) Two code paths to maintain (streaming vs non-streaming)
- (-) Streaming trait not part of core `InferenceBackend` trait
- (-) Need to handle partial/incomplete chunks in streaming
- (-) Error handling differs between streaming and non-streaming

## Implementation

**Code Location:**
- Streaming trait: `crates/matric-inference/src/streaming.rs`
- SSE parsing: `crates/matric-inference/src/openai/streaming.rs`

**Streaming Trait:**

```rust
#[async_trait]
pub trait StreamingGeneration: Send + Sync {
    /// Stream generation results as they become available
    async fn generate_stream(
        &self,
        prompt: &str,
        options: &GenerationOptions,
    ) -> Result<impl Stream<Item = Result<String>>>;
}
```

**SSE Parsing:**

```rust
// Parse SSE events from OpenAI-compatible APIs
pub fn parse_sse_stream(
    response: Response,
) -> impl Stream<Item = Result<GenerationChunk>> {
    response
        .bytes_stream()
        .try_filter_map(|bytes| async move {
            let line = String::from_utf8_lossy(&bytes);
            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    Ok(None)  // End of stream
                } else {
                    let chunk: GenerationChunk = serde_json::from_str(data)?;
                    Ok(Some(chunk))
                }
            } else {
                Ok(None)  // Skip non-data lines
            }
        })
}
```

## References

- ARCH-010-openai-backend.md (Section 13)
- OpenAI Streaming API documentation
- Server-Sent Events specification

# ADR-004: Unified Error Types

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-010-openai-backend.md

## Context

OpenAI API returns structured error responses with specific error codes and messages. Other backends have their own error formats. Callers shouldn't need to handle different error types depending on which backend is used.

## Decision

Map all backend-specific errors to existing `matric_core::Error` variants. Add helper functions for common error mappings. Backend-specific error details are preserved in the error context where useful.

Error mapping follows this pattern:
- HTTP 401/403 → `Error::Authentication`
- HTTP 429 → `Error::RateLimited`
- HTTP 500+ → `Error::Backend`
- Connection failures → `Error::Connection`
- Invalid response → `Error::Parse`

## Consequences

### Positive
- (+) Consistent error handling across backends
- (+) No need to add OpenAI-specific error handling in callers
- (+) Existing error handling code works unchanged
- (+) Single error type in public API

### Negative
- (-) Some loss of error detail in mapping (e.g., specific OpenAI error codes)
- (-) OpenAI-specific error codes not directly accessible
- (-) May need to unwrap error chain for detailed debugging

## Implementation

**Code Location:**
- Core errors: `crates/matric-core/src/error.rs`
- OpenAI mapping: `crates/matric-inference/src/openai/error.rs`

**Error Mapping:**

```rust
// openai/error.rs
impl From<OpenAIError> for matric_core::Error {
    fn from(err: OpenAIError) -> Self {
        match err {
            OpenAIError::Authentication(_) => Error::Authentication(err.to_string()),
            OpenAIError::RateLimit { retry_after } =>
                Error::RateLimited { retry_after_secs: retry_after },
            OpenAIError::InvalidRequest(msg) => Error::Validation(msg),
            OpenAIError::ServerError(status, msg) =>
                Error::Backend(format!("OpenAI server error {}: {}", status, msg)),
            OpenAIError::Network(e) => Error::Connection(e.to_string()),
        }
    }
}
```

**Preserving Context:**

```rust
// For detailed debugging, original error preserved
let result = backend.embed_texts(&texts).await
    .map_err(|e| Error::Backend(format!("embedding failed: {}", e)))?;
```

## References

- ARCH-010-openai-backend.md (Section 13)
- matric-core error types

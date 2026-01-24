# Issue #131: Add OpenRouter API Header Support

## Summary

Successfully implemented optional HTTP-Referer and X-Title header support for the OpenAI-compatible backend in `matric-inference` crate. These headers are required by OpenRouter.ai for app identification and ranking purposes.

## Changes Made

### 1. Core Implementation

**File: `/home/roctinam/dev/matric-memory/crates/matric-inference/src/openai/backend.rs`**

- Added two new optional fields to `OpenAIConfig`:
  - `http_referer: Option<String>` - HTTP-Referer header for OpenRouter.ai rankings
  - `x_title: Option<String>` - X-Title header for app name on OpenRouter.ai

- Updated `Default` implementation to initialize new fields as `None`

- Enhanced `from_env()` method to read environment variables:
  - `OPENAI_HTTP_REFERER` - Sets the HTTP-Referer header
  - `OPENAI_X_TITLE` - Sets the X-Title header

- Modified `build_request()` method to conditionally add headers when configured:
  ```rust
  if let Some(ref referer) = self.config.http_referer {
      req = req.header("HTTP-Referer", referer);
  }

  if let Some(ref title) = self.config.x_title {
      req = req.header("X-Title", title);
  }
  ```

### 2. Documentation Updates

**File: `/home/roctinam/dev/matric-memory/crates/matric-inference/src/openai/mod.rs`**

- Updated module-level documentation example to include new fields

### 3. Unit Tests

**File: `/home/roctinam/dev/matric-memory/crates/matric-inference/src/openai/backend.rs`**

Added 3 new unit tests:
- `test_openrouter_headers_in_config()` - Verify headers stored in config
- `test_config_with_only_http_referer()` - Test partial header configuration
- `test_config_with_only_x_title()` - Test partial header configuration

### 4. Integration Tests

**File: `/home/roctinam/dev/matric-memory/crates/matric-inference/tests/openrouter_headers_test.rs`**

Created comprehensive integration test suite with 5 tests using wiremock:
- `test_openrouter_headers_sent_in_request()` - Verify headers sent for embeddings
- `test_generation_with_openrouter_headers()` - Verify headers sent for generation
- `test_headers_not_sent_when_not_configured()` - Verify no headers when not set
- `test_only_http_referer_header()` - Test partial header configuration
- `test_only_x_title_header()` - Test partial header configuration

### 5. Example Code

**File: `/home/roctinam/dev/matric-memory/crates/matric-inference/examples/openrouter_headers.rs`**

Created a comprehensive example demonstrating:
- Using environment variables
- Direct configuration
- Partial header configurations
- Standard usage without headers

## Test Results

### Unit Tests
```
9 unit tests passed in openai::backend::tests module
All 268 tests in matric-inference passed
```

### Integration Tests
```
5 integration tests passed in openrouter_headers_test
All mock server assertions verified
```

### Code Quality
- `cargo fmt --check` - PASSED
- `cargo clippy -- -D warnings` - PASSED
- All doctests - PASSED

## Usage Examples

### Environment Variables
```bash
export OPENAI_BASE_URL=https://openrouter.ai/api/v1
export OPENAI_API_KEY=sk-or-v1-your-key
export OPENAI_HTTP_REFERER=https://myapp.com
export OPENAI_X_TITLE="My App"
```

### Direct Configuration
```rust
use matric_inference::openai::{OpenAIBackend, OpenAIConfig};

let config = OpenAIConfig {
    base_url: "https://openrouter.ai/api/v1".to_string(),
    api_key: Some("sk-or-v1-...".to_string()),
    embed_model: "text-embedding-3-small".to_string(),
    gen_model: "openai/gpt-4o-mini".to_string(),
    embed_dimension: 1536,
    timeout_seconds: 120,
    skip_tls_verify: false,
    http_referer: Some("https://myapp.com".to_string()),
    x_title: Some("My App".to_string()),
};

let backend = OpenAIBackend::new(config)?;
```

### Optional Headers
Both headers are optional and can be used independently:
```rust
// Only HTTP-Referer
let config = OpenAIConfig {
    http_referer: Some("https://myapp.com".to_string()),
    x_title: None,
    ..Default::default()
};

// Only X-Title
let config = OpenAIConfig {
    http_referer: None,
    x_title: Some("My App".to_string()),
    ..Default::default()
};
```

## Files Modified
- `/home/roctinam/dev/matric-memory/crates/matric-inference/src/openai/backend.rs`
- `/home/roctinam/dev/matric-memory/crates/matric-inference/src/openai/mod.rs`

## Files Created
- `/home/roctinam/dev/matric-memory/crates/matric-inference/tests/openrouter_headers_test.rs`
- `/home/roctinam/dev/matric-memory/crates/matric-inference/examples/openrouter_headers.rs`

## Backward Compatibility

This change is fully backward compatible:
- New fields are optional (default to `None`)
- Existing configurations continue to work without modification
- Headers are only sent when explicitly configured
- No breaking changes to existing APIs

## Test Coverage

Coverage for new functionality:
- Unit tests: 3 new tests covering config validation
- Integration tests: 5 new tests covering HTTP request behavior
- All tests verify both positive and negative cases
- Mock server tests ensure headers are actually sent in HTTP requests

## Notes

- Headers are only added to POST requests (embeddings and chat completions)
- GET requests (like health checks) do not include these headers
- Both headers are fully optional and independent
- Implementation follows existing code patterns in the crate
- All tests pass with both single-threaded and multi-threaded execution

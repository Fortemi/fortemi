# Raw Mode Support for Thinking Models

## Overview

This implementation adds automatic raw mode support for thinking models like DeepSeek R1 that require `raw: true` in Ollama API calls to expose their internal reasoning via `<think>` tags.

## What is Raw Mode?

Raw mode (`raw: true` in Ollama API) disables prompt templating and allows models to output their native format. This is crucial for thinking models that use special tags like `<think>...</think>` to expose their chain-of-thought reasoning.

Without raw mode, Ollama applies a default prompt template that may strip or hide these thinking tags.

## Implementation

### Files Modified

1. **`src/model_config.rs`** (new)
   - Contains `requires_raw_mode()` function
   - Detects models that need raw mode based on model name patterns
   - Comprehensive test suite with 14 test cases

2. **`src/ollama.rs`**
   - Added `raw: Option<bool>` field to `GenerateRequest`
   - Modified `generate_with_system()` to check model name and set raw mode
   - Added debug logging for raw mode status
   - Added tests for raw mode serialization

3. **`src/lib.rs`**
   - Exported `model_config` module

## Supported Models

### Models Requiring Raw Mode

Based on research in `docs/research/MODEL_INVENTORY.md`:

- **DeepSeek R1** variants: `deepseek-r1:14b`, `deepseek-r1:70b`, etc.
- **Mistral Nemo Thinking** variants: `Mistral-Nemo-12B-Thinking`, etc.

### Detection Logic

The `requires_raw_mode()` function uses pattern matching:

```rust
pub fn requires_raw_mode(model_name: &str) -> bool {
    let model_lower = model_name.to_lowercase();

    // DeepSeek R1 models (all variants)
    if model_lower.starts_with("deepseek-r1:") || model_lower.starts_with("deepseek-r1-") {
        return true;
    }

    // Mistral Nemo Thinking models
    if model_lower.contains("mistral-nemo") && model_lower.contains("thinking") {
        return true;
    }

    false
}
```

## Usage

### Automatic Application

Raw mode is automatically applied when using thinking models:

```rust
use matric_inference::OllamaBackend;

let backend = OllamaBackend::with_config(
    "http://localhost:11434".to_string(),
    "nomic-embed-text".to_string(),
    "deepseek-r1:14b".to_string(),  // Raw mode will be automatically enabled
    768,
);

// Raw mode is automatically set to true
let response = backend.generate("Explain quantum computing").await?;
// Response will include <think>...</think> tags with reasoning
```

### Manual Checking

You can also check if a model requires raw mode:

```rust
use matric_inference::model_config::requires_raw_mode;

assert!(requires_raw_mode("deepseek-r1:14b"));           // true
assert!(requires_raw_mode("Mistral-Nemo-12B-Thinking")); // true
assert!(!requires_raw_mode("llama3.1:8b"));              // false
```

## Test Coverage

### Model Config Tests (14 tests)

- DeepSeek R1 variants: 5 tests
- Mistral Nemo Thinking variants: 3 tests
- Regular models (should NOT require raw): 4 tests
- Edge cases: 2 tests

### Ollama Integration Tests (2 new tests)

- Serialization with raw mode
- Raw mode configuration logic

All 36 tests pass successfully.

## Example Output

### Without Raw Mode (incorrect)
```
Quantum computing uses quantum bits...
```

### With Raw Mode (correct)
```
<think>
Let me break down quantum computing step by step...
- Quantum bits can be in superposition
- This allows parallel computation
- Need to consider decoherence effects
</think>

Quantum computing uses quantum bits (qubits) that can exist in superposition...
```

## Performance Impact

- Minimal: Only adds a simple string pattern check before each generation
- No network overhead: Decision is made locally
- No additional API calls: Raw mode is just a boolean field in the existing request

## Future Enhancements

Potential improvements for future versions:

1. Add configuration file support for custom model patterns
2. Add environment variable to override detection (e.g., `FORCE_RAW_MODE=true`)
3. Add metadata about thinking type (explicit tags vs verbose reasoning)
4. Support for dynamic model discovery from Ollama API

## References

- Research data: `docs/research/MODEL_INVENTORY.md`
- Test logs: `docs/research/all_models_test_*.log`
- Original ticket: #119

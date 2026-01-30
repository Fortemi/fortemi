# Ollama Model Inventory & Performance Data

**Generated:** 2026-01-18
**Hardware:** RTX 3090/4090 class (24GB VRAM)
**Models Tested:** 35 language models

## Quick Reference

### Best Models by Use Case

| Use Case | Model | Why |
|----------|-------|-----|
| **General Inference** | gpt-oss:20b | 98K ctx, 7.6K output, 179 tok/s |
| **Long Documents** | llama3.1:8b | 98K native context |
| **Fast Queries** | qwen2.5-coder:1.5b | 373 tok/s |
| **Code Generation** | qwen2.5-coder:7b | 4K output, 161 tok/s |
| **Complex Reasoning** | deepseek-r1:14b | Explicit `<think>` tags |
| **Medical Domain** | HuatuoGPT-o1-8B | 8K output, reasoning |
| **Large Output** | HuatuoGPT-o1-8B | 8,192 max output tokens |

---

## Complete Model Data

### Tier 1: Large Context (98K+ tokens)

| Model | Size | Context | Max Output | Speed | Thinking |
|-------|------|---------|------------|-------|----------|
| gpt-oss:20b | 20.9B | 98,376 | 7,611 | 179 tok/s | standard |
| Mistral-Nemo-12B-Thinking | 12.2B | 98,375 | 4,096 | 14 tok/s | **explicit_tags** |
| granite4:3b | 3.4B | 98,339 | 1,277 | 244 tok/s | standard |
| llama3.2:latest | 3.2B | 98,334 | 2,900 | 275 tok/s | standard |
| yi-coder:9b | 8.8B | 98,328 | 492 | 45 tok/s | standard |
| cogito:8b | 8.0B | 98,319 | 2,048 | 28 tok/s | none |
| Qwen3-24B-MoE | 17.9B | 98,319 | N/A | N/A | **verbose_reasoning** |
| HuatuoGPT-o1-8B | 8.0B | 98,319 | 8,192 | 28 tok/s | **verbose_reasoning** |
| Llama-3.1-8B-Freedom | 8.0B | 98,319 | 1,276 | 29 tok/s | standard |
| llama3.1:8b | 8.0B | 98,319 | 1,024 | 29 tok/s | none |
| hermes3:8b | 8.0B | 98,318 | 1,339 | 29 tok/s | standard |
| phi3:mini | 3.8B | 98,318 | 1,024 | 22 tok/s | **verbose_reasoning** |
| granite-code:8b | 8.1B | 98,316 | 281 | 27 tok/s | standard |
| deepseek-r1:14b | 14.8B | 98,312 | 2,824 | 9 tok/s | **explicit_tags** |

### Tier 2: Medium Context (32K-41K tokens)

| Model | Size | Context | Max Output | Speed | Thinking |
|-------|------|---------|------------|-------|----------|
| Qwen3-8B-Jailbroken (i1) | 8.2B | 40,960 | 5,119 | 148 tok/s | standard |
| Qwen3-8B-Jailbroken (Q5) | 8.2B | 40,960 | 4,024 | 133 tok/s | standard |
| qwen3:8b | 8.2B | 40,960 | 4,096 | 144 tok/s | standard |
| codestral:latest | 22.2B | 32,768 | 2,048 | 16 tok/s | standard |
| exaone-deep:7.8b | 7.8B | 32,768 | 3,464 | 165 tok/s | **pattern_based** |
| Qwen2.5-7B-Jailbroken | 7.6B | 32,768 | 1,362 | 161 tok/s | standard |
| mistral:latest | 7.2B | 32,768 | 1,024 | 174 tok/s | none |
| qwen2.5-coder:1.5b | 1.5B | 32,768 | 1,024 | 373 tok/s | standard |
| qwen2.5-coder:14b | 14.8B | 32,768 | 1,003 | 88 tok/s | standard |
| qwen2.5-coder:7b | 7.6B | 32,768 | 4,096 | 161 tok/s | standard |
| qwen2.5:14b | 14.8B | 32,768 | 2,048 | 88 tok/s | standard |
| qwen2.5:32b | 32.8B | 32,768 | 2,048 | 6 tok/s | standard |
| qwen2.5:7b | 7.6B | 32,768 | 1,673 | 162 tok/s | none |

### Tier 3: Standard Context (8K-16K tokens)

| Model | Size | Context | Max Output | Speed | Thinking |
|-------|------|---------|------------|-------|----------|
| deepseek-coder-v2:16b | 15.7B | 16,397 | 1,350 | 242 tok/s | standard |
| codellama:13b | 13B | 16,384 | 1,024 | 30 tok/s | standard |
| starcoder2:7b | 7B | 16,384 | 71 | 167 tok/s | **broken** |
| command-r7b:latest | 8.0B | 8,192 | 2,048 | 134 tok/s | standard |
| gemma2:9b | 9.2B | 8,192 | 434 | 116 tok/s | none |
| Meta-Llama-3-8B-Jailbroken | 8.0B | 8,192 | 874 | 152 tok/s | standard |
| Mirai-Nova-Llama3 | 8.0B | 8,192 | 2 | 17 tok/s | **broken** |
| smollm2:1.7b | 1.7B | 8,192 | 1,219 | 336 tok/s | standard |

---

## Thinking/Reasoning Models

Models with chain-of-thought reasoning capabilities.

### Confirmed Thinking Models (6)

| Model | Type | Detection | Notes |
|-------|------|-----------|-------|
| **deepseek-r1:14b** | explicit_tags | `<think>` tags | Requires `raw: true` mode |
| **Mistral-Nemo-12B-Thinking** | explicit_tags | `<think>` tags | Direct thinking output |
| **HuatuoGPT-o1-8B** | verbose_reasoning | Pattern-based | Medical domain |
| **Qwen3-24B-MoE** | verbose_reasoning | Pattern-based | Large MoE model |
| **phi3:mini** | verbose_reasoning | Step-by-step | Small but capable |
| **exaone-deep:7.8b** | pattern_based | "Step N:" patterns | Fast reasoning |

### Thinking Type Definitions

- **explicit_tags**: Outputs `<think>...</think>` wrapping internal reasoning
- **verbose_reasoning**: Extended step-by-step output without explicit tags
- **pattern_based**: Uses structured patterns like "Step N:", "Let me think"

### Important: deepseek-r1 Configuration

```python
# deepseek-r1 requires raw mode to see thinking tags
resp = requests.post('http://localhost:11434/api/generate', json={
    'model': 'deepseek-r1:14b',
    'prompt': prompt,
    'raw': True,  # Required!
    'options': {'num_ctx': 98312}
})
```

---

## Speed Tiers

### Ultra Fast (>300 tok/s)
- qwen2.5-coder:1.5b: 373 tok/s
- smollm2:1.7b: 336 tok/s

### Fast (150-300 tok/s)
- llama3.2:latest: 275 tok/s
- granite4:3b: 244 tok/s
- deepseek-coder-v2:16b: 242 tok/s
- gpt-oss:20b: 179 tok/s
- mistral:latest: 174 tok/s
- starcoder2:7b: 167 tok/s
- exaone-deep:7.8b: 165 tok/s
- qwen2.5:7b: 162 tok/s
- qwen2.5-coder:7b: 161 tok/s
- Meta-Llama-3-8B-Jailbroken: 152 tok/s

### Slow (<30 tok/s)
- deepseek-r1:14b: 9 tok/s (reasoning overhead)
- Mistral-Nemo-12B-Thinking: 14 tok/s (thinking overhead)
- codestral:latest: 16 tok/s (large model)
- qwen2.5:32b: 6 tok/s (large model)

---

## Broken/Limited Models

| Model | Issue | Recommendation |
|-------|-------|----------------|
| starcoder2:7b | Only 32-71 token output | Avoid for generation |
| Mirai-Nova-Llama3 | Only 2 token output | Do not use |
| granite-code:8b | Only 76-281 token output | Limited use only |

---

## Hardware Limits (24GB VRAM)

- **Safe maximum num_ctx**: 131,072 (128K)
- **Absolute maximum**: 196,608 (192K) for some models
- **OOM threshold**: 262,144 (256K) causes crashes
- **Memory per 8K tokens**: ~50-100MB

---

## Ollama Configuration Guide

### For matric-memory (General Inference)
```toml
model = "gpt-oss:20b"
num_ctx = 131072
num_predict = 4096
```

### For Reasoning Tasks
```toml
model = "deepseek-r1:14b"
num_ctx = 98312
num_predict = 4096
raw = true  # Important!
```

### For Fast Queries
```toml
model = "qwen2.5-coder:1.5b"
num_ctx = 32768
num_predict = 1024
```

### For Code Generation
```toml
model = "qwen2.5-coder:7b"
num_ctx = 32768
num_predict = 4096
```

---

## Data Sources

All raw data files are in `/docs/research/`:

| File | Contents |
|------|----------|
| `consolidated_model_data.json` | Merged data from all tests |
| `smart_context_results_*.json` | Context length test results |
| `thinking_test_results_*.json` | Thinking model detection |
| `context_length_report.md` | Detailed report |
| `all_models_test_*.log` | Raw test output logs |

---

## Testing Methodology

### Context Length Testing
1. Send progressively larger inputs (4K → 8K → 16K → 32K → 64K → 128K → 192K)
2. Check `prompt_eval_count` vs input size
3. When truncation detected, that's the native limit
4. Stop testing that model (practical max found)

### Output Limit Testing
1. Set `num_predict` to increasing values (1K → 2K → 4K → 8K)
2. Measure actual tokens generated vs requested
3. Record speed (tokens/second)

### Thinking Model Detection
1. Send reasoning prompts with `raw: true`
2. Check for `<think>` tags
3. Check for patterns: "Step N:", "Let me think", etc.
4. Classify by detection method

---

*Generated by matric-memory research pipeline*

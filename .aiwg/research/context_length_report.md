# Ollama Model Context Length & Performance Report

**Generated:** 2026-01-18
**Host:** Local development machine (24GB VRAM - RTX 3090/4090 class)
**Models Tested:** 34 language models

## Executive Summary

Comprehensive testing of all 34 language models installed on the local Ollama instance. Testing measured:
- **Native context limit** - actual tokens processed before truncation
- **Maximum output** - longest generation achieved
- **Output speed** - tokens per second generation rate

### Top Performers by Category

| Category | Model | Key Metric |
|----------|-------|------------|
| **Largest Context** | llama3.1:8b, llama3.2 | 131K tokens |
| **Fastest Output** | qwen2.5-coder:1.5b | 373 tok/s |
| **Best Output Length** | HuatuoGPT-o1-8B | 8K tokens |
| **Best All-Around** | gpt-oss:20b | 98K ctx, 7K out, 177 tok/s |
| **Best Reasoning** | deepseek-r1:14b | explicit `<think>` tags |

---

## Complete Model Results

### Tier 1: Large Context (>64K tokens)

| Model | Size | Native Context | Max Output | Speed (tok/s) |
|-------|------|----------------|------------|---------------|
| llama3.1:8b | 8B | 131,072 | 1,024 | 29 |
| llama3.2:latest | 3.2B | 98,334 | 2,900 | 270 |
| gpt-oss:20b | 20.9B | 98,376 | 7,611 | 177 |
| phi3:mini | 3.8B | 98,318 | 1,024 | 21 |
| cogito:8b | 8B | 98,319 | 2,048 | 28 |
| deepseek-r1:14b | 14.8B | 98,312 | 2,824 | 9 |
| hermes3:8b | 8B | 98,318 | 1,339 | 29 |
| granite-code:8b | 8.1B | 98,316 | 281 | 27 |
| granite4:3b | 3.4B | 98,339 | 1,277 | 243 |
| yi-coder:9b | 8.8B | 98,328 | 492 | 45 |
| HuatuoGPT-o1-8B | 8B | 98,319 | 8,192 | 27 |
| Llama-3.1-8B-Freedom | 8B | 98,319 | 1,276 | 29 |
| Mistral-Nemo-12B-Thinking | 12.2B | 98,375 | 4,096 | 13 |
| Qwen3-24B-MoE | 17.9B | 98,319 | N/A | N/A |

### Tier 2: Medium Context (32K-64K tokens)

| Model | Size | Native Context | Max Output | Speed (tok/s) |
|-------|------|----------------|------------|---------------|
| qwen2.5:14b | 14.8B | 32,768 | 2,048 | 88 |
| qwen2.5:32b | 32.8B | 32,768 | 2,048 | 6.5 |
| qwen2.5:7b | 7.6B | 32,768 | 1,673 | 160 |
| qwen2.5-coder:14b | 14.8B | 32,768 | 1,003 | 88 |
| qwen2.5-coder:7b | 7.6B | 32,768 | 4,096 | 160 |
| qwen2.5-coder:1.5b | 1.5B | 32,768 | 1,024 | 373 |
| codestral:latest | 22.2B | 32,768 | 2,048 | 16 |
| exaone-deep:7.8b | 7.8B | 32,768 | 3,464 | 163 |
| mistral:latest | 7.2B | 32,768 | 1,024 | 174 |
| Qwen2.5-7B-Jailbroken | 7.6B | 32,768 | 1,362 | 161 |
| qwen3:8b | 8.2B | 40,960 | 4,096 | 141 |
| Qwen3-8B-Jailbroken | 8.2B | 40,960 | 5,119 | 143 |

### Tier 3: Standard Context (8K-16K tokens)

| Model | Size | Native Context | Max Output | Speed (tok/s) |
|-------|------|----------------|------------|---------------|
| codellama:13b | 13B | 16,384 | 1,024 | 30 |
| deepseek-coder-v2:16b | 15.7B | 16,397 | 1,350 | 240 |
| starcoder2:7b | 7B | 16,384 | 71 | 150 |
| gemma2:9b | 9.2B | 8,192 | 434 | 116 |
| command-r7b:latest | 8B | 8,192 | 2,048 | 132 |
| smollm2:1.7b | 1.7B | 8,192 | 1,219 | 336 |
| Meta-Llama-3-8B-Jailbroken | 8B | 8,192 | 874 | 152 |
| Mirai-Nova-Llama3 | 8B | 8,192 | 2 | 17 |

### Tier 4: Small Context (<8K tokens)

| Model | Size | Native Context | Max Output | Speed (tok/s) |
|-------|------|----------------|------------|---------------|
| nemotron-mini:4b | 4.2B | 4,096 | 34 | 141 |

---

## gpt-oss:20b - Primary Model Configuration

As the primary inference model for matric-memory:

### Optimal Settings
```toml
[ollama]
model = "gpt-oss:20b"
num_ctx = 131072      # 128K - safe maximum
num_predict = 4096    # Practical output limit
```

### Performance Profile
| Metric | Value |
|--------|-------|
| Native Context | 98,376 tokens |
| Hardware Limit | 196,608 num_ctx (192K) |
| Maximum Output | ~7,600 tokens |
| Output Speed | 177 tok/s |
| Input Processing | ~4,500 tok/s |

---

## Speed Tiers

### Fastest Models (>200 tok/s)
1. **qwen2.5-coder:1.5b** - 373 tok/s
2. **smollm2:1.7b** - 336 tok/s
3. **llama3.2:latest** - 270 tok/s
4. **deepseek-coder-v2:16b** - 240 tok/s
5. **granite4:3b** - 243 tok/s

### Fast Models (100-200 tok/s)
- mistral:latest (174 tok/s)
- gpt-oss:20b (177 tok/s)
- exaone-deep:7.8b (163 tok/s)
- qwen2.5-coder:7b (160 tok/s)
- qwen2.5:7b (160 tok/s)
- Meta-Llama-3-8B-Jailbroken (152 tok/s)
- starcoder2:7b (150 tok/s)
- qwen3:8b (141 tok/s)
- command-r7b (132 tok/s)
- gemma2:9b (116 tok/s)

### Slow Models (<30 tok/s)
- deepseek-r1:14b (9 tok/s) - reasoning model
- Mistral-Nemo-12B-Thinking (13 tok/s) - thinking model
- qwen2.5:32b (6.5 tok/s) - large model

---

## Recommendations by Use Case

### Long Document Processing
```
Model: llama3.1:8b or llama3.2:latest
num_ctx: 262144
Reason: 131K native context, fast processing
```

### Code Generation
```
Model: qwen2.5-coder:7b
num_ctx: 32768
Reason: Good output length (4K), fast (160 tok/s)
```

### Quick Queries
```
Model: qwen2.5-coder:1.5b or smollm2:1.7b
num_ctx: 8192
Reason: Fastest models (300+ tok/s)
```

### Reasoning Tasks
```
Model: deepseek-r1:14b (best) or exaone-deep:7.8b (faster)
num_ctx: 131072
raw: true  # Required for deepseek-r1 to show <think> tags
Reason: Explicit thinking models with chain-of-thought reasoning
```

### Medical/Healthcare Reasoning
```
Model: HuatuoGPT-o1-8B
num_ctx: 131072
Reason: Medical-domain thinking model, large output (8K tokens)
```

### General Inference (matric-memory)
```
Model: gpt-oss:20b
num_ctx: 131072
num_predict: 4096
Reason: Best balance of context, output, and speed
```

---

## Hardware Notes

- **GPU:** 24GB VRAM
- **Context scaling:** ~50-100MB per 8K tokens
- **Failure point:** 256K num_ctx exhausts VRAM for most models
- **Cold start:** First inference 3-10x slower
- **Prompt caching:** Subsequent calls faster with same prefix

---

## Broken/Limited Models

| Model | Issue |
|-------|-------|
| nemotron-mini:4b | Only 34 token output |
| starcoder2:7b | Only 32-71 token output |
| granite-code:8b | Only 76-281 token output |
| Mirai-Nova-Llama3 | Only 2 token output (broken) |

---

## Thinking/Reasoning Model Detection

Testing identified models with chain-of-thought reasoning capabilities.

### Confirmed Thinking Models (6)

| Model | Thinking Type | Detection Method |
|-------|---------------|------------------|
| deepseek-r1:14b | explicit_tags | Uses `<think>` tags in raw mode |
| HuatuoGPT-o1-8B | verbose_reasoning | Extended reasoning patterns |
| Mistral-Nemo-12B-Thinking | explicit_tags | Uses `<think>` tags |
| Qwen3-24B-MoE | verbose_reasoning | Extended reasoning patterns |
| phi3:mini | verbose_reasoning | Step-by-step pattern |
| exaone-deep:7.8b | pattern_based | Reasoning patterns (Step N:) |

### Thinking Type Definitions

- **explicit_tags**: Model outputs `<think>...</think>` or similar tags wrapping internal reasoning
- **verbose_reasoning**: Model produces extended step-by-step output without explicit tags
- **pattern_based**: Model uses reasoning patterns like "Step N:", "Let me think", etc.

### Standard Models (No Thinking Detection)

- cogito:8b
- llama3.1:8b
- qwen2.5:7b
- mistral:latest
- gemma2:9b

### Notes

- **deepseek-r1** requires `raw: true` mode to expose `<think>` tags - Ollama strips them in standard mode
- Thinking models typically have slower output speed due to reasoning overhead
- Best for complex reasoning tasks, logic puzzles, math problems

---

## Raw Data Files

| File | Description |
|------|-------------|
| `MODEL_INVENTORY.md` | **Comprehensive model reference document** |
| `consolidated_model_data.json` | **Merged data from all tests (35 models)** |
| `smart_context_results_20260118_*.json` | Context length test results |
| `thinking_test_results_20260118_*.json` | Thinking model detection results |
| `batch_context_results_20260118_*.json` | Batch test data |
| `all_models_test_*.log` | Raw test output logs |

## Test Scripts

| Script | Purpose |
|--------|---------|
| `scripts/smart_context_test.py` | Context & output limit testing |
| `scripts/thinking_test.py` | Thinking model detection |
| `scripts/batch_context_test.py` | Batch testing multiple models |

# Inference Runtime Research: qwen3.5 on RTX 4090 (24GB)

**Date:** 2026-04-01
**Context:** AI revision fails on long videos due to VRAM starvation (#575). Research into optimal model configuration and alternative runtimes.

## Ollama 0.19.0 Errata (Confirmed on RTX 4090)

### 1. VRAM-Based Context Auto-Calculation

Ollama 0.19.0 **auto-computes** a safe default context from total VRAM:
```
vram-based default context  total_vram="24.0 GiB" default_num_ctx=32768
```

The 32K context is NOT a Modelfile default — it's Ollama's VRAM-aware safety calculation. Override via `OLLAMA_CONTEXT_LENGTH` env var (new in 0.19+).

### 2. Graceful Layer Offloading on OOM

When requested context exceeds GPU VRAM, Ollama automatically reduces GPU layers and offloads to CPU:
```
load: KvSize:65536 GPULayers:47  → cudaMalloc failed: out of memory
load: KvSize:65536 GPULayers:42  → succeeded (5 layers offloaded to CPU)
  model weights: 8.8 GiB (GPU) + ~4 GiB (CPU)
  kv cache: 3.8 GiB (GPU, q8_0)
  compute graph: 1.5 GiB
```

This is good behavior — it means passing `num_ctx` from the model profile won't crash Ollama, it'll just offload layers. But CPU-offloaded layers are ~10x slower, so chunk sizing should still match realistic VRAM context.

### 3. `/api/ps` Reports Modelfile Context, Not Actual

`/api/ps` `context_length` field shows the Modelfile's `num_ctx` parameter, not the per-request override or actual KV allocation. This value cannot be trusted for VRAM planning.

### 4. Model Reload After Expiry Can Fail

If a model expires (default `OLLAMA_KEEP_ALIVE=5m`), the reload attempt may fail if VRAM conditions changed (e.g., sidecars allocated memory in the gap). This causes `exit status 2` crashes.

### 5. `OLLAMA_CONTEXT_LENGTH` Global Override (0.19+)

New env var in 0.19.0. Set to a non-zero value to override the VRAM-based default for all models globally. Takes precedence over auto-calculation but is overridden by per-request `num_ctx` in API options.

**Immediate fix:** Pass `num_ctx` in every `/api/chat` request. Set `OLLAMA_CONTEXT_LENGTH` for system-wide control.

## qwen3.5:9b — VRAM Budget on 24GB

Architecture: Dense transformer, 36 layers, 4096 hidden dim, GQA with 32 Q-heads / 8 KV-heads, head_dim=128.
RoPE base: 1,000,000. YaRN scaling for 256K context. Trained to 128K, extended to 256K at inference.

### KV Cache Per Token

```
2 (K+V) × 8 (KV heads) × 128 (head dim) × 36 (layers) × 2 bytes (f16) = 144 KB/token
With q8_0 KV: 72 KB/token
With q4_0 KV: 36 KB/token
```

### Maximum Context by Quantization (24GB, ~23GB usable)

| Quant | Weights | + q8_0 KV @ 128K | Total | Free VRAM |
|-------|---------|------------------|-------|-----------|
| **Q5_K_M** | **~5.8 GB** | **~9.4 GB** | **~15.2 GB** | **~8.8 GB** |
| Q4_K_M | ~5.2 GB | ~9.4 GB | ~14.6 GB | ~9.4 GB |
| Q8_0 | ~9.4 GB | ~9.4 GB | ~18.8 GB | ~5.2 GB |

**Recommended:** Q5_K_M + q8_0 KV cache + flash attention. Sets `num_ctx=131072` (128K).
Leaves 8.8 GB free — enough for nomic-embed-text to stay resident.

### Maximum Context by Quantization (with q4_0 KV, requires llama.cpp)

| Quant | Weights | + q4_0 KV @ 256K | Total | Free VRAM |
|-------|---------|------------------|-------|-----------|
| Q5_K_M | ~5.8 GB | ~9.4 GB | ~15.2 GB | ~8.8 GB |
| Q4_K_M | ~5.2 GB | ~9.4 GB | ~14.6 GB | ~9.4 GB |

With q4_0 KV, full 256K context fits comfortably in under 15 GB.

## qwen3.5:27b — VRAM Budget on 24GB

| Quant | Weights | + q8_0 KV @ 64K | Total | Free | Max ctx (q8_0) |
|-------|---------|-----------------|-------|------|----------------|
| Q4_K_M | ~15 GB | ~4.7 GB | ~19.7 GB | 4.3 GB | ~111K theoretical, 65K safe |
| Q5_K_M | ~17 GB | ~4.7 GB | ~21.7 GB | 2.3 GB | ~83K theoretical, marginal |
| Q6_K | ~20 GB | ~2.2 GB | ~22.2 GB | 1.8 GB | ~42K |

**27b verdict:** Q4_K_M with `num_ctx=65536` is the practical ceiling. The 9b is strictly superior for long-context tasks on 24GB.

## 9b vs 27b for Document Revision

| Attribute | qwen3.5:9b Q5_K_M | qwen3.5:27b Q4_K_M |
|-----------|-------------------|-------------------|
| Weight VRAM | ~5.8 GB | ~15 GB |
| Recommended num_ctx | 131072 | 65536 |
| Total VRAM at ctx | ~15.2 GB | ~19.7 GB |
| Free VRAM | ~8.8 GB | ~4.3 GB |
| Generation speed | ~120 tok/s | ~40 tok/s |
| Relative quality | 97–98% of 27b | Baseline |
| Concurrent embed model? | Yes (easily) | Tight |

For Fortemi's revision pipeline: **use 9b as the primary revision model**. Reserve 27b for explicit "thorough revision" requests or batch reprocessing.

## Immediate Ollama Configuration

### Environment Variables (systemd override)

```bash
# Already set on this host:
OLLAMA_FLASH_ATTENTION=1       # Required for long context — prevents O(N²) memory
OLLAMA_KV_CACHE_TYPE=q8_0      # Halves KV cache memory

# Recommended additions:
OLLAMA_NUM_BATCH=2048          # Faster prefill on RTX 4090 (1 TB/s bandwidth)
OLLAMA_CONTEXT_LENGTH=65536    # Global context override (0.19+). Set based on model:
                                #   65536 for 27b on 24GB (with sidecars stopped)
                                #   131072 for 9b on 24GB
                                #   0 (default) lets Ollama auto-calculate from VRAM
```

### WARNING: Do NOT Send num_ctx Per-Request

Changing `num_ctx` between requests triggers a **full model reload** (~60-90 seconds). This is catastrophic for multi-chunk revision. Set context globally via `OLLAMA_CONTEXT_LENGTH` or custom Modelfiles.

`matric-inference` sends only `num_predict` per-request. Chunk sizing queries `/api/ps` for the actual allocated context.

### qwen3.5:9b Is a Hybrid SSM/Transformer

`full_attention_interval: 4` — only 1 in 4 layers is full attention (rest are SSM/Mamba). KV cache only applies to attention layers (~8 of 32), so actual KV memory is ~4x smaller than pure-transformer estimates:

```
Observed on RTX 4090 with q8_0 KV cache:
  num_ctx=32768:   8.85 GB total (2.65 GB KV cache)
  num_ctx=262144: 16.05 GB total (9.85 GB KV cache)
  Delta for 8x tokens: only 3.7x memory (SSM layers don't scale with context)
```

This means the VRAM budget tables above are **conservative** — actual KV cache is much smaller than the pure-transformer formula suggests.

## llama.cpp Advantages (Longer Term)

| Feature | Ollama | llama.cpp server |
|---------|--------|-----------------|
| KV cache q4_0 | Supported but less tested | First-class, well-tested |
| Flash attention | `OLLAMA_FLASH_ATTENTION=1` | `--flash-attn` |
| Context reporting | Silent capping | Accurate reporting |
| GBNF grammar | `format: "json"` (best-effort) | Guaranteed valid output |
| Speculative decoding | Not supported | `--model-draft` |
| Prompt caching | Implicit with FA | `--slot-save-path` |
| Partial CPU offload | Via `num_gpu` layers | `--n-gpu-layers` |

**Migration cost is low:** llama-server is OpenAI-compatible at `/v1/chat/completions`. The diff is a base URL change and SSE streaming format (vs Ollama's NDJSON).

**Don't use vLLM or SGLang** — they require HuggingFace weights (not GGUF), have no CPU fallback, and their advantages (PagedAttention, RadixAttention) don't apply to single-user sequential pipelines.

## Recommended Rollout

1. **Today (no code):** Set `OLLAMA_FLASH_ATTENTION=1` + `OLLAMA_KV_CACHE_TYPE=q8_0` + `OLLAMA_NUM_BATCH=2048`
2. **#575 fix:** Pass `num_ctx` in API requests, query actual context from `/api/ps`
3. **#576:** Sidecar lifecycle management — free 6.6 GB during generation
4. **Model switch:** Move primary revision from qwen3.5:27b to qwen3.5:9b with 128K context
5. **#577:** llama.cpp backend for q4_0 KV cache, speculative decoding, and full 256K context

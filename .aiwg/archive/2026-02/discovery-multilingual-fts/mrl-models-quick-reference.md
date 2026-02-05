# MRL Models Quick Reference

## Recommended for Seed Data

### 1. jina-embeddings-v3 (BEST MULTILINGUAL)
```
Model: jinaai/jina-embeddings-v3
Dims: 1024 native, MRL: [1024,512,256,128,64,32]
Languages: 89+ (best multilingual coverage)
License: CC-BY-NC-4.0 (non-commercial)
Ollama: NO (custom import needed)
Default Truncate: 512
```

### 2. gte-Qwen2-1.5B-instruct (COMMERCIAL-FRIENDLY)
```
Model: Alibaba-NLP/gte-Qwen2-1.5B-instruct
Dims: 1536 native, MRL: [1536,1024,512,256,128]
Languages: Multilingual
License: Apache 2.0
Ollama: NO (custom import)
Default Truncate: 512
```

### 3. all-MiniLM-L6-v2 (FAST + LLM RE-RANKING)
```
Model: sentence-transformers/all-MiniLM-L6-v2
Dims: 384 (no MRL - already small)
Languages: English
License: Apache 2.0
Ollama: YES
Why: Outperforms larger models with LLM re-ranking (REF-068)
```

### 4. stella_en_1.5B_v5 (OPTIONAL - ENGLISH)
```
Model: NovaSearch/stella_en_1.5B_v5
Dims: 1024 native, MRL: [1024,512,256,128,64]
Languages: English only
License: VERIFY FIRST
Ollama: NO
Default Truncate: 256
```

## Currently in Seed Data (Keep)
- nomic-embed-text-v1.5 (768-dim MRL) - DEFAULT
- mxbai-embed-large-v1 (1024-dim MRL)
- bge-large-en-v1.5 (1024-dim, no MRL)
- multilingual-e5-large (1024-dim, no MRL)

## Code Models (Non-MRL)
- jina-embeddings-v2-base-code (768-dim, 30+ languages)

## DO NOT ADD
- voyage-code-* (API-only, not self-hosted)
- codestral-embed (does not exist)
- snowflake-arctic-embed (MRL unconfirmed)

## Migration SQL Template
```sql
INSERT INTO embedding_config (
    id, name, description, model, dimension,
    chunk_size, chunk_overlap,
    supports_mrl, matryoshka_dims, default_truncate_dim,
    is_default
) VALUES (
    gen_uuid_v7(),
    'MODEL_NAME',
    'DESCRIPTION',
    'HF_MODEL_PATH',
    NATIVE_DIM,
    CHUNK_SIZE,
    OVERLAP,
    TRUE,  -- supports_mrl
    ARRAY[DIM1, DIM2, ...],  -- matryoshka_dims
    DEFAULT_TRUNCATE,
    FALSE  -- is_default
) ON CONFLICT (name) DO UPDATE SET
    supports_mrl = EXCLUDED.supports_mrl,
    matryoshka_dims = EXCLUDED.matryoshka_dims;
```

## Quality Comparison (MTEB NDCG@10)
```
gte-Qwen2-7B:      69.7  (too large for default)
stella-1.5B:       65.3
jina-v3:           65+   (multilingual average)
snowflake-arctic:  59.1  (MRL unclear)
nomic-v1.5:        48.0  (CURRENT)
all-MiniLM:        48.0  (but faster + LLM re-rank)
```

## Storage per 1M Docs
```
@ Full Dim  @ 512   @ 256   @ 128   @ 64
jina-v3:    4GB     2GB     1GB     0.5GB   0.25GB
gte-Qwen2:  6GB     2GB     1GB     0.5GB   -
stella:     4GB     2GB     1GB     0.5GB   0.25GB
nomic:      3GB     2GB     1GB     0.5GB   0.25GB
MiniLM:     1.5GB   N/A     N/A     N/A     N/A
```

## Use Case Mapping
```
General multilingual → jina-v3
Commercial multilingual → gte-Qwen2-1.5B
RAG with Claude/GPT → all-MiniLM-L6-v2
English high-quality → stella-1.5B-v5
Code search → jina-code-v2 (no MRL)
Maximum MRL range → jina-v3 (down to 32-dim)
```

## Testing Checklist
- [ ] Verify gte-Qwen2 matryoshka_dims array
- [ ] Confirm stella license
- [ ] Test jina-v3 Ollama Modelfile import
- [ ] Benchmark retrieval quality at each MRL dimension
- [ ] Measure actual inference speed
- [ ] Update embedding-model-selection.md docs

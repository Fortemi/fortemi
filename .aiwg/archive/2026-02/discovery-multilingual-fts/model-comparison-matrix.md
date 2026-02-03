# Embedding Model Comparison Matrix

## Complete Feature Matrix

| Feature | jina-v3 | gte-Qwen2-1.5B | stella-1.5B | all-MiniLM | jina-code-v2 | nomic-v1.5* | mxbai-large* |
|---------|---------|----------------|-------------|------------|--------------|-------------|--------------|
| **Native Dims** | 1024 | 1536 | 1024 | 384 | 768 | 768 | 1024 |
| **MRL Support** | YES | YES | YES | NO | NO | YES | YES |
| **MRL Range** | 32-1024 | 128-1536 | 64-1024 | N/A | N/A | 64-768 | 64-1024 |
| **Default Truncate** | 512 | 512 | 256 | N/A | N/A | 256 | 256 |
| **Max Compression** | 32× | 12× | 16× | 1× | 1× | 12× | 16× |
| **Languages** | 89+ | Multi | EN | EN | Code | EN | EN |
| **Context Window** | 8192 | 8192 | 8192 | 512 | 8192 | 8192 | 512 |
| **Parameters** | 570M | 1.5B | 1.5B | 22M | 161M | 137M | 335M |
| **License** | CC-BY-NC-4.0 | Apache 2.0 | TBD | Apache 2.0 | Apache 2.0 | Apache 2.0 | Apache 2.0 |
| **Ollama Native** | NO | NO | NO | YES | NO | YES | YES |
| **GPU Memory** | ~2GB | ~3GB | ~3GB | ~500MB | ~1GB | ~1GB | ~2GB |
| **Relative Speed** | 0.5× | 0.3× | 0.3× | 2.4× | 0.8× | 1.0× | 0.5× |

*Already in seed data

---

## MTEB Performance Matrix

| Task Type | jina-v3 | gte-Qwen2 | stella | all-MiniLM | nomic-v1.5 | mxbai |
|-----------|---------|-----------|--------|------------|------------|-------|
| **Retrieval (NDCG@10)** | 65+ | 70+ | 65.3 | 48 | 48 | ~50 |
| **Classification (Acc)** | 86+ | 87+ | 89.8 | 84 | 84 | ~85 |
| **STS (Pearson)** | 87+ | 83+ | 85.8 | 86.7 | 86.7 | ~87 |
| **Multilingual** | Excellent | Good | N/A | N/A | N/A | N/A |
| **Overall Rank** | 2nd | 1st | 2nd | 4th | 4th | 3rd |

---

## Use Case Suitability Matrix

| Use Case | Best Choice | Alternative | Avoid |
|----------|-------------|-------------|-------|
| **Multilingual knowledge base** | jina-v3 | gte-Qwen2 | all-MiniLM, stella |
| **Commercial multilingual** | gte-Qwen2 | jina-v3* | nomic-v1.5 |
| **RAG with LLM re-ranking** | all-MiniLM | nomic-v1.5 | stella, gte-Qwen2 |
| **High-quality English** | stella | mxbai | all-MiniLM |
| **Code search** | jina-code-v2 | nomic-v1.5** | all-MiniLM |
| **Max storage efficiency** | jina-v3 @ 32 | stella @ 64 | all-MiniLM |
| **Fast embedding** | all-MiniLM | nomic-v1.5 | gte-Qwen2, stella |
| **Constrained GPU** | all-MiniLM | nomic-v1.5 | gte-Qwen2, stella |
| **Self-hosted only (Ollama)** | nomic-v1.5 | mxbai | jina-v3, gte-Qwen2 |

*If non-commercial use
**Fine-tune on code corpus

---

## Storage Requirements Matrix

**Per 1 Million Documents** (500 tokens avg)

| Model | Full | 512-dim | 256-dim | 128-dim | 64-dim | 32-dim |
|-------|------|---------|---------|---------|--------|--------|
| **jina-v3** | 4.0 GB | 2.0 GB | 1.0 GB | 0.5 GB | 0.25 GB | 0.125 GB |
| **gte-Qwen2** | 6.0 GB | 2.0 GB | 1.0 GB | 0.5 GB | - | - |
| **stella** | 4.0 GB | 2.0 GB | 1.0 GB | 0.5 GB | 0.25 GB | - |
| **all-MiniLM** | 1.5 GB | - | - | - | - | - |
| **jina-code** | 3.0 GB | - | - | - | - | - |
| **nomic-v1.5** | 3.0 GB | 2.0 GB | 1.0 GB | 0.5 GB | 0.25 GB | - |
| **mxbai** | 4.0 GB | 2.0 GB | 1.0 GB | 0.5 GB | 0.25 GB | - |

---

## License Compatibility Matrix

| Model | License | Commercial Use | Attribution | Share-Alike | Non-Commercial Only |
|-------|---------|----------------|-------------|-------------|---------------------|
| **jina-v3** | CC-BY-NC-4.0 | NO | YES | NO | YES |
| **gte-Qwen2** | Apache 2.0 | YES | NO | NO | NO |
| **stella** | TBD | VERIFY | VERIFY | VERIFY | VERIFY |
| **all-MiniLM** | Apache 2.0 | YES | NO | NO | NO |
| **jina-code** | Apache 2.0 | YES | NO | NO | NO |
| **nomic-v1.5** | Apache 2.0 | YES | NO | NO | NO |
| **mxbai** | Apache 2.0 | YES | NO | NO | NO |

**Commercial Use Safe:** gte-Qwen2, all-MiniLM, jina-code, nomic-v1.5, mxbai
**Non-Commercial Only:** jina-v3
**Needs Verification:** stella

---

## Quality vs Efficiency Trade-off Matrix

### Retrieval Quality vs Storage (1M docs)

```
High Quality (NDCG@10 > 65)
  │
  │  gte-Qwen2 @ full (6GB)
  │  ▲
  │  │ stella @ full (4GB)
  │  │ ▲
  │  │ │ jina-v3 @ full (4GB)
  │  │ │ ▲
  │  │ │ │
  │  │ │ │ gte-Qwen2 @ 512 (2GB)
  │  │ │ │ ▲
  │  │ │ │ │ jina-v3 @ 512 (2GB)
  │  │ │ │ │ ▲
  │  │ │ │ │ │ stella @ 256 (1GB)
  │  │ │ │ │ │ ▲
  │────────────────────────────────────
  │  │ │ │ │ │ │ nomic @ full (3GB)
  │  │ │ │ │ │ │ ▲
  │  │ │ │ │ │ │ │ mxbai @ 256 (1GB)
  │  │ │ │ │ │ │ │ ▲
  │  │ │ │ │ │ │ │ │ jina-v3 @ 128 (0.5GB)
  │  │ │ │ │ │ │ │ │ ▲
  │  │ │ │ │ │ │ │ │ │ all-MiniLM (1.5GB)*
  │  │ │ │ │ │ │ │ │ │ ▲
  │  │ │ │ │ │ │ │ │ │ │ jina-v3 @ 64 (0.25GB)
  │  │ │ │ │ │ │ │ │ │ │ ▲
  │  │ │ │ │ │ │ │ │ │ │ │ jina-v3 @ 32 (0.125GB)
Low Quality (NDCG@10 < 50)           │ │
  └──────────────────────────────────┴─┴─────►
  Low Storage                    High Storage

* all-MiniLM performs better with LLM re-ranking (REF-068)
```

---

## Speed vs Quality Matrix

### Embedding Generation Speed vs Retrieval Quality

```
Fast (2.4× baseline)
  │
  │  all-MiniLM (NDCG: 48)
  │  ▲
  │  │                            *with LLM re-ranking → 60+
  │  │
  │────────────────────────────────────
  │  │ nomic-v1.5 (NDCG: 48) [BASELINE]
  │  │ ▲
  │  │ │ jina-v3 (NDCG: 65+)
  │  │ │ ▲
  │  │ │ │ mxbai (NDCG: ~50)
  │  │ │ │ ▲
  │  │ │ │ │
  │────────────────────────────────────
  │  │ │ │ │ stella (NDCG: 65.3)
  │  │ │ │ │ ▲
  │  │ │ │ │ │ gte-Qwen2 (NDCG: 70+)
Slow (0.3× baseline)  │ │ ▲
  └─────────────────────┴─┴─┴─────────►
  Low Quality            High Quality
```

---

## Decision Matrix

### Step 1: Identify Requirements

| Requirement | Recommended Model(s) |
|-------------|---------------------|
| Must support 50+ languages | jina-v3, gte-Qwen2 |
| Commercial deployment | gte-Qwen2, all-MiniLM, nomic-v1.5, mxbai |
| Research/academic use | jina-v3, stella |
| Need Ollama native support | nomic-v1.5, mxbai, all-MiniLM |
| Custom Ollama import OK | jina-v3, gte-Qwen2, stella |
| Large corpus (>10M docs) | jina-v3 @ 64, stella @ 128 |
| Limited GPU (<2GB) | all-MiniLM, nomic-v1.5 |
| Code search primary | jina-code-v2 |
| Using LLM re-ranking | all-MiniLM |
| Maximum quality | gte-Qwen2 @ full |
| Maximum efficiency | jina-v3 @ 32 |

### Step 2: Apply Constraints

```
START
  │
  ├─ Commercial use? ─NO→ [jina-v3, stella]
  │                   │
  │                  YES
  │                   │
  ├─ Multilingual? ──YES→ [gte-Qwen2]
  │                   │
  │                  NO
  │                   │
  ├─ LLM re-rank? ───YES→ [all-MiniLM]
  │                   │
  │                  NO
  │                   │
  ├─ GPU < 2GB? ─────YES→ [nomic-v1.5, all-MiniLM]
  │                   │
  │                  NO
  │                   │
  ├─ Corpus > 5M? ───YES→ [Enable MRL truncation]
  │                   │
  │                  NO
  │                   │
  └─ Default ───────────→ [nomic-v1.5, mxbai]
```

---

## MRL Dimension Selection Matrix

| Corpus Size | Recommended Dimension | Expected Quality | Storage Savings | Best Models |
|-------------|-----------------------|------------------|-----------------|-------------|
| **< 100K docs** | Full (768-1536) | 100% | 1× | any |
| **100K - 1M docs** | 512 | 98-99% | 2× | jina-v3, gte-Qwen2, stella |
| **1M - 5M docs** | 256 | 95-97% | 4× | jina-v3, stella, nomic |
| **5M - 20M docs** | 128 | 92-95% | 8× | jina-v3, stella, mxbai |
| **> 20M docs** | 64 | 88-92% | 16× | jina-v3, stella |
| **Experimental** | 32 | 80-85% | 32× | jina-v3 only |

---

## Ollama Integration Matrix

| Model | Ollama Native | Custom Import Difficulty | Import Method |
|-------|---------------|-------------------------|---------------|
| nomic-v1.5 | YES | N/A | `ollama pull nomic-embed-text` |
| mxbai | YES | N/A | `ollama pull mxbai-embed-large` |
| all-MiniLM | YES | N/A | `ollama pull all-minilm` |
| jina-v3 | NO | Medium | Modelfile from HF |
| gte-Qwen2 | NO | Medium | Modelfile from HF |
| stella | NO | Medium | Modelfile from HF |
| jina-code | NO | Medium | Modelfile from HF |

**Import Process (Custom):**
```bash
# 1. Download model from HuggingFace
git clone https://huggingface.co/jinaai/jina-embeddings-v3

# 2. Create Modelfile
cat > Modelfile.jina-v3 <<EOF
FROM ./jina-embeddings-v3
TEMPLATE "[INST] {{ .Prompt }} [/INST]"
EOF

# 3. Import to Ollama
ollama create jina-v3 -f Modelfile.jina-v3

# 4. Test
ollama run jina-v3 "embed: test query"
```

---

## Priority Recommendation Matrix

| Priority | Model | Why Add | Blocks |
|----------|-------|---------|--------|
| **HIGH** | jina-v3 | Best multilingual MRL, widest truncation range | License review |
| **HIGH** | gte-Qwen2-1.5B | Commercial-friendly multilingual MRL | Verify matryoshka_dims |
| **HIGH** | all-MiniLM-L6-v2 | Per REF-068, beats larger models with LLM re-rank | None |
| **MEDIUM** | stella-1.5B-v5 | Excellent English quality | License verification |
| **LOW** | jina-code-v2 | Only code model found, but no MRL | None |
| **HOLD** | snowflake-arctic | MRL status unclear | Verify MRL support |

---

## Final Recommendation Summary

### Add to Seed Data (Recommended)

1. **jina-embeddings-v3** (pending license approval for intended use)
2. **gte-Qwen2-1.5B-instruct** (commercial-safe multilingual MRL)
3. **all-MiniLM-L6-v2** (fast + LLM re-ranking optimized)

### Optional Additions

4. **stella_en_1.5B_v5** (if license allows)
5. **jina-embeddings-v2-base-code** (code search)

### Keep Current Defaults

- **nomic-embed-text-v1.5** (default, best Ollama integration)
- **mxbai-embed-large-v1** (high-quality MRL alternative)
- **bge-large-en-v1.5** (non-MRL baseline)
- **multilingual-e5-large** (non-MRL multilingual)

---

**Matrix Last Updated:** 2026-02-01
**Source:** mrl-embedding-models-research.md

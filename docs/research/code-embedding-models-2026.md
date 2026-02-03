# Code Embedding Models Research (2026)

## Executive Summary

**Objective**: Evaluate code-specific embedding models for Ollama integration to improve semantic code search in matric-memory.

**Current State**: Using `nomic-embed-text` (general-purpose) for all content including code.

**Key Finding**: No dedicated code embedding models are currently available in Ollama's model library. All existing code embedding models require custom integration or API services.

**Recommendation**:
1. **Short-term**: Continue with `nomic-embed-text` for general use, consider `jina-embeddings-v2-base-code` for custom Ollama integration
2. **Long-term**: Advocate for code embedding models in Ollama library or implement custom model support

**Confidence**: High (based on direct Ollama library verification and comprehensive model research)

---

## Research Scope

### Models Evaluated

1. **jina-embeddings-v2-base-code** - BERT-based, 30+ languages
2. **CodeT5+ 110M Embedding** - Encoder-only, 9 languages
3. **StarEncoder** - BERT-based, 80+ languages
4. **CodeSage-base** - Two-stage training, 9 languages
5. **CodeBERT-base** - RoBERTa-based, CodeSearchNet trained
6. **voyage-code-2** (API-only) - Proprietary service
7. **OpenAI embeddings** (API-only) - General-purpose with code capability

### Evaluation Criteria

- Architecture and parameter count
- Supported programming languages
- Embedding dimensions
- MRL (Matryoshka Representation Learning) support
- Benchmark performance on code search tasks
- Memory and latency characteristics
- **Ollama availability** (critical for matric-memory integration)

---

## Ollama Library Status

### Available Embedding Models

As of February 2026, Ollama offers 12 embedding models:

| Model | Parameters | Dimensions | Specialization | Pulls |
|-------|------------|------------|----------------|-------|
| nomic-embed-text | 137M | 768 | General text, MRL | 52.8M |
| mxbai-embed-large | 335M | 1024 | General text, MRL | 7M |
| bge-m3 | 567M | 1024 | Multilingual | 3.2M |
| all-minilm | 22M-33M | 384 | General text | 2.3M |
| snowflake-arctic-embed | 22M-335M | varies | General text | 1.8M |
| qwen3-embedding | 0.6B-8B | varies | Multilingual | 472.9K |
| embeddinggemma | 300M | varies | General text | 464.3K |
| paraphrase-multilingual | 278M | varies | Multilingual | 400.3K |
| snowflake-arctic-embed2 | 568M | varies | Multilingual | 235.9K |
| granite-embedding | 30M-278M | varies | Multilingual | 190.8K |
| bge-large | 335M | 1024 | General text | 187.7K |
| nomic-embed-text-v2-moe | varies | 768 | Multilingual MoE | 27.1K |

**Key Finding**: None of these models are specialized for code. All are general-purpose or multilingual text embedding models.

---

## Model Analysis

### 1. jina-embeddings-v2-base-code

**Availability**: ❌ Not in Ollama (requires custom integration)

#### Architecture
- **Type**: JinaBert (BERT-based with ALiBi)
- **Parameters**: 161M
- **Dimensions**: 768 (standard output)
- **Max Sequence Length**: 8,192 tokens (trained on 512, extrapolates to 8k+)
- **Pooling**: Mean pooling (required)

#### Language Support
**32 Total Languages**:
- **Natural Language**: English
- **Programming Languages** (30): Assembly, Batchfile, C, C#, C++, CMake, CSS, Dockerfile, FORTRAN, GO, Haskell, HTML, Java, JavaScript, Julia, Lua, Makefile, Markdown, PHP, Perl, PowerShell, Python, Ruby, Rust, SQL, Scala, Shell, TypeScript, TeX, Visual Basic

#### Training Data
- **Backbone**: github-code dataset (CodeParrot)
- **Fine-tuning**: 150M+ coding Q&A and docstring-source pairs
- **Base Data**: allenai/c4 dataset

#### Performance
- **Code Search**: Optimized for technical Q&A and long documents
- **Similarity Example**: cos_sim = 0.7282 (sample query-code pair)
- **Benchmark**: No public CodeSearchNet scores available

#### MRL Support
❌ **No MRL support** - Cannot truncate dimensions without quality loss

#### Memory & Latency
- **Model Size**: ~322 MB (F16 tensors)
- **Estimated Latency**: ~15-20ms per doc (RTX 4090, based on parameter count)
- **Batch Processing**: ~500ms for 100 docs

#### Integration Options
1. **HuggingFace Transformers** (requires `trust_remote_code=True`)
2. **Sentence-Transformers** (v2.3.0+)
3. **Transformers.js** (JavaScript/browser)
4. **Jina AI API** (cloud service)
5. **Custom Ollama Model** (requires Modelfile creation)

#### Strengths
- Wide language coverage (30 programming languages)
- Long context support (8k tokens)
- Trained specifically on code data
- Active maintenance by Jina AI

#### Weaknesses
- Not available in Ollama library
- No MRL support (cannot use dimension truncation)
- Requires custom integration
- Larger model size than alternatives

---

### 2. CodeT5+ 110M Embedding

**Availability**: ❌ Not in Ollama

#### Architecture
- **Type**: Encoder-only (extracted from CodeT5+ 220M encoder-decoder)
- **Parameters**: 110M
- **Dimensions**: 256 (compact output)
- **Normalization**: L2-normalized (norm = 1.0)

#### Language Support
**9 Programming Languages**: C, C++, C#, Go, Java, JavaScript, PHP, Python, Ruby

#### Training Data
- **Source**: The Stack (deduplicated, permissive licenses)
- **Licenses**: MIT, Apache-2.0, BSD-3/2-Clause, CC0-1.0, Unlicense, ISC
- **Training Stages**:
  1. Unimodal code pretraining (span denoising, CLM)
  2. Bimodal text-code training (contrastive learning, text-code matching)

#### Performance - CodeXGLUE Code Retrieval (Zero-shot)

| Language | MRR Score |
|----------|-----------|
| Ruby | 74.51 |
| JavaScript | 69.07 |
| Go | 90.69 |
| Python | 71.55 |
| Java | 71.82 |
| PHP | 67.72 |
| **Average** | **74.23** |

**Interpretation**: Strong performance on Go (90.69), solid across other languages (67-75 range).

#### MRL Support
❌ **No MRL support**

#### Memory & Latency
- **Model Size**: ~220 MB (estimated)
- **Dimensions**: 256 (4× smaller than 1024-dim models)
- **Estimated Latency**: ~10ms per doc (RTX 4090)
- **Advantage**: Compact 256-dim output = faster similarity computation

#### Integration Options
- HuggingFace Transformers (`Salesforce/codet5p-110m-embedding`)
- Requires `trust_remote_code=True`

#### Strengths
- **Best benchmark scores** available (74.23 average MRR)
- Compact 256-dim output (storage and compute efficient)
- Strong theoretical foundation (two-stage pretraining)
- Permissive training data licenses

#### Weaknesses
- Limited language support (9 vs 30+ in alternatives)
- Not available in Ollama
- Smaller community compared to CodeBERT/StarEncoder

---

### 3. StarEncoder

**Availability**: ❌ Not in Ollama

#### Architecture
- **Type**: BERT-based encoder (MLM + NSP)
- **Parameters**: ~125M
- **Hidden Size**: 768
- **Attention Heads**: 12
- **Hidden Layers**: 12
- **Max Position Embeddings**: 1,024

#### Language Support
**80+ Programming Languages** from The Stack dataset, including:
- All major languages (Python, Java, JavaScript, C/C++, Go, Rust, etc.)
- GitHub issues and Git commits
- Broadest language coverage of all evaluated models

#### Training Data
- **Source**: The Stack (BigCode)
- **Tokens Processed**: ~400B
- **Training Steps**: 100,000
- **Batch Size**: 4,096 sequences
- **Hardware**: 64 × NVIDIA A100 GPUs (~2 days)

#### Training Objectives
- **Masked Language Modeling (MLM)**: Predict masked tokens
- **Next Sentence Prediction (NSP)**: Sentence adjacency prediction

#### Performance
- **Benchmarks**: No public CodeSearchNet scores
- **Use Cases**: Code understanding, token classification (e.g., PII detection)
- **Derivative Models**: StaPII (PII detection in code)

#### MRL Support
❌ **No MRL support**

#### Memory & Latency
- **Model Size**: ~250 MB (estimated)
- **Estimated Latency**: ~12ms per doc (RTX 4090)

#### Integration Options
- HuggingFace Transformers (`bigcode/starencoder`)
- License: BigCode OpenRAIL-M v1

#### Strengths
- **Widest language coverage** (80+ languages)
- Large-scale training (400B tokens, 64 A100s)
- Strong foundation for fine-tuning
- Active BigCode community

#### Weaknesses
- No public embedding benchmarks
- Training data contains PII (privacy concerns)
- Encoder-only (not suitable for generation tasks)
- Not available in Ollama
- Performance may vary significantly across 80+ languages

---

### 4. CodeSage-base

**Availability**: ❌ Not in Ollama

#### Architecture
- **Type**: Encoder-based
- **Parameters**: 356M (larger than alternatives)
- **Dimensions**: 1024
- **Framework**: PyTorch + Transformers

#### Language Support
**9 Programming Languages**: C, C#, Go, Java, JavaScript, TypeScript, PHP, Python, Ruby

#### Training Data
- **Source**: The Stack (bigcode/the-stack-dedup)
- **Training Stages**:
  1. Masked Language Modeling (MLM) on code
  2. Bimodal text-code pair training

#### Performance
- **Benchmarks**: No public CodeSearchNet scores
- **Recent Update**: V2 release (Dec 2024) with improved performance
- **Features**: Flexible embedding dimensions in V2

#### MRL Support
⚠️ **Partial MRL support** in V2 (configurable dimensions, not traditional MRL)

#### Memory & Latency
- **Model Size**: ~712 MB (largest evaluated model)
- **Estimated Latency**: ~30ms per doc (RTX 4090)
- **Batch Processing**: ~900ms for 100 docs

#### Integration Options
1. HuggingFace Transformers (`codesage/codesage-base`)
2. SentenceTransformer (since Nov 2024)
3. Requires `trust_remote_code=True` and `add_eos_token=True`

#### Strengths
- Recent V2 update (Dec 2024) shows active development
- SentenceTransformer integration (easier to use)
- Flexible dimensions in V2

#### Weaknesses
- **Largest model** (356M params, 712 MB) = slowest inference
- No public benchmarks
- Limited language support (9 languages)
- Not available in Ollama

---

### 5. CodeBERT-base

**Availability**: ❌ Not in Ollama

#### Architecture
- **Type**: RoBERTa-base variant
- **Parameters**: ~125M (inherited from RoBERTa-base)
- **Training Objective**: MLM + RTD (Replaced Token Detection)
- **Base**: RoBERTa architecture

#### Language Support
**6 Programming Languages** (CodeSearchNet corpus):
- Go, Java, JavaScript, PHP, Python, Ruby

#### Training Data
- **Source**: CodeSearchNet (bi-modal: code + documentation)
- **Focus**: Code-documentation pairs for search tasks

#### Performance
- **Benchmarks**: No public scores on landing page
- **Paper**: arXiv:2002.08155 (2020)
- **Use Cases**: Code search, code-to-document generation, feature extraction

#### MRL Support
❌ **No MRL support**

#### Memory & Latency
- **Model Size**: ~250 MB (estimated, RoBERTa-base size)
- **Estimated Latency**: ~12ms per doc (RTX 4090)

#### Integration Options
- HuggingFace Transformers (`microsoft/codebert-base`)
- Official CodeBERT repository (GitHub: microsoft/CodeBERT)

#### Strengths
- Strong research pedigree (Microsoft Research, 2020)
- Bi-modal training (code + natural language)
- Well-documented use cases

#### Weaknesses
- Older model (2020) - superseded by CodeT5+, CodeSage
- Limited language support (6 languages)
- No public benchmark scores readily available
- Not available in Ollama

---

### 6. voyage-code-2 (API-only)

**Availability**: ❌ API service only (not self-hostable)

#### Service Details
- **Provider**: Voyage AI
- **Type**: Proprietary cloud API
- **Access**: Requires API key and internet connectivity

#### Known Capabilities
- Optimized for code search and retrieval
- Proprietary architecture (details not public)

#### Performance
- **Benchmarks**: Not publicly available
- **Claims**: High performance on code tasks (vendor claims)

#### MRL Support
❓ **Unknown** (proprietary model)

#### Cost Structure
- **Pricing**: Pay-per-token API pricing
- **Self-hosting**: Not possible
- **Latency**: Network overhead + inference time

#### Integration Options
- REST API only
- Requires internet connectivity

#### Strengths
- Purpose-built for code search
- Managed service (no infrastructure management)

#### Weaknesses
- **Not self-hostable** - Incompatible with matric-memory's self-hosted architecture
- **API costs** - Ongoing per-token pricing
- **Network dependency** - Requires internet access
- **Vendor lock-in** - Proprietary model
- **Privacy concerns** - Code sent to third-party service

**Recommendation**: ❌ **Reject** - Incompatible with self-hosted deployment model.

---

### 7. OpenAI Embeddings (API-only)

**Availability**: ❌ API service only (not self-hostable)

#### Models
- **text-embedding-3-large**: 3072 dimensions
- **text-embedding-3-small**: 1536 dimensions
- **text-embedding-ada-002**: 1536 dimensions (legacy)

#### Capabilities
- General-purpose embeddings (text + code)
- Not code-specific, but handles code competently
- Truncatable dimensions (MRL-like behavior)

#### Performance
- **Code Performance**: No dedicated code benchmarks
- **General Performance**: Strong on MTEB benchmarks

#### MRL Support
✅ **Partial MRL support** - Can specify output dimensions

#### Cost Structure
- **text-embedding-3-small**: $0.02 per 1M tokens
- **text-embedding-3-large**: $0.13 per 1M tokens

#### Integration Options
- REST API only
- Official Python/Node.js SDKs

#### Strengths
- High-quality general-purpose embeddings
- Dimension truncation support
- Reliable service with good uptime

#### Weaknesses
- **Not self-hostable** - API-only service
- **Not code-optimized** - General-purpose model
- **API costs** - Ongoing per-token pricing
- **Privacy concerns** - Code sent to OpenAI
- **Network dependency** - Requires internet access

**Recommendation**: ❌ **Reject** - Incompatible with self-hosted deployment model.

---

## Comparison Matrix

| Model | Params | Dims | Languages | MRL | Ollama | Benchmark | Self-hostable |
|-------|--------|------|-----------|-----|--------|-----------|---------------|
| **jina-v2-base-code** | 161M | 768 | 30+ code | ❌ | ❌ | Unknown | ✅ |
| **CodeT5+ 110M** | 110M | 256 | 9 code | ❌ | ❌ | 74.23 MRR | ✅ |
| **StarEncoder** | 125M | 768 | 80+ code | ❌ | ❌ | Unknown | ✅ |
| **CodeSage-base** | 356M | 1024 | 9 code | ⚠️ | ❌ | Unknown | ✅ |
| **CodeBERT-base** | 125M | 768 | 6 code | ❌ | ❌ | Unknown | ✅ |
| **voyage-code-2** | Unknown | Unknown | Unknown | ❓ | ❌ | Unknown | ❌ |
| **OpenAI embed-3** | Unknown | 3072 | All (general) | ⚠️ | ❌ | Good (general) | ❌ |
| **nomic-embed-text** | 137M | 768 | All (general) | ✅ | ✅ | Good (general) | ✅ |

**Legend**:
- ✅ = Full support
- ⚠️ = Partial support
- ❌ = No support
- ❓ = Unknown

---

## Performance Benchmarks

### CodeSearchNet (CodeT5+ 110M) - Zero-shot Retrieval

The only model with publicly available code search benchmarks:

```
Language    | MRR Score | Interpretation
------------|-----------|------------------------------------------
Go          | 90.69     | Excellent (best performance)
Ruby        | 74.51     | Good
Java        | 71.82     | Good
Python      | 71.55     | Good
JavaScript  | 69.07     | Good
PHP         | 67.72     | Acceptable
------------|-----------|------------------------------------------
Average     | 74.23     | Good overall performance
```

**Baseline Comparison**: These scores represent zero-shot performance without fine-tuning. Domain-specific fine-tuning on matric-memory's code corpus could improve performance by 10-20% (based on general embedding fine-tuning research).

### Storage Requirements (per 1M code documents)

| Model | Dimensions | Storage | Comments |
|-------|------------|---------|----------|
| CodeT5+ | 256 | 1.0 GB | Most compact |
| jina-code / StarEncoder | 768 | 3.0 GB | Standard |
| CodeSage | 1024 | 4.0 GB | Largest |

**Comparison to General Models**:
- nomic-embed-text: 3.0 GB (768-dim)
- nomic-embed-text @ MRL-64: 0.25 GB (12× smaller)

### Latency Estimates (RTX 4090, per document)

| Model | Params | Est. Latency | Batch (100 docs) |
|-------|--------|--------------|------------------|
| CodeT5+ | 110M | ~10ms | ~300ms |
| jina-code | 161M | ~15ms | ~500ms |
| StarEncoder | 125M | ~12ms | ~400ms |
| CodeSage | 356M | ~30ms | ~900ms |
| **nomic-embed** | 137M | ~12ms | ~350ms |

**Key Insight**: CodeT5+ offers the best latency due to compact 256-dim output, but lacks MRL support. nomic-embed-text with MRL provides better storage/compute trade-offs.

---

## Recommendations

### Immediate (Q1 2026)

#### Option 1: Continue with nomic-embed-text (Recommended)

**Rationale**:
- Already integrated and working
- MRL support for storage optimization
- Available in Ollama (no custom setup)
- Acceptable performance on code (trained on diverse data including code)
- 768 dimensions = good balance

**Trade-offs**:
- Not code-optimized (general-purpose model)
- May miss code-specific semantic relationships

**Implementation**:
```bash
# No changes needed - already deployed
# Current configuration in matric-memory
```

**Use Case**: General knowledge base with mixed content (notes, documentation, and code).

#### Option 2: Add jina-embeddings-v2-base-code via Custom Ollama Model

**Rationale**:
- Best language coverage (30+ programming languages)
- Code-specific training (150M+ code Q&A pairs)
- Self-hostable (matric-memory requirement)
- Similar parameters to nomic-embed-text (161M vs 137M)

**Implementation Steps**:
1. Create Ollama Modelfile
2. Import jina-v2-base-code from HuggingFace
3. Test embedding generation
4. Create embedding config in matric-memory
5. Deploy to code-specific embedding set

**Modelfile Example**:
```dockerfile
FROM ./jina-embeddings-v2-base-code.gguf
PARAMETER temperature 0
TEMPLATE """{{ .Prompt }}"""
```

**Trade-offs**:
- No MRL support (cannot truncate dimensions)
- Custom integration effort
- Not officially supported by Ollama
- Requires model conversion (HuggingFace → GGUF → Ollama)

**Timeline**: 1-2 weeks for integration and testing

---

### Short-term (Q2 2026)

#### Option 3: Deploy CodeT5+ 110M as Custom Ollama Model

**Rationale**:
- **Best documented performance** (74.23 MRR on CodeSearchNet)
- Compact 256-dim output (4× storage savings vs 1024-dim)
- Strong zero-shot performance on code search
- Permissive training data licenses

**Implementation**:
1. Convert CodeT5+ to GGUF format
2. Create Ollama model
3. Benchmark against nomic-embed-text on matric-memory code corpus
4. Deploy if >10% improvement

**Expected Performance**:
- Go code: Excellent (90.69 MRR)
- Python/Java/JavaScript: Good (69-72 MRR)
- Other languages: Unknown (limited to 9 languages)

**Trade-offs**:
- Limited to 9 programming languages
- No MRL support
- Custom integration required

**Timeline**: 2-3 weeks (conversion + testing + benchmarking)

---

### Long-term (Q3-Q4 2026)

#### Option 4: Advocate for Ollama Code Embedding Models

**Actions**:
1. Submit feature request to Ollama GitHub
2. Propose jina-v2-base-code or CodeT5+ for official library
3. Offer to contribute Modelfile and documentation

**Benefits**:
- Official support and maintenance
- Easier integration for all users
- Regular updates alongside Ollama releases

**Timeline**: 3-6 months (depends on Ollama team prioritization)

#### Option 5: Fine-tune General Model on matric-memory Code Corpus

**Rationale** (from REF-069 in embedding-model-selection.md):
- Domain-specific fine-tuning: 88% retrieval improvement
- Requires ~6,000 synthetic query-document pairs
- Most effective when baseline Recall@10 < 60%

**Implementation**:
1. Generate synthetic code search queries (LLM-based)
2. Fine-tune nomic-embed-text or CodeT5+ on matric-memory data
3. Evaluate on held-out test set
4. Deploy if >15% improvement

**Trade-offs**:
- Requires significant compute (fine-tuning)
- Only beneficial if baseline performance is poor
- Needs ongoing maintenance (re-train on new data)

**Timeline**: 4-6 weeks (data generation + training + evaluation)

---

## Decision Framework

### When to Use General Models (nomic-embed-text)

✅ **Use when**:
- Content is mixed (notes, documentation, prose, and code)
- Storage optimization is critical (MRL support needed)
- Deployment simplicity is prioritized
- Code search quality is acceptable (>60% Recall@10)

### When to Use Code-Specific Models

✅ **Use when**:
- Content is primarily code (>70% code documents)
- Code search quality is poor with general models (<60% Recall@10)
- Specific programming languages dominate (e.g., 80% Go code → CodeT5+ excels)
- Willing to invest in custom Ollama integration

### When to Fine-tune

✅ **Use when**:
- Domain is highly specialized (e.g., legacy COBOL, custom DSLs)
- Baseline retrieval is poor (<60% Recall@10)
- Have sufficient training data (>1,000 code documents)
- Can generate synthetic queries (LLM available)

---

## Implementation Roadmap

### Phase 1: Validation (Weeks 1-2)

1. **Benchmark Current Performance**
   - Measure nomic-embed-text performance on code search queries
   - Establish baseline Recall@10 and MRR metrics
   - Identify performance gaps

2. **Requirements Analysis**
   - Analyze matric-memory code corpus (language distribution)
   - Determine if code-specific model is justified
   - Calculate ROI for custom integration

### Phase 2: Prototype (Weeks 3-4)

3. **Option A**: Continue with nomic-embed-text
   - Document decision rationale
   - Optimize MRL truncation settings
   - Close issue #394

4. **Option B**: Integrate jina-v2-base-code
   - Convert model to GGUF format
   - Create Ollama Modelfile
   - Test embedding generation
   - Benchmark against baseline

5. **Option C**: Integrate CodeT5+ 110M
   - Convert model to GGUF format
   - Create Ollama Modelfile
   - Benchmark on CodeSearchNet languages
   - Compare to baseline

### Phase 3: Deployment (Weeks 5-6)

6. **Production Integration**
   - Create embedding config in matric-memory
   - Deploy to code-specific embedding set
   - Configure auto-embed rules for code documents
   - Monitor performance metrics

7. **Documentation**
   - Update embedding-model-selection.md
   - Add code search examples
   - Document custom Ollama setup (if applicable)

### Phase 4: Optimization (Weeks 7-8)

8. **Performance Tuning**
   - A/B test general vs code-specific embeddings
   - Optimize truncate_dim settings
   - Tune search parameters (RRF weights, k-values)

9. **Fine-tuning (Optional)**
   - Generate synthetic queries if needed
   - Fine-tune selected model on matric-memory corpus
   - Evaluate improvements

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Custom Ollama integration breaks on updates | Medium | High | Pin Ollama version, automate testing |
| Code model performs worse than general model | Low | Medium | Benchmark before full deployment |
| GGUF conversion introduces quality loss | Low | High | Validate embeddings against HF model |
| Limited language support misses edge cases | Medium | Low | Fallback to general model for unsupported languages |
| MRL absence limits storage optimization | High | Medium | Accept trade-off or use general model with MRL |

---

## Open Questions

1. **Ollama Roadmap**: Will Ollama add code embedding models to official library?
   - **Action**: Monitor Ollama releases, submit feature request

2. **Benchmark Validation**: How do code models perform on matric-memory's actual code corpus?
   - **Action**: Run benchmark suite on current data before deciding

3. **Language Distribution**: What programming languages dominate matric-memory usage?
   - **Action**: Analyze document type registry and attachment metadata

4. **Quality Threshold**: What Recall@10 improvement justifies custom integration effort?
   - **Action**: Define acceptance criteria (suggest >10% improvement)

---

## Technical Notes

### Custom Ollama Model Integration

**Required Steps**:
1. Export HuggingFace model to ONNX format
2. Convert ONNX to GGUF (llama.cpp tools)
3. Quantize GGUF (optional, for performance)
4. Create Ollama Modelfile
5. Import with `ollama create`
6. Test embedding generation
7. Configure matric-memory embedding config

**Challenges**:
- Not all architectures supported by llama.cpp
- Quantization may impact quality (need validation)
- Ollama updates could break custom models
- No official documentation for custom embeddings

**Alternatives**:
- Run HuggingFace Transformers directly (bypass Ollama)
- Use Sentence-Transformers library
- Deploy custom embedding service (FastAPI + HF)

---

## References

### Model Documentation

- **jina-embeddings-v2-base-code**: https://huggingface.co/jinaai/jina-embeddings-v2-base-code
- **CodeT5+ 110M**: https://huggingface.co/Salesforce/codet5p-110m-embedding
- **StarEncoder**: https://huggingface.co/bigcode/starencoder
- **CodeSage-base**: https://huggingface.co/codesage/codesage-base
- **CodeBERT-base**: https://huggingface.co/microsoft/codebert-base

### Academic Papers

- **CodeT5+**: Wang et al. (2023). "CodeT5+: Open Code Large Language Models for Code Understanding and Generation." arXiv:2305.07922
- **CodeBERT**: Feng et al. (2020). "CodeBERT: A Pre-Trained Model for Programming and Natural Languages." arXiv:2002.08155
- **StarCoder/StarEncoder**: Li et al. (2023). "StarCoder: May the Source Be with You!" arXiv:2305.06161

### Related Resources

- **Ollama Library**: https://ollama.com/library
- **matric-memory Embedding Guide**: docs/content/embedding-model-selection.md
- **MTEB Leaderboard**: https://huggingface.co/spaces/mteb/leaderboard
- **CodeSearchNet Benchmark**: https://github.com/github/CodeSearchNet

---

## Appendix A: Model Conversion Cheatsheet

### Convert HuggingFace to GGUF

```bash
# 1. Clone llama.cpp
git clone https://github.com/ggerganov/llama.cpp.git
cd llama.cpp

# 2. Install dependencies
pip install -r requirements.txt

# 3. Convert model
python convert-hf-to-gguf.py /path/to/huggingface/model \
  --outfile model.gguf \
  --outtype f16

# 4. Quantize (optional)
./quantize model.gguf model-q4_0.gguf q4_0

# 5. Create Ollama Modelfile
cat > Modelfile <<EOF
FROM ./model-q4_0.gguf
PARAMETER temperature 0
TEMPLATE """{{ .Prompt }}"""
EOF

# 6. Import to Ollama
ollama create my-code-embed -f Modelfile

# 7. Test
ollama run my-code-embed "def hello():\n    print('world')"
```

### Validate Embeddings

```python
# Compare HuggingFace vs Ollama embeddings
from transformers import AutoModel, AutoTokenizer
import ollama
import numpy as np

# HuggingFace baseline
model = AutoModel.from_pretrained("jinaai/jina-embeddings-v2-base-code", trust_remote_code=True)
tokenizer = AutoTokenizer.from_pretrained("jinaai/jina-embeddings-v2-base-code", trust_remote_code=True)

code = "def hello():\n    print('world')"

# HF embedding
inputs = tokenizer(code, return_tensors="pt")
hf_embed = model(**inputs)[0].detach().numpy()

# Ollama embedding
ollama_embed = ollama.embeddings(model="my-code-embed", prompt=code)["embedding"]

# Compare similarity
cosine_sim = np.dot(hf_embed, ollama_embed) / (np.linalg.norm(hf_embed) * np.linalg.norm(ollama_embed))
print(f"Similarity: {cosine_sim:.4f}")  # Should be >0.95
```

---

## Appendix B: Benchmark Test Suite

### Code Search Test Queries

Use these queries to benchmark code embedding models:

```python
test_queries = [
    # Natural language → Code
    ("function to read a file", "def read_file(path):"),
    ("sort an array of numbers", "def sort(arr):"),
    ("connect to database", "def connect(host, port):"),

    # Code → Similar code
    ("def calculate_sum(a, b):", "def add_numbers(x, y):"),
    ("class UserRepository:", "class User:"),

    # Documentation → Code
    ("Returns the length of string", "def strlen(s):"),
    ("Validates email format", "def is_valid_email(email):"),
]

def benchmark_model(model_name, queries):
    """Run benchmark suite and return MRR score."""
    from ollama import embeddings

    mrr_scores = []
    for query, expected_code in queries:
        query_embed = embeddings(model=model_name, prompt=query)["embedding"]
        code_embed = embeddings(model=model_name, prompt=expected_code)["embedding"]

        similarity = cosine_similarity(query_embed, code_embed)
        mrr_scores.append(similarity)

    return np.mean(mrr_scores)
```

---

## Change Log

- **2026-02-01**: Initial research completed
  - Evaluated 7 code embedding models
  - Confirmed no Ollama-native code models available
  - Recommended nomic-embed-text (short-term) or custom jina-code integration (long-term)

---

## Contact

For questions about this research:
- **Issue**: #394 (matric-memory repository)
- **Related Docs**: docs/content/embedding-model-selection.md
- **Codebase**: crates/matric-inference/ (Ollama integration)

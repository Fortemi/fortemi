# Matric-Memory Model Evaluation System

## Executive Summary

This document outlines a comprehensive evaluation framework for matric-memory, designed to assess LLM and embedding models for memory-specific capabilities: semantic search, content revision, title generation, linking, and retrieval quality.

Unlike general-purpose evals, matric-memory models need specialized assessment for:
- **Embedding quality**: Retrieval accuracy, semantic clustering, dimension efficiency
- **Revision quality**: Content enhancement without hallucination, structure preservation
- **Title generation**: Concise, descriptive, contextually relevant titles
- **Context understanding**: Accurate summarization of linked note relationships

---

## 1. Evaluation Categories

### 1.1 Embedding Model Evaluation

**Models to Test:**
- `nomic-embed-text` (current, 768-dim)
- `mxbai-embed-large` (1024-dim)
- Additional models if available via API

**Metrics:**

| Metric | Description | Weight |
|--------|-------------|--------|
| **Precision@K** | Relevant results in top-K | 20% |
| **Recall@K** | Coverage of all relevant items | 15% |
| **MRR** | Mean Reciprocal Rank | 20% |
| **NDCG@10** | Normalized Discounted Cumulative Gain | 20% |
| **Semantic Accuracy** | Correct similarity judgments | 15% |
| **Latency (p50/p95)** | Embedding generation speed | 5% |
| **Throughput** | Embeddings per second | 5% |

**Test Scenarios:**
1. **Exact Match Retrieval**: Query with known document, verify top result
2. **Semantic Similarity**: Related concepts should cluster together
3. **Dissimilarity Detection**: Unrelated content should be distant
4. **Multi-chunk Coherence**: Same document chunks should be similar
5. **Cross-domain Separation**: Different topics should separate cleanly

### 1.2 LLM Model Evaluation (Generation)

**Models to Test:**
| Model | Size | Category |
|-------|------|----------|
| `gpt-oss:20b` | 20B | Current default |
| `qwen2.5:32b` | 32B | Large |
| `qwen2.5:14b` | 14B | Medium |
| `qwen2.5:7b` | 7B | Small |
| `qwen3:8b` | 8B | Small |
| `llama3.1:8b` | 8B | Small |
| `mistral:latest` | 7B | Small |
| `hermes3:8b` | 8B | Small |
| `deepseek-r1:14b` | 14B | Medium (Reasoning) |
| `cogito:8b` | 8B | Small |
| `exaone-deep:7.8b` | 7.8B | Small |
| `command-r7b` | 7B | Small |
| `nemotron-mini:4b` | 4B | Tiny |
| `granite4:3b` | 3B | Tiny |
| `smollm2:1.7b` | 1.7B | Micro |

**Evaluation Dimensions:**

#### A. AI Revision Quality (40% of LLM score)
| Metric | Description | Weight |
|--------|-------------|--------|
| **Information Preservation** | No content lost from original | 25% |
| **Structure Enhancement** | Markdown formatting, headers, lists | 20% |
| **No Hallucination** | No invented facts or claims | 30% |
| **Contextual Integration** | Uses related notes appropriately | 15% |
| **Readability** | Clear, well-organized output | 10% |

#### B. Title Generation Quality (20% of LLM score)
| Metric | Description | Weight |
|--------|-------------|--------|
| **Relevance** | Title reflects content accurately | 35% |
| **Conciseness** | 3-8 words, no filler | 25% |
| **Uniqueness** | Distinguishable from similar notes | 20% |
| **Format Compliance** | No quotes, proper capitalization | 20% |

#### C. Context Understanding (20% of LLM score)
| Metric | Description | Weight |
|--------|-------------|--------|
| **Summary Accuracy** | Correct representation of links | 40% |
| **Relationship Clarity** | Clear explanation of connections | 30% |
| **Brevity** | Concise without losing meaning | 30% |

#### D. Instruction Following (10% of LLM score)
| Metric | Description | Weight |
|--------|-------------|--------|
| **Mode Compliance** | Respects full/light/none modes | 50% |
| **Format Adherence** | Follows output format specs | 30% |
| **Constraint Respect** | Honors length/style limits | 20% |

#### E. Efficiency (10% of LLM score)
| Metric | Description | Weight |
|--------|-------------|--------|
| **Latency (TTFT)** | Time to first token | 30% |
| **Latency (Total)** | Full response time | 30% |
| **Token Efficiency** | Output quality per token | 40% |

---

## 2. Test Dataset Design

### 2.1 Ground Truth Corpus

Create a curated dataset of 100 notes with:
- **Known relationships**: Pre-defined similar/dissimilar pairs
- **Expected titles**: Human-verified ideal titles
- **Revision examples**: Before/after pairs with quality scores
- **Search queries**: Queries with expected results and rankings

### 2.2 Dataset Categories

```
datasets/
├── embedding_tests/
│   ├── similarity_pairs.json      # 50 similar note pairs
│   ├── dissimilarity_pairs.json   # 50 dissimilar pairs
│   ├── retrieval_queries.json     # 30 queries with relevance judgments
│   └── clustering_sets.json       # 10 topic clusters (5 notes each)
├── revision_tests/
│   ├── full_revision_cases.json   # 20 notes needing full enhancement
│   ├── light_revision_cases.json  # 20 notes for formatting only
│   └── preservation_tests.json    # 10 notes with critical facts
├── title_tests/
│   ├── title_cases.json           # 30 notes with ideal titles
│   └── disambiguation_sets.json   # 10 sets of similar notes
└── context_tests/
    ├── link_summaries.json        # 20 linked note sets
    └── relationship_tests.json    # 15 relationship descriptions
```

### 2.3 Dataset Content Domains

To ensure comprehensive coverage:
1. **Technical** (30%): Programming, architecture, APIs
2. **Research** (25%): Academic concepts, methodologies
3. **Personal** (20%): Observations, opinions, reflections
4. **Procedural** (15%): How-tos, workflows, processes
5. **Mixed** (10%): Cross-domain content

---

## 3. Scoring System

### 3.1 Embedding Model Score (0-100)

```
Score = (Precision@5 × 0.20) + (Recall@10 × 0.15) + (MRR × 0.20) +
        (NDCG@10 × 0.20) + (SemanticAccuracy × 0.15) +
        (LatencyScore × 0.05) + (ThroughputScore × 0.05)

Where:
- LatencyScore = max(0, 100 - (p95_latency_ms / 10))
- ThroughputScore = min(100, embeddings_per_sec × 5)
```

### 3.2 LLM Model Score (0-100)

```
Score = (RevisionQuality × 0.40) + (TitleQuality × 0.20) +
        (ContextQuality × 0.20) + (InstructionFollowing × 0.10) +
        (Efficiency × 0.10)
```

### 3.3 Combined Recommendation Score

For models that can do both embedding AND generation:
```
CombinedScore = (EmbeddingScore × 0.50) + (LLMScore × 0.50)
```

---

## 4. Implementation Architecture

### 4.1 Directory Structure

```
evals/
├── PLAN.md                    # This document
├── README.md                  # Quick start guide
├── package.json               # Node.js dependencies
├── tsconfig.json              # TypeScript config
├── src/
│   ├── index.ts               # CLI entry point
│   ├── runner.ts              # Evaluation orchestrator
│   ├── models/
│   │   ├── ollama.ts          # Ollama backend
│   │   └── types.ts           # Model interfaces
│   ├── evaluators/
│   │   ├── embedding.ts       # Embedding model evaluator
│   │   ├── revision.ts        # AI revision evaluator
│   │   ├── title.ts           # Title generation evaluator
│   │   ├── context.ts         # Context understanding evaluator
│   │   └── instruction.ts     # Instruction following evaluator
│   ├── metrics/
│   │   ├── retrieval.ts       # Precision, Recall, MRR, NDCG
│   │   ├── similarity.ts      # Cosine similarity, clustering
│   │   ├── text.ts            # BLEU, ROUGE, semantic similarity
│   │   └── latency.ts         # Timing and throughput
│   ├── scoring/
│   │   ├── calculator.ts      # Score aggregation
│   │   └── weights.ts         # Configurable weights
│   └── reporters/
│       ├── json.ts            # Raw JSON output
│       ├── markdown.ts        # Human-readable report
│       └── charts.ts          # Chart generation
├── datasets/                  # Test data (see 2.2)
├── scenarios/                 # Evaluation scenario definitions
├── reports/                   # Generated reports
└── charts/                    # Generated visualizations
```

### 4.2 CLI Commands

```bash
# Run full evaluation suite
npm run eval

# Evaluate specific model type
npm run eval:embeddings
npm run eval:llms

# Evaluate specific model
npm run eval -- --model qwen2.5:14b

# Generate report only (from existing data)
npm run report

# Quick smoke test
npm run eval:quick
```

### 4.3 Output Files

Each evaluation run produces:
```
reports/
└── eval-2025-01-14T12-30-00/
    ├── summary.json           # Complete raw data
    ├── report.md              # Human-readable report
    ├── charts/
    │   ├── embedding-comparison.svg
    │   ├── llm-comparison.svg
    │   ├── latency-distribution.svg
    │   ├── score-radar.svg
    │   └── dimension-heatmap.svg
    └── raw/
        ├── embedding-results.json
        └── llm-results.json
```

---

## 5. Chart Specifications

### 5.1 Required Visualizations

1. **Model Comparison Bar Chart**
   - X: Models, Y: Overall Score
   - Color-coded by model size category
   - Error bars for variance

2. **Dimension Radar Chart**
   - One polygon per model
   - Axes: Each scoring dimension
   - Overlay top 3-5 models

3. **Latency vs Quality Scatter**
   - X: Average latency, Y: Quality score
   - Bubble size: Model parameter count
   - Pareto frontier highlighted

4. **Retrieval Metrics Grouped Bar**
   - Groups: P@5, P@10, R@5, R@10, MRR, NDCG
   - Bars: Each embedding model
   - Baseline reference line

5. **Token Efficiency Heatmap**
   - Rows: Models, Columns: Task types
   - Color: Quality per 1K tokens
   - Annotated with raw values

6. **Score Distribution Violin Plot**
   - One violin per model
   - Shows variance across test cases
   - Median and quartiles marked

### 5.2 Chart Library

Use **Vega-Lite** for declarative chart specs, rendered to SVG:
- Portable, no browser needed
- Clean, publication-quality output
- Easy to customize themes

---

## 6. Execution Plan

### Phase 1: Infrastructure (Day 1)
- [ ] Initialize Node.js project with TypeScript
- [ ] Implement Ollama backend interface
- [ ] Create base evaluator framework
- [ ] Set up metrics calculation utilities

### Phase 2: Datasets (Day 1-2)
- [ ] Create embedding test pairs
- [ ] Create revision test cases
- [ ] Create title test cases
- [ ] Create context test cases

### Phase 3: Evaluators (Day 2-3)
- [ ] Implement embedding evaluator
- [ ] Implement revision evaluator
- [ ] Implement title evaluator
- [ ] Implement context evaluator
- [ ] Implement instruction following evaluator

### Phase 4: Scoring & Reporting (Day 3)
- [ ] Implement score calculator
- [ ] Implement JSON reporter
- [ ] Implement Markdown reporter
- [ ] Implement chart generation

### Phase 5: Evaluation Run (Day 4)
- [ ] Run embedding model evaluations
- [ ] Run LLM evaluations
- [ ] Generate final report

### Phase 6: Analysis & Recommendations (Day 4)
- [ ] Analyze results
- [ ] Generate recommendations
- [ ] Document findings

---

## 7. Success Criteria

The evaluation system is complete when:

1. **All models evaluated**: Every available Ollama model has scores
2. **Full metrics coverage**: All defined metrics calculated
3. **Complete report**: JSON + Markdown + Charts generated
4. **Recommendations made**: Clear guidance on best models for each use case
5. **Reproducible**: Can re-run at any time with same methodology

---

## 8. Technical Notes

### 8.1 Ollama API Usage

```typescript
// Embedding
POST http://localhost:11434/api/embed
{
  "model": "nomic-embed-text",
  "input": "text to embed"
}

// Generation
POST http://localhost:11434/api/generate
{
  "model": "qwen2.5:14b",
  "prompt": "...",
  "stream": false
}
```

### 8.2 Similarity Calculation

For embedding comparison:
```typescript
function cosineSimilarity(a: number[], b: number[]): number {
  const dotProduct = a.reduce((sum, ai, i) => sum + ai * b[i], 0);
  const normA = Math.sqrt(a.reduce((sum, ai) => sum + ai * ai, 0));
  const normB = Math.sqrt(b.reduce((sum, bi) => sum + bi * bi, 0));
  return dotProduct / (normA * normB);
}
```

### 8.3 LLM-as-Judge for Quality Assessment

Use a capable model (qwen2.5:32b or external API) to judge:
- Revision quality
- Title appropriateness
- Context accuracy

With structured output scoring 1-10 on each criterion.

---

## Appendix A: Sample Evaluation Scenario

```json
{
  "id": "revision-full-001",
  "type": "revision",
  "mode": "full",
  "input": {
    "content": "rust async is tricky. tokio runtime. futures. pin. etc.",
    "related_notes": [
      "Detailed guide on Rust async patterns...",
      "Comparison of Tokio vs async-std..."
    ]
  },
  "expected": {
    "preserves_concepts": ["async", "tokio", "futures", "pin"],
    "adds_structure": true,
    "no_hallucination": true,
    "min_length": 200,
    "max_length": 1000
  },
  "scoring": {
    "preservation_weight": 0.30,
    "structure_weight": 0.25,
    "hallucination_penalty": 0.30,
    "context_integration_weight": 0.15
  }
}
```

---

## Appendix B: Expected Output Format

### B.1 Summary JSON Structure

```json
{
  "meta": {
    "timestamp": "2025-01-14T12:30:00Z",
    "duration_ms": 180000,
    "models_tested": 16,
    "scenarios_run": 150
  },
  "embedding_results": {
    "nomic-embed-text": {
      "overall_score": 78.5,
      "metrics": {
        "precision_at_5": 0.82,
        "recall_at_10": 0.75,
        "mrr": 0.88,
        "ndcg_at_10": 0.79,
        "semantic_accuracy": 0.85,
        "latency_p50_ms": 45,
        "latency_p95_ms": 120,
        "throughput_per_sec": 22
      }
    }
  },
  "llm_results": {
    "qwen2.5:14b": {
      "overall_score": 82.3,
      "dimensions": {
        "revision_quality": 85.0,
        "title_quality": 78.5,
        "context_quality": 80.0,
        "instruction_following": 90.0,
        "efficiency": 75.0
      }
    }
  },
  "recommendations": {
    "best_embedding": "mxbai-embed-large",
    "best_llm_quality": "qwen2.5:32b",
    "best_llm_balanced": "qwen2.5:14b",
    "best_llm_speed": "qwen3:8b"
  }
}
```

---

*Plan Version: 1.0*
*Created: 2025-01-14*
*Status: Ready for Implementation*

# Embedding Model Evaluator

The embedding model evaluator assesses embedding models across multiple dimensions critical for semantic search and retrieval in matric-memory.

## Overview

The evaluator tests embedding models on:

1. **Retrieval Quality** - How well embeddings support finding relevant documents
2. **Similarity Detection** - Accuracy in judging semantic similarity
3. **Performance** - Latency and throughput characteristics

## Metrics

### Retrieval Metrics (75% of overall score)

| Metric | Weight | Description |
|--------|--------|-------------|
| Precision@5 | 20% | Proportion of relevant results in top 5 |
| Recall@10 | 15% | Coverage of all relevant items in top 10 |
| MRR | 20% | Mean Reciprocal Rank (position of first relevant result) |
| NDCG@10 | 20% | Normalized Discounted Cumulative Gain at 10 |

### Similarity Metrics (15% of overall score)

| Metric | Weight | Description |
|--------|--------|-------------|
| Accuracy | 15% | Correct classification of similarity level (high/medium/low) |
| MAE | - | Mean Absolute Error (if ground truth scores provided) |

### Performance Metrics (10% of overall score)

| Metric | Weight | Description |
|--------|--------|-------------|
| Latency (p95) | 5% | 95th percentile embedding generation time |
| Throughput | 5% | Embeddings generated per second |

## Usage

### Basic Evaluation

```typescript
import { OllamaEmbeddingModel } from '../models/ollama.js';
import { evaluateEmbeddingModel, loadEmbeddingDatasets } from '../evaluators/embedding.js';

// Load test datasets
const datasets = await loadEmbeddingDatasets('./datasets/embedding_tests');

// Create model instance
const model = new OllamaEmbeddingModel('nomic-embed-text', 768);

// Evaluate
const result = await evaluateEmbeddingModel(model, datasets);

console.log(`Overall Score: ${result.overallScore}/100`);
console.log(`Precision@5: ${result.metrics.retrieval.precisionAt5}`);
console.log(`Similarity Accuracy: ${result.metrics.similarity.accuracy}`);
```

### CLI Evaluation

```bash
# Evaluate all embedding models
npm run eval:embeddings

# Run example evaluation script
tsx src/examples/evaluate-embeddings.ts
```

## Test Datasets

### Similarity Pairs

Tests the model's ability to correctly identify semantically similar content.

**Format:**
```json
{
  "id": 1,
  "text1": "First text",
  "text2": "Second text",
  "expected_similarity": "high",
  "similarity_score": 0.85  // Optional ground truth
}
```

**Categories:**
- `high`: Texts express same concept (expected similarity ≥ 0.7)
- `medium`: Related concepts (expected similarity 0.4-0.7)
- `low`: Unrelated concepts (expected similarity < 0.4)

### Dissimilarity Pairs

Tests the model's ability to correctly identify semantically dissimilar content.

**Format:** Same as similarity pairs, but with `expected_similarity: "low"`

### Retrieval Queries

Tests end-to-end retrieval quality with realistic queries and document corpora.

**Format:**
```json
{
  "id": 1,
  "query": "Search query text",
  "documents": [
    {
      "id": "doc1",
      "content": "Document text",
      "relevance": 3  // 0-3 scale
    }
  ]
}
```

**Relevance Scale:**
- `3`: Highly relevant (perfect match)
- `2`: Relevant (good match)
- `1`: Marginally relevant (weak match)
- `0`: Not relevant

## Results Structure

```typescript
interface EmbeddingEvalResult {
  modelName: string;
  overallScore: number;  // 0-100
  metrics: {
    retrieval: {
      precisionAt5: number;   // 0-1
      precisionAt10: number;  // 0-1
      recallAt5: number;      // 0-1
      recallAt10: number;     // 0-1
      mrr: number;            // 0-1
      ndcgAt10: number;       // 0-1
    };
    similarity: {
      accuracy: number;              // 0-1
      meanAbsoluteError?: number;    // If ground truth provided
    };
    latency: {
      p50: number;   // milliseconds
      p95: number;   // milliseconds
      p99: number;   // milliseconds
      mean: number;  // milliseconds
      min: number;   // milliseconds
      max: number;   // milliseconds
    };
    throughput: number;  // embeddings/second
  };
  timestamp: string;  // ISO 8601
}
```

## Scoring Formula

```
Overall Score =
  (Precision@5 × 0.20) +
  (Recall@10 × 0.15) +
  (MRR × 0.20) +
  (NDCG@10 × 0.20) +
  (Similarity Accuracy × 0.15) +
  (Latency Score × 0.05) +
  (Throughput Score × 0.05)

Where:
  Latency Score = max(0, 100 - (p95_latency_ms / 10))
  Throughput Score = min(100, throughput × 5)
```

## Example Output

```
Evaluating nomic-embed-text...
============================================================

Model: nomic-embed-text
Overall Score: 78.45/100

Retrieval Metrics:
  Precision@5:  82.00%
  Precision@10: 76.50%
  Recall@5:     68.00%
  Recall@10:    75.00%
  MRR:          88.00%
  NDCG@10:      79.50%

Similarity Metrics:
  Accuracy:     85.00%

Performance Metrics:
  Latency (p50):  45.23ms
  Latency (p95):  120.45ms
  Latency (p99):  145.67ms
  Latency (mean): 52.34ms
  Throughput:     22.50 embeddings/sec
```

## Adding Custom Models

To evaluate a custom embedding model, implement the `EmbeddingModel` interface:

```typescript
interface EmbeddingModel {
  name: string;
  dimensions: number;
  embed(text: string): Promise<number[]>;
  embedBatch(texts: string[]): Promise<number[][]>;
}
```

Then use it with the evaluator:

```typescript
class CustomEmbeddingModel implements EmbeddingModel {
  name = 'my-custom-model';
  dimensions = 384;

  async embed(text: string): Promise<number[]> {
    // Your embedding logic
    return [...]; // Return embedding vector
  }

  async embedBatch(texts: string[]): Promise<number[][]> {
    // Batch embedding logic
    return texts.map(t => this.embed(t));
  }
}

const model = new CustomEmbeddingModel();
const result = await evaluateEmbeddingModel(model, datasets);
```

## Best Practices

1. **Use Representative Data**: Ensure test datasets reflect actual use cases
2. **Multiple Runs**: Run evaluations multiple times to account for variance
3. **Monitor Latency**: p95/p99 latencies matter more than mean for user experience
4. **Balance Trade-offs**: Higher quality often comes with slower performance
5. **Test at Scale**: Evaluate with realistic corpus sizes

## Interpreting Results

### High Overall Score (>80)
- Model performs well across all dimensions
- Good candidate for production use
- Check individual metrics for specific strengths

### Medium Score (60-80)
- Acceptable for most use cases
- Review individual metrics to identify weaknesses
- Consider if specific strengths align with needs

### Low Score (<60)
- Model may not be suitable for semantic search
- Check if model is properly configured
- Verify model is loaded correctly in Ollama

### Red Flags

- **Low Precision@5**: Users won't find relevant results quickly
- **Low Recall@10**: Missing too many relevant documents
- **Low MRR**: First relevant result appears too far down
- **High Latency (p95 > 500ms)**: May cause poor UX
- **Low Similarity Accuracy**: Model doesn't understand semantic relationships well

## Troubleshooting

### Model Not Found
```
Error: Ollama embed failed: Not Found
```
**Solution**: Pull the model with `ollama pull <model-name>`

### High Latency
- Check system resources (CPU/GPU usage)
- Verify Ollama configuration
- Consider smaller embedding models
- Use batch embedding when possible

### Low Accuracy
- Verify datasets are appropriate for model
- Check embedding dimensions match model
- Ensure model is domain-appropriate
- Consider fine-tuned models for specialized domains

## Related Documentation

- [PLAN.md](../PLAN.md) - Overall evaluation framework design
- [Retrieval Metrics](../src/metrics/retrieval.ts) - Implementation details
- [Similarity Metrics](../src/metrics/similarity.ts) - Similarity calculation
- [Ollama Integration](../src/models/ollama.ts) - Model backend

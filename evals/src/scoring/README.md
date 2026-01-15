# Scoring System

This module implements the scoring and weighting system for model evaluations, based on the formulas defined in PLAN.md.

## Files

### calculator.ts

Score calculation and aggregation functions:

- `calculateEmbeddingScore(metrics)` - Calculates overall embedding model score (0-100) using weighted combination:
  - Precision@5: 20%
  - Recall@10: 15%
  - MRR: 20%
  - NDCG@10: 20%
  - Semantic Accuracy: 15%
  - Latency: 5%
  - Throughput: 5%

- `calculateLLMScore(dimensions)` - Calculates overall LLM model score (0-100) using weighted combination:
  - Revision Quality: 40%
  - Title Quality: 20%
  - Context Quality: 20%
  - Instruction Following: 10%
  - Efficiency: 10%

- `normalizeScore(value, min, max)` - Normalizes a value to 0-100 range
- `normalizeLatencyScore(p95LatencyMs)` - Converts latency to score using formula: max(0, 100 - p95/10)
- `normalizeThroughputScore(embeddingsPerSec)` - Converts throughput to score: min(100, eps × 5)
- `calculateWeightedScore(components, weights)` - Generic weighted score calculator

### weights.ts

Configurable weight constants for all scoring dimensions:

- `EMBEDDING_WEIGHTS` - Weights for embedding metrics
- `LLM_DIMENSION_WEIGHTS` - Weights for LLM dimensions
- `REVISION_QUALITY_WEIGHTS` - Sub-weights for revision quality
- `TITLE_QUALITY_WEIGHTS` - Sub-weights for title generation
- `CONTEXT_QUALITY_WEIGHTS` - Sub-weights for context understanding
- `INSTRUCTION_FOLLOWING_WEIGHTS` - Sub-weights for instruction following
- `EFFICIENCY_WEIGHTS` - Sub-weights for efficiency metrics

All weight sets are validated at module load time to ensure they sum to 1.0.

## Usage

```typescript
import { calculateEmbeddingScore, calculateLLMScore } from './calculator.js';

// Calculate embedding score
const embeddingScore = calculateEmbeddingScore({
  retrieval: {
    precisionAt5: 0.80,
    recallAt10: 0.65,
    mrr: 0.85,
    ndcgAt10: 0.75,
    // ... other retrieval metrics
  },
  similarity: {
    accuracy: 0.90,
  },
  latency: {
    p50: 45,
    p95: 120,
    // ... other latency percentiles
  },
  throughput: 22,
});

// Calculate LLM score
const llmScore = calculateLLMScore({
  revisionQuality: 85.0,
  titleQuality: 78.5,
  contextQuality: 80.0,
  instructionFollowing: 90.0,
  efficiency: 75.0,
});
```

## Testing

Tests are in `calculator.test.ts` and verify:
- Normalization functions work correctly
- Weighted scoring matches expected calculations
- Edge cases (perfect scores, zero scores, clamping) handled properly

Run tests:
```bash
npm test src/scoring/
```

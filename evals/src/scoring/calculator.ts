/**
 * Score calculation and aggregation for evaluation results
 * Implements scoring formulas from PLAN.md Section 3
 */

import type { RetrievalMetrics, LatencyMetrics, SimilarityMetrics } from '../models/types.js';
import {
  EMBEDDING_WEIGHTS,
  LLM_DIMENSION_WEIGHTS,
  LATENCY_NORMALIZATION,
  THROUGHPUT_NORMALIZATION,
} from './weights.js';

// ============================================================================
// Normalization Utilities
// ============================================================================

/**
 * Normalize a value to 0-100 range
 * @param value - Value to normalize
 * @param min - Minimum value in range
 * @param max - Maximum value in range
 * @returns Normalized score between 0 and 100
 */
export function normalizeScore(value: number, min: number, max: number): number {
  if (min === max) {
    return 0;
  }

  const normalized = ((value - min) / (max - min)) * 100;
  return Math.max(0, Math.min(100, normalized));
}

/**
 * Normalize latency to score using PLAN.md formula
 * LatencyScore = max(0, 100 - (p95_latency_ms / 10))
 * @param p95LatencyMs - 95th percentile latency in milliseconds
 * @returns Latency score (0-100, higher is better)
 */
export function normalizeLatencyScore(p95LatencyMs: number): number {
  const score =
    LATENCY_NORMALIZATION.maxScore -
    p95LatencyMs / LATENCY_NORMALIZATION.divisor;

  return Math.max(0, score);
}

/**
 * Normalize throughput to score using PLAN.md formula
 * ThroughputScore = min(100, embeddings_per_sec × 5)
 * @param embeddingsPerSec - Embeddings per second
 * @returns Throughput score (0-100, higher is better)
 */
export function normalizeThroughputScore(embeddingsPerSec: number): number {
  const score = embeddingsPerSec * THROUGHPUT_NORMALIZATION.multiplier;
  return Math.min(THROUGHPUT_NORMALIZATION.maxScore, score);
}

// ============================================================================
// Embedding Model Scoring
// ============================================================================

/**
 * Calculate overall embedding model score
 * Implements formula from PLAN.md Section 3.1
 *
 * Score = (Precision@5 × 0.20) + (Recall@10 × 0.15) + (MRR × 0.20) +
 *         (NDCG@10 × 0.20) + (SemanticAccuracy × 0.15) +
 *         (LatencyScore × 0.05) + (ThroughputScore × 0.05)
 *
 * @param metrics - Embedding evaluation metrics
 * @returns Overall score (0-100)
 */
export function calculateEmbeddingScore(metrics: {
  retrieval: RetrievalMetrics;
  similarity: SimilarityMetrics;
  latency: LatencyMetrics;
  throughput: number;
}): number {
  // Convert metrics to 0-100 scale
  const precisionAt5 = metrics.retrieval.precisionAt5 * 100;
  const recallAt10 = metrics.retrieval.recallAt10 * 100;
  const mrr = metrics.retrieval.mrr * 100;
  const ndcgAt10 = metrics.retrieval.ndcgAt10 * 100;
  const semanticAccuracy = metrics.similarity.accuracy * 100;

  // Calculate performance scores
  const latencyScore = normalizeLatencyScore(metrics.latency.p95);
  const throughputScore = normalizeThroughputScore(metrics.throughput);

  // Weighted sum
  const score =
    precisionAt5 * EMBEDDING_WEIGHTS.precisionAt5 +
    recallAt10 * EMBEDDING_WEIGHTS.recallAt10 +
    mrr * EMBEDDING_WEIGHTS.mrr +
    ndcgAt10 * EMBEDDING_WEIGHTS.ndcgAt10 +
    semanticAccuracy * EMBEDDING_WEIGHTS.semanticAccuracy +
    latencyScore * EMBEDDING_WEIGHTS.latency +
    throughputScore * EMBEDDING_WEIGHTS.throughput;

  return score;
}

// ============================================================================
// LLM Model Scoring
// ============================================================================

/**
 * Calculate overall LLM model score
 * Implements formula from PLAN.md Section 3.2
 *
 * Score = (RevisionQuality × 0.40) + (TitleQuality × 0.20) +
 *         (ContextQuality × 0.20) + (InstructionFollowing × 0.10) +
 *         (Efficiency × 0.10)
 *
 * @param dimensions - LLM evaluation dimensions (each 0-100)
 * @returns Overall score (0-100)
 */
export function calculateLLMScore(dimensions: {
  revisionQuality: number;
  titleQuality: number;
  contextQuality: number;
  instructionFollowing: number;
  efficiency: number;
}): number {
  const score =
    dimensions.revisionQuality * LLM_DIMENSION_WEIGHTS.revisionQuality +
    dimensions.titleQuality * LLM_DIMENSION_WEIGHTS.titleQuality +
    dimensions.contextQuality * LLM_DIMENSION_WEIGHTS.contextQuality +
    dimensions.instructionFollowing * LLM_DIMENSION_WEIGHTS.instructionFollowing +
    dimensions.efficiency * LLM_DIMENSION_WEIGHTS.efficiency;

  return score;
}

// ============================================================================
// Sub-dimension Score Calculation
// ============================================================================

/**
 * Calculate weighted sub-score from component scores
 * Generic helper for calculating dimension scores from sub-components
 *
 * @param components - Component scores (each 0-100)
 * @param weights - Weights for each component (must sum to 1.0)
 * @returns Weighted score (0-100)
 */
export function calculateWeightedScore(
  components: Record<string, number>,
  weights: Record<string, number>
): number {
  let score = 0;

  for (const [key, value] of Object.entries(components)) {
    const weight = weights[key];
    if (weight === undefined) {
      throw new Error(`No weight defined for component: ${key}`);
    }
    score += value * weight;
  }

  return score;
}

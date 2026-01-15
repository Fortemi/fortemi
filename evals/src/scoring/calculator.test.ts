/**
 * Tests for score calculation functions
 */

import { describe, it, expect } from '@jest/globals';
import {
  calculateEmbeddingScore,
  calculateLLMScore,
  normalizeScore,
  normalizeLatencyScore,
  normalizeThroughputScore,
} from './calculator.js';
import type { RetrievalMetrics, LatencyMetrics, SimilarityMetrics } from '../models/types.js';

describe('normalizeScore', () => {
  it('should normalize value to 0-100 range', () => {
    expect(normalizeScore(50, 0, 100)).toBe(50);
    expect(normalizeScore(0, 0, 100)).toBe(0);
    expect(normalizeScore(100, 0, 100)).toBe(100);
  });

  it('should handle non-zero min values', () => {
    expect(normalizeScore(75, 50, 100)).toBe(50);
    expect(normalizeScore(50, 50, 100)).toBe(0);
    expect(normalizeScore(100, 50, 100)).toBe(100);
  });

  it('should clamp values outside range', () => {
    expect(normalizeScore(-10, 0, 100)).toBe(0);
    expect(normalizeScore(150, 0, 100)).toBe(100);
  });

  it('should return 0 when min equals max', () => {
    expect(normalizeScore(50, 50, 50)).toBe(0);
  });
});

describe('normalizeLatencyScore', () => {
  it('should calculate latency score using formula from PLAN.md', () => {
    // LatencyScore = max(0, 100 - (p95_latency_ms / 10))
    expect(normalizeLatencyScore(0)).toBe(100);
    expect(normalizeLatencyScore(100)).toBe(90);
    expect(normalizeLatencyScore(500)).toBe(50);
    expect(normalizeLatencyScore(1000)).toBe(0);
    expect(normalizeLatencyScore(2000)).toBe(0); // Clamped to 0
  });
});

describe('normalizeThroughputScore', () => {
  it('should calculate throughput score using formula from PLAN.md', () => {
    // ThroughputScore = min(100, embeddings_per_sec × 5)
    expect(normalizeThroughputScore(10)).toBe(50);
    expect(normalizeThroughputScore(20)).toBe(100);
    expect(normalizeThroughputScore(30)).toBe(100); // Clamped to 100
    expect(normalizeThroughputScore(0)).toBe(0);
  });
});

describe('calculateEmbeddingScore', () => {
  it('should calculate weighted embedding score', () => {
    const metrics = {
      retrieval: {
        precisionAt5: 0.80,
        precisionAt10: 0.75,
        recallAt5: 0.70,
        recallAt10: 0.65,
        mrr: 0.85,
        ndcgAt10: 0.75,
      } as RetrievalMetrics,
      similarity: {
        accuracy: 0.90,
      } as SimilarityMetrics,
      latency: {
        p50: 45,
        p95: 120,
        p99: 200,
        mean: 60,
        min: 20,
        max: 250,
      } as LatencyMetrics,
      throughput: 22, // embeddings per second
    };

    const score = calculateEmbeddingScore(metrics);

    // Score should be between 0 and 100
    expect(score).toBeGreaterThanOrEqual(0);
    expect(score).toBeLessThanOrEqual(100);

    // Manual calculation:
    // Precision@5: 0.80 * 100 * 0.20 = 16.0
    // Recall@10: 0.65 * 100 * 0.15 = 9.75
    // MRR: 0.85 * 100 * 0.20 = 17.0
    // NDCG@10: 0.75 * 100 * 0.20 = 15.0
    // Semantic Accuracy: 0.90 * 100 * 0.15 = 13.5
    // Latency: max(0, 100 - 120/10) * 0.05 = 88 * 0.05 = 4.4
    // Throughput: min(100, 22 * 5) * 0.05 = 100 * 0.05 = 5.0
    // Total: 16.0 + 9.75 + 17.0 + 15.0 + 13.5 + 4.4 + 5.0 = 80.65
    expect(score).toBeCloseTo(80.65, 1);
  });

  it('should handle perfect scores', () => {
    const metrics = {
      retrieval: {
        precisionAt5: 1.0,
        precisionAt10: 1.0,
        recallAt5: 1.0,
        recallAt10: 1.0,
        mrr: 1.0,
        ndcgAt10: 1.0,
      } as RetrievalMetrics,
      similarity: {
        accuracy: 1.0,
      } as SimilarityMetrics,
      latency: {
        p50: 10,
        p95: 20,
        p99: 30,
        mean: 15,
        min: 5,
        max: 40,
      } as LatencyMetrics,
      throughput: 100,
    };

    const score = calculateEmbeddingScore(metrics);
    expect(score).toBeGreaterThan(95);
  });
});

describe('calculateLLMScore', () => {
  it('should calculate weighted LLM score', () => {
    const dimensions = {
      revisionQuality: 85.0,
      titleQuality: 78.5,
      contextQuality: 80.0,
      instructionFollowing: 90.0,
      efficiency: 75.0,
    };

    const score = calculateLLMScore(dimensions);

    // Manual calculation:
    // Revision: 85.0 * 0.40 = 34.0
    // Title: 78.5 * 0.20 = 15.7
    // Context: 80.0 * 0.20 = 16.0
    // Instruction: 90.0 * 0.10 = 9.0
    // Efficiency: 75.0 * 0.10 = 7.5
    // Total: 34.0 + 15.7 + 16.0 + 9.0 + 7.5 = 82.2
    expect(score).toBeCloseTo(82.2, 1);
  });

  it('should handle perfect scores', () => {
    const dimensions = {
      revisionQuality: 100.0,
      titleQuality: 100.0,
      contextQuality: 100.0,
      instructionFollowing: 100.0,
      efficiency: 100.0,
    };

    const score = calculateLLMScore(dimensions);
    expect(score).toBe(100.0);
  });

  it('should handle zero scores', () => {
    const dimensions = {
      revisionQuality: 0.0,
      titleQuality: 0.0,
      contextQuality: 0.0,
      instructionFollowing: 0.0,
      efficiency: 0.0,
    };

    const score = calculateLLMScore(dimensions);
    expect(score).toBe(0.0);
  });

  it('should maintain score within 0-100 range', () => {
    const dimensions = {
      revisionQuality: 50.0,
      titleQuality: 60.0,
      contextQuality: 70.0,
      instructionFollowing: 80.0,
      efficiency: 90.0,
    };

    const score = calculateLLMScore(dimensions);
    expect(score).toBeGreaterThanOrEqual(0);
    expect(score).toBeLessThanOrEqual(100);
  });
});

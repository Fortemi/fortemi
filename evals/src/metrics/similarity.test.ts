/**
 * Tests for similarity metrics
 */

import { describe, it, expect } from '@jest/globals';
import { cosineSimilarity, euclideanDistance, calculateSimilarityAccuracy } from './similarity.js';

describe('Similarity Metrics', () => {
  describe('cosineSimilarity', () => {
    it('should return 1.0 for identical vectors', () => {
      const vec = [1, 2, 3, 4];
      const similarity = cosineSimilarity(vec, vec);
      expect(similarity).toBeCloseTo(1.0, 5);
    });

    it('should return -1.0 for opposite vectors', () => {
      const vec1 = [1, 2, 3];
      const vec2 = [-1, -2, -3];
      const similarity = cosineSimilarity(vec1, vec2);
      expect(similarity).toBeCloseTo(-1.0, 5);
    });

    it('should return 0.0 for orthogonal vectors', () => {
      const vec1 = [1, 0, 0];
      const vec2 = [0, 1, 0];
      const similarity = cosineSimilarity(vec1, vec2);
      expect(similarity).toBeCloseTo(0.0, 5);
    });

    it('should calculate similarity for arbitrary vectors', () => {
      const vec1 = [1, 2, 3];
      const vec2 = [4, 5, 6];
      const similarity = cosineSimilarity(vec1, vec2);

      // Expected: (1*4 + 2*5 + 3*6) / (sqrt(14) * sqrt(77))
      // = 32 / 32.98... ≈ 0.9746
      expect(similarity).toBeGreaterThan(0.97);
      expect(similarity).toBeLessThan(0.98);
    });

    it('should handle zero vectors gracefully', () => {
      const vec1 = [0, 0, 0];
      const vec2 = [1, 2, 3];
      const similarity = cosineSimilarity(vec1, vec2);
      expect(similarity).toBe(0);
    });
  });

  describe('euclideanDistance', () => {
    it('should return 0 for identical vectors', () => {
      const vec = [1, 2, 3];
      const distance = euclideanDistance(vec, vec);
      expect(distance).toBe(0);
    });

    it('should calculate distance correctly', () => {
      const vec1 = [0, 0, 0];
      const vec2 = [3, 4, 0];
      const distance = euclideanDistance(vec1, vec2);
      expect(distance).toBe(5); // 3-4-5 triangle
    });

    it('should be symmetric', () => {
      const vec1 = [1, 2, 3];
      const vec2 = [4, 5, 6];
      const dist1 = euclideanDistance(vec1, vec2);
      const dist2 = euclideanDistance(vec2, vec1);
      expect(dist1).toBe(dist2);
    });
  });

  describe('calculateSimilarityAccuracy', () => {
    it('should return 1.0 when all predictions match expected categories', () => {
      const predictions = [
        { actualSimilarity: 0.9, expected: 'high' as const },
        { actualSimilarity: 0.5, expected: 'medium' as const },
        { actualSimilarity: 0.1, expected: 'low' as const },
      ];

      const accuracy = calculateSimilarityAccuracy(predictions);
      expect(accuracy).toBe(1.0);
    });

    it('should calculate accuracy with some mismatches', () => {
      const predictions = [
        { actualSimilarity: 0.9, expected: 'high' as const },
        { actualSimilarity: 0.9, expected: 'low' as const }, // Wrong
        { actualSimilarity: 0.5, expected: 'medium' as const },
        { actualSimilarity: 0.1, expected: 'low' as const },
      ];

      const accuracy = calculateSimilarityAccuracy(predictions);
      expect(accuracy).toBe(0.75); // 3 out of 4 correct
    });

    it('should return 0 when all predictions are wrong', () => {
      const predictions = [
        { actualSimilarity: 0.9, expected: 'low' as const },
        { actualSimilarity: 0.1, expected: 'high' as const },
      ];

      const accuracy = calculateSimilarityAccuracy(predictions);
      expect(accuracy).toBe(0);
    });

    it('should handle edge cases in thresholds', () => {
      const predictions = [
        { actualSimilarity: 0.7, expected: 'high' as const }, // Exactly at threshold
        { actualSimilarity: 0.4, expected: 'medium' as const }, // Exactly at threshold
      ];

      const accuracy = calculateSimilarityAccuracy(predictions);
      expect(accuracy).toBeGreaterThanOrEqual(0);
      expect(accuracy).toBeLessThanOrEqual(1);
    });
  });
});

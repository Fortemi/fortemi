/**
 * Tests for retrieval metrics calculations
 */

import { describe, it, expect } from '@jest/globals';
import {
  calculatePrecisionAtK,
  calculateRecallAtK,
  calculateMRR,
  calculateNDCG,
} from './retrieval.js';

describe('Retrieval Metrics', () => {
  describe('calculatePrecisionAtK', () => {
    it('should calculate perfect precision when all top-K results are relevant', () => {
      const ranked = ['doc1', 'doc2', 'doc3', 'doc4', 'doc5'];
      const relevant = new Set(['doc1', 'doc2', 'doc3', 'doc4', 'doc5']);

      const precision = calculatePrecisionAtK(ranked, relevant, 5);
      expect(precision).toBe(1.0);
    });

    it('should calculate 0.6 precision when 3 of 5 results are relevant', () => {
      const ranked = ['doc1', 'doc2', 'doc3', 'doc4', 'doc5'];
      const relevant = new Set(['doc1', 'doc3', 'doc5']);

      const precision = calculatePrecisionAtK(ranked, relevant, 5);
      expect(precision).toBe(0.6);
    });

    it('should handle K larger than results array', () => {
      const ranked = ['doc1', 'doc2'];
      const relevant = new Set(['doc1', 'doc2']);

      const precision = calculatePrecisionAtK(ranked, relevant, 10);
      expect(precision).toBe(1.0);
    });

    it('should return 0 when no results are relevant', () => {
      const ranked = ['doc1', 'doc2', 'doc3'];
      const relevant = new Set(['doc4', 'doc5']);

      const precision = calculatePrecisionAtK(ranked, relevant, 3);
      expect(precision).toBe(0);
    });
  });

  describe('calculateRecallAtK', () => {
    it('should calculate perfect recall when all relevant docs are in top-K', () => {
      const ranked = ['doc1', 'doc2', 'doc3', 'doc4', 'doc5'];
      const relevant = new Set(['doc1', 'doc2', 'doc3']);

      const recall = calculateRecallAtK(ranked, relevant, 5);
      expect(recall).toBe(1.0);
    });

    it('should calculate 0.5 recall when half of relevant docs are in top-K', () => {
      const ranked = ['doc1', 'doc2', 'doc3', 'doc4', 'doc5'];
      const relevant = new Set(['doc1', 'doc3', 'doc6', 'doc7']);

      const recall = calculateRecallAtK(ranked, relevant, 5);
      expect(recall).toBe(0.5);
    });

    it('should return 0 when no relevant docs are in top-K', () => {
      const ranked = ['doc1', 'doc2', 'doc3'];
      const relevant = new Set(['doc4', 'doc5']);

      const recall = calculateRecallAtK(ranked, relevant, 3);
      expect(recall).toBe(0);
    });
  });

  describe('calculateMRR', () => {
    it('should return 1.0 when first result is relevant', () => {
      const queries = [
        { ranked: ['doc1', 'doc2', 'doc3'], relevant: new Set(['doc1']) },
        { ranked: ['doc4', 'doc5', 'doc6'], relevant: new Set(['doc4']) },
      ];

      const mrr = calculateMRR(queries);
      expect(mrr).toBe(1.0);
    });

    it('should calculate average reciprocal rank correctly', () => {
      const queries = [
        { ranked: ['doc1', 'doc2', 'doc3'], relevant: new Set(['doc2']) }, // 1/2 = 0.5
        { ranked: ['doc4', 'doc5', 'doc6'], relevant: new Set(['doc6']) }, // 1/3 ≈ 0.333
      ];

      const mrr = calculateMRR(queries);
      expect(mrr).toBeCloseTo((0.5 + 0.333) / 2, 2);
    });

    it('should return 0 when no relevant results found', () => {
      const queries = [
        { ranked: ['doc1', 'doc2'], relevant: new Set(['doc3']) },
        { ranked: ['doc4', 'doc5'], relevant: new Set(['doc6']) },
      ];

      const mrr = calculateMRR(queries);
      expect(mrr).toBe(0);
    });
  });

  describe('calculateNDCG', () => {
    it('should return 1.0 for perfect ranking', () => {
      const ranked = ['doc1', 'doc2', 'doc3'];
      const relevanceScores = new Map([
        ['doc1', 3],
        ['doc2', 2],
        ['doc3', 1],
      ]);

      const ndcg = calculateNDCG(ranked, relevanceScores, 3);
      expect(ndcg).toBe(1.0);
    });

    it('should calculate NDCG for imperfect ranking', () => {
      const ranked = ['doc2', 'doc1', 'doc3'];
      const relevanceScores = new Map([
        ['doc1', 3],
        ['doc2', 2],
        ['doc3', 1],
      ]);

      const ndcg = calculateNDCG(ranked, relevanceScores, 3);
      expect(ndcg).toBeGreaterThan(0.9);
      expect(ndcg).toBeLessThan(1.0);
    });

    it('should return 0 when no relevant results', () => {
      const ranked = ['doc1', 'doc2', 'doc3'];
      const relevanceScores = new Map([
        ['doc4', 3],
        ['doc5', 2],
      ]);

      const ndcg = calculateNDCG(ranked, relevanceScores, 3);
      expect(ndcg).toBe(0);
    });
  });
});

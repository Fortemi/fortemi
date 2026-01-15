/**
 * Tests for embedding model evaluator
 */

import { describe, it, expect, beforeEach } from '@jest/globals';
import { evaluateEmbeddingModel } from './embedding.js';
import type { EmbeddingModel, SimilarityPair } from '../models/types.js';
import type { EmbeddingDataset } from './embedding.js';

// Mock embedding model for testing
class MockEmbeddingModel implements EmbeddingModel {
  name = 'mock-model';
  dimensions = 3;

  // Track calls for verification
  embedCalls: string[] = [];
  embedBatchCalls: string[][] = [];

  async embed(text: string): Promise<number[]> {
    this.embedCalls.push(text);

    // Simple deterministic embedding: hash text to vector
    const hash = text.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
    return [
      Math.sin(hash),
      Math.cos(hash),
      Math.sin(hash * 2),
    ];
  }

  async embedBatch(texts: string[]): Promise<number[][]> {
    this.embedBatchCalls.push(texts);
    return Promise.all(texts.map(t => this.embed(t)));
  }
}

describe('Embedding Evaluator', () => {
  let mockModel: MockEmbeddingModel;

  beforeEach(() => {
    mockModel = new MockEmbeddingModel();
  });

  describe('evaluateEmbeddingModel', () => {
    it('should return result with model name', async () => {
      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.modelName).toBe('mock-model');
    });

    it('should calculate overall score', async () => {
      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.overallScore).toBeGreaterThanOrEqual(0);
      expect(result.overallScore).toBeLessThanOrEqual(100);
    });

    it('should include timestamp', async () => {
      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
    });

    it('should calculate similarity metrics', async () => {
      const similarityPairs: SimilarityPair[] = [
        {
          id: 'sim1',
          text1: 'hello world',
          text2: 'hello world',
          expectedSimilarity: 'high',
        },
        {
          id: 'sim2',
          text1: 'foo bar',
          text2: 'completely different',
          expectedSimilarity: 'low',
        },
      ];

      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs,
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.metrics.similarity.accuracy).toBeGreaterThanOrEqual(0);
      expect(result.metrics.similarity.accuracy).toBeLessThanOrEqual(1);
    });

    it('should calculate retrieval metrics', async () => {
      const retrievalQueries: EmbeddingDataset['retrievalQueries'] = [
        {
          id: 'q1',
          query: 'test query',
          documents: [
            { id: 'doc1', content: 'highly relevant document', relevance: 3 },
            { id: 'doc2', content: 'somewhat relevant document', relevance: 2 },
            { id: 'doc3', content: 'not relevant document', relevance: 1 },
          ],
        },
      ];

      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries,
      });

      expect(result.metrics.retrieval.precisionAt5).toBeGreaterThanOrEqual(0);
      expect(result.metrics.retrieval.precisionAt5).toBeLessThanOrEqual(1);
      expect(result.metrics.retrieval.recallAt10).toBeGreaterThanOrEqual(0);
      expect(result.metrics.retrieval.recallAt10).toBeLessThanOrEqual(1);
      expect(result.metrics.retrieval.mrr).toBeGreaterThanOrEqual(0);
      expect(result.metrics.retrieval.mrr).toBeLessThanOrEqual(1);
      expect(result.metrics.retrieval.ndcgAt10).toBeGreaterThanOrEqual(0);
      expect(result.metrics.retrieval.ndcgAt10).toBeLessThanOrEqual(1);
    });

    it('should track latency metrics', async () => {
      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [
          {
            id: 'sim1',
            text1: 'test1',
            text2: 'test2',
            expectedSimilarity: 'high',
          },
        ],
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.metrics.latency.p50).toBeGreaterThanOrEqual(0);
      expect(result.metrics.latency.p95).toBeGreaterThanOrEqual(0);
      expect(result.metrics.latency.p99).toBeGreaterThanOrEqual(0);
      expect(result.metrics.latency.mean).toBeGreaterThanOrEqual(0);
      expect(result.metrics.latency.min).toBeGreaterThanOrEqual(0);
      expect(result.metrics.latency.max).toBeGreaterThanOrEqual(0);
    });

    it('should calculate throughput', async () => {
      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [
          {
            id: 'sim1',
            text1: 'test1',
            text2: 'test2',
            expectedSimilarity: 'high',
          },
        ],
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.metrics.throughput).toBeGreaterThanOrEqual(0);
    });

    it('should handle empty datasets gracefully', async () => {
      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries: [],
      });

      expect(result.modelName).toBe('mock-model');
      expect(result.overallScore).toBeGreaterThanOrEqual(0);
    });

    it('should use both similarity and dissimilarity pairs', async () => {
      const similarityPairs: SimilarityPair[] = [
        {
          id: 'sim1',
          text1: 'very similar',
          text2: 'very similar',
          expectedSimilarity: 'high',
        },
      ];

      const dissimilarityPairs: SimilarityPair[] = [
        {
          id: 'dis1',
          text1: 'completely different topic A',
          text2: 'completely different topic B',
          expectedSimilarity: 'low',
        },
      ];

      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs,
        dissimilarityPairs,
        retrievalQueries: [],
      });

      expect(result.metrics.similarity.accuracy).toBeGreaterThanOrEqual(0);
      expect(result.metrics.similarity.accuracy).toBeLessThanOrEqual(1);
    });

    it('should handle multiple retrieval queries', async () => {
      const retrievalQueries: EmbeddingDataset['retrievalQueries'] = [
        {
          id: 'q1',
          query: 'first query',
          documents: [
            { id: 'doc1', content: 'relevant', relevance: 3 },
            { id: 'doc2', content: 'not relevant', relevance: 1 },
          ],
        },
        {
          id: 'q2',
          query: 'second query',
          documents: [
            { id: 'doc3', content: 'relevant', relevance: 3 },
            { id: 'doc4', content: 'not relevant', relevance: 1 },
          ],
        },
      ];

      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries,
      });

      expect(result.metrics.retrieval.mrr).toBeGreaterThanOrEqual(0);
      expect(result.metrics.retrieval.mrr).toBeLessThanOrEqual(1);
    });

    it('should calculate precision@5 and precision@10', async () => {
      const retrievalQueries: EmbeddingDataset['retrievalQueries'] = [
        {
          id: 'q1',
          query: 'test',
          documents: Array.from({ length: 15 }, (_, i) => ({
            id: `doc${i}`,
            content: `document ${i}`,
            relevance: i < 3 ? 3 : 1,
          })),
        },
      ];

      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries,
      });

      expect(result.metrics.retrieval.precisionAt5).toBeDefined();
      expect(result.metrics.retrieval.precisionAt10).toBeDefined();
    });

    it('should calculate recall@5 and recall@10', async () => {
      const retrievalQueries: EmbeddingDataset['retrievalQueries'] = [
        {
          id: 'q1',
          query: 'test',
          documents: Array.from({ length: 20 }, (_, i) => ({
            id: `doc${i}`,
            content: `document ${i}`,
            relevance: i < 8 ? 3 : 1,
          })),
        },
      ];

      const result = await evaluateEmbeddingModel(mockModel, {
        similarityPairs: [],
        dissimilarityPairs: [],
        retrievalQueries,
      });

      expect(result.metrics.retrieval.recallAt5).toBeDefined();
      expect(result.metrics.retrieval.recallAt10).toBeDefined();
    });
  });

  describe('loadEmbeddingDatasets', () => {
    it('should load all dataset files from directory', async () => {
      const { loadEmbeddingDatasets } = await import('./embedding.js');

      const datasets = await loadEmbeddingDatasets(
        '/home/roctinam/dev/matric-memory/evals/datasets/embedding_tests'
      );

      expect(datasets.similarityPairs.length).toBeGreaterThan(0);
      expect(datasets.dissimilarityPairs.length).toBeGreaterThan(0);
      expect(datasets.retrievalQueries.length).toBeGreaterThan(0);
    });

    it('should validate dataset schema', async () => {
      const { loadEmbeddingDatasets } = await import('./embedding.js');

      const datasets = await loadEmbeddingDatasets(
        '/home/roctinam/dev/matric-memory/evals/datasets/embedding_tests'
      );

      // Validate similarity pairs structure
      datasets.similarityPairs.forEach((pair: SimilarityPair) => {
        expect(pair).toHaveProperty('id');
        expect(pair).toHaveProperty('text1');
        expect(pair).toHaveProperty('text2');
        expect(pair).toHaveProperty('expectedSimilarity');
        expect(['high', 'medium', 'low']).toContain(pair.expectedSimilarity);
      });

      // Validate retrieval queries structure
      datasets.retrievalQueries.forEach((query: EmbeddingDataset['retrievalQueries'][0]) => {
        expect(query).toHaveProperty('id');
        expect(query).toHaveProperty('query');
        expect(query).toHaveProperty('documents');
        expect(Array.isArray(query.documents)).toBe(true);
      });
    });
  });
});

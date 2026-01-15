/**
 * Tests for JSON reporter
 */

import { describe, it, expect } from '@jest/globals';
import { generateJSONReport } from './json.js';
import type { EvaluationReport } from '../models/types.js';

describe('generateJSONReport', () => {
  it('should generate valid JSON output', () => {
    const mockReport: EvaluationReport = {
      meta: {
        timestamp: '2026-01-14T22:00:00Z',
        durationMs: 5000,
        modelsTested: 2,
        scenariosRun: 10,
      },
      embeddingResults: {
        'nomic-embed-text': {
          modelName: 'nomic-embed-text',
          overallScore: 78.5,
          metrics: {
            retrieval: {
              precisionAt5: 0.82,
              precisionAt10: 0.75,
              recallAt5: 0.65,
              recallAt10: 0.88,
              mrr: 0.88,
              ndcgAt10: 0.79,
            },
            similarity: {
              accuracy: 0.91,
              meanAbsoluteError: 0.08,
            },
            latency: {
              p50: 42,
              p95: 65,
              p99: 89,
              mean: 45,
              min: 35,
              max: 120,
            },
            throughput: 22.5,
          },
          timestamp: '2026-01-14T22:00:00Z',
        },
      },
      llmResults: {
        'gpt-4o-mini': {
          modelName: 'gpt-4o-mini',
          overallScore: 85.2,
          dimensions: {
            revisionQuality: 88.5,
            titleQuality: 82.0,
            contextQuality: 86.5,
            instructionFollowing: 90.0,
            efficiency: 75.0,
          },
          metrics: {
            latency: {
              p50: 450,
              p95: 850,
              p99: 1200,
              mean: 500,
              min: 350,
              max: 1500,
            },
            tokensPerSecond: 45.5,
          },
          timestamp: '2026-01-14T22:00:00Z',
        },
      },
      recommendations: {
        bestEmbedding: 'nomic-embed-text',
        bestLLMQuality: 'gpt-4o-mini',
        bestLLMBalanced: 'gpt-4o-mini',
        bestLLMSpeed: 'gpt-4o-mini',
      },
    };

    const result = generateJSONReport(mockReport);

    // Should be valid JSON
    expect(() => JSON.parse(result)).not.toThrow();

    // Should preserve all data
    const parsed = JSON.parse(result);
    expect(parsed.meta.timestamp).toBe('2026-01-14T22:00:00Z');
    expect(parsed.meta.durationMs).toBe(5000);
    expect(parsed.embeddingResults['nomic-embed-text'].overallScore).toBe(78.5);
    expect(parsed.llmResults['gpt-4o-mini'].overallScore).toBe(85.2);
  });

  it('should use 2-space indentation', () => {
    const mockReport: EvaluationReport = {
      meta: {
        timestamp: '2026-01-14T22:00:00Z',
        durationMs: 1000,
        modelsTested: 1,
        scenariosRun: 5,
      },
      embeddingResults: {},
      llmResults: {},
      recommendations: {},
    };

    const result = generateJSONReport(mockReport);

    // Check for 2-space indentation
    expect(result).toContain('  "meta": {');
    expect(result).toContain('    "timestamp": ');
  });

  it('should handle empty results', () => {
    const mockReport: EvaluationReport = {
      meta: {
        timestamp: '2026-01-14T22:00:00Z',
        durationMs: 0,
        modelsTested: 0,
        scenariosRun: 0,
      },
      embeddingResults: {},
      llmResults: {},
      recommendations: {},
    };

    const result = generateJSONReport(mockReport);
    const parsed = JSON.parse(result);

    expect(parsed.embeddingResults).toEqual({});
    expect(parsed.llmResults).toEqual({});
    expect(parsed.recommendations).toEqual({});
  });

  it('should include all metric fields', () => {
    const mockReport: EvaluationReport = {
      meta: {
        timestamp: '2026-01-14T22:00:00Z',
        durationMs: 5000,
        modelsTested: 1,
        scenariosRun: 10,
      },
      embeddingResults: {
        'test-model': {
          modelName: 'test-model',
          overallScore: 80.0,
          metrics: {
            retrieval: {
              precisionAt5: 0.8,
              precisionAt10: 0.7,
              recallAt5: 0.6,
              recallAt10: 0.85,
              mrr: 0.85,
              ndcgAt10: 0.75,
            },
            similarity: {
              accuracy: 0.9,
            },
            latency: {
              p50: 50,
              p95: 70,
              p99: 90,
              mean: 55,
              min: 40,
              max: 100,
            },
            throughput: 20.0,
          },
          timestamp: '2026-01-14T22:00:00Z',
        },
      },
      llmResults: {},
      recommendations: {},
    };

    const result = generateJSONReport(mockReport);
    const parsed = JSON.parse(result);

    const metrics = parsed.embeddingResults['test-model'].metrics;
    expect(metrics.retrieval).toHaveProperty('precisionAt5');
    expect(metrics.retrieval).toHaveProperty('precisionAt10');
    expect(metrics.retrieval).toHaveProperty('recallAt5');
    expect(metrics.retrieval).toHaveProperty('recallAt10');
    expect(metrics.retrieval).toHaveProperty('mrr');
    expect(metrics.retrieval).toHaveProperty('ndcgAt10');
    expect(metrics.latency).toHaveProperty('p50');
    expect(metrics.latency).toHaveProperty('p95');
    expect(metrics.latency).toHaveProperty('p99');
  });
});

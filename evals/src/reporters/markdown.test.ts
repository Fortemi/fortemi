/**
 * Tests for Markdown reporter
 */

import { describe, it, expect } from '@jest/globals';
import { generateMarkdownReport } from './markdown.js';
import type { EvaluationReport } from '../models/types.js';

describe('generateMarkdownReport', () => {
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
      'all-minilm-l6-v2': {
        modelName: 'all-minilm-l6-v2',
        overallScore: 72.3,
        metrics: {
          retrieval: {
            precisionAt5: 0.75,
            precisionAt10: 0.68,
            recallAt5: 0.60,
            recallAt10: 0.82,
            mrr: 0.80,
            ndcgAt10: 0.72,
          },
          similarity: {
            accuracy: 0.88,
          },
          latency: {
            p50: 25,
            p95: 38,
            p99: 52,
            mean: 28,
            min: 20,
            max: 65,
          },
          throughput: 35.8,
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
      'claude-3-5-haiku': {
        modelName: 'claude-3-5-haiku',
        overallScore: 82.8,
        dimensions: {
          revisionQuality: 85.0,
          titleQuality: 80.5,
          contextQuality: 84.0,
          instructionFollowing: 88.0,
          efficiency: 82.0,
        },
        metrics: {
          latency: {
            p50: 350,
            p95: 650,
            p99: 950,
            mean: 400,
            min: 280,
            max: 1200,
          },
          tokensPerSecond: 52.3,
        },
        timestamp: '2026-01-14T22:00:00Z',
      },
    },
    recommendations: {
      bestEmbedding: 'nomic-embed-text',
      bestLLMQuality: 'gpt-4o-mini',
      bestLLMBalanced: 'gpt-4o-mini',
      bestLLMSpeed: 'claude-3-5-haiku',
    },
  };

  it('should include executive summary section', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('# Model Evaluation Report');
    expect(result).toContain('## Executive Summary');
  });

  it('should include recommendations in executive summary', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('nomic-embed-text');
    expect(result).toContain('gpt-4o-mini');
    expect(result).toContain('claude-3-5-haiku');
  });

  it('should include embedding comparison table', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('## Embedding Models');
    expect(result).toContain('| Model |');
    expect(result).toContain('nomic-embed-text');
    expect(result).toContain('all-minilm-l6-v2');
    expect(result).toContain('78.5');
    expect(result).toContain('72.3');
  });

  it('should include LLM comparison table', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('## LLM Models');
    expect(result).toContain('| Model |');
    expect(result).toContain('gpt-4o-mini');
    expect(result).toContain('claude-3-5-haiku');
    expect(result).toContain('85.2');
    expect(result).toContain('82.8');
  });

  it('should include per-model detailed breakdowns', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('### nomic-embed-text');
    expect(result).toContain('### all-minilm-l6-v2');
    expect(result).toContain('### gpt-4o-mini');
    expect(result).toContain('### claude-3-5-haiku');
  });

  it('should include methodology section', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('## Methodology');
    expect(result).toContain('weight');
  });

  it('should format percentages correctly', () => {
    const result = generateMarkdownReport(mockReport);

    // Should show percentages with 1 decimal
    expect(result).toMatch(/82\.0%/);
    expect(result).toMatch(/91\.0%/);
  });

  it('should format latency with units', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toMatch(/\d+ms/);
  });

  it('should include retrieval metrics in tables', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('P@5');
    expect(result).toContain('MRR');
    expect(result).toContain('NDCG');
  });

  it('should include LLM dimension scores', () => {
    const result = generateMarkdownReport(mockReport);

    expect(result).toContain('Revision Quality');
    expect(result).toContain('Title Quality');
    expect(result).toContain('Context Quality');
    expect(result).toContain('Instruction Following');
    expect(result).toContain('Efficiency');
  });

  it('should handle empty results gracefully', () => {
    const emptyReport: EvaluationReport = {
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

    const result = generateMarkdownReport(emptyReport);

    expect(result).toContain('# Model Evaluation Report');
    expect(result).toContain('No embedding models were evaluated');
    expect(result).toContain('No LLM models were evaluated');
  });

  it('should sort models by score in tables', () => {
    const result = generateMarkdownReport(mockReport);

    // Extract the embedding table section
    const embeddingTableStart = result.indexOf('## Embedding Models');
    const embeddingTableEnd = result.indexOf('### Detailed Results');
    const embeddingTable = result.substring(embeddingTableStart, embeddingTableEnd);

    // Find indices within the table section
    const nomicInTable = embeddingTable.indexOf('nomic-embed-text');
    const minilmInTable = embeddingTable.indexOf('all-minilm-l6-v2');

    // Higher score (nomic: 78.5) should appear before lower score (minilm: 72.3)
    expect(nomicInTable).toBeLessThan(minilmInTable);
    expect(nomicInTable).toBeGreaterThan(0); // Ensure it's found
    expect(minilmInTable).toBeGreaterThan(0); // Ensure it's found
  });

  it('should include metadata in report', () => {
    const result = generateMarkdownReport(mockReport);

    // Date is formatted as "January 14, 2026" not "2026-01-14"
    expect(result).toContain('January 14, 2026');
    expect(result).toContain('5.0s'); // 5000ms formatted
    expect(result).toContain('2'); // models tested
    expect(result).toContain('10'); // scenarios run
  });

  it('should use markdown table syntax correctly', () => {
    const result = generateMarkdownReport(mockReport);

    // Should have table headers with pipes
    expect(result).toMatch(/\| Model \|.*\|/);

    // Should have separator line with dashes
    expect(result).toMatch(/\|[-]+\|/);
  });
});

/**
 * Tests for title generation evaluator
 */

import { describe, it, expect, jest, beforeEach } from '@jest/globals';
import { evaluateTitle } from './title.js';
import type { GenerationModel, EmbeddingModel } from '../models/types.js';
import { writeFile, mkdir } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';

describe('evaluateTitle', () => {
  let testDatasetPath: string;

  beforeEach(async () => {
    // Create a temporary dataset with just 2 cases for testing
    const testDataset = [
      {
        id: 1,
        content: 'Test content about async Rust and lifetimes',
        ideal_titles: ['Async Rust Lifetime Management', 'Managing Lifetimes in Async Rust'],
        bad_titles: ['Rust', 'Programming'],
      },
      {
        id: 2,
        content: 'Notes on Docker networking and containers',
        ideal_titles: ['Docker Container Networking', 'Fixing Docker Network Issues'],
        bad_titles: ['Docker', 'Containers'],
      },
    ];

    const tmpDir = join(tmpdir(), 'evals-test');
    await mkdir(tmpDir, { recursive: true });
    testDatasetPath = join(tmpDir, 'title-test.json');
    await writeFile(testDatasetPath, JSON.stringify(testDataset));
  });

  it('should evaluate title generation for a single model', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Managing Lifetimes in Async Rust',
        tokensGenerated: 10,
        totalTime: 500,
      }),
    };

    const embedding = new Array(384).fill(0.1);
    const mockEmbedding: EmbeddingModel = {
      name: 'embedding-model',
      dimensions: 384,
      embed: jest.fn<EmbeddingModel['embed']>().mockResolvedValue(embedding),
      embedBatch: jest.fn<EmbeddingModel['embedBatch']>().mockResolvedValue([]),
    };

    const result = await evaluateTitle({
      models: [mockModel],
      embeddingModel: mockEmbedding,
      datasetPath: testDatasetPath,
    });

    expect(result).toHaveLength(1);
    expect(result[0].modelName).toBe('test-model');
    expect(result[0].scores.semanticSimilarity).toBeGreaterThanOrEqual(0);
    expect(result[0].scores.semanticSimilarity).toBeLessThanOrEqual(1);
    expect(result[0].scores.formatCompliance).toBeGreaterThanOrEqual(0);
    expect(result[0].scores.formatCompliance).toBeLessThanOrEqual(1);
    expect(result[0].scores.overallScore).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p50).toBeGreaterThanOrEqual(0);
    expect(result[0].caseResults).toHaveLength(2);
  });

  it('should check format compliance (3-8 words, no quotes)', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>()
        .mockResolvedValueOnce({
          text: 'Valid Title Format Here',
          tokensGenerated: 10,
          totalTime: 500,
        })
        .mockResolvedValueOnce({
          text: '"Invalid With Quotes"',
          tokensGenerated: 10,
          totalTime: 500,
        }),
    };

    const embedding = new Array(384).fill(0.1);
    const mockEmbedding: EmbeddingModel = {
      name: 'embedding-model',
      dimensions: 384,
      embed: jest.fn<EmbeddingModel['embed']>().mockResolvedValue(embedding),
      embedBatch: jest.fn<EmbeddingModel['embedBatch']>().mockResolvedValue([]),
    };

    const result = await evaluateTitle({
      models: [mockModel],
      embeddingModel: mockEmbedding,
      datasetPath: testDatasetPath,
    });

    expect(result).toHaveLength(1);
    // Format compliance should be 50% (1/2 valid)
    expect(result[0].scores.formatCompliance).toBe(0.5);
    expect(result[0].caseResults[0].formatCompliant).toBe(true);
    expect(result[0].caseResults[1].formatCompliant).toBe(false);
  });

  it('should calculate semantic similarity with ideal titles', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Semantic Search Vector Databases',
        tokensGenerated: 10,
        totalTime: 500,
      }),
    };

    const embedding1 = new Array(384).fill(0).map(() => Math.random());
    const embedding2 = new Array(384).fill(0).map(() => Math.random());

    let callCount = 0;
    const mockEmbedding: EmbeddingModel = {
      name: 'embedding-model',
      dimensions: 384,
      embed: jest.fn<EmbeddingModel['embed']>().mockImplementation(async () => {
        callCount++;
        return callCount % 2 === 0 ? embedding2 : embedding1;
      }),
      embedBatch: jest.fn<EmbeddingModel['embedBatch']>().mockResolvedValue([]),
    };

    const result = await evaluateTitle({
      models: [mockModel],
      embeddingModel: mockEmbedding,
      datasetPath: testDatasetPath,
    });

    expect(result[0].scores.semanticSimilarity).toBeGreaterThanOrEqual(0);
    expect(result[0].scores.semanticSimilarity).toBeLessThanOrEqual(1);
  });

  it('should handle multiple models', async () => {
    const mockModel1: GenerationModel = {
      name: 'model-1',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Good Title Here',
        tokensGenerated: 10,
        totalTime: 500,
      }),
    };

    const mockModel2: GenerationModel = {
      name: 'model-2',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Another Good Title',
        tokensGenerated: 10,
        totalTime: 600,
      }),
    };

    const embedding = new Array(384).fill(0.1);
    const mockEmbedding: EmbeddingModel = {
      name: 'embedding-model',
      dimensions: 384,
      embed: jest.fn<EmbeddingModel['embed']>().mockResolvedValue(embedding),
      embedBatch: jest.fn<EmbeddingModel['embedBatch']>().mockResolvedValue([]),
    };

    const result = await evaluateTitle({
      models: [mockModel1, mockModel2],
      embeddingModel: mockEmbedding,
      datasetPath: testDatasetPath,
    });

    expect(result).toHaveLength(2);
    expect(result[0].modelName).toBe('model-1');
    expect(result[1].modelName).toBe('model-2');
  });

  it('should use correct title prompt format', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Test Title',
        tokensGenerated: 10,
        totalTime: 500,
      }),
    };

    const embedding = new Array(384).fill(0.1);
    const mockEmbedding: EmbeddingModel = {
      name: 'embedding-model',
      dimensions: 384,
      embed: jest.fn<EmbeddingModel['embed']>().mockResolvedValue(embedding),
      embedBatch: jest.fn<EmbeddingModel['embedBatch']>().mockResolvedValue([]),
    };

    await evaluateTitle({
      models: [mockModel],
      embeddingModel: mockEmbedding,
      datasetPath: testDatasetPath,
    });

    expect(mockModel.generate).toHaveBeenCalled();
    const call = (mockModel.generate as jest.MockedFunction<typeof mockModel.generate>).mock.calls[0];
    expect(call[0]).toContain('Generate a concise 3-8 word title');
    expect(call[0]).toContain('Return only the title, no quotes');
  });

  it('should track latency metrics', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Test Title Here',
        tokensGenerated: 10,
        totalTime: 750,
      }),
    };

    const embedding = new Array(384).fill(0.1);
    const mockEmbedding: EmbeddingModel = {
      name: 'embedding-model',
      dimensions: 384,
      embed: jest.fn<EmbeddingModel['embed']>().mockResolvedValue(embedding),
      embedBatch: jest.fn<EmbeddingModel['embedBatch']>().mockResolvedValue([]),
    };

    const result = await evaluateTitle({
      models: [mockModel],
      embeddingModel: mockEmbedding,
      datasetPath: testDatasetPath,
    });

    expect(result[0].latency.mean).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p50).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p95).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p99).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.min).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.max).toBeGreaterThanOrEqual(0);
  });
});

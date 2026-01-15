/**
 * Tests for revision quality evaluator
 */

import { describe, it, expect, jest, beforeEach } from '@jest/globals';
import { evaluateRevision } from './revision.js';
import type { GenerationModel } from '../models/types.js';
import { writeFile, mkdir } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';

describe('evaluateRevision', () => {
  let testDatasetPath: string;

  beforeEach(async () => {
    // Create a temporary dataset with just 2 cases for testing
    const testDataset = [
      {
        id: 1,
        original_content: 'test content 1',
        expected_concepts: ['concept1', 'concept2'],
        min_improvement: true,
      },
      {
        id: 2,
        original_content: 'test content 2',
        expected_concepts: ['concept3', 'concept4'],
        min_improvement: true,
      },
    ];

    const tmpDir = join(tmpdir(), 'evals-test');
    await mkdir(tmpDir, { recursive: true });
    testDatasetPath = join(tmpDir, 'revision-test.json');
    await writeFile(testDatasetPath, JSON.stringify(testDataset));
  });

  it('should evaluate revision quality for a single model', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: '## Rust Lifetime Management\n\nWorking with async Rust and Redis...',
        tokensGenerated: 150,
        totalTime: 1000,
        timeToFirstToken: 50,
      }),
    };

    const mockJudge: GenerationModel = {
      name: 'judge-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: JSON.stringify({
          information_preservation: 95,
          structure_enhancement: 90,
          no_hallucination: 100,
          readability: 92,
        }),
        tokensGenerated: 50,
        totalTime: 500,
      }),
    };

    const result = await evaluateRevision({
      models: [mockModel],
      judge: mockJudge,
      datasetPath: testDatasetPath,
    });

    expect(result).toHaveLength(1);
    expect(result[0].modelName).toBe('test-model');
    expect(result[0].scores.averageInformationPreservation).toBeGreaterThan(0);
    expect(result[0].scores.averageStructureEnhancement).toBeGreaterThan(0);
    expect(result[0].scores.averageNoHallucination).toBeGreaterThan(0);
    expect(result[0].scores.averageReadability).toBeGreaterThan(0);
    expect(result[0].scores.overallScore).toBeGreaterThan(0);
    expect(result[0].latency.p50).toBeGreaterThanOrEqual(0);
    expect(result[0].totalTokens).toBeGreaterThan(0);
    expect(result[0].caseResults).toHaveLength(2);
  });

  it('should handle multiple models', async () => {
    const mockModel1: GenerationModel = {
      name: 'model-1',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Revised content',
        tokensGenerated: 100,
        totalTime: 1000,
      }),
    };

    const mockModel2: GenerationModel = {
      name: 'model-2',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Different revision',
        tokensGenerated: 120,
        totalTime: 1200,
      }),
    };

    const mockJudge: GenerationModel = {
      name: 'judge',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: JSON.stringify({
          information_preservation: 85,
          structure_enhancement: 80,
          no_hallucination: 95,
          readability: 88,
        }),
        tokensGenerated: 50,
        totalTime: 500,
      }),
    };

    const result = await evaluateRevision({
      models: [mockModel1, mockModel2],
      judge: mockJudge,
      datasetPath: testDatasetPath,
    });

    expect(result).toHaveLength(2);
    expect(result[0].modelName).toBe('model-1');
    expect(result[1].modelName).toBe('model-2');
  });

  it('should track token counts and latency', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Revised',
        tokensGenerated: 200,
        totalTime: 2000,
      }),
    };

    const mockJudge: GenerationModel = {
      name: 'judge',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: JSON.stringify({
          information_preservation: 90,
          structure_enhancement: 85,
          no_hallucination: 100,
          readability: 90,
        }),
        tokensGenerated: 40,
        totalTime: 400,
      }),
    };

    const result = await evaluateRevision({
      models: [mockModel],
      judge: mockJudge,
      datasetPath: testDatasetPath,
    });

    expect(result[0].totalTokens).toBe(400); // 200 per case * 2 cases
    expect(result[0].latency.mean).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p50).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p95).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.p99).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.min).toBeGreaterThanOrEqual(0);
    expect(result[0].latency.max).toBeGreaterThanOrEqual(0);
  });

  it('should handle judge model returning invalid JSON', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Revised content',
        tokensGenerated: 100,
        totalTime: 1000,
      }),
    };

    const mockJudge: GenerationModel = {
      name: 'judge',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Invalid JSON response',
        tokensGenerated: 10,
        totalTime: 100,
      }),
    };

    const result = await evaluateRevision({
      models: [mockModel],
      judge: mockJudge,
      datasetPath: testDatasetPath,
    });

    // Should still return results with default scores (0)
    expect(result).toHaveLength(1);
    expect(result[0].scores.overallScore).toBe(0);
  });

  it('should use correct revision prompt format', async () => {
    const mockModel: GenerationModel = {
      name: 'test-model',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: 'Revised',
        tokensGenerated: 100,
        totalTime: 1000,
      }),
    };

    const mockJudge: GenerationModel = {
      name: 'judge',
      generate: jest.fn<GenerationModel['generate']>().mockResolvedValue({
        text: JSON.stringify({
          information_preservation: 90,
          structure_enhancement: 85,
          no_hallucination: 100,
          readability: 90,
        }),
        tokensGenerated: 40,
        totalTime: 400,
      }),
    };

    await evaluateRevision({
      models: [mockModel],
      judge: mockJudge,
      datasetPath: testDatasetPath,
    });

    expect(mockModel.generate).toHaveBeenCalled();
    const call = (mockModel.generate as jest.MockedFunction<typeof mockModel.generate>).mock.calls[0];
    expect(call[0]).toContain('Enhance this note with better structure and clarity');
    expect(call[0]).toContain('Add markdown formatting');
    expect(call[0]).toContain('Do not invent facts');
  });
});

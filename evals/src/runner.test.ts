/**
 * Tests for evaluation runner
 */

import { describe, it, expect, beforeEach } from '@jest/globals';
import { filterEmbeddingModels, filterLLMModels, createOutputDirectory } from './runner.js';
import * as fs from 'fs';
import * as path from 'path';

describe('filterEmbeddingModels', () => {
  it('should identify embedding models', () => {
    const models = [
      { name: 'nomic-embed-text', size: 1000000 },
      { name: 'mxbai-embed-large', size: 2000000 },
      { name: 'qwen2.5:14b', size: 9000000000 },
      { name: 'llama3.1:8b', size: 5000000000 },
    ];

    const embeddingModels = filterEmbeddingModels(models);

    expect(embeddingModels).toHaveLength(2);
    expect(embeddingModels.map((m) => m.name)).toEqual([
      'nomic-embed-text',
      'mxbai-embed-large',
    ]);
  });

  it('should handle empty input', () => {
    expect(filterEmbeddingModels([])).toEqual([]);
  });

  it('should filter by specific models if provided', () => {
    const models = [
      { name: 'nomic-embed-text', size: 1000000 },
      { name: 'mxbai-embed-large', size: 2000000 },
    ];

    const filtered = filterEmbeddingModels(models, ['nomic-embed-text']);
    expect(filtered).toHaveLength(1);
    expect(filtered[0].name).toBe('nomic-embed-text');
  });
});

describe('filterLLMModels', () => {
  it('should identify LLM models', () => {
    const models = [
      { name: 'nomic-embed-text', size: 1000000 },
      { name: 'qwen2.5:14b', size: 9000000000 },
      { name: 'llama3.1:8b', size: 5000000000 },
      { name: 'mistral:latest', size: 4000000000 },
    ];

    const llmModels = filterLLMModels(models);

    expect(llmModels.length).toBeGreaterThan(0);
    // Should not include embedding models
    expect(llmModels.map((m) => m.name)).not.toContain('nomic-embed-text');
  });

  it('should handle empty input', () => {
    expect(filterLLMModels([])).toEqual([]);
  });

  it('should filter by specific models if provided', () => {
    const models = [
      { name: 'qwen2.5:14b', size: 9000000000 },
      { name: 'llama3.1:8b', size: 5000000000 },
    ];

    const filtered = filterLLMModels(models, ['qwen2.5:14b']);
    expect(filtered).toHaveLength(1);
    expect(filtered[0].name).toBe('qwen2.5:14b');
  });
});

describe('createOutputDirectory', () => {
  const baseDir = '/tmp/matric-evals-test';

  beforeEach(() => {
    // Clean up test directory
    if (fs.existsSync(baseDir)) {
      fs.rmSync(baseDir, { recursive: true });
    }
  });

  it('should create timestamped directory', () => {
    const dir = createOutputDirectory(baseDir);

    expect(fs.existsSync(dir)).toBe(true);
    expect(dir).toMatch(/eval-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}/);
  });

  it('should create nested directories', () => {
    const dir = createOutputDirectory(baseDir);
    const rawDir = path.join(dir, 'raw');

    expect(fs.existsSync(rawDir)).toBe(true);
  });

  it('should return absolute path', () => {
    const dir = createOutputDirectory(baseDir);
    expect(path.isAbsolute(dir)).toBe(true);
  });
});

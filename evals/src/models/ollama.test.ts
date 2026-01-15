/**
 * Tests for Ollama API client
 */

import { describe, it, expect, jest, beforeEach } from '@jest/globals';
import { OllamaEmbeddingModel, OllamaGenerationModel } from './ollama.js';

// Mock fetch globally
global.fetch = jest.fn() as jest.MockedFunction<typeof fetch>;

describe('OllamaEmbeddingModel', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('should create instance with correct properties', () => {
    const model = new OllamaEmbeddingModel('nomic-embed-text', 768);

    expect(model.name).toBe('nomic-embed-text');
    expect(model.dimensions).toBe(768);
  });

  it('should call embed API correctly', async () => {
    const mockResponse = {
      embeddings: [[0.1, 0.2, 0.3]],
    };

    (global.fetch as jest.MockedFunction<typeof fetch>).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response);

    const model = new OllamaEmbeddingModel('nomic-embed-text', 768);
    const embedding = await model.embed('test text');

    expect(embedding).toEqual([0.1, 0.2, 0.3]);
    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:11434/api/embed',
      expect.objectContaining({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'nomic-embed-text',
          input: 'test text',
        }),
      })
    );
  });

  it('should handle embed API errors', async () => {
    (global.fetch as jest.MockedFunction<typeof fetch>).mockResolvedValueOnce({
      ok: false,
      statusText: 'Internal Server Error',
    } as Response);

    const model = new OllamaEmbeddingModel('nomic-embed-text', 768);

    await expect(model.embed('test text')).rejects.toThrow('Ollama embed failed');
  });

  it('should call embedBatch API correctly', async () => {
    const mockResponse = {
      embeddings: [
        [0.1, 0.2, 0.3],
        [0.4, 0.5, 0.6],
      ],
    };

    (global.fetch as jest.MockedFunction<typeof fetch>).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response);

    const model = new OllamaEmbeddingModel('nomic-embed-text', 768);
    const embeddings = await model.embedBatch(['text1', 'text2']);

    expect(embeddings).toEqual([
      [0.1, 0.2, 0.3],
      [0.4, 0.5, 0.6],
    ]);
  });
});

describe('OllamaGenerationModel', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('should create instance with correct properties', () => {
    const model = new OllamaGenerationModel('qwen2.5:14b');

    expect(model.name).toBe('qwen2.5:14b');
  });

  it('should call generate API correctly', async () => {
    const mockResponse = {
      response: 'Generated text',
      eval_count: 15,
      total_duration: 1000000000, // 1 second in nanoseconds
    };

    (global.fetch as jest.MockedFunction<typeof fetch>).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response);

    const model = new OllamaGenerationModel('qwen2.5:14b');
    const result = await model.generate('test prompt');

    expect(result.text).toBe('Generated text');
    expect(result.tokensGenerated).toBe(15);
    // totalTime includes fetch time, so just verify it's a reasonable number
    expect(result.totalTime).toBeGreaterThanOrEqual(0);
    expect(result.totalTime).toBeLessThan(10000); // Should be < 10 seconds

    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:11434/api/generate',
      expect.objectContaining({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'qwen2.5:14b',
          prompt: 'test prompt',
          stream: false,
        }),
      })
    );
  });

  it('should handle generate API errors', async () => {
    (global.fetch as jest.MockedFunction<typeof fetch>).mockResolvedValueOnce({
      ok: false,
      statusText: 'Internal Server Error',
    } as Response);

    const model = new OllamaGenerationModel('qwen2.5:14b');

    await expect(model.generate('test prompt')).rejects.toThrow('Ollama generate failed');
  });

  it('should pass generation options correctly', async () => {
    const mockResponse = {
      response: 'Generated text',
      eval_count: 15,
      total_duration: 1000000000,
    };

    (global.fetch as jest.MockedFunction<typeof fetch>).mockResolvedValueOnce({
      ok: true,
      json: async () => mockResponse,
    } as Response);

    const model = new OllamaGenerationModel('qwen2.5:14b');
    await model.generate('test prompt', {
      temperature: 0.7,
      maxTokens: 100,
    });

    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:11434/api/generate',
      expect.objectContaining({
        body: expect.stringContaining('"temperature":0.7'),
      })
    );
  });
});

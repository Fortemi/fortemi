/**
 * Ollama API client for embeddings and text generation
 */

import type {
  EmbeddingModel,
  GenerationModel,
  GenerationOptions,
  GenerationResult,
} from './types.js';

const OLLAMA_BASE_URL = process.env.OLLAMA_BASE_URL || 'http://localhost:11434';

// Response type definitions for Ollama API
interface OllamaEmbedResponse {
  embeddings: number[][];
}

interface OllamaGenerateResponse {
  response: string;
  eval_count?: number;
  total_duration?: number;
  prompt_eval_duration?: number;
}

interface OllamaTagsResponse {
  models: Array<{
    name: string;
    size: number;
  }>;
}

/**
 * Ollama embedding model client
 */
export class OllamaEmbeddingModel implements EmbeddingModel {
  constructor(
    public readonly name: string,
    public readonly dimensions: number
  ) {}

  async embed(text: string): Promise<number[]> {
    const response = await fetch(`${OLLAMA_BASE_URL}/api/embed`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: this.name,
        input: text,
      }),
    });

    if (!response.ok) {
      throw new Error(
        `Ollama embed failed: ${response.statusText}`
      );
    }

    const data = (await response.json()) as OllamaEmbedResponse;
    return data.embeddings[0];
  }

  async embedBatch(texts: string[]): Promise<number[][]> {
    const response = await fetch(`${OLLAMA_BASE_URL}/api/embed`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: this.name,
        input: texts,
      }),
    });

    if (!response.ok) {
      throw new Error(
        `Ollama embed failed: ${response.statusText}`
      );
    }

    const data = (await response.json()) as OllamaEmbedResponse;
    return data.embeddings;
  }
}

/**
 * Ollama generation model client
 */
export class OllamaGenerationModel implements GenerationModel {
  constructor(public readonly name: string) {}

  async generate(
    prompt: string,
    options?: GenerationOptions
  ): Promise<GenerationResult> {
    const startTime = Date.now();

    const requestBody: Record<string, unknown> = {
      model: this.name,
      prompt,
      stream: options?.stream ?? false,
    };

    if (options?.temperature !== undefined) {
      requestBody.temperature = options.temperature;
    }

    if (options?.maxTokens !== undefined) {
      requestBody.num_predict = options.maxTokens;
    }

    if (options?.stopSequences !== undefined) {
      requestBody.stop = options.stopSequences;
    }

    const response = await fetch(`${OLLAMA_BASE_URL}/api/generate`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(requestBody),
    });

    if (!response.ok) {
      throw new Error(
        `Ollama generate failed: ${response.statusText}`
      );
    }

    const data = (await response.json()) as OllamaGenerateResponse;
    const totalTime = Date.now() - startTime;

    return {
      text: data.response,
      tokensGenerated: data.eval_count ?? 0,
      totalTime,
      timeToFirstToken: data.prompt_eval_duration
        ? data.prompt_eval_duration / 1_000_000 // Convert nanoseconds to milliseconds
        : undefined,
    };
  }
}

/**
 * List available models from Ollama
 */
export async function listOllamaModels(): Promise<
  Array<{ name: string; size: number }>
> {
  const response = await fetch(`${OLLAMA_BASE_URL}/api/tags`);

  if (!response.ok) {
    throw new Error(`Failed to list Ollama models: ${response.statusText}`);
  }

  const data = (await response.json()) as OllamaTagsResponse;
  return data.models.map((model) => ({
    name: model.name,
    size: model.size,
  }));
}

/**
 * Check if Ollama is available
 */
export async function checkOllamaAvailable(): Promise<boolean> {
  try {
    const response = await fetch(`${OLLAMA_BASE_URL}/api/tags`, {
      signal: AbortSignal.timeout(5000),
    });
    return response.ok;
  } catch {
    return false;
  }
}

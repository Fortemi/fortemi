/**
 * Title generation evaluator
 * Evaluates LLM performance on title generation using semantic similarity
 */

import { readFile } from 'fs/promises';
import { LatencyTracker } from '../metrics/latency.js';
import { cosineSimilarity } from '../metrics/similarity.js';
import type { GenerationModel, EmbeddingModel } from '../models/types.js';
import type { LatencyMetrics } from '../models/types.js';

/**
 * Configuration for title evaluation
 */
export interface TitleEvalConfig {
  models: GenerationModel[];
  embeddingModel: EmbeddingModel;
  datasetPath: string;
}

/**
 * Test case from dataset
 */
interface TitleTestCase {
  id: number;
  content: string;
  ideal_titles: string[];
  bad_titles: string[];
}

/**
 * Result for a single model's title evaluation
 */
export interface TitleEvalResult {
  modelName: string;
  scores: {
    semanticSimilarity: number;
    formatCompliance: number;
    overallScore: number;
  };
  latency: LatencyMetrics;
  caseResults: Array<{
    caseId: number;
    generatedTitle: string;
    similarity: number;
    formatCompliant: boolean;
    latency: number;
  }>;
}

/**
 * Title generation prompt template
 */
function createTitlePrompt(content: string): string {
  return `Generate a concise 3-8 word title for this note. Return only the title, no quotes: ${content}`;
}

/**
 * Check if title meets format requirements
 */
function checkFormatCompliance(title: string): boolean {
  // Remove quotes if present
  const cleaned = title.trim().replace(/^["']|["']$/g, '');

  // Count words
  const words = cleaned.split(/\s+/).filter(w => w.length > 0);
  const wordCount = words.length;

  // Check: 3-8 words and no quotes in original
  const hasNoQuotes = !title.includes('"') && !title.includes("'");
  const validWordCount = wordCount >= 3 && wordCount <= 8;

  return hasNoQuotes && validWordCount;
}

/**
 * Calculate semantic similarity between generated title and ideal titles
 */
async function calculateSemanticSimilarity(
  generatedTitle: string,
  idealTitles: string[],
  embeddingModel: EmbeddingModel
): Promise<number> {
  // Get embedding for generated title
  const generatedEmbedding = await embeddingModel.embed(generatedTitle);

  // Get embeddings for all ideal titles
  const similarities: number[] = [];
  for (const idealTitle of idealTitles) {
    const idealEmbedding = await embeddingModel.embed(idealTitle);
    const similarity = cosineSimilarity(generatedEmbedding, idealEmbedding);
    similarities.push(similarity);
  }

  // Return the maximum similarity (best match)
  return Math.max(...similarities);
}

/**
 * Evaluate a single model on title generation tasks
 */
async function evaluateModel(
  model: GenerationModel,
  embeddingModel: EmbeddingModel,
  testCases: TitleTestCase[]
): Promise<TitleEvalResult> {
  const latencyTracker = new LatencyTracker();
  const caseResults: TitleEvalResult['caseResults'] = [];

  for (const testCase of testCases) {
    const timer = latencyTracker.start();

    // Generate title
    const prompt = createTitlePrompt(testCase.content);
    const result = await model.generate(prompt, { temperature: 0.7, maxTokens: 50 });

    const latency = timer.end();
    latencyTracker.record(latency);
    const generatedTitle = result.text.trim();

    // Check format compliance
    const formatCompliant = checkFormatCompliance(generatedTitle);

    // Calculate semantic similarity with ideal titles
    const similarity = await calculateSemanticSimilarity(
      generatedTitle,
      testCase.ideal_titles,
      embeddingModel
    );

    caseResults.push({
      caseId: testCase.id,
      generatedTitle,
      similarity,
      formatCompliant,
      latency,
    });
  }

  // Calculate aggregate scores
  const avgSimilarity = caseResults.reduce((sum, r) => sum + r.similarity, 0) / caseResults.length;
  const formatComplianceRate = caseResults.filter(r => r.formatCompliant).length / caseResults.length;

  // Overall score is weighted average: 70% semantic similarity, 30% format compliance
  const overallScore = (avgSimilarity * 0.7) + (formatComplianceRate * 0.3);

  return {
    modelName: model.name,
    scores: {
      semanticSimilarity: avgSimilarity,
      formatCompliance: formatComplianceRate,
      overallScore,
    },
    latency: latencyTracker.getMetrics(),
    caseResults,
  };
}

/**
 * Evaluate title generation across multiple models
 */
export async function evaluateTitle(
  config: TitleEvalConfig
): Promise<TitleEvalResult[]> {
  // Load test cases
  const datasetContent = await readFile(config.datasetPath, 'utf-8');
  const testCases: TitleTestCase[] = JSON.parse(datasetContent);

  // Evaluate each model
  const results: TitleEvalResult[] = [];
  for (const model of config.models) {
    const result = await evaluateModel(model, config.embeddingModel, testCases);
    results.push(result);
  }

  return results;
}

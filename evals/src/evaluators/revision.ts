/**
 * Revision quality evaluator
 * Evaluates LLM performance on note revision tasks using LLM-as-judge
 */

import { readFile } from 'fs/promises';
import { LatencyTracker } from '../metrics/latency.js';
import type { GenerationModel } from '../models/types.js';
import type { LatencyMetrics } from '../models/types.js';

/**
 * Configuration for revision evaluation
 */
export interface RevisionEvalConfig {
  models: GenerationModel[];
  judge: GenerationModel;
  datasetPath: string;
}

/**
 * Revision quality scores from LLM judge
 */
export interface RevisionScores {
  information_preservation: number;
  structure_enhancement: number;
  no_hallucination: number;
  readability: number;
}

/**
 * Test case from dataset
 */
interface RevisionTestCase {
  id: number;
  original_content: string;
  expected_concepts: string[];
  min_improvement: boolean;
}

/**
 * Result for a single model's revision evaluation
 */
export interface RevisionEvalResult {
  modelName: string;
  scores: {
    averageInformationPreservation: number;
    averageStructureEnhancement: number;
    averageNoHallucination: number;
    averageReadability: number;
    overallScore: number;
  };
  latency: LatencyMetrics;
  totalTokens: number;
  caseResults: Array<{
    caseId: number;
    scores: RevisionScores;
    latency: number;
    tokens: number;
  }>;
}

/**
 * Revision prompt template
 */
function createRevisionPrompt(content: string): string {
  return `Enhance this note with better structure and clarity. Add markdown formatting. Do not invent facts. Original: ${content}`;
}

/**
 * Judge prompt template for scoring revision quality
 */
function createJudgePrompt(original: string, revised: string, concepts: string[]): string {
  return `Evaluate this revision on a scale of 0-100 for each criterion.

Original: ${original}

Revised: ${revised}

Expected concepts to preserve: ${concepts.join(', ')}

Respond with valid JSON only:
{
  "information_preservation": <score 0-100>,
  "structure_enhancement": <score 0-100>,
  "no_hallucination": <score 0-100>,
  "readability": <score 0-100>
}`;
}

/**
 * Parse judge response, extracting JSON from markdown code blocks if needed
 */
function parseJudgeResponse(response: string): RevisionScores {
  try {
    // Try to extract JSON from markdown code block
    const jsonMatch = response.match(/```(?:json)?\s*(\{[\s\S]*?\})\s*```/);
    const jsonStr = jsonMatch ? jsonMatch[1] : response;

    const parsed = JSON.parse(jsonStr.trim());

    return {
      information_preservation: parsed.information_preservation ?? 0,
      structure_enhancement: parsed.structure_enhancement ?? 0,
      no_hallucination: parsed.no_hallucination ?? 0,
      readability: parsed.readability ?? 0,
    };
  } catch (error) {
    // Return default scores if parsing fails
    console.warn(`Failed to parse judge response: ${error}`);
    return {
      information_preservation: 0,
      structure_enhancement: 0,
      no_hallucination: 0,
      readability: 0,
    };
  }
}

/**
 * Evaluate a single model on revision tasks
 */
async function evaluateModel(
  model: GenerationModel,
  judge: GenerationModel,
  testCases: RevisionTestCase[]
): Promise<RevisionEvalResult> {
  const latencyTracker = new LatencyTracker();
  let totalTokens = 0;
  const caseResults: RevisionEvalResult['caseResults'] = [];

  for (const testCase of testCases) {
    const timer = latencyTracker.start();

    // Generate revision
    const prompt = createRevisionPrompt(testCase.original_content);
    const result = await model.generate(prompt);

    const latency = timer.end();
    latencyTracker.record(latency);
    totalTokens += result.tokensGenerated;

    // Judge the revision
    const judgePrompt = createJudgePrompt(
      testCase.original_content,
      result.text,
      testCase.expected_concepts
    );

    const judgeResult = await judge.generate(judgePrompt, { temperature: 0 });
    const scores = parseJudgeResponse(judgeResult.text);

    caseResults.push({
      caseId: testCase.id,
      scores,
      latency,
      tokens: result.tokensGenerated,
    });
  }

  // Calculate averages
  const avgInfoPreservation = caseResults.reduce((sum, r) => sum + r.scores.information_preservation, 0) / caseResults.length;
  const avgStructure = caseResults.reduce((sum, r) => sum + r.scores.structure_enhancement, 0) / caseResults.length;
  const avgNoHallucination = caseResults.reduce((sum, r) => sum + r.scores.no_hallucination, 0) / caseResults.length;
  const avgReadability = caseResults.reduce((sum, r) => sum + r.scores.readability, 0) / caseResults.length;
  const overallScore = (avgInfoPreservation + avgStructure + avgNoHallucination + avgReadability) / 4;

  return {
    modelName: model.name,
    scores: {
      averageInformationPreservation: avgInfoPreservation,
      averageStructureEnhancement: avgStructure,
      averageNoHallucination: avgNoHallucination,
      averageReadability: avgReadability,
      overallScore,
    },
    latency: latencyTracker.getMetrics(),
    totalTokens,
    caseResults,
  };
}

/**
 * Evaluate revision quality across multiple models
 */
export async function evaluateRevision(
  config: RevisionEvalConfig
): Promise<RevisionEvalResult[]> {
  // Load test cases
  const datasetContent = await readFile(config.datasetPath, 'utf-8');
  const testCases: RevisionTestCase[] = JSON.parse(datasetContent);

  // Evaluate each model
  const results: RevisionEvalResult[] = [];
  for (const model of config.models) {
    const result = await evaluateModel(model, config.judge, testCases);
    results.push(result);
  }

  return results;
}

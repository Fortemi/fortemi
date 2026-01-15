/**
 * Example usage of the report generators
 *
 * This file demonstrates how to use the JSON and Markdown reporters
 * with sample evaluation data.
 */

import { generateJSONReport, generateMarkdownReport } from './index.js';
import type { EvaluationReport } from '../models/types.js';

// Sample evaluation data
const sampleReport: EvaluationReport = {
  meta: {
    timestamp: new Date().toISOString(),
    durationMs: 12500,
    modelsTested: 3,
    scenariosRun: 25,
  },
  embeddingResults: {
    'nomic-embed-text': {
      modelName: 'nomic-embed-text',
      overallScore: 82.5,
      metrics: {
        retrieval: {
          precisionAt5: 0.85,
          precisionAt10: 0.78,
          recallAt5: 0.68,
          recallAt10: 0.91,
          mrr: 0.89,
          ndcgAt10: 0.82,
        },
        similarity: {
          accuracy: 0.93,
          meanAbsoluteError: 0.06,
        },
        latency: {
          p50: 38,
          p95: 62,
          p99: 85,
          mean: 42,
          min: 30,
          max: 110,
        },
        throughput: 24.8,
      },
      timestamp: new Date().toISOString(),
    },
    'all-minilm-l6-v2': {
      modelName: 'all-minilm-l6-v2',
      overallScore: 75.2,
      metrics: {
        retrieval: {
          precisionAt5: 0.78,
          precisionAt10: 0.71,
          recallAt5: 0.62,
          recallAt10: 0.85,
          mrr: 0.82,
          ndcgAt10: 0.74,
        },
        similarity: {
          accuracy: 0.89,
        },
        latency: {
          p50: 22,
          p95: 35,
          p99: 48,
          mean: 25,
          min: 18,
          max: 60,
        },
        throughput: 42.5,
      },
      timestamp: new Date().toISOString(),
    },
  },
  llmResults: {
    'gpt-4o-mini': {
      modelName: 'gpt-4o-mini',
      overallScore: 87.8,
      dimensions: {
        revisionQuality: 90.2,
        titleQuality: 85.5,
        contextQuality: 88.0,
        instructionFollowing: 92.5,
        efficiency: 78.0,
      },
      metrics: {
        latency: {
          p50: 420,
          p95: 780,
          p99: 1100,
          mean: 480,
          min: 320,
          max: 1400,
        },
        tokensPerSecond: 48.2,
      },
      timestamp: new Date().toISOString(),
    },
    'claude-3-5-haiku': {
      modelName: 'claude-3-5-haiku',
      overallScore: 85.5,
      dimensions: {
        revisionQuality: 87.0,
        titleQuality: 83.5,
        contextQuality: 86.0,
        instructionFollowing: 89.5,
        efficiency: 85.0,
      },
      metrics: {
        latency: {
          p50: 320,
          p95: 580,
          p99: 850,
          mean: 360,
          min: 250,
          max: 1000,
        },
        tokensPerSecond: 55.8,
      },
      timestamp: new Date().toISOString(),
    },
    'llama-3.1-8b': {
      modelName: 'llama-3.1-8b',
      overallScore: 78.2,
      dimensions: {
        revisionQuality: 80.5,
        titleQuality: 76.0,
        contextQuality: 79.5,
        instructionFollowing: 82.0,
        efficiency: 72.5,
      },
      metrics: {
        latency: {
          p50: 650,
          p95: 1200,
          p99: 1800,
          mean: 720,
          min: 480,
          max: 2200,
        },
        tokensPerSecond: 28.5,
      },
      timestamp: new Date().toISOString(),
    },
  },
  recommendations: {
    bestEmbedding: 'nomic-embed-text',
    bestLLMQuality: 'gpt-4o-mini',
    bestLLMBalanced: 'gpt-4o-mini',
    bestLLMSpeed: 'claude-3-5-haiku',
  },
};

// Generate and display reports
console.log('='.repeat(80));
console.log('JSON REPORT');
console.log('='.repeat(80));
console.log(generateJSONReport(sampleReport));

console.log('\n\n');
console.log('='.repeat(80));
console.log('MARKDOWN REPORT');
console.log('='.repeat(80));
console.log(generateMarkdownReport(sampleReport));

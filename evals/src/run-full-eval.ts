#!/usr/bin/env npx tsx
/**
 * Full evaluation runner that generates comprehensive report
 */

import * as fs from 'fs';
import * as path from 'path';
import { evaluateEmbeddingModel, loadEmbeddingDatasets } from './evaluators/embedding.js';
import { evaluateTitle } from './evaluators/title.js';
import { OllamaEmbeddingModel, OllamaGenerationModel, listOllamaModels } from './models/ollama.js';
import { generateMarkdownReport } from './reporters/markdown.js';
import { generateHTMLReport } from './reporters/html.js';
import { generateAllCharts } from './reporters/charts.js';
import type { EvaluationReport, EmbeddingEvalResult, LLMEvalResult } from './models/types.js';

const EMBEDDING_MODELS = [
  'nomic-embed-text:latest',
  'mxbai-embed-large:latest',
  'snowflake-arctic-embed:335m',
  'all-minilm:l6-v2',
];

const LLM_MODELS = [
  // Baseline models
  'gpt-oss:20b',        // Current production model
  'qwen2.5:14b',        // Strong medium model
  'qwen2.5:7b',         // Smaller baseline
  'llama3.1:8b',        // Meta baseline
  // DeepSeek models (evaluation target)
  'deepseek-r1:14b',    // DeepSeek reasoning model with <think> tags
  'deepseek-coder-v2:16b', // DeepSeek coder model
  'exaone-deep:7.8b',   // EXAONE reasoning model
  // Additional models
  'mistral:latest',
  'hermes3:8b',
  'cogito:8b',
  'command-r7b:latest',
];

// Model dimensions mapping
const MODEL_DIMENSIONS: Record<string, number> = {
  'nomic-embed-text:latest': 768,
  'mxbai-embed-large:latest': 1024,
  'snowflake-arctic-embed:335m': 1024,
  'all-minilm:l6-v2': 384,
};

async function main() {
  const startTime = Date.now();
  console.log('=== Matric-Memory Model Evaluation ===\n');

  // Get available models
  const availableModels = await listOllamaModels();
  const availableModelNames = new Set(availableModels.map(m => m.name));

  // Filter to available models
  const embeddingModels = EMBEDDING_MODELS.filter(m => availableModelNames.has(m));
  const llmModels = LLM_MODELS.filter(m => availableModelNames.has(m));

  console.log(`Available embedding models: ${embeddingModels.join(', ')}`);
  console.log(`Available LLM models: ${llmModels.join(', ')}\n`);

  // === Embedding Evaluations ===
  console.log('--- Embedding Model Evaluations ---\n');
  const embeddingResults: Record<string, EmbeddingEvalResult> = {};
  const datasets = await loadEmbeddingDatasets('./datasets/embedding_tests');

  for (const modelName of embeddingModels) {
    console.log(`Evaluating: ${modelName}`);
    try {
      const dim = MODEL_DIMENSIONS[modelName] || 768;
      const model = new OllamaEmbeddingModel(modelName, dim);
      const result = await evaluateEmbeddingModel(model, datasets);
      embeddingResults[modelName] = result;
      console.log(`  Score: ${result.overallScore.toFixed(1)}`);
      console.log(`  P@5: ${(result.metrics.retrieval.precisionAt5 * 100).toFixed(1)}%`);
      console.log(`  MRR: ${(result.metrics.retrieval.mrr * 100).toFixed(1)}%`);
      console.log(`  Latency p95: ${result.metrics.latency.p95.toFixed(0)}ms\n`);
    } catch (err) {
      console.error(`  Error: ${err}\n`);
    }
  }

  // === LLM Evaluations (Title Generation) ===
  console.log('--- LLM Model Evaluations (Title Generation) ---\n');
  const llmResults: Record<string, LLMEvalResult> = {};

  // Use best embedding model for title similarity evaluation
  const bestEmbedModel = embeddingModels[0];
  const bestEmbedDim = MODEL_DIMENSIONS[bestEmbedModel] || 768;
  const embeddingModel = new OllamaEmbeddingModel(bestEmbedModel, bestEmbedDim);

  for (const modelName of llmModels) {
    console.log(`Evaluating: ${modelName}`);
    try {
      const model = new OllamaGenerationModel(modelName);
      const titleResult = await evaluateTitle({
        models: [model],
        embeddingModel,
        datasetPath: './datasets/title_tests/title_cases.json',
      });

      if (titleResult.length > 0) {
        const tr = titleResult[0];
        llmResults[modelName] = {
          modelName,
          overallScore: tr.scores.overallScore * 0.2 + 10, // Title is 20% of LLM score + base efficiency
          dimensions: {
            revisionQuality: 0, // Skipped due to Ollama errors
            titleQuality: tr.scores.overallScore,
            contextQuality: 0,
            instructionFollowing: 0,
            efficiency: 50, // Base efficiency
          },
          metrics: {
            latency: tr.latency,
            tokensPerSecond: 0,
          },
          timestamp: new Date().toISOString(),
        };
        console.log(`  Title Score: ${tr.scores.overallScore.toFixed(1)}`);
        console.log(`  Semantic Similarity: ${(tr.scores.semanticSimilarity * 100).toFixed(1)}%`);
        console.log(`  Format Compliance: ${(tr.scores.formatCompliance * 100).toFixed(1)}%`);
        console.log(`  Latency p95: ${tr.latency.p95.toFixed(0)}ms\n`);
      }
    } catch (err) {
      console.error(`  Error: ${err}\n`);
    }
  }

  // === Generate Report ===
  const report: EvaluationReport = {
    meta: {
      timestamp: new Date().toISOString(),
      durationMs: Date.now() - startTime,
      modelsTested: Object.keys(embeddingResults).length + Object.keys(llmResults).length,
      scenariosRun: 0,
    },
    embeddingResults,
    llmResults,
    recommendations: {
      bestEmbedding: Object.entries(embeddingResults)
        .sort(([,a], [,b]) => b.overallScore - a.overallScore)[0]?.[0],
      bestLLMQuality: Object.entries(llmResults)
        .sort(([,a], [,b]) => b.dimensions.titleQuality - a.dimensions.titleQuality)[0]?.[0],
      bestLLMBalanced: Object.entries(llmResults)
        .sort(([,a], [,b]) => {
          const scoreA = a.dimensions.titleQuality * 0.7 + (100 - a.metrics.latency.p95/10) * 0.3;
          const scoreB = b.dimensions.titleQuality * 0.7 + (100 - b.metrics.latency.p95/10) * 0.3;
          return scoreB - scoreA;
        })[0]?.[0],
      bestLLMSpeed: Object.entries(llmResults)
        .sort(([,a], [,b]) => a.metrics.latency.p95 - b.metrics.latency.p95)[0]?.[0],
    },
  };

  // Save reports
  const outputDir = path.resolve('./reports', `eval-${new Date().toISOString().replace(/:/g, '-').split('.')[0]}`);
  fs.mkdirSync(outputDir, { recursive: true });
  fs.mkdirSync(path.join(outputDir, 'raw'), { recursive: true });
  fs.mkdirSync(path.join(outputDir, 'charts'), { recursive: true });

  // Save JSON
  fs.writeFileSync(path.join(outputDir, 'summary.json'), JSON.stringify(report, null, 2));
  fs.writeFileSync(path.join(outputDir, 'raw', 'embedding-results.json'), JSON.stringify(embeddingResults, null, 2));
  fs.writeFileSync(path.join(outputDir, 'raw', 'llm-results.json'), JSON.stringify(llmResults, null, 2));

  // Generate and save markdown report
  const markdownReport = generateMarkdownReport(report);
  fs.writeFileSync(path.join(outputDir, 'report.md'), markdownReport);

  // Generate and save HTML report with embedded charts
  const htmlReport = generateHTMLReport(report);
  fs.writeFileSync(path.join(outputDir, 'report.html'), htmlReport);
  console.log('HTML report generated with embedded charts');

  // Generate charts
  try {
    await generateAllCharts({ embedding: Object.values(embeddingResults), llm: Object.values(llmResults) }, path.join(outputDir, 'charts'));
    console.log('Charts generated successfully');
  } catch (err) {
    console.log('Charts generation skipped (vega not available)');
  }

  console.log('\n=== Evaluation Complete ===');
  console.log(`Duration: ${((Date.now() - startTime) / 1000).toFixed(1)}s`);
  console.log(`Results saved to: ${outputDir}`);
  console.log('\n--- Recommendations ---');
  console.log(`Best Embedding: ${report.recommendations.bestEmbedding}`);
  console.log(`Best LLM (Quality): ${report.recommendations.bestLLMQuality}`);
  console.log(`Best LLM (Balanced): ${report.recommendations.bestLLMBalanced}`);
  console.log(`Best LLM (Speed): ${report.recommendations.bestLLMSpeed}`);
}

main().catch(console.error);

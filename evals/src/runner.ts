/**
 * Evaluation orchestrator - coordinates model testing and result aggregation
 */

import * as fs from 'fs';
import * as path from 'path';
import type { EvaluationConfig, EvaluationReport, EmbeddingEvalResult, LLMEvalResult } from './models/types.js';
import { listOllamaModels, OllamaEmbeddingModel, OllamaGenerationModel } from './models/ollama.js';
import { evaluateEmbeddingModel, loadEmbeddingDatasets, evaluateRevision, evaluateTitle } from './evaluators/index.js';
import type { EmbeddingDataset, RevisionEvalResult, TitleEvalResult } from './evaluators/index.js';
import { generateMarkdownReport } from './reporters/index.js';
import { generateAllCharts } from './reporters/charts.js';
import { calculateLLMScore } from './scoring/calculator.js';
import { normalizeLatencyScore } from './scoring/calculator.js';

// ============================================================================
// Model Filtering
// ============================================================================

/**
 * Known embedding model patterns
 */
const EMBEDDING_MODEL_PATTERNS = [
  /embed/i,
  /bge-/i,
  /e5-/i,
  /gte-/i,
  /instructor/i,
  /sentence-transformer/i,
];

/**
 * Filter models to only embedding models
 */
export function filterEmbeddingModels(
  models: Array<{ name: string; size: number }>,
  specificModels?: string[]
): Array<{ name: string; size: number }> {
  let filtered = models.filter((model) =>
    EMBEDDING_MODEL_PATTERNS.some((pattern) => pattern.test(model.name))
  );

  // If specific models requested, filter to those
  if (specificModels && specificModels.length > 0) {
    filtered = filtered.filter((model) => specificModels.includes(model.name));
  }

  return filtered;
}

/**
 * Filter models to only LLM (generation) models
 */
export function filterLLMModels(
  models: Array<{ name: string; size: number }>,
  specificModels?: string[]
): Array<{ name: string; size: number }> {
  // Exclude embedding models
  let filtered = models.filter(
    (model) => !EMBEDDING_MODEL_PATTERNS.some((pattern) => pattern.test(model.name))
  );

  // If specific models requested, filter to those
  if (specificModels && specificModels.length > 0) {
    filtered = filtered.filter((model) => specificModels.includes(model.name));
  }

  return filtered;
}

// ============================================================================
// Configuration Loading
// ============================================================================

/**
 * Load evaluation configuration
 * @param configPath - Optional path to config file
 * @returns Evaluation configuration
 */
export function loadConfig(configPath?: string): EvaluationConfig {
  // Default configuration
  const defaultConfig: EvaluationConfig = {
    models: {},
    datasets: {
      embeddingTests: './datasets/embedding_tests',
      revisionTests: './datasets/revision_tests/full_revision_cases.json',
      titleTests: './datasets/title_tests/title_cases.json',
    },
    output: {
      directory: './reports',
      format: 'both',
      generateCharts: false,
    },
    parallel: false,
    verbose: false,
  };

  // If no config path provided, use defaults
  if (!configPath) {
    return defaultConfig;
  }

  // Load from file if it exists
  if (fs.existsSync(configPath)) {
    const configData = fs.readFileSync(configPath, 'utf-8');
    const loadedConfig = JSON.parse(configData) as Partial<EvaluationConfig>;

    // Merge with defaults
    return {
      ...defaultConfig,
      ...loadedConfig,
      models: { ...defaultConfig.models, ...loadedConfig.models },
      datasets: { ...defaultConfig.datasets, ...loadedConfig.datasets },
      output: { ...defaultConfig.output, ...loadedConfig.output },
    };
  }

  return defaultConfig;
}

// ============================================================================
// Output Directory Management
// ============================================================================

/**
 * Create timestamped output directory
 * @param baseDir - Base directory for reports
 * @returns Absolute path to created directory
 */
export function createOutputDirectory(baseDir: string): string {
  const timestamp = new Date().toISOString().replace(/:/g, '-').split('.')[0];
  const outputDir = path.resolve(baseDir, `eval-${timestamp}`);

  // Create directory structure
  fs.mkdirSync(outputDir, { recursive: true });
  fs.mkdirSync(path.join(outputDir, 'raw'), { recursive: true });

  return outputDir;
}

// ============================================================================
// Evaluation Orchestration
// ============================================================================

/**
 * Run embedding model evaluations
 * @param models - Models to evaluate
 * @param config - Evaluation configuration
 * @param verbose - Enable verbose logging
 * @returns Evaluation results
 */
export async function runEmbeddingEvaluations(
  models: string[],
  config: EvaluationConfig,
  verbose: boolean = false
): Promise<Record<string, EmbeddingEvalResult>> {
  if (verbose) {
    console.log(`\n=== Running Embedding Evaluations ===`);
    console.log(`Models to evaluate: ${models.join(', ')}`);
  }

  const results: Record<string, EmbeddingEvalResult> = {};

  // Load datasets
  const datasetPath = config.datasets.embeddingTests || './datasets/embedding_tests';
  let dataset: EmbeddingDataset;

  try {
    if (verbose) {
      console.log(`Loading embedding datasets from: ${datasetPath}`);
    }
    dataset = await loadEmbeddingDatasets(datasetPath);
  } catch (error) {
    console.error(`Failed to load embedding datasets: ${error}`);
    return results;
  }

  for (const modelName of models) {
    if (verbose) {
      console.log(`\nEvaluating embedding model: ${modelName}`);
    }

    try {
      // Create model instance (assuming 768 dimensions, will be auto-detected from first embedding)
      const model = new OllamaEmbeddingModel(modelName, 768);

      // Run evaluation
      const result = await evaluateEmbeddingModel(model, dataset);
      results[modelName] = result;

      if (verbose) {
        console.log(`  Overall Score: ${result.overallScore.toFixed(1)}`);
      }
    } catch (error) {
      console.error(`Failed to evaluate ${modelName}: ${error}`);
      // Continue to next model on failure
    }
  }

  return results;
}

/**
 * Run LLM model evaluations
 * @param models - Models to evaluate
 * @param config - Evaluation configuration
 * @param verbose - Enable verbose logging
 * @returns Evaluation results
 */
export async function runLLMEvaluations(
  models: string[],
  config: EvaluationConfig,
  verbose: boolean = false
): Promise<Record<string, LLMEvalResult>> {
  if (verbose) {
    console.log(`\n=== Running LLM Evaluations ===`);
    console.log(`Models to evaluate: ${models.join(', ')}`);
  }

  const results: Record<string, LLMEvalResult> = {};

  // Determine judge model - prefer qwen2.5:14b for stability, fallback to qwen2.5:32b
  const allModels = await listOllamaModels();
  const judgeModelName = allModels.find(m => m.name === 'qwen2.5:14b')?.name
    || allModels.find(m => m.name === 'qwen2.5:32b')?.name;

  if (!judgeModelName) {
    console.error('No suitable judge model found (qwen2.5:32b or qwen2.5:14b). Skipping LLM evaluations.');
    return results;
  }

  const judgeModel = new OllamaGenerationModel(judgeModelName);

  if (verbose) {
    console.log(`Using judge model: ${judgeModelName}`);
  }

  // Create model instances
  const generationModels = models.map(name => new OllamaGenerationModel(name));

  // Determine which datasets to run
  const revisionPath = config.datasets.revisionTests || './datasets/revision_tests/full_revision_cases.json';
  const titlePath = config.datasets.titleTests || './datasets/title_tests/title_cases.json';

  // Track test counts for scenarios
  let totalScenarios = 0;

  // Run revision evaluations if dataset exists
  let revisionResults: RevisionEvalResult[] = [];
  if (fs.existsSync(revisionPath)) {
    if (verbose) {
      console.log(`\nRunning revision quality evaluations...`);
    }

    try {
      revisionResults = await evaluateRevision({
        models: generationModels,
        judge: judgeModel,
        datasetPath: revisionPath,
      });

      // Count scenarios from first model's results
      if (revisionResults.length > 0) {
        totalScenarios += revisionResults[0].caseResults.length;
      }

      if (verbose) {
        console.log(`  Completed revision evaluations`);
      }
    } catch (error) {
      console.error(`Failed to run revision evaluations: ${error}`);
    }
  } else if (verbose) {
    console.warn(`Revision test dataset not found: ${revisionPath}`);
  }

  // For title evaluations, we need an embedding model
  let titleResults: TitleEvalResult[] = [];
  const embeddingModelsAvailable = allModels.filter(m =>
    EMBEDDING_MODEL_PATTERNS.some(pattern => pattern.test(m.name))
  );

  if (fs.existsSync(titlePath) && embeddingModelsAvailable.length > 0) {
    if (verbose) {
      console.log(`\nRunning title generation evaluations...`);
    }

    try {
      // Use first available embedding model for semantic similarity
      const embeddingModel = new OllamaEmbeddingModel(embeddingModelsAvailable[0].name, 768);

      titleResults = await evaluateTitle({
        models: generationModels,
        embeddingModel,
        datasetPath: titlePath,
      });

      // Count scenarios from first model's results
      if (titleResults.length > 0) {
        totalScenarios += titleResults[0].caseResults.length;
      }

      if (verbose) {
        console.log(`  Completed title evaluations`);
      }
    } catch (error) {
      console.error(`Failed to run title evaluations: ${error}`);
    }
  } else if (verbose) {
    if (!fs.existsSync(titlePath)) {
      console.warn(`Title test dataset not found: ${titlePath}`);
    }
    if (embeddingModelsAvailable.length === 0) {
      console.warn(`No embedding model available for title evaluations`);
    }
  }

  // Combine results into LLM eval structure
  for (let i = 0; i < models.length; i++) {
    const modelName = models[i];

    try {
      // Extract revision metrics if available
      const revisionResult = revisionResults.find(r => r.modelName === modelName);
      const titleResult = titleResults.find(r => r.modelName === modelName);

      // Calculate dimension scores
      const revisionQuality = revisionResult
        ? revisionResult.scores.overallScore
        : 0;

      const titleQuality = titleResult
        ? titleResult.scores.overallScore * 100 // Convert 0-1 to 0-100
        : 0;

      // For now, use placeholders for dimensions we haven't implemented yet
      const contextQuality = 0; // TODO: Implement context evaluator
      const instructionFollowing = 0; // TODO: Implement instruction following evaluator

      // Calculate efficiency from latency
      const latency = revisionResult?.latency || titleResult?.latency || {
        p50: 0, p95: 0, p99: 0, mean: 0, min: 0, max: 0
      };

      const efficiency = normalizeLatencyScore(latency.p95);

      // Calculate tokens per second from revision results
      const tokensPerSecond = revisionResult && latency.mean > 0
        ? (revisionResult.totalTokens / (latency.mean * revisionResult.caseResults.length)) * 1000
        : 0;

      const dimensions = {
        revisionQuality,
        titleQuality,
        contextQuality,
        instructionFollowing,
        efficiency,
      };

      // Calculate overall score
      const overallScore = calculateLLMScore(dimensions);

      results[modelName] = {
        modelName,
        overallScore,
        dimensions,
        metrics: {
          latency,
          tokensPerSecond,
        },
        timestamp: new Date().toISOString(),
      };

      if (verbose) {
        console.log(`\n${modelName}:`);
        console.log(`  Overall Score: ${overallScore.toFixed(1)}`);
        console.log(`  Revision Quality: ${revisionQuality.toFixed(1)}`);
        console.log(`  Title Quality: ${titleQuality.toFixed(1)}`);
      }
    } catch (error) {
      console.error(`Failed to process results for ${modelName}: ${error}`);
      // Continue to next model on failure
    }
  }

  return results;
}

/**
 * Generate recommendations from evaluation results
 */
function generateRecommendations(
  embeddingResults: Record<string, EmbeddingEvalResult>,
  llmResults: Record<string, LLMEvalResult>
): EvaluationReport['recommendations'] {
  const recommendations: EvaluationReport['recommendations'] = {};

  // Find best embedding model
  if (Object.keys(embeddingResults).length > 0) {
    const bestEmbedding = Object.entries(embeddingResults).sort(
      ([, a], [, b]) => b.overallScore - a.overallScore
    )[0];
    recommendations.bestEmbedding = bestEmbedding[0];
  }

  // Find best LLM models by different criteria
  if (Object.keys(llmResults).length > 0) {
    // Best quality (highest overall score)
    const bestQuality = Object.entries(llmResults).sort(
      ([, a], [, b]) => b.overallScore - a.overallScore
    )[0];
    recommendations.bestLLMQuality = bestQuality[0];

    // Best balanced (considering quality and efficiency)
    const bestBalanced = Object.entries(llmResults).sort(
      ([, a], [, b]) => {
        const scoreA = a.overallScore * 0.7 + a.dimensions.efficiency * 0.3;
        const scoreB = b.overallScore * 0.7 + b.dimensions.efficiency * 0.3;
        return scoreB - scoreA;
      }
    )[0];
    recommendations.bestLLMBalanced = bestBalanced[0];

    // Best speed (highest efficiency)
    const bestSpeed = Object.entries(llmResults).sort(
      ([, a], [, b]) => b.dimensions.efficiency - a.dimensions.efficiency
    )[0];
    recommendations.bestLLMSpeed = bestSpeed[0];
  }

  return recommendations;
}

/**
 * Save evaluation results to files
 */
function saveResults(
  outputDir: string,
  report: EvaluationReport,
  config: EvaluationConfig
): void {
  // Save summary JSON
  const summaryPath = path.join(outputDir, 'summary.json');
  fs.writeFileSync(summaryPath, JSON.stringify(report, null, 2));

  // Save raw results
  const rawEmbeddingPath = path.join(outputDir, 'raw', 'embedding-results.json');
  fs.writeFileSync(
    rawEmbeddingPath,
    JSON.stringify(report.embeddingResults, null, 2)
  );

  const rawLLMPath = path.join(outputDir, 'raw', 'llm-results.json');
  fs.writeFileSync(rawLLMPath, JSON.stringify(report.llmResults, null, 2));

  // Generate markdown report if configured
  if (config.output.format === 'markdown' || config.output.format === 'both') {
    const markdownReport = generateMarkdownReport(report);
    const markdownPath = path.join(outputDir, 'report.md');
    fs.writeFileSync(markdownPath, markdownReport);

    if (config.verbose) {
      console.log(`Generated markdown report: ${markdownPath}`);
    }
  }

  // Generate charts if enabled
  if (config.output.generateCharts) {
    try {
      const chartsDir = path.join(outputDir, 'charts');
      const chartResults = {
        embedding: Object.values(report.embeddingResults),
        llm: Object.values(report.llmResults),
      };

      generateAllCharts(chartResults, chartsDir).catch(error => {
        console.warn(`Failed to generate charts: ${error}`);
      });
    } catch (error) {
      console.warn(`Failed to generate charts: ${error}`);
    }
  }
}

/**
 * Run full evaluation suite
 * @param config - Evaluation configuration
 * @param specificModels - Optional list of specific models to test
 * @returns Evaluation report
 */
export async function runFullEvaluation(
  config: EvaluationConfig,
  specificModels?: string[]
): Promise<EvaluationReport> {
  const startTime = Date.now();
  const verbose = config.verbose ?? false;

  if (verbose) {
    console.log('Starting full evaluation suite...');
  }

  // Discover available models
  const allModels = await listOllamaModels();

  if (verbose) {
    console.log(`\nDiscovered ${allModels.length} Ollama models`);
  }

  // Filter models by type
  const embeddingModels = filterEmbeddingModels(
    allModels,
    specificModels ?? config.models.embeddings
  );
  const llmModels = filterLLMModels(
    allModels,
    specificModels ?? config.models.llms
  );

  if (verbose) {
    console.log(`Embedding models: ${embeddingModels.length}`);
    console.log(`LLM models: ${llmModels.length}`);
  }

  // Track scenario count
  let totalScenarios = 0;

  // Run evaluations
  const embeddingResults = await runEmbeddingEvaluations(
    embeddingModels.map((m) => m.name),
    config,
    verbose
  );

  // Count embedding scenarios (estimate based on typical dataset size)
  if (Object.keys(embeddingResults).length > 0) {
    totalScenarios += 50; // Placeholder, will be accurate when datasets are loaded
  }

  const llmResults = await runLLMEvaluations(
    llmModels.map((m) => m.name),
    config,
    verbose
  );

  // Generate recommendations
  const recommendations = generateRecommendations(embeddingResults, llmResults);

  // Create report
  const report: EvaluationReport = {
    meta: {
      timestamp: new Date().toISOString(),
      durationMs: Date.now() - startTime,
      modelsTested: embeddingModels.length + llmModels.length,
      scenariosRun: totalScenarios,
    },
    embeddingResults,
    llmResults,
    recommendations,
  };

  // Create output directory and save results
  const outputDir = createOutputDirectory(config.output.directory);
  saveResults(outputDir, report, config);

  if (verbose) {
    console.log(`\n✓ Evaluation complete!`);
    console.log(`Results saved to: ${outputDir}`);
    console.log(`Duration: ${report.meta.durationMs}ms`);
  }

  return report;
}

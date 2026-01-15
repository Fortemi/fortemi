#!/usr/bin/env node
/**
 * CLI entry point for matric-memory model evaluation framework
 */

import { Command } from 'commander';
import { checkOllamaAvailable, listOllamaModels } from './models/ollama.js';
import {
  loadConfig,
  runFullEvaluation,
  runEmbeddingEvaluations,
  runLLMEvaluations,
  filterEmbeddingModels,
  filterLLMModels,
} from './runner.js';

const program = new Command();

program
  .name('matric-evals')
  .description('Model evaluation framework for matric-memory')
  .version('0.1.0');

/**
 * Main evaluation command
 */
program
  .command('eval')
  .description('Run full evaluation suite (embeddings + LLMs)')
  .option('-m, --models <models...>', 'Specific models to evaluate')
  .option('-o, --output <dir>', 'Output directory', './reports')
  .option('-c, --config <path>', 'Path to configuration file')
  .option('-v, --verbose', 'Verbose output', false)
  .action(async (options) => {
    // Check Ollama availability
    const available = await checkOllamaAvailable();
    if (!available) {
      console.error('Error: Ollama is not available at http://localhost:11434');
      console.error('Please start Ollama before running evaluations.');
      process.exit(1);
    }

    try {
      // Load configuration
      const config = loadConfig(options.config);

      // Override config with CLI options
      if (options.output) {
        config.output.directory = options.output;
      }
      if (options.verbose) {
        config.verbose = true;
      }

      // Run full evaluation
      const report = await runFullEvaluation(config, options.models);

      if (!options.verbose) {
        console.log('\n✓ Evaluation complete!');
        console.log(`\nResults:`);
        console.log(`  - Embedding models tested: ${Object.keys(report.embeddingResults).length}`);
        console.log(`  - LLM models tested: ${Object.keys(report.llmResults).length}`);
        console.log(`  - Duration: ${report.meta.durationMs}ms`);

        if (report.recommendations.bestEmbedding) {
          console.log(`\nRecommendations:`);
          console.log(`  - Best embedding: ${report.recommendations.bestEmbedding}`);
          if (report.recommendations.bestLLMQuality) {
            console.log(`  - Best LLM (quality): ${report.recommendations.bestLLMQuality}`);
          }
          if (report.recommendations.bestLLMBalanced) {
            console.log(`  - Best LLM (balanced): ${report.recommendations.bestLLMBalanced}`);
          }
          if (report.recommendations.bestLLMSpeed) {
            console.log(`  - Best LLM (speed): ${report.recommendations.bestLLMSpeed}`);
          }
        }
      }
    } catch (error) {
      console.error('Evaluation failed:', error);
      process.exit(1);
    }
  });

/**
 * Embedding evaluation command
 */
program
  .command('eval:embeddings')
  .description('Run embedding model evaluations')
  .option('-m, --models <models...>', 'Specific embedding models to evaluate')
  .option('-o, --output <dir>', 'Output directory', './reports')
  .option('-c, --config <path>', 'Path to configuration file')
  .option('-v, --verbose', 'Verbose output', false)
  .action(async (options) => {
    const available = await checkOllamaAvailable();
    if (!available) {
      console.error('Error: Ollama is not available');
      process.exit(1);
    }

    try {
      // Load configuration
      const config = loadConfig(options.config);
      if (options.output) {
        config.output.directory = options.output;
      }
      if (options.verbose) {
        config.verbose = true;
      }

      // Get available models
      const allModels = await listOllamaModels();
      const embeddingModels = filterEmbeddingModels(allModels, options.models);

      if (embeddingModels.length === 0) {
        console.error('No embedding models found');
        process.exit(1);
      }

      // Run embedding evaluations
      const results = await runEmbeddingEvaluations(
        embeddingModels.map((m) => m.name),
        config,
        options.verbose
      );

      console.log(`\n✓ Evaluated ${Object.keys(results).length} embedding models`);
    } catch (error) {
      console.error('Evaluation failed:', error);
      process.exit(1);
    }
  });

/**
 * LLM evaluation command
 */
program
  .command('eval:llms')
  .description('Run LLM generation evaluations')
  .option('-m, --models <models...>', 'Specific LLM models to evaluate')
  .option('-o, --output <dir>', 'Output directory', './reports')
  .option('-c, --config <path>', 'Path to configuration file')
  .option('-v, --verbose', 'Verbose output', false)
  .action(async (options) => {
    const available = await checkOllamaAvailable();
    if (!available) {
      console.error('Error: Ollama is not available');
      process.exit(1);
    }

    try {
      // Load configuration
      const config = loadConfig(options.config);
      if (options.output) {
        config.output.directory = options.output;
      }
      if (options.verbose) {
        config.verbose = true;
      }

      // Get available models
      const allModels = await listOllamaModels();
      const llmModels = filterLLMModels(allModels, options.models);

      if (llmModels.length === 0) {
        console.error('No LLM models found');
        process.exit(1);
      }

      // Run LLM evaluations
      const results = await runLLMEvaluations(
        llmModels.map((m) => m.name),
        config,
        options.verbose
      );

      console.log(`\n✓ Evaluated ${Object.keys(results).length} LLM models`);
    } catch (error) {
      console.error('Evaluation failed:', error);
      process.exit(1);
    }
  });

/**
 * Report generation command
 */
program
  .command('report')
  .description('Generate report from existing evaluation data')
  .option('-i, --input <dir>', 'Input directory with evaluation data')
  .option('-o, --output <dir>', 'Output directory', './reports')
  .option('-f, --format <format>', 'Output format (json, markdown, both)', 'both')
  .action(async (options) => {
    console.log('Generating report...');
    console.log('Options:', options);

    console.log('\n[Not yet implemented: Report generation]');
    console.log('TODO: Implement report generation from saved results');
  });

/**
 * List available models command
 */
program
  .command('list')
  .description('List available Ollama models')
  .action(async () => {
    const available = await checkOllamaAvailable();
    if (!available) {
      console.error('Error: Ollama is not available at http://localhost:11434');
      process.exit(1);
    }

    try {
      const models = await listOllamaModels();
      console.log(`\nAvailable Ollama models (${models.length}):\n`);

      models.forEach((model) => {
        const sizeMB = (model.size / 1024 / 1024).toFixed(0);
        console.log(`  ${model.name.padEnd(30)} ${sizeMB.padStart(6)} MB`);
      });
    } catch (error) {
      console.error('Failed to list models:', error);
      process.exit(1);
    }
  });

/**
 * Check Ollama status command
 */
program
  .command('status')
  .description('Check Ollama connection status')
  .action(async () => {
    console.log('Checking Ollama status...');

    const available = await checkOllamaAvailable();
    if (available) {
      console.log('✓ Ollama is available at http://localhost:11434');

      try {
        const models = await listOllamaModels();
        console.log(`✓ Found ${models.length} models`);
      } catch (error) {
        console.error('✗ Failed to list models:', error);
      }
    } else {
      console.error('✗ Ollama is not available');
      console.error('  Please start Ollama: ollama serve');
      process.exit(1);
    }
  });

// Parse command line arguments
program.parse();

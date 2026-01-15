/**
 * Example script to evaluate embedding models
 *
 * Usage:
 *   tsx src/examples/evaluate-embeddings.ts
 */

import { OllamaEmbeddingModel, checkOllamaAvailable } from '../models/ollama.js';
import { evaluateEmbeddingModel, loadEmbeddingDatasets } from '../evaluators/embedding.js';
import { join } from 'path';

const MODELS = [
  { name: 'nomic-embed-text', dimensions: 768 },
  { name: 'mxbai-embed-large', dimensions: 1024 },
];

async function main() {
  // Check if Ollama is available
  const isAvailable = await checkOllamaAvailable();
  if (!isAvailable) {
    console.error('Error: Ollama is not available at http://localhost:11434');
    console.error('Please ensure Ollama is running before evaluating models.');
    process.exit(1);
  }

  console.log('Loading embedding test datasets...');
  const datasetsPath = join(process.cwd(), 'datasets', 'embedding_tests');
  const datasets = await loadEmbeddingDatasets(datasetsPath);

  console.log(`Loaded ${datasets.similarityPairs.length} similarity pairs`);
  console.log(`Loaded ${datasets.dissimilarityPairs.length} dissimilarity pairs`);
  console.log(`Loaded ${datasets.retrievalQueries.length} retrieval queries\n`);

  const results = [];

  for (const modelConfig of MODELS) {
    console.log(`\nEvaluating ${modelConfig.name}...`);
    console.log('='.repeat(60));

    try {
      const model = new OllamaEmbeddingModel(modelConfig.name, modelConfig.dimensions);

      const result = await evaluateEmbeddingModel(model, datasets);

      results.push(result);

      // Display results
      console.log(`\nModel: ${result.modelName}`);
      console.log(`Overall Score: ${result.overallScore.toFixed(2)}/100\n`);

      console.log('Retrieval Metrics:');
      console.log(`  Precision@5:  ${(result.metrics.retrieval.precisionAt5 * 100).toFixed(2)}%`);
      console.log(`  Precision@10: ${(result.metrics.retrieval.precisionAt10 * 100).toFixed(2)}%`);
      console.log(`  Recall@5:     ${(result.metrics.retrieval.recallAt5 * 100).toFixed(2)}%`);
      console.log(`  Recall@10:    ${(result.metrics.retrieval.recallAt10 * 100).toFixed(2)}%`);
      console.log(`  MRR:          ${(result.metrics.retrieval.mrr * 100).toFixed(2)}%`);
      console.log(`  NDCG@10:      ${(result.metrics.retrieval.ndcgAt10 * 100).toFixed(2)}%\n`);

      console.log('Similarity Metrics:');
      console.log(`  Accuracy:     ${(result.metrics.similarity.accuracy * 100).toFixed(2)}%`);
      if (result.metrics.similarity.meanAbsoluteError !== undefined) {
        console.log(`  MAE:          ${result.metrics.similarity.meanAbsoluteError.toFixed(4)}`);
      }
      console.log();

      console.log('Performance Metrics:');
      console.log(`  Latency (p50):  ${result.metrics.latency.p50.toFixed(2)}ms`);
      console.log(`  Latency (p95):  ${result.metrics.latency.p95.toFixed(2)}ms`);
      console.log(`  Latency (p99):  ${result.metrics.latency.p99.toFixed(2)}ms`);
      console.log(`  Latency (mean): ${result.metrics.latency.mean.toFixed(2)}ms`);
      console.log(`  Throughput:     ${result.metrics.throughput.toFixed(2)} embeddings/sec`);
    } catch (error) {
      console.error(`Error evaluating ${modelConfig.name}:`, error);
      if (error instanceof Error && error.message.includes('model')) {
        console.error(`\nHint: Make sure ${modelConfig.name} is pulled in Ollama:`);
        console.error(`  ollama pull ${modelConfig.name}`);
      }
    }
  }

  // Summary comparison
  if (results.length > 1) {
    console.log('\n\n');
    console.log('='.repeat(60));
    console.log('SUMMARY COMPARISON');
    console.log('='.repeat(60));
    console.log();

    // Sort by overall score
    const sorted = [...results].sort((a, b) => b.overallScore - a.overallScore);

    console.log('Ranking by Overall Score:');
    sorted.forEach((result, index) => {
      console.log(`  ${index + 1}. ${result.modelName.padEnd(20)} ${result.overallScore.toFixed(2)}/100`);
    });
    console.log();

    // Best in each category
    console.log('Best Performance by Category:');

    const bestRetrieval = results.reduce((best, curr) =>
      curr.metrics.retrieval.ndcgAt10 > best.metrics.retrieval.ndcgAt10 ? curr : best
    );
    console.log(`  Best Retrieval:  ${bestRetrieval.modelName} (NDCG@10: ${(bestRetrieval.metrics.retrieval.ndcgAt10 * 100).toFixed(2)}%)`);

    const bestSimilarity = results.reduce((best, curr) =>
      curr.metrics.similarity.accuracy > best.metrics.similarity.accuracy ? curr : best
    );
    console.log(`  Best Similarity: ${bestSimilarity.modelName} (Accuracy: ${(bestSimilarity.metrics.similarity.accuracy * 100).toFixed(2)}%)`);

    const bestLatency = results.reduce((best, curr) =>
      curr.metrics.latency.p95 < best.metrics.latency.p95 ? curr : best
    );
    console.log(`  Best Latency:    ${bestLatency.modelName} (p95: ${bestLatency.metrics.latency.p95.toFixed(2)}ms)`);

    const bestThroughput = results.reduce((best, curr) =>
      curr.metrics.throughput > best.metrics.throughput ? curr : best
    );
    console.log(`  Best Throughput: ${bestThroughput.modelName} (${bestThroughput.metrics.throughput.toFixed(2)} emb/sec)`);
  }

  console.log('\n');
}

main().catch(console.error);
